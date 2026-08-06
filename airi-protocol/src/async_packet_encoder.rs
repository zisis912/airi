use aes::cipher::KeyIvInit;
use async_compression::{Level, tokio::write::ZlibEncoder};
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    CompressionLevel, CompressionThreshold, MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, VarInt,
    connection::{Aes128Cfb8Enc, AsyncStreamEncryptor},
    packet_encoder::PacketEncodeError,
};

pub enum AsyncEncryptionWriter<W: AsyncWrite + Unpin> {
    Encrypt(Box<AsyncStreamEncryptor<W>>),
    None(W),
}

impl<W: AsyncWrite + Unpin> AsyncEncryptionWriter<W> {
    pub fn upgrade(self, cipher: Aes128Cfb8Enc) -> Self {
        match self {
            Self::None(stream) => {
                Self::Encrypt(Box::new(AsyncStreamEncryptor::new(cipher, stream)))
            }
            _ => panic!("Cannot upgrade a stream that already has a cipher!"),
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for AsyncEncryptionWriter<W> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_write(cx, buf)
            }
            Self::None(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_write(cx, buf)
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_flush(cx)
            }
            Self::None(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_shutdown(cx)
            }
            Self::None(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_shutdown(cx)
            }
        }
    }
}

/// Encoder: Server -> Client
/// Supports ZLib endecoding/compression
/// Supports Aes128 Encryption
pub struct AsyncNetworkEncoder<W: AsyncWrite + Unpin> {
    writer: AsyncEncryptionWriter<W>,
    // compression and compression threshold
    compression: Option<(CompressionThreshold, CompressionLevel)>,
}

impl<W: AsyncWrite + Unpin> AsyncNetworkEncoder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: AsyncEncryptionWriter::None(writer),
            compression: None,
        }
    }

    pub fn set_compression(&mut self, compression_info: (CompressionThreshold, CompressionLevel)) {
        self.compression = Some(compression_info);
    }

    /// NOTE: Encryption can only be set; a minecraft stream cannot go back to being unencrypted
    pub fn set_encryption(&mut self, key: &[u8; 16]) {
        if matches!(self.writer, AsyncEncryptionWriter::Encrypt(_)) {
            panic!("Cannot upgrade a stream that already has a cipher!");
        }
        let cipher = Aes128Cfb8Enc::new_from_slices(key, key).expect("invalid key");
        take_mut::take(&mut self.writer, |encoder| encoder.upgrade(cipher));
    }

    pub fn is_encrypted(&self) -> bool {
        matches!(self.writer, AsyncEncryptionWriter::Encrypt(_))
    }

    pub fn has_compression(&self) -> bool {
        self.compression.is_some()
    }

    /// Appends a Clientbound `ClientPacket` to the internal buffer and applies compression when needed.
    ///
    /// If compression is enabled and the packet size exceeds the threshold, the packet is compressed.
    /// The packet is prefixed with its length and, if compressed, the uncompressed data length.
    /// The packet format is as follows:
    ///
    /// **Uncompressed:**
    /// |-----------------------|
    /// | Packet Length (VarInt)|
    /// |-----------------------|
    /// | Packet ID (VarInt)    |
    /// |-----------------------|
    /// | Data (Byte Array)     |
    /// |-----------------------|
    ///
    /// **Compressed:**
    /// |------------------------|
    /// | Packet Length (VarInt) |
    /// |------------------------|
    /// | Data Length (VarInt)   |
    /// |------------------------|
    /// | Packet ID (VarInt)     |
    /// |------------------------|
    /// | Data (Byte Array)      |
    /// |------------------------|
    ///
    /// -   `Packet Length`: The total length of the packet *excluding* the `Packet Length` field itself.
    /// -   `Data Length`: (Only present in compressed packets) The length of the uncompressed `Packet ID` and `Data`.
    /// -   `Packet ID`: The ID of the packet.
    /// -   `Data`: The packet's data.
    pub async fn write_packet(&mut self, packet_data: &[u8]) -> Result<(), PacketEncodeError> {
        // We need to know the length of the compressed buffer and serde is not async :(
        // We need to write to a buffer here 😔

        let data_len = packet_data.len();
        if data_len > MAX_PACKET_DATA_SIZE {
            return Err(PacketEncodeError::TooLong(data_len));
        }

        let data_len_var_int: VarInt = data_len.try_into().map_err(|_| {
            PacketEncodeError::Message(format!(
                "Packet data length is too large to fit in VarInt! ({data_len})"
            ))
        })?;

        if let Some((compression_threshold, compression_level)) = self.compression {
            if data_len >= compression_threshold {
                // Pushed before data:
                // Length of (Data Length) + length of compressed (Packet ID + Data)
                // Length of uncompressed (Packet ID + Data)

                // TODO: We need the compressed length at the beginning of the packet so we need to write to
                // buf here :( Is there a magic way to find a compressed length?
                let mut compressed_buf = Vec::new();
                let mut compressor = ZlibEncoder::with_quality(
                    &mut compressed_buf,
                    Level::Precise(compression_level as i32),
                );

                compressor
                    .write_all(packet_data)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                compressor
                    .flush()
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                debug_assert!(!compressed_buf.is_empty());

                let full_packet_len_var_int: VarInt = (data_len_var_int.written_size()
                    + compressed_buf.len())
                .try_into()
                .map_err(|_| {
                    PacketEncodeError::Message(format!(
                        "Full packet length is too large to fit in VarInt! ({data_len})"
                    ))
                })?;

                let complete_serialization_length =
                    full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
                if complete_serialization_length > MAX_PACKET_SIZE as usize {
                    return Err(PacketEncodeError::TooLong(complete_serialization_length));
                }

                full_packet_len_var_int
                    .write_to_async(&mut self.writer)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                data_len_var_int
                    .write_to_async(&mut self.writer)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                self.writer
                    .write_all(&compressed_buf)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            } else {
                // Pushed before data:
                // Length of (Data Length) + length of compressed (Packet ID + Data)
                // 0 to indicate uncompressed

                let data_len_var_int: VarInt = 0.into();
                let full_packet_len_var_int: VarInt = (data_len_var_int.written_size() + data_len)
                    .try_into()
                    .map_err(|_| {
                        PacketEncodeError::Message(format!(
                            "Full packet length is too large to fit in VarInt! ({data_len})"
                        ))
                    })?;

                let complete_serialization_length =
                    full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
                if complete_serialization_length > MAX_PACKET_SIZE as usize {
                    return Err(PacketEncodeError::TooLong(complete_serialization_length));
                }

                full_packet_len_var_int
                    .write_to_async(&mut self.writer)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                data_len_var_int
                    .write_to_async(&mut self.writer)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                self.writer
                    .write_all(packet_data)
                    .await
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            }
        } else {
            // Pushed before data:
            // Length of Packet ID + Data

            let full_packet_len_var_int: VarInt = data_len_var_int;

            let complete_serialization_length =
                full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
            if complete_serialization_length > MAX_PACKET_SIZE as usize {
                return Err(PacketEncodeError::TooLong(complete_serialization_length));
            }

            full_packet_len_var_int
                .write_to_async(&mut self.writer)
                .await
                .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            self.writer
                .write_all(packet_data)
                .await
                .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
        }

        self.writer
            .flush()
            .await
            .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
        Ok(())
    }
}

#[derive(Error, Debug)]
#[error("Invalid compression Level")]
pub struct CompressionLevelError;
