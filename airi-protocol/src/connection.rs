use std::{
    io::{self, Read, Write},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite};

pub(super) type Aes128Cfb8Enc = cfb8::Encryptor<aes::Aes128>;
pub(super) type Aes128Cfb8Dec = cfb8::Decryptor<aes::Aes128>;

pub struct StreamDecryptor<R: Read> {
    cipher: Aes128Cfb8Dec,
    reader: R,
}

impl<R: Read> StreamDecryptor<R> {
    pub fn new(cipher: Aes128Cfb8Dec, reader: R) -> Self {
        Self { cipher, reader }
    }
}

impl<R: Read> Read for StreamDecryptor<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes_read = self.reader.read(buf)?;

        self.cipher.decrypt(&mut buf[..bytes_read]);

        Ok(bytes_read)
    }
}

#[derive(Debug)]
pub(super) struct AsyncStreamDecryptor<R: AsyncRead + Unpin> {
    cipher: Aes128Cfb8Dec,
    reader: R,
}

impl<R: AsyncRead + Unpin> AsyncStreamDecryptor<R> {
    pub fn new(cipher: Aes128Cfb8Dec, reader: R) -> Self {
        Self { cipher, reader }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for AsyncStreamDecryptor<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        let cipher = &mut this.cipher;

        // Get the starting position
        let original_fill = buf.filled().len();

        // Read the raw data
        let reader = Pin::new(&mut this.reader);
        let internal_poll = reader.poll_read(cx, buf);

        if let Poll::Ready(Ok(())) = internal_poll {
            // Decrypt the raw data in-place, note that our block size is 1 byte, so this is always safe
            cipher.decrypt(&mut buf.filled_mut()[original_fill..]);
        };

        internal_poll
    }
}

///NOTE: This makes lots of small writes; make sure there is a buffer somewhere down the line
/// or atleast this is the documentation that came along with the skidded code before i converted it
/// to synchronous writes
pub(super) struct StreamEncryptor<W: Write> {
    cipher: Aes128Cfb8Enc,
    writer: W,
    pending: Vec<u8>,
    pending_pos: usize,
    // last_unwritten_encrypted_byte: Option<u8>,
}

impl<W: Write> StreamEncryptor<W> {
    pub fn new(cipher: Aes128Cfb8Enc, writer: W) -> Self {
        Self {
            cipher,
            writer,
            pending: Vec::new(),
            pending_pos: 0,
        }
    }

    fn flush_pending(&mut self) -> io::Result<()> {
        while self.pending_pos < self.pending.len() {
            let n = self.writer.write(&self.pending[self.pending_pos..])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "underlying writer accepted 0 bytes of buffered ciphertext",
                ));
            }
            self.pending_pos += n;
        }

        Ok(())
    }
}

impl<W: Write> Write for StreamEncryptor<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.flush_pending()?;

        if buf.is_empty() {
            return Ok(0);
        }

        // make just enough space for the encrypted bytes, unwrap is safe
        self.pending.resize(buf.len(), 0);
        self.cipher.encrypt_b2b(buf, &mut self.pending).unwrap();

        self.pending_pos = self.writer.write(&self.pending)?;

        // this is always equal to scratch.len()
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_pending()?;
        self.writer.flush()
    }
}

pub(super) struct AsyncStreamEncryptor<W: AsyncWrite + Unpin> {
    cipher: Aes128Cfb8Enc,
    writer: W,
    pending: Vec<u8>,
    pending_pos: usize,
}

impl<W: AsyncWrite + Unpin> AsyncStreamEncryptor<W> {
    pub fn new(cipher: Aes128Cfb8Enc, writer: W) -> Self {
        Self {
            cipher,
            writer,
            pending: Vec::new(),
            pending_pos: 0,
        }
    }

    fn poll_flush_pending(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.pending_pos < self.pending.len() {
            let writer = Pin::new(&mut self.writer);
            match writer.poll_write(cx, &self.pending[self.pending_pos..]) {
                Poll::Ready(result) => match result {
                    Ok(n) => {
                        if n == 0 {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::WriteZero,
                                "underlying writer accepted 0 bytes of buffered ciphertext",
                            )));
                        }

                        self.pending_pos += n;
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                },
                Poll::Pending => return Poll::Pending,
            };
        }
        Poll::Ready(Ok(()))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for AsyncStreamEncryptor<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let this = self.get_mut();

        // flush pending ciphertext
        match this.poll_flush_pending(cx) {
            Poll::Ready(result) => match result {
                Ok(()) => {}
                Err(e) => return Poll::Ready(Err(e)),
            },
            Poll::Pending => return Poll::Pending,
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        // load the buf into a Vec<u8> then encrypt in-place
        this.pending.clear();
        this.pending.extend_from_slice(buf);
        this.cipher.encrypt(&mut this.pending);

        // write some bytes now, keep the rest in pending for later
        let writer = Pin::new(&mut this.writer);
        match writer.poll_write(cx, &this.pending) {
            Poll::Ready(result) => match result {
                Ok(n) => this.pending_pos += n,
                Err(e) => return Poll::Ready(Err(e)),
            },
            Poll::Pending => {}
        };

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.as_mut().poll_flush_pending(cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        match self.as_mut().poll_flush_pending(cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }

        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}
