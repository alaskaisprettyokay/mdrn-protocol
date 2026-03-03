//! Cryptographic primitives
//!
//! Handles:
//! - ChaCha20-Poly1305 stream encryption
//! - HKDF-SHA256 key derivation
//! - Ephemeral ECDH for backchannel
//! - Stream key generation

mod encryption;
mod keys;

pub use encryption::{decrypt, encrypt, StreamCipher};
pub use keys::{derive_stream_key, generate_stream_key};

/// Nonce size for ChaCha20-Poly1305 (12 bytes)
pub const NONCE_SIZE: usize = 12;

/// Key size for ChaCha20-Poly1305 (32 bytes)
pub const KEY_SIZE: usize = 32;

/// Authentication tag size (16 bytes)
pub const TAG_SIZE: usize = 16;
