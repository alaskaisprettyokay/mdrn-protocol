//! Relay node implementation
//!
//! A relay node forwards audio chunks between broadcasters and listeners.
//! Key responsibilities:
//! - Listen for peer connections on specified port
//! - Subscribe to stream topics and re-broadcast chunks
//! - Track metrics (peers, streams, bytes forwarded)
//! - Handle graceful shutdown with statistics

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use libp2p::gossipsub;
use libp2p::swarm::SwarmEvent;
use libp2p::Multiaddr;
use thiserror::Error;
use tokio::sync::Mutex;

use mdrn_core::identity::Keypair;
use mdrn_core::stream::Chunk;
use mdrn_core::transport::{stream_topic, MdrnBehaviourEvent, MdrnSwarm, TransportConfig};

/// Relay errors
#[derive(Debug, Error)]
pub enum RelayError {
    #[error("failed to create swarm: {0}")]
    SwarmCreation(String),

    #[error("failed to listen: {0}")]
    ListenFailed(String),

    #[error("failed to subscribe: {0}")]
    SubscribeFailed(String),

    #[error("failed to publish: {0}")]
    PublishFailed(String),

    #[error("relay not running")]
    NotRunning,

    #[error("port conflict: {0}")]
    PortConflict(String),

    #[error("already running")]
    AlreadyRunning,
}

/// Relay configuration
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Port to listen on (0 for random)
    pub port: u16,

    /// Price per minute in base units (0 for free)
    pub price_per_min: u64,

    /// Optional keypair (generates new if None)
    pub keypair: Option<Keypair>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            port: 9000,
            price_per_min: 0,
            keypair: None,
        }
    }
}

/// Relay metrics
#[derive(Debug, Clone, Default)]
pub struct RelayMetrics {
    /// Number of currently connected peers
    pub peers_connected: usize,

    /// Number of streams being relayed
    pub streams_relayed: usize,

    /// Total bytes forwarded
    pub bytes_forwarded: u64,

    /// Total chunks forwarded
    pub chunks_forwarded: u64,

    /// Uptime in seconds
    pub uptime_secs: u64,
}

/// Relay node
///
/// Manages a libp2p swarm that relays audio streams between peers.
pub struct RelayNode {
    /// Configuration
    config: RelayConfig,

    /// The libp2p swarm (None until started)
    swarm: Option<MdrnSwarm>,

    /// Local keypair
    keypair: Keypair,

    /// Listen address (set after start)
    listen_addr: Option<Multiaddr>,

    /// Stream addresses being relayed
    relayed_streams: HashSet<[u8; 32]>,

    /// Running flag
    running: Arc<AtomicBool>,

    /// Connected peer count
    peer_count: Arc<AtomicU64>,

    /// Bytes forwarded
    bytes_forwarded: Arc<AtomicU64>,

    /// Chunks forwarded
    chunks_forwarded: Arc<AtomicU64>,

    /// Start time
    start_time: Option<Instant>,

    /// Final metrics (set after shutdown)
    final_metrics: Option<RelayMetrics>,

    /// Received chunks queue for testing
    #[allow(dead_code)]
    received_chunks: Arc<Mutex<Vec<Chunk>>>,
}

