use std::{
    collections::HashMap,
    io::{self, Read},
};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::{ReadingError, Serializable, WritingError};

#[derive(Debug, Clone)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>), // prefix i32
    String(String),     // prefix u16
    List(Vec<Tag>),     // prefixed by type id (i8) and length i32 if empty list, type id can be END
    Compound(HashMap<String, Tag>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn new_compound() -> Tag {
        Tag::Compound(HashMap::new())
    }

    pub fn new_list() -> Tag {
        Tag::List(Vec::new())
    }

    pub fn get(&self, name: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(val) => val.get(name),
            _ => None,
        }
    }

    /// WILL panic if this is not a compound
    pub fn put(&mut self, name: &str, tag: Tag) {
        match *self {
            Tag::Compound(ref mut val) => val.insert(name.to_owned(), tag),
            _ => panic!("not a compound tag"),
        };
    }

    pub fn is_compound(&self) -> bool {
        matches!(*self, Tag::Compound(_))
    }

    pub fn as_byte(&self) -> Option<i8> {
        match *self {
            Tag::Byte(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_short(&self) -> Option<i16> {
        match *self {
            Tag::Short(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match *self {
            Tag::Int(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_long(&self) -> Option<i64> {
        match *self {
            Tag::Long(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match *self {
            Tag::Float(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_double(&self) -> Option<f64> {
        match *self {
            Tag::Double(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_byte_array(&self) -> Option<&[u8]> {
        match self {
            Tag::ByteArray(val) => Some(val),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match *self {
            Tag::String(ref val) => Some(&val[..]),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Tag]> {
        match *self {
            Tag::List(ref val) => Some(&val[..]),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&HashMap<String, Tag>> {
        match *self {
            Tag::Compound(ref val) => Some(val),
            _ => None,
        }
    }

    pub fn as_int_array(&self) -> Option<&[i32]> {
        match *self {
            Tag::IntArray(ref val) => Some(&val[..]),
            _ => None,
        }
    }

    pub fn as_long_array(&self) -> Option<&[i64]> {
        match *self {
            Tag::LongArray(ref val) => Some(&val[..]),
            _ => None,
        }
    }

    fn internal_id(&self) -> i8 {
        match self {
            Tag::End => 0,
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }

    fn read_type<R: io::Read>(id: u8, buf: &mut R) -> Result<Tag, ReadingError> {
        // println!("readtype {}", id);
        match id {
            0 => Ok(Tag::End),
            1 => Ok(Tag::Byte(buf.read_i8()?)),
            2 => Ok(Tag::Short(buf.read_i16::<BigEndian>()?)),
            3 => Ok(Tag::Int(buf.read_i32::<BigEndian>()?)),
            4 => Ok(Tag::Long(buf.read_i64::<BigEndian>()?)),
            5 => Ok(Tag::Float(buf.read_f32::<BigEndian>()?)),
            6 => Ok(Tag::Double(buf.read_f64::<BigEndian>()?)),
            7 => Ok(Tag::ByteArray({
                let len: i32 = Serializable::read_from(buf)?;
                let mut data = Vec::with_capacity(len as usize);
                buf.take(len as u64).read_to_end(&mut data)?;
                data
            })),
            8 => Ok(Tag::String(read_string(buf)?)),
            9 => {
                let mut l = Vec::new();
                let ty = buf.read_u8()?;
                let len: i32 = Serializable::read_from(buf)?;
                for _ in 0..len {
                    l.push(Tag::read_type(ty, buf)?);
                }
                Ok(Tag::List(l))
            }
            10 => {
                let mut c = Tag::new_compound();
                loop {
                    let ty = buf.read_u8()?;
                    if ty == 0 {
                        break;
                    }
                    let name: String = read_string(buf)?;
                    c.put(&name[..], Tag::read_type(ty, buf)?);
                }
                Ok(c)
            }
            11 => Ok(Tag::IntArray({
                let len: i32 = Serializable::read_from(buf)?;
                let mut data = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    data.push(buf.read_i32::<BigEndian>()?);
                }
                data
            })),
            12 => Ok(Tag::LongArray({
                let len: i32 = Serializable::read_from(buf)?;
                let mut data = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    data.push(buf.read_i64::<BigEndian>()?);
                }
                data
            })),
            _ => Err(ReadingError::Message(format!("invalid tag"))),
        }
    }
}

impl Serializable for Tag {
    fn read_from<R: io::Read>(buf: &mut R) -> Result<Tag, ReadingError> {
        let ty = buf.read_u8()?;
        Tag::read_type(ty, buf)
    }

    fn write_to<W: io::Write>(&self, buf: &mut W) -> Result<(), WritingError> {
        match *self {
            Tag::End => {}
            Tag::Byte(val) => buf.write_i8(val)?,
            Tag::Short(val) => buf.write_i16::<BigEndian>(val)?,
            Tag::Int(val) => buf.write_i32::<BigEndian>(val)?,
            Tag::Long(val) => buf.write_i64::<BigEndian>(val)?,
            Tag::Float(val) => buf.write_f32::<BigEndian>(val)?,
            Tag::Double(val) => buf.write_f64::<BigEndian>(val)?,
            Tag::ByteArray(ref val) => {
                (val.len() as i32).write_to(buf)?;
                buf.write_all(val)?;
            }
            Tag::String(ref val) => write_string(buf, val)?,
            Tag::List(ref val) => {
                if val.is_empty() {
                    buf.write_i8(0)?;
                    buf.write_i32::<BigEndian>(0)?;
                } else {
                    buf.write_i8(val[0].internal_id())?;
                    buf.write_i32::<BigEndian>(val.len() as i32)?;
                    for e in val {
                        e.write_to(buf)?;
                    }
                }
            }
            Tag::Compound(ref val) => {
                for (k, v) in val {
                    v.internal_id().write_to(buf)?;
                    write_string(buf, k)?;
                    v.write_to(buf)?;
                }
                buf.write_u8(0)?;
            }
            Tag::IntArray(ref val) => {
                (val.len() as i32).write_to(buf)?;
                for v in val {
                    v.write_to(buf)?;
                }
            }
            Tag::LongArray(ref val) => {
                (val.len() as i32).write_to(buf)?;
                for v in val {
                    v.write_to(buf)?;
                }
            }
        }
        Result::Ok(())
    }
}

pub fn write_string<W: io::Write>(buf: &mut W, s: &str) -> Result<(), WritingError> {
    let data = s.as_bytes();
    (data.len() as i16).write_to(buf)?;
    buf.write_all(data).map_err(|v| v.into())
}

pub fn read_string<R: io::Read>(buf: &mut R) -> Result<String, ReadingError> {
    let len: i16 = buf.read_i16::<BigEndian>()?;
    let mut bytes = Vec::<u8>::new();
    buf.take(len as u64).read_to_end(&mut bytes)?;
    let ret = String::from_utf8(bytes).unwrap();
    Result::Ok(ret)
}
