//! MDRN swarm implementation
//!
//! This module provides a real libp2p swarm with:
//! - TCP and QUIC transports
//! - Noise encryption
//! - Yamux multiplexing
//! - Kademlia DHT
//! - Gossipsub pub/sub
//! - Identify protocol

use std::collections::{HashMap, HashSet};

use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity};
use libp2p::kad::{self, store::MemoryStore};
use libp2p::swarm::SwarmEvent;
use libp2p::{identify, noise, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder};
use thiserror::Error;

use super::TransportConfig;
use crate::identity::{KeyType, Keypair};

/// Swarm errors
#[derive(Debug, Error)]
pub enum SwarmError {
    #[error("failed to create swarm: {0}")]
    CreationFailed(String),
    #[error("failed to listen: {0}")]
    ListenFailed(String),
    #[error("failed to dial peer: {0}")]
    DialFailed(String),
    #[error("failed to publish: {0}")]
    PublishFailed(String),
    #[error("not subscribed to topic: {0}")]
    NotSubscribed(String),
    #[error("DHT operation failed: {0}")]
    DhtError(String),
    #[error("transport error: {0}")]
    TransportError(String),
}

/// MDRN protocol identifier
pub const MDRN_PROTOCOL_ID: &str = "/mdrn/1.0.0";

/// Combined behavior for MDRN swarm
#[derive(libp2p::swarm::NetworkBehaviour)]
pub struct MdrnBehaviour {
    /// Kademlia DHT for peer and content discovery
    pub kademlia: kad::Behaviour<MemoryStore>,
    /// Gossipsub for pub/sub messaging
    pub gossipsub: gossipsub::Behaviour,
    /// Identify protocol for peer info exchange
    pub identify: identify::Behaviour,
}

/// MDRN network swarm
///
/// Real libp2p swarm with:
/// - TCP and QUIC transports with Noise encryption
/// - Kademlia DHT for peer/content discovery
/// - Gossipsub for pub/sub messaging
/// - Identify protocol for peer info exchange
pub struct MdrnSwarm {
    /// Real libp2p swarm
    swarm: Swarm<MdrnBehaviour>,
    /// Transport configuration
    config: TransportConfig,
    /// Subscribed gossipsub topics
    subscribed_topics: HashSet<String>,
    /// Local DHT store for immediate get/put operations
    dht_store: HashMap<Vec<u8>, Vec<u8>>,
}

impl MdrnSwarm {
    /// Create a new swarm with the given keypair and configuration
    pub fn new(keypair: Keypair, config: TransportConfig) -> Result<Self, SwarmError> {
        // Convert MDRN keypair to libp2p keypair
        let libp2p_keypair = Self::convert_keypair(&keypair)?;

        // Create swarm using SwarmBuilder
        let swarm = SwarmBuilder::with_existing_identity(libp2p_keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| SwarmError::CreationFailed(e.to_string()))?
            .with_quic()
            .with_behaviour(|key| {
                // Configure Kademlia DHT
                let store = MemoryStore::new(key.public().to_peer_id());
                let mut kademlia = kad::Behaviour::new(key.public().to_peer_id(), store);
                kademlia.set_mode(Some(kad::Mode::Server));

                // Configure Gossipsub
                let gossipsub_config = gossipsub::Config::default();
                let gossipsub = gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                ).expect("Valid network behaviour");

                // Configure Identify
                let identify = identify::Behaviour::new(
                    identify::Config::new(MDRN_PROTOCOL_ID.to_string(), key.public())
                        .with_push_listen_addr_updates(true)
                );

                MdrnBehaviour {
                    kademlia,
                    gossipsub,
                    identify,
                }
            })
            .map_err(|e| SwarmError::CreationFailed(e.to_string()))?
            .build();

