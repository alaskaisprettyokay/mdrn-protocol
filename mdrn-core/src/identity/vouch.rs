//! Vouch credentials for broadcaster admission

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Identity, Keypair};

/// Errors during vouch operations
#[derive(Debug, Error)]
pub enum VouchError {
    #[error("vouch has expired")]
    Expired,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
}

/// Vouch credential - signed attestation from existing broadcaster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vouch {
    /// New broadcaster being vouched for
    pub subject: Identity,
    /// Broadcaster issuing the vouch
    pub issuer: Identity,
    /// Unix timestamp when issued
    pub issued_at: u64,
    /// Optional expiration timestamp
    pub expires_at: Option<u64>,
    /// Signature over canonical CBOR of above fields
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl Vouch {
    /// Create a new vouch (unsigned)
    fn new_unsigned(subject: Identity, issuer: Identity, expires_at: Option<u64>) -> Self {
        let issued_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            subject,
            issuer,
            issued_at,
            expires_at,
            signature: Vec::new(),
        }
    }

    /// Create and sign a vouch
    pub fn create(
        subject: Identity,
        issuer_keypair: &Keypair,
        expires_at: Option<u64>,
    ) -> Result<Self, VouchError> {
        let mut vouch = Self::new_unsigned(subject, issuer_keypair.identity().clone(), expires_at);

        // Serialize the unsigned vouch data for signing
        let sign_data = vouch.signing_data()?;
        vouch.signature = issuer_keypair.sign(&sign_data);

        Ok(vouch)
    }

    /// Get the data to sign (canonical CBOR of fields except signature)
    fn signing_data(&self) -> Result<Vec<u8>, VouchError> {
        // Create a signable struct without the signature field
        #[derive(Serialize)]
        struct VouchSignable<'a> {
            subject: &'a Identity,
            issuer: &'a Identity,
            issued_at: u64,
            expires_at: Option<u64>,
        }

        let signable = VouchSignable {
            subject: &self.subject,
            issuer: &self.issuer,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&signable, &mut buf)
            .map_err(|e| VouchError::SerializationFailed(e.to_string()))?;
        Ok(buf)
    }

    /// Verify the vouch signature and expiration
    pub fn verify(&self) -> Result<(), VouchError> {
        // Check expiration
        if let Some(expires_at) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now > expires_at {
                return Err(VouchError::Expired);
            }
        }

        // Verify signature
        let sign_data = self.signing_data()?;
        self.issuer
            .verify(&sign_data, &self.signature)
            .map_err(|_| VouchError::InvalidSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vouch_create_and_verify() {
        let issuer = Keypair::generate_ed25519().unwrap();
        let subject = Keypair::generate_ed25519().unwrap();

        let vouch = Vouch::create(subject.identity().clone(), &issuer, None).unwrap();

        vouch.verify().unwrap();
    }

    #[test]
    fn test_vouch_expired() {
        let issuer = Keypair::generate_ed25519().unwrap();
        let subject = Keypair::generate_ed25519().unwrap();

        // Create vouch that expired 1 second ago
        let vouch = Vouch::create(subject.identity().clone(), &issuer, Some(0)).unwrap();

        assert!(matches!(vouch.verify(), Err(VouchError::Expired)));
    }
}
