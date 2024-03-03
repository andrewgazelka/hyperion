use ser::{Packet, Readable, Writable};
use uuid::Uuid;

// Status Response
// packet id 0x0
#[derive(Packet, Readable, Writable, Debug, Eq, PartialEq, Clone)]
#[packet(0x00)]
pub struct StatusResponse<'a> {
    pub json: &'a str,
}

// Pong
// packet id 0x01
#[derive(Packet, Writable, Debug)]
#[packet(0x01)]
pub struct Pong {
    pub payload: i64,
}

#[derive(Packet, Readable, Writable, Debug)]
#[packet(0x02)]
pub struct LoginSuccess<'a> {
    pub uuid: Uuid,
    pub username: &'a str,
    pub properties: Vec<Property<'a>>,
}

#[derive(Readable, Writable, Debug)]
pub struct PropertyHeader<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub is_signed: bool,
}

#[derive(Debug)]
pub struct Property<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub is_signed: bool,
    pub signature: Option<&'a str>,
}

impl<'a> Writable for Property<'a> {
    fn write(&self, writer: &mut impl std::io::Write) -> anyhow::Result<()> {
        self.name.write(writer)?;
        self.value.write(writer)?;
        self.is_signed.write(writer)?;
        if let Some(signature) = self.signature {
            true.write(writer)?;
            signature.write(writer)?;
        } else {
            false.write(writer)?;
        }
        Ok(())
    }
}

impl<'a> Readable<'a> for Property<'a> {
    fn decode(r: &mut &'a [u8]) -> anyhow::Result<Self> {
        let PropertyHeader {
            name,
            value,
            is_signed,
        } = PropertyHeader::decode(r)?;
        let signature = if is_signed {
            Some(<&str>::decode(r)?)
        } else {
            None
        };
        Ok(Self {
            name,
            value,
            is_signed,
            signature,
        })
    }
}
