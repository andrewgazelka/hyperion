use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Text;

/// Action to take on click of the text.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum ClickEvent<'a> {
    /// Opens an URL
    OpenUrl(Cow<'a, str>),
    /// Only usable by internal servers for security reasons.
    OpenFile(Cow<'a, str>),
    /// Sends a chat command. Doesn't actually have to be a command, can be a
    /// normal chat message.
    RunCommand(Cow<'a, str>),
    /// Replaces the contents of the chat box with the text, not necessarily a
    /// command.
    SuggestCommand(Cow<'a, str>),
    /// Only usable within written books. Changes the page of the book. Indexing
    /// starts at 1.
    ChangePage(i32),
    /// Copies the given text to clipboard
    CopyToClipboard(Cow<'a, str>),
}

/// Action to take when mouse-hovering on the text.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "action", content = "contents", rename_all = "snake_case")]
#[expect(clippy::enum_variant_names)]
pub enum HoverEvent<'a> {
    /// Displays a tooltip with the given text.
    ShowText(Text<'a>),
    /// Shows an item.
    ShowItem {
        /// Resource identifier of the item (ident)
        id: Cow<'a, str>,
        /// Number of the items in the stack
        count: Option<i32>,
        /// NBT information about the item (sNBT format)
        tag: Cow<'a, str>,
    },
    /// Shows an entity.
    ShowEntity {
        /// The entity's UUID
        id: Uuid,
        /// Resource identifier of the entity
        #[serde(rename = "type")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<Cow<'a, str>>,
        /// Optional custom name for the entity
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<Text<'a>>,
    },
}
