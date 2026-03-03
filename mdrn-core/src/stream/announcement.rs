//! Stream announcement (DHT record)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::{Identity, Vouch};

use super::Codec;

/// Stream announcement published to DHT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamAnnouncement {
    /// SHA-256(broadcaster_identity || stream_id)
    #[serde(with = "serde_bytes")]
    pub stream_addr: [u8; 32],
    /// Broadcaster identity
    pub broadcaster: Identity,
    /// Human-readable stream identifier
    pub stream_id: String,
    /// Audio codec
    pub codec: Codec,
    /// Bitrate in kbps
    pub bitrate: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels (1=mono, 2=stereo)
    pub channels: u8,
    /// Whether stream is encrypted
    pub encrypted: bool,
    /// Minimum price (optional)
    pub price_min: Option<u64>,
    /// Vouch credential proving broadcaster admission
    pub vouch: Vouch,
    /// Searchable tags
    pub tags: Vec<String>,
    /// Unix timestamp when stream started
    pub started_at: u64,
    /// Time-to-live in seconds (default 300)
    pub ttl: u32,
}

impl StreamAnnouncement {
    /// Compute stream address: SHA-256(broadcaster_identity || stream_id)
    pub fn compute_stream_addr(broadcaster: &Identity, stream_id: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(broadcaster.as_bytes());
        hasher.update(stream_id.as_bytes());
        hasher.finalize().into()
    }

    /// Create a new stream announcement
    pub fn new(
        broadcaster: Identity,
        stream_id: String,
        codec: Codec,
        bitrate: u32,
        sample_rate: u32,
        channels: u8,
        encrypted: bool,
        vouch: Vouch,
    ) -> Self {
        let stream_addr = Self::compute_stream_addr(&broadcaster, &stream_id);
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            stream_addr,
            broadcaster,
            stream_id,
            codec,
            bitrate,
            sample_rate,
            channels,
            encrypted,
            price_min: None,
            vouch,
            tags: Vec::new(),
            started_at,
            ttl: 300,
        }
    }

    /// Verify the announcement (check vouch validity)
    pub fn verify(&self) -> Result<(), crate::identity::VouchError> {
        self.vouch.verify()
    }
}
