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

/// Genesis broadcasters (TBD - placeholder)
pub fn genesis_broadcasters() -> Vec<Identity> {
    // TODO: Add genesis broadcaster public keys
    Vec::new()
}
