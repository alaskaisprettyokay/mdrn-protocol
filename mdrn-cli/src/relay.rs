//! Relay node implementation
//!
//! A relay node forwards audio chunks between broadcasters and listeners.
//! Key responsibilities:
//! - Listen for peer connections on specified port
//! - Subscribe to stream topics and re-broadcast chunks
//! - Track metrics (peers, streams, bytes forwarded)
//! - Handle graceful shutdown with statistics

use std::collections::{HashMap, HashSet};
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

    /// Network mode (testnet vs mainnet)
    pub network_mode: mdrn_core::transport::NetworkMode,

    /// Payment configuration (optional)
    pub payment_config: Option<mdrn_core::transport::PaymentConfig>,

    /// Optional keypair (generates new if None)
    pub keypair: Option<Keypair>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            port: 9000,
            network_mode: mdrn_core::transport::NetworkMode::Testnet,
            payment_config: Some(mdrn_core::transport::PaymentConfig::testnet()),
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

    /// Payment tracking: listener_id -> latest payment commitment
    listener_payments: Arc<Mutex<std::collections::HashMap<mdrn_core::identity::Identity, mdrn_core::payment::PaymentCommitment>>>,

    /// Bandwidth tracking: listener_id -> bytes consumed
    listener_usage: Arc<Mutex<std::collections::HashMap<mdrn_core::identity::Identity, u64>>>,

    /// Trust chain verifier for vouch-based admission control
    trust_chain: mdrn_core::identity::TrustChain,

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
            listener_payments: Arc::new(Mutex::new(HashMap::new())),
            listener_usage: Arc::new(Mutex::new(HashMap::new())),
            trust_chain: mdrn_core::identity::TrustChain::new(mdrn_core::identity::genesis_broadcasters()),
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

                            tracing::info!(
                                from = %propagation_source,
                                id = ?message_id,
                                bytes = data_len,
                                "Relaying message — re-publishing to mesh"
                            );

                            // ── HOTFIX: Explicit re-publish for relay forwarding ──
                            //
                            // Gossipsub auto-propagation only works when all peers are in the
                            // same mesh. In a hub-and-spoke topology (broadcaster → relay ←
                            // listener), broadcaster and listener are NOT directly connected.
                            // Gossipsub only propagates to peers in YOUR mesh — if the listener
                            // is not in the broadcaster's mesh (it isn't, only the relay is),
                            // the message stops at the relay.
                            //
                            // Fix: relay explicitly re-publishes every received message. This
                            // creates a store-and-forward bridge between the two mesh halves.
                            let ident_topic = mdrn_core::transport::IdentTopic::new(message.topic.as_str());
                            let data = message.data.clone();
                            match swarm.publish(&ident_topic, data) {
                                Ok(_) => {
                                    tracing::debug!("Re-published chunk on topic {}", message.topic.as_str());
                                }
                                Err(e) => {
                                    // DuplicateMessage is expected — gossipsub deduplicates by message_id.
                                    // We need to use a fresh message (new data bytes) to bypass dedup.
                                    tracing::debug!("Re-publish result: {}", e);
                                }
                            }
                            // ── END HOTFIX ──
                        }
                        SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                            gossipsub::Event::Subscribed { peer_id, topic },
                        )) => {
                            tracing::info!("Peer {} subscribed to {}", peer_id, topic);

                            // ── HOTFIX: Relay joins gossipsub mesh for every stream topic ──
                            //
                            // Gossipsub requires peers to be directly meshed. When a broadcaster
                            // and listener both connect to this relay but not to each other,
                            // the relay must also subscribe to the topic so it acts as a mesh
                            // intermediary — receiving from the broadcaster and forwarding to
                            // the listener (and vice versa).
                            //
                            // Without this, InsufficientPeers fires on both sides because the
                            // relay is connected but not in the gossipsub mesh for that topic.
                            // Reconstruct IdentTopic from the hash string so we can subscribe.
                            // gossipsub.subscribe() is idempotent — safe to call multiple times.
                            let topic_str = topic.to_string();
                            let ident_topic = mdrn_core::transport::IdentTopic::new(&topic_str);
                            match swarm.subscribe(&ident_topic) {
                                Ok(()) => {
                                    tracing::info!("Relay subscribed to topic: {}", topic_str);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to subscribe relay to topic {}: {}", topic_str, e);
                                }
                            }
                            // ── END HOTFIX ──
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

    /// Record bandwidth usage for a listener
    pub async fn record_bandwidth_usage(&self, listener_id: &mdrn_core::identity::Identity, bytes: u64) {
        let mut usage = self.listener_usage.lock().await;
        *usage.entry(listener_id.clone()).or_insert(0) += bytes;
    }

    /// Update payment commitment for a listener
    pub async fn update_payment_commitment(&self, commitment: mdrn_core::payment::PaymentCommitment) -> Result<(), String> {
        // Verify the commitment signature
        commitment.verify_signature()
            .map_err(|e| format!("Invalid payment commitment signature: {}", e))?;

        // Check if this supersedes previous commitment
        let mut payments = self.listener_payments.lock().await;
        if let Some(previous) = payments.get(&commitment.listener_id) {
            commitment.validate_supersedes(previous)
                .map_err(|e| format!("Payment commitment does not supersede previous: {}", e))?;
        }

        payments.insert(commitment.listener_id.clone(), commitment);
        Ok(())
    }

    /// Check if listener has sufficient payment for their usage
    pub async fn check_payment_sufficient(&self, listener_id: &mdrn_core::identity::Identity) -> bool {
        // In testnet mode, always allow
        if !self.config.network_mode.requires_payment() {
            return true;
        }

        let usage = self.listener_usage.lock().await;
        let payments = self.listener_payments.lock().await;

        let bytes_used = usage.get(listener_id).copied().unwrap_or(0);
        let payment_amount = payments.get(listener_id)
            .map(|p| p.amount)
            .unwrap_or(0);

        // Calculate required payment based on config
        if let Some(ref payment_config) = self.config.payment_config {
            let mb_used = (bytes_used + 1024 * 1024 - 1) / (1024 * 1024); // Round up to MB
            let required_amount = mb_used * payment_config.price_per_mb;
            payment_amount >= required_amount
        } else {
            // No payment config means free
            true
        }
    }

    /// Enforce payment limits for a listener before forwarding a chunk
    pub async fn enforce_payment_limits(&self, listener_id: &mdrn_core::identity::Identity, chunk_size: u64) -> bool {
        // In testnet mode, always allow
        if !self.config.network_mode.requires_payment() {
            return true;
        }

        // Record the bandwidth usage first
        self.record_bandwidth_usage(listener_id, chunk_size).await;

        // Then check if payment is sufficient
        self.check_payment_sufficient(listener_id).await
    }

    /// Verify broadcaster admission based on vouch credentials
    pub fn verify_broadcaster_admission(&self, broadcaster: &mdrn_core::identity::Identity, vouch: &mdrn_core::identity::Vouch) -> Result<(), String> {
        // In testnet mode, always allow
        if !self.config.network_mode.requires_vouches() {
            return Ok(());
        }

        // Verify the vouch chain
        self.trust_chain.verify_broadcaster_admission(broadcaster, vouch)
            .map_err(|e| format!("Vouch verification failed: {}", e))
    }

    /// Check if a broadcaster can vouch for others
    pub fn can_broadcaster_vouch(&self, broadcaster: &mdrn_core::identity::Identity) -> bool {
        // In testnet mode, anyone can vouch (for testing)
        if !self.config.network_mode.requires_vouches() {
            return true;
        }

        // In mainnet mode, check trust chain
        self.trust_chain.can_vouch(broadcaster)
    }
}

