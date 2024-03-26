#![allow(clippy::module_name_repetitions)]
use std::fmt::Debug;

pub type Idx = u16;

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct OptionalIdx(Idx);

impl TryFrom<OptionalIdx> for Idx {
    type Error = ();

    fn try_from(value: OptionalIdx) -> Result<Self, Self::Error> {
        if value.is_null() {
            Err(())
        } else {
            Ok(value.0)
        }
    }
}

impl Default for OptionalIdx {
    fn default() -> Self {
        Self::NONE
    }
}

impl Debug for OptionalIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_null() {
            write!(f, "NodeId::NULL")
        } else {
            write!(f, "NodeId({})", self.0)
        }
    }
}

pub const NULL_ID: Idx = Idx::MAX;

impl OptionalIdx {
    pub const NONE: Self = Self(NULL_ID);

    pub const fn inner(self) -> Option<Idx> {
        if self.is_null() {
            None
        } else {
            Some(self.0)
        }
    }

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == NULL_ID
    }

    #[must_use]
    pub const fn some(id: Idx) -> Self {
        debug_assert!(id != NULL_ID);
        Self(id)
    }
}

impl From<usize> for OptionalIdx {
    fn from(value: usize) -> Self {
        Self(value as Idx)
    }
}
