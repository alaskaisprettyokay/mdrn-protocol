//! Payment receipt

use serde::{Deserialize, Serialize};

use crate::identity::Identity;

/// Payment receipt (relay acknowledgment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentReceipt {
    /// Relay identity
    pub relay_id: Identity,
    /// Listener identity
    pub listener_id: Identity,
    /// Stream address
    #[serde(with = "serde_bytes")]
    pub stream_addr: [u8; 32],
    /// Acknowledged commitment sequence number
    pub commitment_seq: u64,
    /// Acknowledged cumulative amount
    pub amount: u64,
    /// Unix timestamp
    pub timestamp: u64,
    /// Relay signature
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}