        Ok(Self {
            swarm,
            config,
            subscribed_topics: HashSet::new(),
            dht_store: HashMap::new(),
        })
    }

    /// Convert MDRN keypair to libp2p keypair
    fn convert_keypair(keypair: &Keypair) -> Result<libp2p::identity::Keypair, SwarmError> {
        match keypair.key_type() {
            KeyType::Ed25519 => {
                let secret_bytes = keypair.secret_bytes();
                if secret_bytes.len() != 32 {
                    return Err(SwarmError::CreationFailed(format!("Invalid Ed25519 secret key length: {}", secret_bytes.len())));
                }

                // Convert to array for libp2p
                let mut key_bytes = [0u8; 32];
                key_bytes.copy_from_slice(secret_bytes);

                Ok(libp2p::identity::Keypair::ed25519_from_bytes(key_bytes)
                    .map_err(|e| SwarmError::CreationFailed(format!("Keypair conversion failed: {}", e)))?)
            }
            KeyType::Secp256k1 => {
                // TODO: Implement secp256k1 support
                Err(SwarmError::CreationFailed("secp256k1 not yet supported".to_string()))
            }
        }
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    /// Get protocol identifier
    pub fn protocol_id() -> &'static str {
        MDRN_PROTOCOL_ID
    }

    /// Start listening on the given address (async)
    pub async fn listen(&mut self, addr: Multiaddr) -> Result<(), SwarmError> {
        self.swarm.listen_on(addr)
            .map_err(|e| SwarmError::ListenFailed(e.to_string()))?;
        Ok(())
    }

    /// Dial a peer (async)
    pub async fn dial(&mut self, addr: Multiaddr) -> Result<(), SwarmError> {
        self.swarm.dial(addr)
            .map_err(|e| SwarmError::DialFailed(e.to_string()))?;
        Ok(())
    }

    /// Get iterator over listening addresses
    pub fn listeners(&self) -> impl Iterator<Item = &Multiaddr> {
        self.swarm.listeners()
    }

    /// Subscribe to a gossipsub topic
    pub fn subscribe(&mut self, topic: &IdentTopic) -> Result<(), SwarmError> {
        self.swarm.behaviour_mut().gossipsub.subscribe(topic)
            .map_err(|e| SwarmError::PublishFailed(e.to_string()))?;
        self.subscribed_topics.insert(topic.to_string());
        Ok(())
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &IdentTopic) -> Result<(), SwarmError> {
        self.swarm.behaviour_mut().gossipsub.unsubscribe(topic)
            .map_err(|e| SwarmError::PublishFailed(e.to_string()))?;
        self.subscribed_topics.remove(&topic.to_string());
        Ok(())
    }

    /// Publish data to a topic
    pub fn publish(&mut self, topic: &IdentTopic, data: Vec<u8>) -> Result<(), SwarmError> {
        if !self.subscribed_topics.contains(&topic.to_string()) {
            return Err(SwarmError::NotSubscribed(topic.to_string()));
        }

        self.swarm.behaviour_mut().gossipsub.publish(topic.clone(), data)
            .map_err(|e| SwarmError::PublishFailed(e.to_string()))?;
        Ok(())
    }

    /// Store a key-value pair in DHT
    pub fn dht_put(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<(), SwarmError> {
        // Store locally for immediate retrieval
        self.dht_store.insert(key.clone(), value.clone());

        // Also store in Kademlia DHT for network propagation
        let kad_key = kad::RecordKey::new(&key);
        let record = kad::Record::new(kad_key, value);
        self.swarm.behaviour_mut().kademlia.put_record(record, kad::Quorum::One)
            .map_err(|e| SwarmError::DhtError(e.to_string()))?;

        Ok(())
    }

    /// Get a value from local DHT store
    pub fn dht_get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.dht_store.get(key).cloned()
    }

    /// Get iterator over DHT store entries
    pub fn dht_iter(&self) -> impl Iterator<Item = (&Vec<u8>, &Vec<u8>)> {
        self.dht_store.iter()
    }

    /// Check if subscribed to a topic
    pub fn is_subscribed(&self, topic: &IdentTopic) -> bool {
        self.subscribed_topics.contains(&topic.to_string())
    }

    /// Get the swarm configuration
    pub fn config(&self) -> &TransportConfig {
        &self.config
    }

    /// Get access to the inner swarm for advanced operations
    pub fn inner(&self) -> &Swarm<MdrnBehaviour> {
        &self.swarm
    }

    /// Get mutable access to the inner swarm
    pub fn inner_mut(&mut self) -> &mut Swarm<MdrnBehaviour> {
        &mut self.swarm
    }

    /// Main event loop for the swarm
    pub async fn run(&mut self) -> Result<(), SwarmError> {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => {
                    tracing::info!("Listening on {}", address);
                }
                SwarmEvent::Behaviour(event) => {
                    match event {
                        MdrnBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source,
                            message_id: _,
                            message,
                        }) => {
                            tracing::debug!(
                                "Received message from {}: {} bytes on topic {}",
                                propagation_source,
                                message.data.len(),
                                message.topic
                            );
                            // Message handling should be done by the consumer
                        }
                        MdrnBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                            id,
                            result,
                            ..
                        }) => {
                            tracing::debug!("Kademlia query {} progressed: {:?}", id, result);
                        }
                        MdrnBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                            tracing::debug!("Identified peer {}: {:?}", peer_id, info);
                        }
                        _ => {
                            // Handle other events as needed
                        }
                    }
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    tracing::info!("Connected to peer: {}", peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    tracing::info!("Disconnected from peer: {}", peer_id);
                }
                _ => {
                    // Handle other swarm events
                }
            }
        }
    }
}
