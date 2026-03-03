//! Message envelope structure

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::{Identity, Keypair};

use super::MessageType;

/// Errors during message operations
#[derive(Debug, Error)]
pub enum MessageError {
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    #[error("deserialization failed: {0}")]
    DeserializationFailed(String),
    #[error("signature verification failed")]
    InvalidSignature,
}

/// Message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Protocol version (1 for this spec)
    pub version: u32,
    /// Message type code
    pub msg_type: MessageType,
    /// Sender identity (multicodec-prefixed public key)
    pub sender: Identity,
    /// Unique nonce per message (12 bytes)
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// Type-specific CBOR payload
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    /// Signature over (version || type || nonce || payload)
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

impl Message {
    /// Create a new message (unsigned)
    pub fn new(msg_type: MessageType, sender: Identity, payload: Vec<u8>) -> Self {
        use rand::RngCore;

        let mut nonce = vec![0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);

        Self {
            version: crate::PROTOCOL_VERSION,
            msg_type,
            sender,
            nonce,
            payload,
            sig: Vec::new(),
        }
    }

    /// Create and sign a message
    pub fn create(
        msg_type: MessageType,
        keypair: &Keypair,
        payload: Vec<u8>,
    ) -> Result<Self, MessageError> {
        let mut msg = Self::new(msg_type, keypair.identity().clone(), payload);
        let sign_data = msg.signing_data();
        msg.sig = keypair.sign(&sign_data);
        Ok(msg)
    }

    /// Get the data to sign: version || type || nonce || payload
    fn signing_data(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.version.to_be_bytes());
        data.push(self.msg_type.code());
        data.extend_from_slice(&self.nonce);
        data.extend_from_slice(&self.payload);
        data
    }

    /// Verify the message signature
    pub fn verify(&self) -> Result<(), MessageError> {
        let sign_data = self.signing_data();
        self.sender
            .verify(&sign_data, &self.sig)
            .map_err(|_| MessageError::InvalidSignature)
    }

    /// Serialize to CBOR bytes
    pub fn to_cbor(&self) -> Result<Vec<u8>, MessageError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .map_err(|e| MessageError::SerializationFailed(e.to_string()))?;
        Ok(buf)
    }

    /// Deserialize from CBOR bytes
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, MessageError> {
        ciborium::from_reader(bytes)
            .map_err(|e| MessageError::DeserializationFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_roundtrip() {
        let keypair = Keypair::generate_ed25519().unwrap();
        let payload = b"test payload".to_vec();

        let msg = Message::create(MessageType::Ping, &keypair, payload.clone()).unwrap();
        msg.verify().unwrap();

        let cbor = msg.to_cbor().unwrap();
        let parsed = Message::from_cbor(&cbor).unwrap();
        parsed.verify().unwrap();

        assert_eq!(parsed.payload, payload);
    }
}
