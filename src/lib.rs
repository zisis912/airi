use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use core::fmt;
use hex::FromHexError;
use macros::Serializable;
use std::{
    fmt::Display,
    io::{self, Read},
    marker::PhantomData,
    string::FromUtf8Error,
};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub mod async_packet_decoder;
pub mod async_packet_encoder;
pub mod bitset;
pub mod connection;
pub mod nbt;
pub mod packet;
pub mod packet_decoder;
pub mod packet_encoder;
pub mod slot;
pub mod text_component;

pub const MAX_PACKET_SIZE: u64 = 2097152;
pub const MAX_PACKET_DATA_SIZE: usize = 8388608;

pub type CompressionThreshold = usize;
pub type CompressionLevel = u32;

#[derive(Debug)]
pub struct RawPacket {
    pub id: i32,
    pub payload: Vec<u8>,
}

// #[derive(Error, Debug)]
// pub enum Error {
//     #[error("custom serialize error")]
//     SerializeError(String),
//     #[error("Io Error: {0}")]
//     IoError(#[from] io::Error),
//     #[error("utf8 decode error")]
//     Utf8Error(#[from] FromUtf8Error),
//     #[error("json parsing error")]
//     JsonError(#[from] serde_json::Error),
// }

#[derive(Debug, Error)]
pub enum WritingError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serde failure: {0}")]
    Serde(String),
    #[error("Failed to serialize packet: {0}")]
    Message(String),
}

#[derive(Debug, Error)]
pub enum ReadingError {
    // addition
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("utf8 decode error: {0}")]
    Utf8Error(#[from] FromUtf8Error),
    #[error("serde failure: {0}")]
    Serde(#[from] serde_json::Error),
    //
    #[error("EOF, Tried to read {0} but No bytes left to consume")]
    CleanEOF(String),
    #[error("incomplete: {0}")]
    Incomplete(String),
    #[error("too large: {0}")]
    TooLarge(String),
    #[error("{0}")]
    Message(String),
}

pub trait Serializable: Sized {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError>;
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError>;
}

pub trait Lengthable: Serializable {
    fn from_len(val: usize) -> Self;
    fn into_len(self) -> usize;
}

impl Serializable for bool {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<bool, ReadingError> {
        Ok(buf.read_u8()? != 0)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VarInt(pub i32);

const SEGMENT_BITS: u8 = 0x7F;
const CONTINUE_BIT: u8 = 0x80;

impl Serializable for VarInt {
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut val = self.0 as u32;

        while val > 0x7F {
            buf.write_u8(val as u8 | 0x80)?;

            val >>= 7;
        }

        buf.write_u8(val as u8)?;
        return Ok(());
    }

    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut value = 0u32;
        let mut position = 0u8;

        loop {
            let current_byte = buf.read_u8()?;
            value |= (current_byte as u32 & 0x7F) << position;

            if (current_byte & CONTINUE_BIT) == 0 {
                break;
            }

            position += 7;

            if position >= 32 {
                return Err(ReadingError::Message("VarInt is too big".to_owned()));
            }
        }

        Ok(VarInt(value as i32))
    }
}

impl VarInt {
    pub fn written_size(&self) -> usize {
        match self.0 {
            0 => 1,
            n => (31 - n.leading_zeros() as usize) / 7 + 1,
        }
    }

    async fn read_async<R: AsyncRead + Unpin>(buf: &mut R) -> Result<VarInt, ReadingError> {
        let mut value = 0u32;
        let mut position = 0u8;

        loop {
            let current_byte = buf.read_u8().await?;
            value |= (current_byte as u32 & 0x7F) << position;

            if (current_byte & CONTINUE_BIT) == 0 {
                break;
            }

            position += 7;

            if position >= 32 {
                return Err(ReadingError::Message(format!("VarInt is too big")));
            }
        }

        Ok(VarInt(value as i32))
    }

    async fn write_to_async<W: AsyncWrite + Unpin>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut value = self.0 as u32;
        loop {
            if (value as u8 & !0x7F) == 0 {
                buf.write_u8(value as u8).await?;
                return Ok(());
            }

            buf.write_u8((value as u8 & SEGMENT_BITS) | CONTINUE_BIT)
                .await?;

            value >>= 7;
        }
    }
}

