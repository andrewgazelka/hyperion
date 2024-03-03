use std::fmt::Debug;

// re-export the `ser-macro` crate
#[cfg(feature = "ser-macro")]
pub use ser_macro::*;

pub mod types;

pub trait Writable {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()>;
}

pub trait Readable<'a>: Sized {
    /// Reads this object from the provided byte slice.
    ///
    /// Implementations of `Readable` are expected to shrink the slice from the
    /// front as bytes are read.
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self>;
}

// pub trait WriteExt: Write {
//     fn write_type<T: Writable>(&mut self, data: T) -> anyhow::Result<&mut Self>
//     where
//         Self: Sized,
//     {
//         data.write(self)?;
//         Ok(self)
//     }
// }

// impl<T: Write> WriteExt for T {}

pub trait Packet {
    const ID: i32;
    const NAME: &'static str;
}

#[derive(Debug)]
pub struct ExactPacket<T>(pub T);

pub struct GenericPacket {
    pub id: i32,
    pub data: Vec<u8>,
}
