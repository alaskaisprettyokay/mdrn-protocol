//! Audio codec identifiers

use serde_repr::{Deserialize_repr, Serialize_repr};

/// Audio codec identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Codec {
    /// Opus (mandatory-to-implement, RFC 6716)
    Opus = 1,
    /// FLAC (optional)
    Flac = 2,
    /// Codec2 (optional, low bitrate)
    Codec2 = 3,
}

impl Codec {
    /// Get codec from u8 value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Opus),
            2 => Some(Self::Flac),
            3 => Some(Self::Codec2),
            _ => None,
        }
    }

    /// Get the codec name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Opus => "Opus",
            Self::Flac => "FLAC",
            Self::Codec2 => "Codec2",
        }
    }
}
