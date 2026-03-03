//! Relay advertisement (DHT record)

use serde::{Deserialize, Serialize};

use crate::identity::Identity;
use crate::payment::PaymentMethod;

/// Network endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Multiaddr string (e.g., "/ip4/1.2.3.4/tcp/9000")
    pub addr: String,
    /// Transport protocol
    pub transport: Transport,
}

/// Transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Tcp,
    Quic,
    WebRtc,
}

/// Relay advertisement published to DHT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayAdvertisement {
    /// Relay identity
    pub relay_id: Identity,
    /// Stream address being relayed
    #[serde(with = "serde_bytes")]
    pub stream_addr: [u8; 32],
    /// Price per minute (0 = free)
    pub price_per_min: u64,
    /// Supported payment methods
    pub payment_methods: Vec<PaymentMethod>,
    /// Current listener capacity
    pub capacity: u32,
    /// Approximate latency in milliseconds
    pub latency_ms: u32,
    /// Network endpoints
    pub endpoints: Vec<Endpoint>,
    /// Time-to-live in seconds
    pub ttl: u32,
}

impl RelayAdvertisement {
    /// Check if this relay offers free access
    pub fn is_free(&self) -> bool {
        self.price_per_min == 0 || self.payment_methods.contains(&PaymentMethod::Free)
    }
}
