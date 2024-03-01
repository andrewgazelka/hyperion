use std::io::Cursor;

// re-export the `ser-macro` crate
#[cfg(feature = "ser-macro")]
pub use ser_macro::*;

use crate::types::{VarInt, VarUInt};

pub mod types;

pub trait Writable {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()>;
}

pub trait Readable {
    fn read(reader: &mut impl std::io::BufRead) -> std::io::Result<Self>
    where
        Self: Sized;
}

// ext trait on std::io::BufRead
pub trait ReadExt: std::io::BufRead {
    fn read_type<T: Readable>(&mut self) -> std::io::Result<T>
    where
        Self: Sized,
    {
        T::read(self)
    }
}

impl<T: std::io::BufRead> ReadExt for T {}

pub trait Packet {
    const ID: u32;
    const STATE: types::PacketState;
}

#[derive(Debug)]
pub struct PacketData<T> {
    pub length: VarUInt,
    pub id: VarInt,
    pub data: T,
}

impl<T: Readable> Readable for PacketData<T> {
    fn read(reader: &mut impl std::io::BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let VarUInt(length) = reader.read_type()?;
        let VarInt(id) = reader.read_type()?;
        let mut data = vec![0; length as usize];

        reader.read_exact(&mut data)?;

        let mut cursor = Cursor::new(data);

        Ok(Self {
            length: VarUInt(length),
            id: VarInt(id),
            data: T::read(&mut cursor)?,
        })
    }
}
