//! Identity module
//!
//! Handles:
//! - KeyType enum (Ed25519, secp256k1)
//! - Identity struct (multicodec-prefixed public keys)
//! - Vouch credentials for broadcaster admission
//! - Signature verification
//! - Genesis broadcaster list

mod keypair;
mod vouch;

pub use keypair::{Identity, IdentityError, KeyType, Keypair};
pub use vouch::{Vouch, VouchError};

/// Multicodec prefix for Ed25519 public keys (0xED01)
pub const ED25519_MULTICODEC: [u8; 2] = [0xED, 0x01];

/// Multicodec prefix for secp256k1 public keys (0xE701)
pub const SECP256K1_MULTICODEC: [u8; 2] = [0xE7, 0x01];

/// Get genesis broadcasters for the network
pub fn genesis_broadcasters() -> Vec<Identity> {
    // TODO: Add actual genesis broadcaster public keys
    // For Phase 2 development, using empty list - vouches will be tested with test keys
    Vec::new()
}

/// Trust chain verification - checks if a broadcaster is vouched by the trust network
pub struct TrustChain {
    genesis_keys: Vec<Identity>,
}

impl TrustChain {
    /// Create a new trust chain verifier
    pub fn new(genesis_keys: Vec<Identity>) -> Self {
        Self { genesis_keys }
    }

    /// Verify that a broadcaster has a valid vouch chain back to genesis
    pub fn verify_broadcaster_admission(&self, broadcaster: &Identity, vouch: &Vouch) -> Result<(), VouchError> {
        // First verify the vouch itself
        vouch.verify()?;

        // Check if vouch is for the right broadcaster
        if &vouch.subject != broadcaster {
            return Err(VouchError::InvalidSignature);
        }

        // In Phase 2, we'll implement simple one-hop verification:
        // Either the broadcaster is vouched by a genesis key, or they are a genesis key
        if self.genesis_keys.contains(broadcaster) {
            // Broadcaster is a genesis key - always valid
            Ok(())
        } else if self.genesis_keys.contains(&vouch.issuer) {
            // Broadcaster is vouched by a genesis key - valid
            Ok(())
        } else {
            // For now, reject multi-hop vouches (Phase 3 feature)
            // TODO: Implement full trust chain traversal
            Err(VouchError::InvalidSignature)
        }
    }

    /// Check if a broadcaster can vouch for others (is in trust network)
    pub fn can_vouch(&self, broadcaster: &Identity) -> bool {
        // For Phase 2, only genesis keys can vouch for others
        // TODO: Extend to include vouched broadcasters
        self.genesis_keys.contains(broadcaster)
    }
}
