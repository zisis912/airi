use std::io::{self, Write};

use aes::cipher::KeyIvInit;
use flate2::{Compression, write::ZlibEncoder};
use thiserror::Error;

use crate::{
    CompressionLevel, CompressionThreshold, MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, Serializable,
    VarInt,
    connection::{Aes128Cfb8Enc, StreamEncryptor},
};

/// Errors that can occur during packet encoding.
#[derive(Error, Debug)]
pub enum PacketEncodeError {
    #[error("Packet exceeds maximum length: {0}")]
    TooLong(usize),
    #[error("Compression failed {0}")]
    CompressionFailed(String),
    #[error("Writing packet failed: {0}")]
    Message(String),
    #[error("Invalid Aes Key")]
    EncryptionFailed,
}

#[derive(Error, Debug)]
#[error("Invalid compression Level")]
pub struct CompressionLevelError;

/// Supports ZLib endecoding/compression
/// Supports Aes128 Encryption
pub struct NetworkEncoder<W: Write> {
    writer: EncryptionWriter<W>,
    // compression and compression threshold
    compression: Option<(CompressionThreshold, CompressionLevel)>,
}

impl<W: Write> NetworkEncoder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: EncryptionWriter::None(writer),
            compression: None,
        }
    }

    pub fn set_compression(&mut self, compression_info: (CompressionThreshold, CompressionLevel)) {
        self.compression = Some(compression_info);
    }

    /// NOTE: Encryption can only be set; a minecraft stream cannot go back to being unencrypted
    pub fn set_encryption(&mut self, key: &[u8; 16]) -> Result<(), PacketEncodeError> {
        if matches!(self.writer, EncryptionWriter::Encrypt(_)) {
            panic!("Cannot upgrade a stream that already has a cipher!");
        }
        let cipher = Aes128Cfb8Enc::new_from_slices(key, key)
            .map_err(|_| PacketEncodeError::EncryptionFailed)?;
        take_mut::take(&mut self.writer, |encoder| encoder.upgrade(cipher));
        // if let Some(writer) = self.writer.take
        Ok(())
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
        let data_len = packet_data.len();
        if data_len > MAX_PACKET_DATA_SIZE {
            return Err(PacketEncodeError::TooLong(data_len));
        }
        let data_len_varint: VarInt = data_len.try_into().map_err(|_| {
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
                let mut compressed_buf: Vec<u8> = Vec::new();
                let mut compressor =
                    ZlibEncoder::new(&mut compressed_buf, Compression::new(compression_level));

                compressor
                    .write_all(packet_data)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                compressor
                    .flush()
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;

                let compressed_buf = compressor
                    .finish()
                    .map_err(|_| PacketEncodeError::Message("compressor failed".to_owned()))?;

                debug_assert!(!compressed_buf.is_empty());
                let full_packet_len: VarInt = (data_len_varint.written_size()
                    + compressed_buf.len())
                .try_into()
                .map_err(|_| {
                    PacketEncodeError::Message(format!(
                        "Full packet length is too large to fit in VarInt! ({data_len})"
                    ))
                })?;

                let complete_serialization_length =
                    full_packet_len.written_size() + full_packet_len.0 as usize;
                if complete_serialization_length > MAX_PACKET_SIZE as usize {
                    return Err(PacketEncodeError::TooLong(complete_serialization_length));
                }

                full_packet_len
                    .write_to(&mut self.writer)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                data_len_varint
                    .write_to(&mut self.writer)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                self.writer
                    .write_all(compressed_buf)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            } else {
                // Pushed before data:
                // Length of (Data Length) + length of compressed (Packet ID + Data)
                // 0 to indicate uncompressed

                // let data_len_var_int = VarInt(0);
                let full_packet_len = VarInt::try_from(1 + data_len).map_err(|_| {
                    PacketEncodeError::Message(format!(
                        "Full packet length is too large to fit in VarInt! ({data_len})"
                    ))
                })?;

                let complete_serialization_length =
                    full_packet_len.written_size() + full_packet_len.0 as usize;
                if complete_serialization_length > MAX_PACKET_SIZE as usize {
                    return Err(PacketEncodeError::TooLong(complete_serialization_length));
                }

                full_packet_len
                    .write_to(&mut self.writer)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                VarInt(0)
                    .write_to(&mut self.writer)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                self.writer
                    .write_all(packet_data)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            }
        } else {
            // Pushed before data:
            // Length of Packet ID + Data

            let full_packet_len_var_int: VarInt = data_len_varint;

            let complete_serialization_length =
                full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
            if complete_serialization_length > MAX_PACKET_SIZE as usize {
                return Err(PacketEncodeError::TooLong(complete_serialization_length));
            }

            full_packet_len_var_int
                .write_to(&mut self.writer)
                .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            self.writer
                .write_all(packet_data)
                .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
        }

        self.writer
            .flush()
            .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
        Ok(())
    }
}

pub enum EncryptionWriter<W: Write> {
    Encrypt(Box<StreamEncryptor<W>>),
    None(W),
}

impl<W: Write> EncryptionWriter<W> {
    pub fn upgrade(self, cipher: Aes128Cfb8Enc) -> Self {
        match self {
            Self::None(stream) => Self::Encrypt(Box::new(StreamEncryptor::new(cipher, stream))),
            _ => panic!("cannot upgrade a stream that already has a cipher"),
        }
    }
}

impl<W: Write> Write for EncryptionWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Encrypt(writer) => writer.write(buf),
            Self::None(writer) => writer.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Encrypt(writer) => writer.flush(),
            Self::None(writer) => writer.flush(),
        }
    }
}