impl RelayNode {
    /// Create a new relay node
    pub fn new(config: RelayConfig) -> Result<Self, RelayError> {
        // Generate or use provided keypair
        let keypair = config
            .keypair
            .clone()
            .unwrap_or_else(|| Keypair::generate_ed25519().expect("keypair generation"));

        Ok(Self {
            config,
            swarm: None,
            keypair,
            listen_addr: None,
            relayed_streams: HashSet::new(),
            running: Arc::new(AtomicBool::new(false)),
            peer_count: Arc::new(AtomicU64::new(0)),
            bytes_forwarded: Arc::new(AtomicU64::new(0)),
            chunks_forwarded: Arc::new(AtomicU64::new(0)),
            start_time: None,
            final_metrics: None,
            received_chunks: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> Option<libp2p::PeerId> {
        self.swarm.as_ref().map(|s| *s.local_peer_id())
    }

    /// Start the relay node
    pub async fn start(&mut self) -> Result<(), RelayError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(RelayError::AlreadyRunning);
        }

        // Create swarm configuration
        let listen_addr = if self.config.port == 0 {
            "/ip4/127.0.0.1/tcp/0".to_string()
        } else {
            format!("/ip4/0.0.0.0/tcp/{}", self.config.port)
        };

        let swarm_config = TransportConfig {
            listen_addrs: vec![listen_addr.clone()],
            bootstrap_nodes: vec![],
            ..TransportConfig::default()
        };

        // Create swarm
        let mut swarm = MdrnSwarm::new(self.keypair.clone(), swarm_config)
            .map_err(|e| RelayError::SwarmCreation(e.to_string()))?;

        // Start listening
        let addr: Multiaddr = listen_addr
            .parse()
            .map_err(|e| RelayError::ListenFailed(format!("{}", e)))?;

        swarm
            .listen(addr)
            .await
            .map_err(|e| RelayError::ListenFailed(e.to_string()))?;

        // Wait for actual listen address
        let actual_addr = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match swarm.inner_mut().select_next_some().await {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        return Ok(address);
                    }
                    SwarmEvent::ListenerError { error, .. } => {
                        return Err(RelayError::ListenFailed(error.to_string()));
                    }
                    _ => continue,
                }
            }
        })
        .await
        .map_err(|_| RelayError::ListenFailed("timeout waiting for listen address".to_string()))?
        .map_err(|e| {
            // Check if it's a port conflict
            let err_str = e.to_string();
            if err_str.contains("Address already in use") || err_str.contains("address in use") {
                RelayError::PortConflict(err_str)
            } else {
                e
            }
        })?;

        self.listen_addr = Some(actual_addr);
        self.swarm = Some(swarm);
        self.running.store(true, Ordering::SeqCst);
        self.start_time = Some(Instant::now());

        tracing::info!(
            peer_id = %hex::encode(self.keypair.identity().as_bytes()),
            addr = ?self.listen_addr,
            "Relay node started"
        );

        Ok(())
    }

    /// Get the listen address
    pub fn listen_addr(&self) -> Option<Multiaddr> {
        self.listen_addr.clone()
    }

    /// Check if relay is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Subscribe to a stream for relaying
    pub fn subscribe_stream(&mut self, stream_addr: &[u8; 32]) -> Result<(), RelayError> {
        let swarm = self
            .swarm
            .as_mut()
            .ok_or(RelayError::NotRunning)?;

        let topic = stream_topic(stream_addr);

        swarm
            .subscribe(&topic)
            .map_err(|e| RelayError::SubscribeFailed(e.to_string()))?;

        self.relayed_streams.insert(*stream_addr);

        tracing::info!(
            stream_addr = %hex::encode(stream_addr),
            "Subscribed to stream for relaying"
        );

        Ok(())
    }

    /// Check if relaying a specific stream
    pub fn is_relaying_stream(&self, stream_addr: &[u8; 32]) -> bool {
        self.relayed_streams.contains(stream_addr)
    }

    /// Get current metrics
    pub fn metrics(&self) -> RelayMetrics {
        let uptime_secs = self
            .start_time
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);

        RelayMetrics {
            peers_connected: self.peer_count.load(Ordering::SeqCst) as usize,
            streams_relayed: self.relayed_streams.len(),
            bytes_forwarded: self.bytes_forwarded.load(Ordering::SeqCst),
            chunks_forwarded: self.chunks_forwarded.load(Ordering::SeqCst),
            uptime_secs,
        }
    }

    /// Wait for and return next received chunk (for testing)
    pub async fn wait_for_chunk(&mut self) -> Option<Chunk> {
        if !self.running.load(Ordering::SeqCst) {
            return None;
        }

        let swarm = self.swarm.as_mut()?;

        // Poll swarm for gossipsub messages
        loop {
            tokio::select! {
                event = swarm.inner_mut().select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message { message, .. },
                        )) => {
                            // Try to parse as chunk
                            if let Ok(chunk) = ciborium::from_reader::<Chunk, _>(&message.data[..]) {
                                // Track metrics
                                self.bytes_forwarded.fetch_add(message.data.len() as u64, Ordering::SeqCst);
                                self.chunks_forwarded.fetch_add(1, Ordering::SeqCst);

                                return Some(chunk);
                            }
                        }
                        SwarmEvent::ConnectionEstablished { .. } => {
                            self.peer_count.fetch_add(1, Ordering::SeqCst);
                        }
                        SwarmEvent::ConnectionClosed { .. } => {
                            let current = self.peer_count.load(Ordering::SeqCst);
                            if current > 0 {
                                self.peer_count.fetch_sub(1, Ordering::SeqCst);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Run the relay event loop
    ///
    /// This processes incoming connections and messages, forwarding chunks
    /// to all subscribed peers.
    pub async fn run(&mut self) -> Result<(), RelayError> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(RelayError::NotRunning);
        }

        let swarm = self.swarm.as_mut().ok_or(RelayError::NotRunning)?;

        tracing::info!("Relay event loop starting");

        loop {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            tokio::select! {
                event = swarm.inner_mut().select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            tracing::info!("Listening on {}", address);
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            self.peer_count.fetch_add(1, Ordering::SeqCst);
                            tracing::info!("Peer connected: {}", peer_id);
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            let current = self.peer_count.load(Ordering::SeqCst);
                            if current > 0 {
                                self.peer_count.fetch_sub(1, Ordering::SeqCst);
                            }
                            tracing::info!("Peer disconnected: {}", peer_id);
                        }
                        SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message {
                                propagation_source,
                                message_id,
                                message,
                            },
                        )) => {
                            // Track bytes
                            let data_len = message.data.len() as u64;
                            self.bytes_forwarded.fetch_add(data_len, Ordering::SeqCst);
                            self.chunks_forwarded.fetch_add(1, Ordering::SeqCst);

                            tracing::debug!(
                                from = %propagation_source,
                                id = ?message_id,
                                bytes = data_len,
                                "Relaying message"
                            );

                            // Note: gossipsub automatically propagates to other peers
                            // We just need to be subscribed to the topic
                        }
                        SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                            gossipsub::Event::Subscribed { peer_id, topic },
                        )) => {
                            tracing::info!("Peer {} subscribed to {}", peer_id, topic);
                        }
                        SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                            gossipsub::Event::Unsubscribed { peer_id, topic },
                        )) => {
                            tracing::info!("Peer {} unsubscribed from {}", peer_id, topic);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    /// Shutdown the relay gracefully
    pub async fn shutdown(&mut self) -> Result<(), RelayError> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(()); // Already stopped
        }

        self.running.store(false, Ordering::SeqCst);

        // Capture final metrics
        self.final_metrics = Some(self.metrics());

        tracing::info!(
            peers = self.peer_count.load(Ordering::SeqCst),
            streams = self.relayed_streams.len(),
            bytes = self.bytes_forwarded.load(Ordering::SeqCst),
            chunks = self.chunks_forwarded.load(Ordering::SeqCst),
            "Relay shutting down"
        );

        // Drop swarm to close connections
        self.swarm = None;
        self.listen_addr = None;

        Ok(())
    }

    /// Get final metrics (after shutdown)
    pub fn final_metrics(&self) -> Option<RelayMetrics> {
        self.final_metrics.clone()
    }
}

