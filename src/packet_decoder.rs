use std::io::{self, BufReader, Read};

use aes::cipher::KeyIvInit;
use flate2::bufread::ZlibDecoder;
use thiserror::Error;

use crate::{
    CompressionThreshold, MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, RawPacket, ReadingError,
    Serializable, VarInt,
    connection::{Aes128Cfb8Dec, StreamDecryptor},
};

#[derive(Error, Debug)]
pub enum PacketDecodeError {
    #[error("failed to decode packet ID")]
    DecodeID,
    #[error("packet exceeds maximum length")]
    TooLong,
    #[error("packet length is out of bounds")]
    OutOfBounds,
    #[error("malformed packet length VarInt: {0}")]
    MalformedLength(String),
    #[error("failed to decompress packet: {0}")]
    FailedDecompression(String), // Updated to include error details
    #[error("packet is uncompressed but greater than the threshold")]
    NotCompressed,
    #[error("the connection has closed")]
    ConnectionClosed,
}

impl From<ReadingError> for PacketDecodeError {
    fn from(value: ReadingError) -> Self {
        Self::FailedDecompression(value.to_string())
    }
}

/// Decoder: Client -> Server
/// Supports ZLib decoding/decompression
/// Supports Aes128 Encryption
pub struct NetworkDecoder<R: Read> {
    reader: DecryptionReader<R>,
    compression: Option<CompressionThreshold>,
}

impl<R: Read> NetworkDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: DecryptionReader::None(reader),
            compression: None,
        }
    }

    pub fn set_compression(&mut self, threshold: CompressionThreshold) {
        self.compression = Some(threshold);
    }

    /// NOTE: Encryption can only be set; a minecraft stream cannot go back to being unencrypted
    pub fn set_encryption(&mut self, key: &[u8; 16]) {
        if matches!(self.reader, DecryptionReader::Decrypt(_)) {
            panic!("Cannot upgrade a stream that already has a cipher!");
        }
        let cipher = Aes128Cfb8Dec::new_from_slices(key, key).expect("invalid key");
        take_mut::take(&mut self.reader, |decoder| decoder.upgrade(cipher));
        // self.reader = self.reader.upgrade(cipher);
    }

    pub fn get_raw_packet(&mut self) -> Result<RawPacket, PacketDecodeError> {
        let packet_len = VarInt::read_from(&mut self.reader)?.0 as u64;

        if !(0..=MAX_PACKET_SIZE).contains(&packet_len) {
            return Err(PacketDecodeError::OutOfBounds);
        }

        let mut bounded_reader = (&mut self.reader).take(packet_len);

        let mut reader = if let Some(threshold) = self.compression {
            let decompressed_length = VarInt::read_from(&mut bounded_reader)?;
            let raw_packet_len = packet_len - decompressed_length.written_size() as u64;
            let decompressed_len = decompressed_length.0 as usize;

            if !(0..=MAX_PACKET_DATA_SIZE).contains(&decompressed_len) {
                Err(PacketDecodeError::TooLong)?
            }

            if decompressed_len > 0 {
                DecompressionReader::Decompress(ZlibDecoder::new(BufReader::new(bounded_reader)))
            } else {
                // Validate that we are not less than the compression threshold
                if raw_packet_len > threshold as u64 {
                    Err(PacketDecodeError::NotCompressed)?
                }

                DecompressionReader::None(bounded_reader)
            }
        } else {
            DecompressionReader::None(bounded_reader)
        };

        let packet_id = VarInt::read_from(&mut reader)
            .map_err(|_| PacketDecodeError::DecodeID)?
            .0;

        let mut payload = Vec::new();
        reader
            .read_to_end(&mut payload)
            .map_err(|err| PacketDecodeError::FailedDecompression(err.to_string()))?;

        Ok(RawPacket {
            id: packet_id,
            payload,
        })
    }
}

// decrypt -> decompress -> raw
pub enum DecompressionReader<R: Read> {
    Decompress(ZlibDecoder<BufReader<R>>),
    None(R),
}

impl<R: Read> Read for DecompressionReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Decompress(reader) => reader.read(buf),
            Self::None(reader) => reader.read(buf),
        }
    }
}

pub enum DecryptionReader<R: Read> {
    Decrypt(Box<StreamDecryptor<R>>),
    None(R),
}

impl<R: Read> DecryptionReader<R> {
    pub fn upgrade(self, cipher: Aes128Cfb8Dec) -> Self {
        match self {
            Self::None(stream) => Self::Decrypt(Box::new(StreamDecryptor::new(cipher, stream))),
            _ => panic!("cannot upgrade a stream that already has a cipher"),
        }
    }
}

impl<R: Read> Read for DecryptionReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Decrypt(reader) => reader.read(buf),
            Self::None(reader) => reader.read(buf),
        }
    }
}
