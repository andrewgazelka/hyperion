use std::io::{Cursor, Write};

pub enum EncodeError<E> {
    Encode(E),
    Io(std::io::Error),
}

pub enum DecodeError<E> {
    Decode(E),
    Io(std::io::Error),
}

impl From<std::io::Error> for EncodeError<std::io::Error> {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub trait Encode {
    type Error;

    fn encode(&self, w: impl Write) -> Result<(), EncodeError<Self::Error>>;
}

pub trait Decode<'a> {
    type Error;

    fn decode(r: Cursor<&'a [u8]>) -> Result<Self, DecodeError<Self::Error>>
    where
        Self: Sized;
}
