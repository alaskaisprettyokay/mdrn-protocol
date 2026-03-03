//! MDRN Core Protocol Library
//!
//! This crate implements the core MDRN (Massively Distributed Radio Network) protocol:
//! - Identity management (Ed25519/secp256k1 keypairs, multicodec encoding)
//! - Protocol messages (CBOR-encoded message types)
//! - Stream encryption (ChaCha20-Poly1305)
//! - Audio streaming (chunks, announcements, subscriptions)
//! - Payment commitments (cumulative signed commitments)
//! - Backchannel messaging (E2E encrypted listener-to-broadcaster)
//! - Transport layer (libp2p with gossipsub and Kademlia DHT)

pub mod backchannel;
pub mod crypto;
pub mod identity;
pub mod payment;
pub mod protocol;
pub mod stream;
pub mod transport;

/// Protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Protocol ID for libp2p
pub const PROTOCOL_ID: &str = "/mdrn/1.0.0";
