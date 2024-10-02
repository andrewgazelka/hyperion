use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Scoreboard value.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ScoreboardValueContent<'a> {
    /// The name of the score holder whose score should be displayed. This
    /// can be a [`selector`] or an explicit name.
    ///
    /// [`selector`]: https://minecraft.wiki/w/Target_selectors
    pub name: Cow<'a, str>,
    /// The internal name of the objective to display the player's score in.
    pub objective: Cow<'a, str>,
    /// If present, this value is displayed regardless of what the score
    /// would have been.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Cow<'a, str>>,
}
