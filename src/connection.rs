use aes::cipher::{BlockDecryptMut, BlockEncryptMut, BlockSizeUser, generic_array::GenericArray};
use std::{
    io::{self, Error, Read, Write},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};

pub type Aes128Cfb8Enc = cfb8::Encryptor<aes::Aes128>;
pub type Aes128Cfb8Dec = cfb8::Decryptor<aes::Aes128>;

pub struct StreamDecryptor<R: Read> {
    cipher: Aes128Cfb8Dec,
    reader: R,
}

#[derive(Debug)]
pub struct AsyncStreamDecryptor<R: AsyncRead + Unpin> {
    cipher: Aes128Cfb8Dec,
    read: R,
}

impl<R: Read> StreamDecryptor<R> {
    pub fn new(cipher: Aes128Cfb8Dec, reader: R) -> Self {
        Self { cipher, reader }
    }
}

impl<R: AsyncRead + Unpin> AsyncStreamDecryptor<R> {
    pub fn new(cipher: Aes128Cfb8Dec, read: R) -> Self {
        Self { cipher, read }
    }
}

impl<R: Read> Read for StreamDecryptor<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let reader = &mut self.reader;
        let cipher = &mut self.cipher;

        let bytes_read = reader.read(buf)?;

        for block in buf[..bytes_read].chunks_mut(Aes128Cfb8Dec::block_size()) {
            cipher.decrypt_block_mut(block.into());
        }

        Ok(bytes_read)
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for AsyncStreamDecryptor<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let ref_self = self.get_mut();
        let read = Pin::new(&mut ref_self.read);
        let cipher = &mut ref_self.cipher;

        // Get the starting position
        let original_fill = buf.filled().len();
        // Read the raw data
        let internal_poll = read.poll_read(cx, buf);

        if matches!(internal_poll, Poll::Ready(Ok(_))) {
            // Decrypt the raw data in-place, note that our block size is 1 byte, so this is always safe
            for block in buf.filled_mut()[original_fill..].chunks_mut(Aes128Cfb8Dec::block_size()) {
                cipher.decrypt_block_mut(block.into());
            }
        }

        internal_poll
    }
}

///NOTE: This makes lots of small writes; make sure there is a buffer somewhere down the line
/// or atleast this is the documentation that came along with the skidded code before i converted it
/// to synchronous writes
pub struct StreamEncryptor<W: Write> {
    cipher: Aes128Cfb8Enc,
    writer: W,
    // last_unwritten_encrypted_byte: Option<u8>,
}

pub struct AsyncStreamEncryptor<W: AsyncWrite + Unpin> {
    cipher: Aes128Cfb8Enc,
    write: W,
    last_unwritten_encrypted_byte: Option<u8>,
}

impl<W: Write> StreamEncryptor<W> {
    pub fn new(cipher: Aes128Cfb8Enc, writer: W) -> Self {
        Self { cipher, writer }
    }
}

impl<W: AsyncWrite + Unpin> AsyncStreamEncryptor<W> {
    pub fn new(cipher: Aes128Cfb8Enc, write: W) -> Self {
        Self {
            cipher,
            write,
            last_unwritten_encrypted_byte: None,
        }
    }
}

impl<W: Write> Write for StreamEncryptor<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let cipher = &mut self.cipher;
        let writer = &mut self.writer;

        let mut total_written = 0;

        for block in buf.chunks(Aes128Cfb8Enc::block_size()) {
            let mut out = [0u8];

            let out_block = GenericArray::from_mut_slice(&mut out);
            cipher.encrypt_block_b2b_mut(block.into(), out_block);

            let bytes_written = writer.write(&out)?;
            total_written += bytes_written
        }

        Ok(total_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let writer = &mut self.writer;
        writer.flush()
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for AsyncStreamEncryptor<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let ref_self = self.get_mut();
        let cipher = &mut ref_self.cipher;

        let mut total_written = 0;
        // Decrypt the raw data, note that our block size is 1 byte, so this is always safe
        for block in buf.chunks(Aes128Cfb8Enc::block_size()) {
            let mut out = [0u8];

            if let Some(out_to_use) = ref_self.last_unwritten_encrypted_byte {
                // This assumes that this `poll_write` is called on the same stream of bytes which I
                // think is a fair assumption, since thats an invariant for the TCP stream anyway.

                // This should never panic
                out[0] = out_to_use;
            } else {
                let out_block = GenericArray::from_mut_slice(&mut out);
                cipher.encrypt_block_b2b_mut(block.into(), out_block);
            }

            let write = Pin::new(&mut ref_self.write);
            match write.poll_write(cx, &out) {
                Poll::Pending => {
                    ref_self.last_unwritten_encrypted_byte = Some(out[0]);
                    if total_written == 0 {
                        //If we didn't write anything, return pending
                        return Poll::Pending;
                    } else {
                        // Otherwise, we actually did write something
                        return Poll::Ready(Ok(total_written));
                    }
                }
                Poll::Ready(result) => {
                    ref_self.last_unwritten_encrypted_byte = None;
                    match result {
                        Ok(written) => total_written += written,
                        Err(err) => return Poll::Ready(Err(err)),
                    }
                }
            }
        }

        Poll::Ready(Ok(total_written))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let ref_self = self.get_mut();
        let write = Pin::new(&mut ref_self.write);
        write.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let ref_self = self.get_mut();
        let write = Pin::new(&mut ref_self.write);
        write.poll_shutdown(cx)
    }
}
