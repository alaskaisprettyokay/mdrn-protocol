//! Transport layer
//!
//! Handles:
//! - libp2p swarm configuration
//! - gossipsub topic management
//! - DHT record publishing/querying
//! - Relay behavior

mod config;
mod swarm;

pub use config::TransportConfig;
pub use swarm::{MdrnBehaviour, MdrnBehaviourEvent, MdrnSwarm, SwarmError, MDRN_PROTOCOL_ID};

// Re-export libp2p types commonly used with MdrnSwarm
pub use libp2p::Multiaddr;
pub use libp2p::gossipsub::IdentTopic;

/// Create a gossipsub topic for a stream
pub fn stream_topic(stream_addr: &[u8; 32]) -> IdentTopic {
    let hex = hex::encode(stream_addr);
    IdentTopic::new(format!("/mdrn/stream/{}", hex))
}

/// DHT namespace for stream announcements
pub const DHT_STREAM_NAMESPACE: &str = "/mdrn/streams/";

/// DHT namespace for relay advertisements
pub const DHT_RELAY_NAMESPACE: &str = "/mdrn/relays/";
