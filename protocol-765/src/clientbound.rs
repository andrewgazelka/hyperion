use ser::{Packet, Readable, Writable};

// Status Response
// packet id 0x0
#[derive(Packet, Readable, Writable, Debug, Eq, PartialEq, Clone)]
#[packet(0x00, Handshake)]
pub struct StatusResponse {
    pub json: String,
}

// Pong
// packet id 0x01
#[derive(Packet, Writable, Debug)]
#[packet(0x01, Handshake)]
pub struct Pong {
    pub payload: i64,
}

// // Encryption Request
// // Packet ID	State	Bound To	Field Name	Field Type	Notes
// // 0x01	Login	Client	Server ID	String (20)	Appears to be empty.
// // Public Key Length	VarInt	Length of Public Key
// // Public Key	Byte Array	The server's public key, in bytes.
// // Verify Token Length	VarInt	Length of Verify Token. Always 4 for Notchian servers.
// // Verify Token	Byte Array	A sequence of random bytes generated by the server.
// #[derive(Packet, Writable, Debug)]
// #[packet(0x01, Handshake)]
// pub struct EncryptionRequest {
//     pub server_id: String,
//     pub public_key: Vec<u8>,
//     pub verify_token: Vec<u8>
// }

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ser::{ReadExt, Writable};

    use crate::clientbound::StatusResponse;

    #[test]
    fn test_round_trip() {
        let json = r#"{"version":{"name":"1.16.5","protocol":754},"players":{"max":20,"online":0,"sample":[]},"description":{"text":"Hello world"}}"#;
        let status_response = super::StatusResponse {
            json: json.to_string(),
        };
        let mut data = Vec::new();
        status_response.clone().write(&mut data).unwrap();
        let mut reader = std::io::Cursor::new(data);
        let status_response2: StatusResponse = reader.read_type().unwrap();
        assert_eq!(status_response, status_response2);
    }
}
