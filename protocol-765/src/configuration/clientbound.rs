use ser::{Packet, Readable, Writable};

// Finish Configuration
// Sent by the server to notify the client that the configuration process has finished. The client
// answers with its own Finish Configuration whenever it is ready to continue.
//
// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x02	Configuration	Client	no fields
#[derive(Packet, Writable, Readable, Debug)]
#[packet(0x2)]
#[allow(clippy::module_name_repetitions)]
pub struct FinishConfiguration;
