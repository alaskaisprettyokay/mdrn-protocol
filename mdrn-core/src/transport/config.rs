//! Transport configuration

use std::time::Duration;

use crate::identity::Identity;
use crate::payment::PaymentMethod;

/// Network mode (testnet vs mainnet)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    /// Testnet mode - free operation, no payments or vouch verification required
    Testnet,
    /// Mainnet mode - paid operation, vouch verification enforced
    Mainnet,
}

impl NetworkMode {
    /// Check if this mode requires payment
    pub fn requires_payment(&self) -> bool {
        matches!(self, NetworkMode::Mainnet)
    }

    /// Check if this mode requires vouch verification
    pub fn requires_vouches(&self) -> bool {
        matches!(self, NetworkMode::Mainnet)
    }
}

impl Default for NetworkMode {
    fn default() -> Self {
        // Default to testnet for development
        NetworkMode::Testnet
    }
}

/// Payment configuration for relay nodes
#[derive(Debug, Clone)]
pub struct PaymentConfig {
    /// Payment method to accept
    pub method: PaymentMethod,
    /// Currency code (e.g., "USDC", "BTC")
    pub currency: String,
    /// Price per MB in base units
    pub price_per_mb: u64,
    /// Settlement contract address (for on-chain methods)
    pub settlement_contract: Option<String>,
}

impl PaymentConfig {
    /// Create a new payment config
    pub fn new(
        method: PaymentMethod,
        currency: String,
        price_per_mb: u64,
        settlement_contract: Option<String>,
    ) -> Self {
        Self {
            method,
            currency,
            price_per_mb,
            settlement_contract,
        }
    }

    /// Create a free testnet config
    pub fn testnet() -> Self {
        Self {
            method: PaymentMethod::Free,
            currency: "FREE".to_string(),
            price_per_mb: 0,
            settlement_contract: None,
        }
    }
}

/// Transport layer configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Network mode (testnet vs mainnet)
    pub network_mode: NetworkMode,
    /// Genesis broadcaster keys (for vouch verification)
    pub genesis_keys: Vec<Identity>,
    /// Payment configuration (optional)
    pub payment_config: Option<PaymentConfig>,
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
            network_mode: NetworkMode::default(),
            genesis_keys: crate::identity::genesis_broadcasters(),
            payment_config: None,
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
