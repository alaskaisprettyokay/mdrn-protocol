//! Backchannel payload types

use serde::{Deserialize, Serialize};

/// Reaction type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reaction {
    /// Generic like/heart
    Like,
    /// Fire/awesome
    Fire,
    /// Clapping
    Clap,
    /// Laughing
    Laugh,
    /// Mind blown
    MindBlown,
}

/// Backchannel payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackchannelPayload {
    /// Text message
    Text(String),
    /// Reaction
    Reaction(Reaction),
    /// Tip notification (actual payment via PaymentCommitment)
    Tip {
        /// Amount in base units
        amount: u64,
        /// Currency code
        currency: String,
        /// Optional message
        message: Option<String>,
    },
}

impl BackchannelPayload {
    /// Create a text message
    pub fn text(msg: impl Into<String>) -> Self {
        Self::Text(msg.into())
    }

    /// Create a reaction
    pub fn reaction(reaction: Reaction) -> Self {
        Self::Reaction(reaction)
    }

    /// Create a tip notification
    pub fn tip(amount: u64, currency: impl Into<String>, message: Option<String>) -> Self {
        Self::Tip {
            amount,
            currency: currency.into(),
            message,
        }
    }
}
