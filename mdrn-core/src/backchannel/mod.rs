//! Backchannel messaging
//!
//! E2E encrypted listener-to-broadcaster messages via ephemeral ECDH + ChaCha20-Poly1305.
//!
//! Handles:
//! - BackchannelMessage
//! - Payload types (TEXT, REACTION, TIP)
//! - Encryption/decryption

mod message;
mod payload;

pub use message::BackchannelMessage;
pub use payload::{BackchannelPayload, Reaction};
