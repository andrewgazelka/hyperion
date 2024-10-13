use std::borrow::Cow;

use crate::{Text, TextContent};

impl<'a> Text<'a> {
    /// Creates a new `Text` instance from a string slice.
    #[must_use]
    pub const fn new(s: &'a str) -> Self {
        Text {
            content: TextContent::Text {
                text: Cow::Borrowed(s),
            },
            color: None,
            font: None,
            bold: None,
            italic: None,
            underlined: None,
            strikethrough: None,
            obfuscated: None,
            insertion: None,
            click_event: None,
            hover_event: None,
            extra: Vec::new(),
        }
    }
}

// Implement From trait for &str to Text conversion
impl<'a> From<&'a str> for Text<'a> {
    fn from(s: &'a str) -> Self {
        Text::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_creation_and_conversion() {
        let text1 = Text::new("Hello, world!");
        let text2: Text<'_> = "Hello, world!".into();

        assert_eq!(text1, text2);
    }
}
