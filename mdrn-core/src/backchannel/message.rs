//! Backchannel message with E2E encryption

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{self, KEY_SIZE, NONCE_SIZE};
use crate::identity::Identity;

use super::BackchannelPayload;

/// Backchannel errors
#[derive(Debug, Error)]
pub enum BackchannelError {
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
}

/// E2E encrypted backchannel message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackchannelMessage {
    /// Sender identity (listener)
    pub sender: Identity,
    /// Recipient identity (broadcaster)
    pub recipient: Identity,
    /// Stream address
    #[serde(with = "serde_bytes")]
    pub stream_addr: [u8; 32],
    /// Ephemeral public key for ECDH (compressed secp256k1, 33 bytes)
    #[serde(with = "serde_bytes")]
    pub ephemeral_pubkey: Vec<u8>,
    /// Encryption nonce
    #[serde(with = "serde_bytes")]
    pub nonce: [u8; NONCE_SIZE],
    /// Encrypted payload (CBOR-encoded BackchannelPayload)
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    /// Message sequence number
    pub seq: u64,
    /// Unix timestamp
    pub timestamp: u64,
}

impl BackchannelMessage {
    /// Create and encrypt a backchannel message
    ///
    /// In a full implementation, this would:
    /// 1. Generate ephemeral keypair
    /// 2. Perform ECDH with recipient's public key
    /// 3. Derive encryption key with HKDF
    /// 4. Encrypt payload
    ///
    /// For now, this is a stub that uses a pre-shared key approach.
    pub fn create(
        sender: Identity,
        recipient: Identity,
        stream_addr: [u8; 32],
        payload: &BackchannelPayload,
        shared_key: &[u8; KEY_SIZE],
        seq: u64,
    ) -> Result<Self, BackchannelError> {
        // Serialize payload
        let mut payload_bytes = Vec::new();
        ciborium::into_writer(payload, &mut payload_bytes)
            .map_err(|e| BackchannelError::SerializationFailed(e.to_string()))?;

        // Encrypt
        let (ciphertext, nonce) = crypto::encrypt(shared_key, &payload_bytes)
            .map_err(|e| BackchannelError::EncryptionFailed(e.to_string()))?;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(Self {
            sender,
            recipient,
            stream_addr,
            ephemeral_pubkey: Vec::new(), // TODO: Real ECDH implementation
            nonce,
            ciphertext,
            seq,
            timestamp,
        })
    }

    /// Decrypt the message payload
    pub fn decrypt(
        &self,
        shared_key: &[u8; KEY_SIZE],
    ) -> Result<BackchannelPayload, BackchannelError> {
        let plaintext = crypto::decrypt(shared_key, &self.ciphertext, &self.nonce)
            .map_err(|e| BackchannelError::DecryptionFailed(e.to_string()))?;

        ciborium::from_reader(&plaintext[..])
            .map_err(|e| BackchannelError::DecryptionFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;

    #[test]
    fn test_backchannel_roundtrip() {
        let sender = Keypair::generate_ed25519().unwrap();
        let recipient = Keypair::generate_ed25519().unwrap();
        let stream_addr = [0u8; 32];
        let shared_key = crypto::generate_stream_key();

        let payload = BackchannelPayload::text("Hello broadcaster!");

        let msg = BackchannelMessage::create(
            sender.identity().clone(),
            recipient.identity().clone(),
            stream_addr,
            &payload,
            &shared_key,
            1,
        )
        .unwrap();

        let decrypted = msg.decrypt(&shared_key).unwrap();

        match decrypted {
            BackchannelPayload::Text(text) => assert_eq!(text, "Hello broadcaster!"),
            _ => panic!("Wrong payload type"),
        }
    }
}
