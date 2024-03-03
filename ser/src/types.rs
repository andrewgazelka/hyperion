use std::{
    fmt::{Debug, Display},
    io::Read,
};

use anyhow::{bail, ensure};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::Serialize;
use tracing::debug;
use uuid::Uuid;

use crate::{Readable, Writable};

/// The maximum number of bytes in a single Minecraft packet.
pub const MAX_PACKET_SIZE: i32 = 2_097_152;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PacketState {
    Handshake = 0x00,
}

impl Writable for i8 {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_i8(*self)?;
        Ok(())
    }
}

impl Writable for u8 {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_u8(*self)?;
        Ok(())
    }
}

impl Readable<'_> for u8 {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        Ok(r.read_u8()?)
    }
}

pub struct UntilEnd<'a>(&'a [u8]);

impl Debug for UntilEnd<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = bytes::Bytes::copy_from_slice(self.0);
        write!(f, "{:?}", bytes)
    }
}

impl<'a> Readable<'a> for UntilEnd<'a> {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        // take the entire slice
        let (data, rest) = r.split_at(r.len());
        *r = rest;
        Ok(Self(data))
    }
}

impl Writable for i64 {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_i64::<BigEndian>(*self)?;
        Ok(())
    }
}

impl Writable for i32 {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_i32::<BigEndian>(*self)?;
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
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
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
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;

        let str_length = self.len();

        debug!("Writing string (sync): {self} with: {length} bytes and {str_length} characters");

        VarUInt(length).write(writer)?;
        writer.write_all(bytes)?;
        Ok(())
    }
}

impl Readable<'_> for u16 {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        Ok(r.read_u16::<BigEndian>()?)
    }
}

impl Writable for u16 {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
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
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        let string = serde_json::to_string_pretty(&self.0)?;
        debug!("Writing JSON:\n{string}");
        string.write(writer)
    }
}

// Variable-length data encoding a two's complement signed 32-bit integer; more info in their
// section
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarInt(pub i32);

impl Debug for VarInt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self.0))
    }
}

impl Display for VarInt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))
    }
}

/// Identifiers are a namespaced location, in the form of minecraft:thing. If the namespace is not
/// provided, it defaults to minecraft (i.e. thing is minecraft:thing). Custom content should always
/// be in its own namespace, not the default one. Both the namespace and value can use all lowercase
/// alphanumeric characters (a-z and 0-9), dot (.), dash (-), and underscore (_). In addition,
/// values can use slash (/). The naming convention is `lower_case_with_underscores`. More
/// information. For ease of determining whether a namespace or value is valid, here are regular
/// expressions for each:
///
/// Namespace: [a-z0-9.-_]
/// Value: [a-z0-9.-_/]
#[derive(Copy, Clone)]
pub struct Identifier<'a>(pub &'a str);

pub struct BitSet(pub Vec<u64>);

impl Default for BitSet {
    fn default() -> Self {
        Self(vec![0])
    }
}

impl BitSet {
    #[must_use]
    pub fn get(&self, index: usize) -> bool {
        let word = index / 64;
        let bit = index % 64;
        (self.0[word] & (1 << bit)) != 0
    }
}

impl Readable<'_> for BitSet {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        let len = VarInt::decode(r)?;

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss
        )]
        let mut vec = Vec::with_capacity(len.0 as usize);
        for _ in 0..len.0 {
            vec.push(r.read_u64::<BigEndian>()?);
        }
        Ok(Self(vec))
    }
}

impl Writable for BitSet {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss
        )]
        VarInt(self.0.len() as i32).write(writer)?;

        writer.write_all(bytemuck::cast_slice(&self.0))?;
        Ok(())
    }
}

#[derive(Default)]
pub struct Nbt(pub valence_nbt::Compound);

impl From<valence_nbt::Compound> for Nbt {
    fn from(value: valence_nbt::Compound) -> Self {
        Self(value)
    }
}

impl Writable for Nbt {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        valence_nbt::to_binary(&self.0, writer, "")?;
        Ok(())
    }
}

impl<'a> Debug for Identifier<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl<'a> TryFrom<&'a str> for Identifier<'a> {
    type Error = anyhow::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        // namespace:value
        static REGEX: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
            regex::Regex::new(r"^[a-z0-9.-_]+:[a-z0-9.-_/]+$").unwrap()
        });

        ensure!(REGEX.is_match(value), "invalid identifier: {}", value);

        Ok(Self(value))
    }
}

