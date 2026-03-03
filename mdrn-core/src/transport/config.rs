//! Transport configuration

use std::time::Duration;

/// Transport layer configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Listen addresses (multiaddr format)
    pub listen_addrs: Vec<String>,
    /// Bootstrap nodes for DHT
    pub bootstrap_nodes: Vec<String>,
    /// Kademlia replication factor (k)
    pub kademlia_k: usize,
    /// Kademlia parallelism factor (alpha)
    pub kademlia_alpha: usize,
    /// Gossipsub heartbeat interval
    pub gossipsub_heartbeat: Duration,
    /// Connection idle timeout
    pub idle_timeout: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            listen_addrs: vec![
                "/ip4/0.0.0.0/tcp/0".to_string(),
                "/ip4/0.0.0.0/udp/0/quic-v1".to_string(),
            ],
            bootstrap_nodes: Vec::new(), // TBD
            kademlia_k: 20,
            kademlia_alpha: 3,
            gossipsub_heartbeat: Duration::from_secs(1),
            idle_timeout: Duration::from_secs(60),
        }
    }
}