impl TryFrom<usize> for VarInt {
    type Error = <i32 as TryFrom<usize>>::Error;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(VarInt(value.try_into()?))
    }
}

impl From<i32> for VarInt {
    fn from(value: i32) -> Self {
        VarInt(value.into())
    }
}

#[derive(Debug)]
pub struct VarLong(i64);

impl Serializable for VarLong {
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut value = self.0 as u64;
        loop {
            if (value & !0x7F) == 0 {
                buf.write_u8(value as u8)?;
                return Ok(());
            }

            buf.write_u8((value as u8 & SEGMENT_BITS) | CONTINUE_BIT)?;

            value >>= 7;
        }
    }

    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut value = 0u64;
        let mut position = 0u8;

        loop {
            let current_byte = buf.read_u8()?;
            value |= (current_byte as u64 & 0x7F) << position;

            if (current_byte & CONTINUE_BIT) == 0 {
                break;
            }

            position += 7;

            if position >= 64 {
                return Err(ReadingError::Message("VarLong is too big".to_owned()));
            }
        }

        Ok(VarLong(value as i64))
    }
}

impl Serializable for u8 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_u8()?)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_u8(*self)?;
        Ok(())
    }
}

impl Lengthable for u8 {
    fn from_len(val: usize) -> Self {
        val as u8
    }
    fn into_len(self) -> usize {
        self as usize
    }
}

impl Lengthable for VarInt {
    fn from_len(val: usize) -> Self {
        VarInt(val as i32)
    }
    fn into_len(self) -> usize {
        self.0 as usize
    }
}

impl Serializable for String {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let len = VarInt::read_from(buf)?.into_len();
        if !(0..=32767).contains(&len) {
            return Err(ReadingError::Message("Invalid string size".to_owned()));
        }
        let mut bytes: Vec<u8> = Vec::new();
        buf.take(len as u64).read_to_end(&mut bytes)?;
        Ok(String::from_utf8(bytes)?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let bytes = self.as_bytes();
        let len = bytes.len();
        if len > 32767 {
            return Err(WritingError::Message(format!(
                "Invalid string size: {}",
                len
            )));
        }
        VarInt::from_len(len).write_to(buf)?;
        buf.write_all(bytes)?;
        Ok(())
    }
}

impl Serializable for u16 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_u16::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_u16::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Serializable for u64 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_u64::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_u64::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Serializable for serde_json::Value {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(serde_json::from_str(&String::read_from(buf)?)?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        self.to_string().write_to(buf)?;
        Ok(())
    }
}

pub struct LenPrefixedBytes<L: Lengthable> {
    pub data: Vec<u8>,
    _phantom_l: PhantomData<L>,
}

impl<L: Lengthable> LenPrefixedBytes<L> {
    fn new(data: Vec<u8>) -> Self {
        LenPrefixedBytes {
            data,
            _phantom_l: PhantomData,
        }
    }
}

impl<L: Lengthable> From<Vec<u8>> for LenPrefixedBytes<L> {
    fn from(data: Vec<u8>) -> Self {
        LenPrefixedBytes {
            data,
            _phantom_l: PhantomData,
        }
    }
}

impl<L: Lengthable> fmt::Debug for LenPrefixedBytes<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LenPrefixedBytes ({} bytes)", self.data.len())
    }
}

impl<L: Lengthable> Serializable for LenPrefixedBytes<L> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let len = L::read_from(buf)?.into_len();
        let mut data: Vec<u8> = Vec::with_capacity(len);
        buf.take(len as u64).read_to_end(&mut data)?;
        Ok(LenPrefixedBytes {
            data,
            _phantom_l: PhantomData,
        })
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let len = self.data.len();
        L::from_len(len).write_to(buf)?;
        buf.write_all(&self.data)?;
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct UUID(pub u128);

