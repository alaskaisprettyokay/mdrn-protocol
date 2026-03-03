//! Audio chunk format

use serde::{Deserialize, Serialize};

use super::Codec;

bitflags::bitflags! {
    /// Chunk flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ChunkFlags: u8 {
        /// Chunk data is encrypted
        const ENCRYPTED = 0x01;
        /// Chunk is a keyframe
        const KEYFRAME = 0x02;
    }
}

/// Audio data chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Stream address
    #[serde(with = "serde_bytes")]
    pub stream_addr: [u8; 32],
    /// Monotonically increasing sequence number
    pub seq: u64,
    /// Presentation timestamp in microseconds
    pub timestamp: u64,
    /// Audio codec
    pub codec: Codec,
    /// Chunk flags
    pub flags: ChunkFlags,
    /// Duration in microseconds
    pub duration_us: u32,
    /// Encoded audio data
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    /// Encryption nonce (present only if ENCRYPTED flag set)
    #[serde(with = "serde_bytes")]
    pub nonce: Option<[u8; 12]>,
}

impl Chunk {
    /// Create a new unencrypted chunk
    pub fn new(
        stream_addr: [u8; 32],
        seq: u64,
        timestamp: u64,
        codec: Codec,
        duration_us: u32,
        data: Vec<u8>,
    ) -> Self {
        Self {
            stream_addr,
            seq,
            timestamp,
            codec,
            flags: ChunkFlags::empty(),
            duration_us,
            data,
            nonce: None,
        }
    }

    /// Create a new encrypted chunk
    pub fn new_encrypted(
        stream_addr: [u8; 32],
        seq: u64,
        timestamp: u64,
        codec: Codec,
        duration_us: u32,
        data: Vec<u8>,
        nonce: [u8; 12],
    ) -> Self {
        Self {
            stream_addr,
            seq,
            timestamp,
            codec,
            flags: ChunkFlags::ENCRYPTED,
            duration_us,
            data,
            nonce: Some(nonce),
        }
    }

    /// Check if chunk is encrypted
    pub fn is_encrypted(&self) -> bool {
        self.flags.contains(ChunkFlags::ENCRYPTED)
    }

    /// Check if chunk is a keyframe
    pub fn is_keyframe(&self) -> bool {
        self.flags.contains(ChunkFlags::KEYFRAME)
    }

    /// Set keyframe flag
    pub fn set_keyframe(&mut self) {
        self.flags |= ChunkFlags::KEYFRAME;
    }
}
