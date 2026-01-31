use std::io::Read;

use super::Serializable;

use crate::{Error, PrefixedArray, ReadingError, WritingError};

#[derive(Debug, Serializable)]
pub struct BitSet {
    data: PrefixedArray<i64>,
}

impl BitSet {
    pub fn get(&self, i: u64) -> bool {
        (self.data.data[i as usize / 64] & (1 << (i % 64))) != 0
    }

    pub fn set(&mut self, i: u64) {
        self.data.data[i as usize / 64] |= 1 << (i % 64)
    }

    pub fn new(size: usize) -> Self {
        Self {
            data: PrefixedArray {
                data: vec![0; size],
            },
        }
    }
}

#[derive(Debug)]
pub struct FixedBitSet<const L: usize> {
    data: Vec<u8>,
}

impl<const L: usize> Serializable for FixedBitSet<L> {
    fn read_from<R: std::io::Read>(buf: &mut R) -> Result<Self, ReadingError> {
        let size = (L + 7) / 8; // integer division rounding up
        let mut data = Vec::with_capacity(size);
        buf.take(size as u64).read_to_end(&mut data)?;
        Ok(FixedBitSet { data })
    }
    fn write_to<W: std::io::Write>(&self, buf: &mut W) -> Result<(), crate::WritingError> {
        let byte_size = (L + 7) / 8;
        if self.data.len() != byte_size {
            return Err(WritingError::Message(format!(
                "wrong fixed bitset<{}> data length: {} bytes, should be {}",
                L,
                self.data.len(),
                byte_size,
            )));
        }
        buf.write_all(&self.data)?;
        Ok(())
    }
}

impl<const L: usize> FixedBitSet<L> {
    pub fn get(&self, i: u64) -> bool {
        (self.data[i as usize / 8] & (1 << (i % 8))) != 0
    }

    pub fn set(&mut self, i: u64) {
        self.data[i as usize / 8] |= 1 << (i % 8)
    }

    pub fn new() -> Self {
        Self { data: vec![0; L] }
    }
}
