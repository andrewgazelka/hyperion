use std::{fmt::Debug, io, io::BufRead};

use byteorder::ReadBytesExt;

use crate::{Readable, Writable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PacketState {
    Handshake = 0x00,
}

impl Readable for String {
    fn read(reader: &mut impl BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let length = VarUInt::read(reader)?.0 as usize;
        let mut buffer = vec![0; length];
        reader.read_exact(&mut buffer)?;
        Ok(Self::from_utf8(buffer).unwrap())
    }
}

impl Readable for u16 {
    fn read(reader: &mut impl BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        reader.read_u16::<byteorder::BigEndian>()
    }
}

// Variable-length data encoding a two's complement signed 32-bit integer; more info in their
// section
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarInt(pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarUInt(pub u32);

const SEGMENT_BITS: u8 = 0x7F;
const CONTINUE_BIT: u8 = 0x80;

impl Readable for VarUInt {
    fn read(reader: &mut impl BufRead) -> io::Result<Self>
    where
        Self: Sized,
    {
        #[allow(clippy::cast_sign_loss)]
        Ok(Self(VarInt::read(reader)?.0 as u32))
    }
}

impl Readable for VarInt {
    fn read(reader: &mut impl BufRead) -> io::Result<Self>
    where
        Self: Sized,
    {
        let mut value = 0i32;
        let mut position = 0;

        loop {
            let mut current_byte = [0u8; 1];
            reader.read_exact(&mut current_byte)?;
            let current_byte = current_byte[0];

            value |= ((current_byte & SEGMENT_BITS) as i32) << position;

            if (current_byte & CONTINUE_BIT) == 0 {
                break;
            }

            position += 7;

            if position >= 32 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "VarInt is too big",
                ));
            }
        }

        Ok(Self(value))
    }
}

impl Writable for VarInt {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
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
}

impl Writable for VarLong {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
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

#[cfg(test)]
mod tests {
    use crate::{
        types::{VarInt, VarLong},
        Readable, Writable,
    };

    #[test]
    fn test_round_trip_varint() {
        let original = VarInt(0x1234_5678);
        let mut buffer = Vec::new();
        original.write(&mut buffer).unwrap();

        let mut cursor = std::io::Cursor::new(buffer);
        let round_tripped = VarInt::read(&mut cursor).unwrap();
        assert_eq!(original.0, round_tripped.0);
    }

    #[test]
    fn test_round_trip_varlong() {
        let original = VarLong(0x1234_5678_9ABC_DEF0);
        let mut buffer = Vec::new();
        original.write(&mut buffer).unwrap();

        let mut cursor = std::io::Cursor::new(buffer);
        let round_tripped = VarLong::read(&mut cursor).unwrap();
        assert_eq!(original.0, round_tripped.0);
    }
}