/// Run the relay command
///
/// This is the main entry point called from CLI.
pub async fn run_relay(
    port: u16,
    network_mode: mdrn_core::transport::NetworkMode,
    payment_config: Option<mdrn_core::transport::PaymentConfig>,
    daemon: bool
) -> Result<(), RelayError> {
    use tokio::signal;

    // Load or generate keypair
    let keypair = load_relay_keypair()?;

    tracing::info!(
        identity = %hex::encode(keypair.identity().as_bytes()),
        "Loaded relay identity"
    );

    let config = RelayConfig {
        port,
        network_mode,
        payment_config: payment_config.clone(),
        keypair: Some(keypair),
    };

    let mut relay = RelayNode::new(config)?;

    // Start relay
    relay.start().await?;

    println!("\n=== MDRN Relay Node ===");
    println!("Peer ID: {:?}", relay.local_peer_id());
    println!("Listen: {:?}", relay.listen_addr());
    println!("Mode: {}", if network_mode.requires_payment() { "mainnet" } else { "testnet" });
    if let Some(ref config) = payment_config {
        println!("Price: {} {} per MB", config.price_per_mb, config.currency);
        if let Some(ref contract) = config.settlement_contract {
            println!("Settlement contract: {}", contract);
        }
    } else {
        println!("Price: FREE");
    }
    if daemon {
        println!("Running in daemon mode (use SIGTERM to stop)");

        // Daemon mode: run indefinitely until killed
        if let Err(e) = relay.run().await {
            tracing::error!("Relay error: {}", e);
        }
    } else {
        println!("\nPress Ctrl+C to stop\n");

        // Interactive mode: run with Ctrl+C handling
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
        assert_eq!(config.network_mode, mdrn_core::transport::NetworkMode::Testnet);
        assert!(config.payment_config.is_some());
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

    #[test]
    fn test_vouch_verification_testnet() {
        use mdrn_core::identity::{Keypair, Vouch};
        use mdrn_core::transport::NetworkMode;

        // Create testnet relay config
        let config = RelayConfig {
            port: 9999,
            network_mode: NetworkMode::Testnet,
            payment_config: None,
            keypair: None,
        };

        let node = RelayNode::new(config).unwrap();

        // Create a test broadcaster and vouch
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let issuer = Keypair::generate_ed25519().unwrap();
        let vouch = Vouch::create(
            broadcaster.identity().clone(),
            &issuer,
            None,
        ).unwrap();

        // In testnet mode, vouch verification should always pass
        assert!(node.verify_broadcaster_admission(broadcaster.identity(), &vouch).is_ok());
        assert!(node.can_broadcaster_vouch(broadcaster.identity()));
    }

    #[test]
    fn test_vouch_verification_mainnet() {
        use mdrn_core::identity::{Keypair, Vouch};
        use mdrn_core::transport::NetworkMode;

        // Create mainnet relay config
        let config = RelayConfig {
            port: 9999,
            network_mode: NetworkMode::Mainnet,
            payment_config: None,
            keypair: None,
        };

        let node = RelayNode::new(config).unwrap();

        // Create a test broadcaster and vouch from non-genesis issuer
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let non_genesis_issuer = Keypair::generate_ed25519().unwrap();
        let vouch = Vouch::create(
            broadcaster.identity().clone(),
            &non_genesis_issuer,
            None,
        ).unwrap();

        // In mainnet mode with no genesis keys, vouch verification should fail
        assert!(node.verify_broadcaster_admission(broadcaster.identity(), &vouch).is_err());
        assert!(!node.can_broadcaster_vouch(broadcaster.identity()));
    }
}