/// Run the relay command
///
/// This is the main entry point called from CLI.
pub async fn run_relay(port: u16, price: u64) -> Result<(), RelayError> {
    use tokio::signal;

    // Load or generate keypair
    let keypair = load_relay_keypair()?;

    tracing::info!(
        identity = %hex::encode(keypair.identity().as_bytes()),
        "Loaded relay identity"
    );

    let config = RelayConfig {
        port,
        price_per_min: price,
        keypair: Some(keypair),
    };

    let mut relay = RelayNode::new(config)?;

    // Start relay
    relay.start().await?;

    println!("\n=== MDRN Relay Node ===");
    println!("Peer ID: {:?}", relay.local_peer_id());
    println!("Listen: {:?}", relay.listen_addr());
    println!("Price: {} per minute", price);
    println!("\nPress Ctrl+C to stop\n");

    // Run event loop with Ctrl+C handling
    tokio::select! {
        result = relay.run() => {
            if let Err(e) = result {
                tracing::error!("Relay error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            println!("\nReceived shutdown signal...");
        }
    }

    // Graceful shutdown
    relay.shutdown().await?;

    // Print final stats
    if let Some(metrics) = relay.final_metrics() {
        println!("\n=== Relay Statistics ===");
        println!("Uptime: {} seconds", metrics.uptime_secs);
        println!("Peers connected: {}", metrics.peers_connected);
        println!("Streams relayed: {}", metrics.streams_relayed);
        println!("Chunks forwarded: {}", metrics.chunks_forwarded);
        println!("Bytes forwarded: {}", metrics.bytes_forwarded);
    }

    Ok(())
}

/// Load relay keypair from default location or generate new one
fn load_relay_keypair() -> Result<Keypair, RelayError> {
    use std::path::PathBuf;

    // Check environment variable first
    if let Ok(env_path) = std::env::var("MDRN_KEYPAIR") {
        let bytes = std::fs::read(&env_path)
            .map_err(|e| RelayError::SwarmCreation(format!("Failed to read keypair: {}", e)))?;
        let keypair = Keypair::from_cbor(&bytes)
            .map_err(|e| RelayError::SwarmCreation(format!("Invalid keypair: {}", e)))?;
        return Ok(keypair);
    }

    // Check default location
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let default_path = PathBuf::from(home).join(".mdrn").join("keypair.cbor");

    if default_path.exists() {
        let bytes = std::fs::read(&default_path)
            .map_err(|e| RelayError::SwarmCreation(format!("Failed to read keypair: {}", e)))?;
        let keypair = Keypair::from_cbor(&bytes)
            .map_err(|e| RelayError::SwarmCreation(format!("Invalid keypair: {}", e)))?;
        return Ok(keypair);
    }

    // Generate new keypair
    tracing::info!("No keypair found, generating new one");
    Keypair::generate_ed25519()
        .map_err(|e| RelayError::SwarmCreation(format!("Failed to generate keypair: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_config_default() {
        let config = RelayConfig::default();
        assert_eq!(config.port, 9000);
        assert_eq!(config.price_per_min, 0);
        assert!(config.keypair.is_none());
    }

    #[test]
    fn test_relay_metrics_default() {
        let metrics = RelayMetrics::default();
        assert_eq!(metrics.peers_connected, 0);
        assert_eq!(metrics.streams_relayed, 0);
        assert_eq!(metrics.bytes_forwarded, 0);
        assert_eq!(metrics.chunks_forwarded, 0);
        assert_eq!(metrics.uptime_secs, 0);
    }

    #[test]
    fn test_relay_node_creation() {
        let config = RelayConfig::default();
        let relay = RelayNode::new(config);
        assert!(relay.is_ok());

        let relay = relay.unwrap();
        assert!(!relay.is_running());
        assert!(relay.listen_addr().is_none());
    }
}
