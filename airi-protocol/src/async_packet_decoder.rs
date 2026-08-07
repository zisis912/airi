use std::{
    io::{self},
    pin::Pin,
};

use aes::cipher::KeyIvInit;
use tokio::io::{AsyncReadExt, BufReader};

use async_compression::tokio::bufread::ZlibDecoder;
use tokio::io::AsyncRead;

use crate::{
    CompressionThreshold, MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, RawPacket, VarInt,
    connection::{Aes128Cfb8Dec, AsyncStreamDecryptor},
    packet_decoder::PacketDecodeError,
};

/// Wrapper type over an implementor of [`tokio::io::AsyncRead`]. Provides the async [`get_raw_packet`]
/// method, used to read a raw minecraft packet (id and payload) from an encrypted/compressed
/// stream.
///
/// Supports Zlib decompression and Aes128-Cfb8 decryption.
///
/// [`get_raw_packet`]: AsyncNetworkDecoder::get_raw_packet
#[derive(Debug)]
pub struct AsyncNetworkDecoder<R: AsyncRead + Unpin> {
    reader: AsyncDecryptionReader<R>,
    compression: Option<CompressionThreshold>,
}

impl<R: AsyncRead + Unpin> AsyncNetworkDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: AsyncDecryptionReader::None(reader),
            compression: None,
        }
    }

    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.compression = Some(threshold);
    }

    /// NOTE: Encryption can only be set; a minecraft stream cannot go back to being unencrypted
    pub fn set_encryption(&mut self, key: &[u8; 16]) {
        // if matches!(self.reader, AsyncDecryptionReader::Decrypt(_)) {
        //     panic!("Cannot upgrade a stream that already has a cipher!");
        // }
        let cipher = Aes128Cfb8Dec::new_from_slices(key, key).expect("invalid key");
        take_mut::take(&mut self.reader, |decoder| decoder.upgrade(cipher));
    }

    pub async fn get_raw_packet(&mut self) -> Result<RawPacket, PacketDecodeError> {
        let packet_len = VarInt::read_async(&mut self.reader).await?;

        // .map_err(|err| match err {
        //     ReadingError::CleanEOF(_) => PacketDecodeError::ConnectionClosed,
        //     err => PacketDecodeError::MalformedLength(err.to_string()),
        // })?;

        let packet_len = packet_len.0 as u64;

        if !(0..=MAX_PACKET_SIZE).contains(&packet_len) {
            Err(PacketDecodeError::OutOfBounds)?
        }

        let mut bounded_reader = (&mut self.reader).take(packet_len);

        let mut reader = if let Some(threshold) = self.compression {
            let decompressed_length = VarInt::read_async(&mut bounded_reader).await?;
            let raw_packet_length = packet_len - decompressed_length.written_size() as u64;
            let decompressed_length = VarInt::read_async(&mut bounded_reader).await?.0 as usize;

            if !(0..=MAX_PACKET_DATA_SIZE).contains(&decompressed_length) {
                Err(PacketDecodeError::TooLong)?
            }

            // if packet is uncompressed
            if decompressed_length == 0 {
                // Validate that we are not less than the compression threshold
                if raw_packet_length > threshold as u64 {
                    Err(PacketDecodeError::NotCompressed)?
                }

                AsyncDecompressionReader::None(bounded_reader)
            } else {
                AsyncDecompressionReader::Decompress(ZlibDecoder::new(BufReader::new(
                    bounded_reader,
                )))
            }
        } else {
            AsyncDecompressionReader::None(bounded_reader)
        };

        let packet_id = VarInt::read_async(&mut reader)
            .await
            .map_err(|_| PacketDecodeError::DecodeID)?
            .0;

        let mut payload = Vec::new();
        reader
            .read_to_end(&mut payload)
            .await
            .map_err(|err| PacketDecodeError::FailedDecompression(err.to_string()))?;

        Ok(RawPacket {
            id: packet_id,
            payload,
        })
    }
}

enum AsyncDecompressionReader<R: AsyncRead + Unpin> {
    Decompress(ZlibDecoder<BufReader<R>>),
    None(R),
}

impl<R: AsyncRead + Unpin> AsyncRead for AsyncDecompressionReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Decompress(reader) => {
                let reader = Pin::new(reader);
                reader.poll_read(cx, buf)
            }
            Self::None(reader) => {
                let reader = Pin::new(reader);
                reader.poll_read(cx, buf)
            }
        }
    }
}

#[derive(Debug)]
enum AsyncDecryptionReader<R: AsyncRead + Unpin> {
    Decrypt(Box<AsyncStreamDecryptor<R>>),
    None(R),
}

impl<R: AsyncRead + Unpin> AsyncDecryptionReader<R> {
    pub fn upgrade(self, cipher: Aes128Cfb8Dec) -> Self {
        match self {
            Self::None(stream) => {
                Self::Decrypt(Box::new(AsyncStreamDecryptor::new(cipher, stream)))
            }
            _ => panic!("cannot upgrade a stream that already has a cipher"),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for AsyncDecryptionReader<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Decrypt(reader) => {
                let reader = Pin::new(reader);
                reader.poll_read(cx, buf)
            }
            Self::None(reader) => {
                let reader = Pin::new(reader);
                reader.poll_read(cx, buf)
            }
        }
    }
}
