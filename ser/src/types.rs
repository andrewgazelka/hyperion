use std::{
    fmt::Debug,
    io::{Read, Write},
};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::Serialize;
use tracing::debug;
use uuid::Uuid;

use crate::{Readable, Writable, WriteExt};

/// The maximum number of bytes in a single Minecraft packet.
pub const MAX_PACKET_SIZE: i32 = 2_097_152;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PacketState {
    Handshake = 0x00,
}

impl Writable for i64 {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        writer.write_i64::<BigEndian>(*self)?;
        Ok(())
    }
}

impl<'a> Readable<'a> for &'a str {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        let len = VarInt::decode(r)?;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss
        )]
        let len = len.0 as usize;
        let s = std::str::from_utf8(&r[..len])?;
        *r = &r[len..];
        Ok(s)
    }
}

impl<'a, T: Readable<'a>> Readable<'a> for Vec<T> {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        let len = VarInt::decode(r)?;

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss
        )]
        let mut vec = Self::with_capacity(len.0 as usize);
        for _ in 0..len.0 {
            vec.push(T::decode(r)?);
        }
        Ok(vec)
    }
}

impl<T: Writable> Writable for Vec<T> {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss
        )]
        VarInt(self.len() as i32).write(writer)?;
        for item in self {
            item.write(writer)?;
        }
        Ok(())
    }
}

impl Writable for String {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;

        let str_length = self.len();

        debug!("Writing string (sync): {self} with: {length} bytes and {str_length} characters");

        writer.write_type(VarUInt(length))?.write_all(bytes)?;
        Ok(())
    }
}

impl Readable<'_> for u16 {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        Ok(r.read_u16::<BigEndian>()?)
    }
}

impl Writable for u16 {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        writer.write_u16::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Readable<'_> for i16 {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        Ok(r.read_i16::<BigEndian>()?)
    }
}

impl Readable<'_> for i64 {
    fn decode(r: &mut &'_ [u8]) -> anyhow::Result<Self> {
        Ok(r.read_i64::<BigEndian>()?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Json<T>(pub T);

impl<'a, T: serde::de::Deserialize<'a>> Readable<'a> for Json<T> {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        let s = std::str::from_utf8(r)?;
        let value = serde_json::from_str(s)?;
        *r = &r[s.len()..];
        Ok(Self(value))
    }
}

impl<T: Serialize> Writable for Json<T> {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        let string = serde_json::to_string_pretty(&self.0)?;
        debug!("Writing JSON:\n{string}");
        string.write(writer)
    }
}

// Variable-length data encoding a two's complement signed 32-bit integer; more info in their
// section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarInt(pub i32);

pub enum VarIntDecodeError {
    Incomplete,
    TooLarge,
}

impl VarInt {
    /// The maximum number of bytes a `VarInt` could occupy when read from and
    /// written to the Minecraft protocol.
    pub const MAX_SIZE: usize = 5;

    pub fn decode_partial(mut r: impl Read) -> Result<i32, VarIntDecodeError> {
        let mut val = 0;
        for i in 0..Self::MAX_SIZE {
            let byte = r.read_u8().map_err(|_| VarIntDecodeError::Incomplete)?;
            val |= (byte as i32 & 0b0111_1111) << (i * 7);
            if byte & 0b1000_0000 == 0 {
                return Ok(val);
            }
        }

        Err(VarIntDecodeError::TooLarge)
    }

    #[must_use]
    pub const fn written_size(self) -> usize {
        let mut value = self.0;
        let mut size = 0;
        loop {
            size += 1;
            value >>= 7;
            if value == 0 {
                break;
            }
        }
        size
    }
}

impl From<i32> for VarInt {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl Readable<'_> for bool {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        Ok(r.read_u8()? != 0)
    }
}

impl Writable for bool {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        writer.write_all(&[*self as u8])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarUInt(pub u32);

impl Writable for VarUInt {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
        let value = VarInt(self.0 as i32);
        value.write(writer)
    }
}

impl Readable<'_> for Uuid {
    fn decode(r: &mut &'_ [u8]) -> anyhow::Result<Self> {
        let x = r.read_u128::<BigEndian>()?;
        Ok(Self::from_u128(x))
    }
}

impl Writable for Uuid {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        writer.write_u128::<BigEndian>(self.as_u128())?;
        Ok(())
    }
}

impl Readable<'_> for VarInt {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        let mut result = 0;
        let mut shift = 0;
        loop {
            let byte = r.read_u8()?;
            result |= ((byte & 0x7F) as i32) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(Self(result))
    }
}

const SEGMENT_BITS: u8 = 0x7F;
const CONTINUE_BIT: u8 = 0x80;

impl Writable for VarInt {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        // todO:
        // should this take by value?
        let mut value = self.0;
        loop {
            if (value & !SEGMENT_BITS as i32) == 0 {
                writer.write_all(&[value as u8])?;
                return Ok(());
            }
            writer.write_all(&[(value as u8 & SEGMENT_BITS) | CONTINUE_BIT])?;
            // Note: Rust does not have a logical right shift operator (>>>), but since we're
            // working with a signed int, converting to u32 for the shift operation
            // achieves the same effect of not preserving the sign bit.
            value = ((value as u32) >> 7) as i32;
        }
    }
}

pub struct VarLong(pub i64);

impl Readable<'_> for VarLong {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        let mut result = 0;
        let mut shift = 0;
        loop {
            let byte = r.read_u8()?;
            result |= ((byte & 0x7F) as i64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(Self(result))
    }
}

impl Writable for &str {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;
        writer.write_type(VarUInt(length))?.write_all(bytes)?;
        Ok(())
    }
}

impl Writable for VarLong {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
        let mut value = self.0;
        loop {
            #[allow(clippy::cast_sign_loss)]
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            writer.write_all(&[byte])?;
            if value == 0 {
                break;
            }
        }
        Ok(())
    }
}