#[derive(Debug)]
pub struct UUIDParseError;

impl From<FromHexError> for UUIDParseError {
    fn from(value: FromHexError) -> Self {
        UUIDParseError
    }
}

impl std::str::FromStr for UUID {
    type Err = UUIDParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 36 {
            return Err(UUIDParseError);
        }
        let mut parts = hex::decode(&s[..8])?;
        parts.extend_from_slice(&hex::decode(&s[9..13])?);
        parts.extend_from_slice(&hex::decode(&s[14..18])?);
        parts.extend_from_slice(&hex::decode(&s[19..23])?);
        parts.extend_from_slice(&hex::decode(&s[24..36])?);
        let mut value = 0u128;
        for i in 0..16 {
            value |= (parts[i] as u128) << (120 - i * 8);
        }
        Ok(UUID(value))
    }
}

impl Display for UUID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut val = hex::encode(self.0.to_be_bytes());
        val.insert(9, '-');
        val.insert(14, '-');
        val.insert(19, '-');
        val.insert(24, '-');
        write!(f, "{}", val)
    }
}

#[derive(Debug)]
pub struct PrefixedArray<V: Serializable> {
    pub data: Vec<V>,
}

impl<V: Serializable> PrefixedArray<V> {
    fn new(data: Vec<V>) -> Self {
        PrefixedArray { data }
    }
}

impl<V: Serializable> Serializable for PrefixedArray<V> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let len = VarInt::read_from(buf)?.into_len();

        let mut data: Vec<V> = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(Serializable::read_from(buf)?);
        }

        Ok(PrefixedArray { data })
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let len = self.data.len();
        VarInt::from_len(len).write_to(buf)?;
        for item in &self.data {
            item.write_to(buf)?;
        }
        Ok(())
    }
}

impl<T: Serializable> Serializable for Option<T> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        if bool::read_from(buf)? {
            Ok(Some(Serializable::read_from(buf)?))
        } else {
            Ok(None)
        }
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        self.is_some().write_to(buf)?;
        if let Some(val) = self {
            val.write_to(buf)?;
        }
        Ok(())
    }
}

type Identifier = String;

impl Serializable for Vec<u8> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut bytes = Vec::new();
        buf.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_all(self)?;
        Ok(())
    }
}

// pub type JsonTextComponent = serde_json::Value;
pub type JsonTextComponent = String;
pub type TextComponent = nbt::Tag;

impl Serializable for i32 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_i32::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_i32::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Serializable for i64 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_i64::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_i64::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Serializable for i16 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_i16::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_i16::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Serializable for i8 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_i8()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_i8(*self)?;
        Ok(())
    }
}

impl Lengthable for bool {
    fn from_len(val: usize) -> Self {
        val != 0
    }
    fn into_len(self) -> usize {
        self as usize
    }
}

impl Serializable for f64 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_f64::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_f64::<BigEndian>(*self)?;
        Ok(())
    }
}

/// Use `Angle::to_radians()` to use the angle, its raw value is not accessible
#[derive(Debug, Serializable)]
pub struct Angle(i8);

impl Angle {
    fn from_radians(rad: f32) -> Self {
        let val = rad * (256. / 360.);
        Angle(val as i8)
    }
    fn to_radians(&self) -> f32 {
        self.0 as f32 * (360. / 256.)
    }
}

#[derive(Debug)]
pub struct Position {
    x: i32,
    y: i32,
    z: i32,
}

impl Serializable for Position {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let val = buf.read_u64::<BigEndian>()?;
        let x: i32 = (val >> 38) as i32;
        let y: i32 = ((val << 52) >> 52) as i32;
        let z: i32 = ((val << 26) >> 38) as i32;
        Ok(Position { x, y, z })
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        let mut val = 0u64;
        val |= (self.x as u64 & 0x3FFFFFF) << 38;
        val |= (self.z as u64 & 0x3FFFFFF) << 12;
        val |= self.y as u64 & 0xFFF;
        buf.write_u64::<BigEndian>(val)?;
        Ok(())
    }
}

