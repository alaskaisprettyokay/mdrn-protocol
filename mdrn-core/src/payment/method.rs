//! Payment method codes

use serde_repr::{Deserialize_repr, Serialize_repr};

/// Payment method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum PaymentMethod {
    /// Free (no payment required)
    Free = 0,
    /// EVM L2 (Base, etc.)
    EvmL2 = 1,
    /// Lightning Network
    Lightning = 2,
    /// Superfluid streaming
    Superfluid = 3,
}

impl PaymentMethod {
    /// Get payment method from u8 value
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Free),
            1 => Some(Self::EvmL2),
            2 => Some(Self::Lightning),
            3 => Some(Self::Superfluid),
            _ => None,
        }
    }

    /// Get the method name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Free => "FREE",
            Self::EvmL2 => "EVM_L2",
            Self::Lightning => "LIGHTNING",
            Self::Superfluid => "SUPERFLUID",
        }
    }

    /// Check if this method requires on-chain settlement
    pub fn requires_settlement(&self) -> bool {
        !matches!(self, Self::Free)
    }
}
