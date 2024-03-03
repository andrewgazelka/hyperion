use ser::{
    types::{Identifier, UntilEnd},
    EnumReadable, Packet, Readable,
};

// Sent when the player connects, or when settings are changed.
//
// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x00	Configuration	Server	Locale	String (16)	e.g. en_GB.
// View Distance	Byte	Client-side render distance, in chunks.
// Chat Mode	VarInt Enum	0: enabled, 1: commands only, 2: hidden. See Chat#Client chat mode for more
// information. Chat Colors	Boolean	“Colors” multiplayer setting. Can the chat be colored?
// Displayed Skin Parts	Unsigned Byte	Bit mask, see below.
// Main Hand	VarInt Enum	0: Left, 1: Right.
// Enable text filtering	Boolean	Enables filtering of text on signs and written book titles. Currently
// always false (i.e. the filtering is disabled) Allow server listings	Boolean	Servers usually list
// online players, this option should let you not show up in that list.
#[derive(Packet, Readable, Debug)]
#[packet(0x0)]
pub struct Configuration<'a> {
    pub locale: &'a str,
    pub view_distance: u8,
    pub chat_mode: ChatMode,
    pub chat_colors: bool,
    pub displayed_skin_parts: u8,
    pub main_hand: MainHand,
    pub enable_text_filtering: bool,
    pub allow_server_listings: bool,
}

#[derive(EnumReadable, Debug)]
pub enum ChatMode {
    Enabled,
    CommandsOnly,
    Hidden,
}

#[derive(EnumReadable, Debug)]
pub enum MainHand {
    Left,
    Right,
}

// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x01	Configuration	Server	Channel	Identifier	Name of the plugin channel used to send the data.
// Data	Byte Array (32767)	Any data, depending on the channel. minecraft: channels are documented
// here. The length of this array must be inferred from the packet length.
#[derive(Packet, Readable, Debug)]
#[packet(0x1)]
pub struct PluginMessage<'a> {
    pub channel: Identifier<'a>,
    pub data: UntilEnd<'a>,
}

// Finish Configuration
// Sent by the client to notify the client that the configuration process has finished. It is sent
// in response to the server's Finish Configuration.
//
// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x02	Configuration	Server	no fields
#[derive(Packet, Readable, Debug)]
#[packet(0x2)]
pub struct FinishConfiguration;
