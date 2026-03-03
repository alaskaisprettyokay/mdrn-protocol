//! Audio streaming types
//!
//! Handles:
//! - StreamAnnouncement (DHT record)
//! - RelayAdvertisement (DHT record)
//! - Chunk format
//! - Codec identifiers
//! - Subscription state machine

mod announcement;
mod chunk;
mod codec;
mod relay;
mod subscription;

pub use announcement::StreamAnnouncement;
pub use chunk::{Chunk, ChunkFlags};
pub use codec::Codec;
pub use relay::{Endpoint, RelayAdvertisement};
pub use subscription::SubscriptionState;