impl<'a, R: Readable<'a>> Readable<'a> for Option<R> {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        let present = bool::decode(r)?;
        if present {
            Ok(Some(R::decode(r)?))
        } else {
            Ok(None)
        }
    }
}

impl<T: Writable> Writable for Option<T> {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        self.is_some().write(writer)?;
        if let Some(value) = self {
            value.write(writer)?;
        }
        Ok(())
    }
}

impl<'a> Readable<'a> for Identifier<'a> {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        let s = <&str>::decode(r)?;
        Self::try_from(s)
    }
}

impl<'a> Writable for Identifier<'a> {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        self.0.write(writer)
    }
}

#[derive(Copy, Clone)]
pub struct Position(pub i64);

impl Readable<'_> for Position {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        Ok(Self(r.read_i64::<BigEndian>()?))
    }
}

impl Writable for Position {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_i64::<BigEndian>(self.0)?;
        Ok(())
    }
}

impl Position {
    #[must_use]
    pub const fn x(self) -> i32 {
        (self.0 >> 38) as i32
    }

    #[must_use]
    pub const fn y(self) -> i32 {
        (self.0 << 26 >> 52) as i32
    }

    #[must_use]
    pub const fn z(self) -> i32 {
        (self.0 << 38 >> 38) as i32
    }

    #[must_use]
    pub const fn from(x: i32, y: i32, z: i32) -> Self {
        Self((x as i64) << 38 | (y as i64) << 26 | z as i64)
    }
}

impl Debug for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Position")
            .field("x", &self.x())
            .field("y", &self.y())
            .field("z", &self.z())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VarIntDecodeError {
    Incomplete,
    TooLarge,
}

impl VarInt {
    /// The maximum number of bytes a `VarInt` could occupy when read from and
    /// written to the Minecraft protocol.
    pub const MAX_BYTES: usize = 5;

    pub fn decode_partial(mut r: impl Read) -> Result<i32, VarIntDecodeError> {
        let mut val = 0;
        for i in 0..Self::MAX_BYTES {
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
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_u8(*self as u8)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarUInt(pub u32);

impl Writable for VarUInt {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
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
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        writer.write_u128::<BigEndian>(self.as_u128())?;
        Ok(())
    }
}

impl Readable<'_> for VarInt {
    fn decode(r: &mut &[u8]) -> anyhow::Result<Self> {
        const SEGMENT_BITS: u8 = 0x7F;
        const CONTINUE_BIT: u8 = 0x80;

        let mut value = 0i32;
        let mut position = 0;

        loop {
            let current_byte = r.read_u8()?;
            value |= ((current_byte & SEGMENT_BITS) as i32) << position;

            if (current_byte & CONTINUE_BIT) == 0 {
                return Ok(Self(value));
            }

            position += 7;
            if position >= 32 {
                // Instead of throwing a RuntimeException, we return an error.
                bail!("VarInt is too big");
            }
        }
    }
}

impl Writable for VarInt {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        const SEGMENT_BITS: u32 = 0x7F;
        const CONTINUE_BIT: u32 = 0x80;

        let mut value = self.0 as u32;

        loop {
            if (value & !SEGMENT_BITS) == 0 {
                writer.write_u8(value as u8)?;
                return Ok(());
            }

            writer.write_u8(((value & SEGMENT_BITS) | CONTINUE_BIT) as u8)?;

            // Rust does not have an equivalent to Java's >>> operator, but since we're dealing with
            // u32, a regular right shift (>>) acts as a logical shift for unsigned
            // types.
            value >>= 7;
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
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;

        VarUInt(length).write(writer)?;
        writer.write_all(bytes)?;
        Ok(())
    }
}

// todo: fix
impl Writable for VarLong {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        let mut value = self.0;
        loop {
            #[allow(clippy::cast_sign_loss)]
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            writer.write_u8(byte)?;
            if value == 0 {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::VarInt;
    use crate::{Readable, Writable};

    #[test]
    fn var_int_round_circle() {
        // 233861
        let value = 233_861;

        let mut buffer = Vec::new();
        VarInt(value).write(&mut buffer).unwrap();

        let decoded = VarInt::decode(&mut &buffer[..]).unwrap();
        assert_eq!(decoded.0, value);
    }
}
