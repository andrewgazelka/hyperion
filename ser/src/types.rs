use std::{
    fmt::Debug,
    io,
    io::{BufRead, Write},
};

use byteorder::{ReadBytesExt, WriteBytesExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncBufRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::debug;
use uuid::Uuid;

use crate::{Readable, Writable, WriteExt, WriteExtAsync};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PacketState {
    Handshake = 0x00,
}

impl Writable for i64 {
    fn write(self, writer: &mut impl Write) -> io::Result<()> {
        writer.write_i64::<byteorder::BigEndian>(self)
    }

    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        writer.write_i64(self).await
    }
}

impl Readable for String {
    fn read(reader: &mut impl BufRead) -> io::Result<Self>
    where
        Self: Sized,
    {
        let length = VarUInt::read(reader)?.0 as usize;
        let mut buffer = vec![0; length];
        reader.read_exact(&mut buffer)?;
        Ok(Self::from_utf8(buffer).unwrap())
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Self>
    where
        Self: Sized,
    {
        let length = VarUInt::read_async(reader).await?.0 as usize;
        let mut buffer = vec![0; length];
        reader.read_exact(&mut buffer).await?;
        Ok(Self::from_utf8(buffer).unwrap())
    }
}

impl Writable for String {
    fn write(self, writer: &mut impl Write) -> io::Result<()> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;

        debug!("Writing string (sync): {self} with: {length} bytes");

        writer.write_type(VarUInt(length))?.write_all(bytes)
    }

    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        let bytes = self.as_bytes();
        let length = bytes.len() as u32;

        debug!("Writing string: {self} with: {length} bytes");

        writer
            .write_type(VarUInt(length))
            .await?
            .write_all(bytes)
            .await
    }
}

impl Readable for u16 {
    fn read(reader: &mut impl BufRead) -> io::Result<Self>
    where
        Self: Sized,
    {
        reader.read_u16::<byteorder::BigEndian>()
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Self>
    where
        Self: Sized,
    {
        let mut buffer = [0; 2];
        reader.read_exact(&mut buffer).await?;
        Ok(Self::from_be_bytes(buffer))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Json<T>(pub T);

impl<T: DeserializeOwned> Readable for Json<T> {
    fn read(reader: &mut impl BufRead) -> io::Result<Self>
    where
        Self: Sized,
    {
        let string = String::read(reader)?;
        let Ok(value) = serde_json::from_str(&string) else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid JSON"));
        };

        Ok(Self(value))
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Self>
    where
        Self: Sized,
    {
        let string = String::read_async(reader).await?;
        let Ok(value) = serde_json::from_str(&string) else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid JSON"));
        };

        Ok(Self(value))
    }
}

impl<T: Serialize> Writable for Json<T> {
    fn write(self, writer: &mut impl Write) -> io::Result<()> {
        let string = serde_json::to_string_pretty(&self.0)?;
        debug!("Writing JSON:\n{string}");
        string.write(writer)
    }

    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        let string = serde_json::to_string_pretty(&self.0)?;
        debug!("Writing JSON:\n{string}");
        string.write_async(writer).await
    }
}

// Variable-length data encoding a two's complement signed 32-bit integer; more info in their
// section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarInt(pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarUInt(pub u32);

impl Writable for VarUInt {
    fn write(self, writer: &mut impl Write) -> io::Result<()> {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
        let value = VarInt(self.0 as i32);
        value.write(writer)
    }

    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
        let value = VarInt(self.0 as i32);
        value.write_async(writer).await
    }
}

