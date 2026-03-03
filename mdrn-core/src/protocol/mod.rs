//! Protocol message types and CBOR serialization
//!
//! Handles:
//! - MessageType enum
//! - Message envelope structure
//! - CBOR serialization/deserialization
//! - Canonical encoding for signatures

mod envelope;
mod types;

pub use envelope::Message;
pub use types::MessageType;
