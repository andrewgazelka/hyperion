use std::{fmt::Debug, future::Future, io::Cursor};

// re-export the `ser-macro` crate
#[cfg(feature = "ser-macro")]
pub use ser_macro::*;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::debug;

use crate::types::{VarInt, VarUInt};

pub mod types;

pub trait Writable {
    fn write(self, writer: &mut impl std::io::Write) -> std::io::Result<()>;
    fn write_async(
        self,
        writer: &mut (impl AsyncWrite + Unpin),
    ) -> impl Future<Output = std::io::Result<()>>
    where
        Self: Sized;
}

pub trait Readable {
    fn read(reader: &mut impl std::io::BufRead) -> std::io::Result<Self>
    where
        Self: Sized;

    fn read_async(
        reader: &mut (impl tokio::io::AsyncBufRead + Unpin),
    ) -> impl Future<Output = std::io::Result<Self>>
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

pub trait ReadExtAsync: tokio::io::AsyncBufRead + Unpin {
    fn read_type<T: Readable>(&mut self) -> impl Future<Output = std::io::Result<T>>
    where
        Self: Sized,
    {
        T::read_async(self)
    }
}

impl<T: std::io::BufRead> ReadExt for T {}
impl<T: tokio::io::AsyncBufRead + Unpin> ReadExtAsync for T {}

pub trait WriteExt: std::io::Write {
    fn write_type<T: Writable>(&mut self, data: T) -> std::io::Result<&mut Self>
    where
        Self: Sized,
    {
        data.write(self)?;
        Ok(self)
    }
}

pub trait WriteExtAsync: AsyncWrite + Unpin {
    fn write_type<T: Writable>(
        &mut self,
        data: T,
    ) -> impl Future<Output = std::io::Result<&mut Self>>
    where
        Self: Sized,
    {
        async move {
            data.write_async(self).await?;
            Ok(self)
        }
    }
}

impl<T: std::io::Write> WriteExt for T {}
impl<T: AsyncWrite + Unpin> WriteExtAsync for T {}

pub trait Packet {
    const ID: i32;
    const STATE: types::PacketState;
}

#[derive(Debug)]
pub struct ExactPacket<T>(pub T);

pub struct GenericPacket {
    pub id: i32,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct WritePacket<T> {
    pub id: VarInt,
    pub data: T,
}

impl<T: Packet> WritePacket<T> {
    pub const fn new(data: T) -> Self {
        Self {
            id: VarInt(T::ID),
            data,
        }
    }
}

impl<T: Writable + Debug> Writable for WritePacket<T> {
    fn write(self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        let mut data = Vec::new();
        self.id.write(&mut data)?;
        self.data.write(&mut data)?;

        // todo: unnecessary allocation
        VarUInt(data.len() as u32).write(writer)?;
        writer.write_all(&data)?;

        debug!("wrote packet ID: {:#x} length: {}", self.id.0, data.len());

        Ok(())
    }

    // #[tracing::instrument(skip(writer))]
    async fn write_async(self, writer: &mut (impl AsyncWrite + Unpin)) -> std::io::Result<()> {
        let mut data = Vec::new();
        self.id.write(&mut data)?;
        self.data.write(&mut data)?;

        // todo: unnecessary allocation
        VarUInt(data.len() as u32).write_async(writer).await?;
        writer.write_all(&data).await?;

        debug!("wrote packet ID: {:#x} length: {}", self.id.0, data.len());

        // format hex debug! raw data 0x00
        // debug!("{:#x?}", data);

        Ok(())
    }
}

impl<T: Readable + Packet + Debug> Readable for ExactPacket<T> {
    fn read(reader: &mut impl std::io::BufRead) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        debug!("reading packet");
        let length = VarUInt::read(reader)?;
        let mut data = vec![0; length.0 as usize];

        reader.read_exact(&mut data)?;

        let mut cursor = Cursor::new(data);

        let _id = VarInt::read(&mut cursor)?;

        let result = T::read(&mut cursor)?;

        debug!("read packet {:?}", result);

        Ok(Self(result))
    }

    async fn read_async(
        reader: &mut (impl tokio::io::AsyncBufRead + Unpin),
    ) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        debug!("reading packet");
        let VarUInt(length) = reader.read_type().await?;
        let mut data = vec![0; length as usize];

        reader.read_exact(&mut data).await?;

        let mut cursor = Cursor::new(data);

        let id = VarInt::read(&mut cursor)?;

        if id.0 != T::ID {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("expected packet ID: {:#x} got: {:#x}", T::ID, id.0),
            ));
        }

        let result = T::read(&mut cursor)?;
        debug!("read packet {:?}", result);

        Ok(Self(result))
    }
}