impl Readable for VarUInt {
    fn read(reader: &mut impl BufRead) -> io::Result<Self>
    where
        Self: Sized,
    {
        #[allow(clippy::cast_sign_loss)]
        Ok(Self(VarInt::read(reader)?.0 as u32))
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Self>
    where
        Self: Sized,
    {
        #[allow(clippy::cast_sign_loss)]
        Ok(Self(VarInt::read_async(reader).await?.0 as u32))
    }
}

impl Readable for Uuid {
    fn read(reader: &mut impl BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        // Encoded as an unsigned 128-bit integer (or two unsigned 64-bit integers: the most
        // significant 64 bits and then the least significant 64 bits)
        let value = reader.read_u128::<byteorder::BigEndian>()?;
        debug!("Read UUID: {}", value);
        Ok(Self::from_u128(value))
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let value = reader.read_u128().await?;
        debug!("Read UUID: {}", value);
        Ok(Self::from_u128(value))
    }
}

const SEGMENT_BITS: u8 = 0x7F;
const CONTINUE_BIT: u8 = 0x80;

impl Readable for VarInt {
    fn read(reader: &mut impl BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let mut value = 0i32;
        let mut position = 0;

        loop {
            let mut buffer = [0u8; 1];
            reader.read_exact(&mut buffer)?;
            let current_byte = buffer[0];

            let segment_value = (current_byte & SEGMENT_BITS) as i32;
            // Ensure we're not shifting bits into oblivion, which can happen with a malformed
            // VarInt.
            if position > 32 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "VarInt is too big",
                ));
            }
            // SAFETY: `position` is guaranteed to be at most 28 here, ensuring the shift is safe.
            value |= segment_value << position;

            if current_byte & CONTINUE_BIT == 0 {
                break;
            }

            position += 7;
        }
        Ok(Self(value))
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let mut value = 0i32;
        let mut position = 0;

        loop {
            let mut buffer = [0u8; 1];
            reader.read_exact(&mut buffer).await?;
            let current_byte = buffer[0];

            let segment_value = (current_byte & SEGMENT_BITS) as i32;
            // Ensure we're not shifting bits into oblivion, which can happen with a malformed
            // VarInt.
            if position > 32 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "VarInt is too big",
                ));
            }
            // SAFETY: `position` is guaranteed to be at most 28 here, ensuring the shift is safe.
            value |= segment_value << position;

            if current_byte & CONTINUE_BIT == 0 {
                break;
            }

            position += 7;
        }
        Ok(Self(value))
    }
}

impl Writable for VarInt {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    fn write(self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
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

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> std::io::Result<()> {
        let mut value = self.0;
        loop {
            if (value & !SEGMENT_BITS as i32) == 0 {
                writer.write_all(&[value as u8]).await?;
                return Ok(());
            }
            writer.write_all(&[(value as u8 & SEGMENT_BITS) | CONTINUE_BIT]).await?;
            // Note: Rust does not have a logical right shift operator (>>>), but since we're
            // working with a signed int, converting to u32 for the shift operation
            // achieves the same effect of not preserving the sign bit.
            value = ((value as u32) >> 7) as i32;
        }
    }
}

pub struct VarLong(pub i64);

impl Readable for VarLong {
    fn read(reader: &mut impl BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let mut result = 0;
        let mut shift = 0;
        loop {
            let byte = reader.read_u8()?;
            result |= ((byte & 0x7F) as i64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(Self(result))
    }

    async fn read_async(reader: &mut (impl AsyncBufRead + Unpin)) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let mut result = 0;
        let mut shift = 0;
        loop {
            let byte = reader.read_u8().await?;
            result |= ((byte & 0x7F) as i64) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(Self(result))
    }
}

impl Writable for VarLong {
    fn write(self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
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

    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> std::io::Result<()> {
        let mut value = self.0;
        loop {
            #[allow(clippy::cast_sign_loss)]
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            writer.write_all(&[byte]).await?;
            if value == 0 {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{types::VarInt, Readable, Writable};

    fn round_trip(num: i32) {
        let original = VarInt(num);
        let mut buffer = Vec::new();
        original.write(&mut buffer).unwrap();

        let mut cursor = std::io::Cursor::new(buffer);
        let round_tripped = VarInt::read(&mut cursor).unwrap();
        assert_eq!(original.0, round_tripped.0);
    }

    #[test]
    fn test_round_trip_varint() {
        round_trip(0x1234_5678);
        round_trip(32);
    }
}