impl Serializable for f32 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_f32::<BigEndian>()?)
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_f32::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Serializable for () {
    fn read_from<R: io::Read>(_: &mut R) -> Result<Self, ReadingError> {
        Ok(())
    }

    fn write_to<W: io::Write>(&self, _: &mut W) -> Result<(), WritingError> {
        Ok(())
    }
}

#[derive(Debug, Serializable)]
pub struct Vec3<T: Serializable> {
    x: T,
    y: T,
    z: T,
}

#[derive(Debug, Serializable)]
pub struct Vec4<T: Serializable> {
    x: T,
    y: T,
    z: T,
    w: T,
}

#[derive(Debug)]
pub enum IdSet {
    ByTag { tag_name: Identifier },
    IdArray(Vec<VarInt>),
}

impl Serializable for IdSet {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let ty = VarInt::read_from(buf)?.0;
        if ty == 0 {
            Ok(IdSet::ByTag {
                tag_name: Serializable::read_from(buf)?,
            })
        } else {
            let mut ids = Vec::new();
            let len = ty - 1;
            for _ in 0..len {
                ids.push(Serializable::read_from(buf)?);
            }
            Ok(IdSet::IdArray(ids))
        }
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        match self {
            IdSet::ByTag { tag_name } => {
                VarInt(0).write_to(buf)?;
                tag_name.write_to(buf)?;
            }
            IdSet::IdArray(ids) => {
                VarInt(ids.len() as i32 + 1).write_to(buf)?;
                for id in ids {
                    id.write_to(buf)?;
                }
            }
        };
        Ok(())
    }
}

#[derive(Debug)]
pub enum IdOrX<T: Serializable> {
    Id(VarInt),
    X(T),
}

impl<T: Serializable> Serializable for IdOrX<T> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let id = VarInt::read_from(buf)?;
        if id.0 == 0 {
            Ok(IdOrX::X(T::read_from(buf)?))
        } else {
            Ok(IdOrX::Id(VarInt(id.0 - 1)))
        }
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        match self {
            IdOrX::Id(id) => VarInt(id.0 + 1).write_to(buf)?,
            IdOrX::X(val) => {
                VarInt(0).write_to(buf)?;
                val.write_to(buf)?;
            }
        };
        Ok(())
    }
}

impl<T: Serializable> Serializable for Box<T> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(Box::new(Serializable::read_from(buf)?))
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        (**self).write_to(buf)?;
        Ok(())
    }
}

impl Lengthable for i8 {
    fn from_len(val: usize) -> Self {
        val as i8
    }
    fn into_len(self) -> usize {
        self as usize
    }
}

#[derive(Debug)]
pub struct StaticLenBytes<const L: usize> {
    data: Vec<u8>,
}

impl<const L: usize> Serializable for StaticLenBytes<L> {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let mut data: Vec<u8> = Vec::with_capacity(L);
        buf.take(L as u64).read_to_end(&mut data)?;
        Ok(StaticLenBytes { data })
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        if self.data.len() != L {
            return Err(WritingError::Message(format!(
                "wrong static len bytes length: {}",
                L
            )));
        }
        buf.write_all(&self.data)?;
        Ok(())
    }
}

impl Serializable for u32 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_u32::<BigEndian>()?)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_u32::<BigEndian>(*self)?;
        Ok(())
    }
}

impl<A: Serializable, B: Serializable, C: Serializable> Serializable for (A, B, C) {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok((
            Serializable::read_from(buf)?,
            Serializable::read_from(buf)?,
            Serializable::read_from(buf)?,
        ))
    }
    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        self.0.write_to(buf)?;
        self.1.write_to(buf)?;
        self.2.write_to(buf)?;
        Ok(())
    }
}

impl Serializable for u128 {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        Ok(buf.read_u128::<BigEndian>()?)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        buf.write_u128::<BigEndian>(*self)?;
        Ok(())
    }
}
