use std::{fmt::Debug, io::Write};

// re-export the `ser-macro` crate
#[cfg(feature = "ser-macro")]
pub use ser_macro::*;
use tracing::debug;

use crate::types::{VarInt, VarUInt};

pub mod types;

pub trait Writable {
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()>;
    // fn write_async(
    //     &self,
    //     writer: &mut (impl AsyncWrite + Unpin),
    // ) -> impl Future<Output = anyhow::Result<()>>
    // where
    //     Self: Sized;
}

pub trait Readable<'a>: Sized {
    /// Reads this object from the provided byte slice.
    ///
    /// Implementations of `Readable` are expected to shrink the slice from the
    /// front as bytes are read.
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self>;
}

// // ext trait on std::io::BufRead
// pub trait ReadExt: std::io::BufRead {
//     fn read_type<T: Readable>(&mut self) -> std::anyhow::Result<T>
//     where
//         Self: Sized,
//     {
//         T::read(self)
//     }
// }
//
// pub trait ReadExtAsync: tokio::io::AsyncBufRead + Unpin {
//     fn read_type<T: Readable>(&mut self) -> impl Future<Output = std::anyhow::Result<T>>
//     where
//         Self: Sized,
//     {
//         T::read_async(self)
//     }
// }
//
// impl<T: std::io::BufRead> ReadExt for T {}
// impl<T: tokio::io::AsyncBufRead + Unpin> ReadExtAsync for T {}

pub trait WriteExt: Write {
    fn write_type<T: Writable>(&mut self, data: T) -> anyhow::Result<&mut Self>
    where
        Self: Sized,
    {
        data.write(self)?;
        Ok(self)
    }
}

// pub trait WriteExtAsync: AsyncWrite + Unpin {
//     fn write_type<T: Writable>(
//         &mut self,
//         data: T,
//     ) -> impl Future<Output = anyhow::Result<&mut Self>>
//     where
//         Self: Sized,
//     {
//         async move {
//             data.write_async(self).await?;
//             Ok(self)
//         }
//     }
// }

impl<T: Write> WriteExt for T {}
// impl<T: AsyncWrite + Unpin> WriteExtAsync for T {}

pub trait Packet {
    const ID: i32;
    const STATE: types::PacketState;
    const NAME: &'static str;
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
    fn write(&self, writer: &mut impl Write) -> anyhow::Result<()> {
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
    // async fn write_async(&self, writer: &mut (impl AsyncWrite + Unpin)) -> anyhow::Result<()> {
    //     let mut data = Vec::new();
    //     self.id.write(&mut data)?;
    //     self.data.write(&mut data)?;
    //
    //     // todo: unnecessary allocation
    //     VarUInt(data.len() as u32).write_async(writer).await?;
    //     writer.write_all(&data).await?;
    //
    //     debug!("wrote packet ID: {:#x} length: {}", self.id.0, data.len());
    //
    //     // format hex debug! raw data 0x00
    //     // debug!("{:#x?}", data);
    //
    //     Ok(())
    // }
}
