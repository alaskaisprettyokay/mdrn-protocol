//! Keypair and identity types

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{ED25519_MULTICODEC, SECP256K1_MULTICODEC};

/// Key type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyType {
    Ed25519,
    Secp256k1,
}

/// Errors during identity operations
#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("invalid multicodec prefix")]
    InvalidPrefix,
    #[error("invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },
    #[error("signature verification failed")]
    SignatureVerificationFailed,
    #[error("key generation failed: {0}")]
    KeyGenerationFailed(String),
}

/// Identity is a multicodec-prefixed public key
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identity(Vec<u8>);

impl Identity {
    /// Create an Ed25519 identity from a 32-byte public key
    pub fn ed25519(public_key: [u8; 32]) -> Self {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&ED25519_MULTICODEC);
        bytes.extend_from_slice(&public_key);
        Self(bytes)
    }

    /// Create a secp256k1 identity from a 33-byte compressed public key
    pub fn secp256k1(public_key: [u8; 33]) -> Self {
        let mut bytes = Vec::with_capacity(35);
        bytes.extend_from_slice(&SECP256K1_MULTICODEC);
        bytes.extend_from_slice(&public_key);
        Self(bytes)
    }

    /// Parse identity from multicodec-prefixed bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() < 2 {
            return Err(IdentityError::InvalidPrefix);
        }

        let prefix = [bytes[0], bytes[1]];
        match prefix {
            ED25519_MULTICODEC if bytes.len() == 34 => Ok(Self(bytes.to_vec())),
            SECP256K1_MULTICODEC if bytes.len() == 35 => Ok(Self(bytes.to_vec())),
            ED25519_MULTICODEC => Err(IdentityError::InvalidKeyLength {
                expected: 34,
                got: bytes.len(),
            }),
            SECP256K1_MULTICODEC => Err(IdentityError::InvalidKeyLength {
                expected: 35,
                got: bytes.len(),
            }),
            _ => Err(IdentityError::InvalidPrefix),
        }
    }

    /// Get the key type
    pub fn key_type(&self) -> KeyType {
        match [self.0[0], self.0[1]] {
            ED25519_MULTICODEC => KeyType::Ed25519,
            SECP256K1_MULTICODEC => KeyType::Secp256k1,
            _ => unreachable!("Identity should always have valid prefix"),
        }
    }

    /// Get the raw public key bytes (without multicodec prefix)
    pub fn public_key_bytes(&self) -> &[u8] {
        &self.0[2..]
    }

    /// Get the full multicodec-prefixed bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Keypair for signing and identity
pub struct Keypair {
    key_type: KeyType,
    /// The identity (public key)
    identity: Identity,
    /// Private key bytes (Ed25519: 32 bytes, secp256k1: 32 bytes)
    secret: Vec<u8>,
}

impl Keypair {
    /// Generate a new Ed25519 keypair
    pub fn generate_ed25519() -> Result<Self, IdentityError> {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            key_type: KeyType::Ed25519,
            identity: Identity::ed25519(verifying_key.to_bytes()),
            secret: signing_key.to_bytes().to_vec(),
        })
    }

    /// Generate a new secp256k1 keypair
    pub fn generate_secp256k1() -> Result<Self, IdentityError> {
        use k256::ecdsa::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let compressed = verifying_key.to_encoded_point(true);
        let public_bytes: [u8; 33] = compressed
            .as_bytes()
            .try_into()
            .map_err(|_| IdentityError::KeyGenerationFailed("compressed key wrong size".into()))?;

        Ok(Self {
            key_type: KeyType::Secp256k1,
            identity: Identity::secp256k1(public_bytes),
            secret: signing_key.to_bytes().to_vec(),
        })
    }

    /// Get the identity (public key)
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Get the key type
    pub fn key_type(&self) -> KeyType {
        self.key_type
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        match self.key_type {
            KeyType::Ed25519 => {
                use ed25519_dalek::{Signature, Signer, SigningKey};
                let secret_bytes: [u8; 32] = self.secret.as_slice().try_into().unwrap();
                let signing_key = SigningKey::from_bytes(&secret_bytes);
                let signature: Signature = signing_key.sign(message);
                signature.to_bytes().to_vec()
            }
            KeyType::Secp256k1 => {
                use k256::ecdsa::{signature::Signer, Signature, SigningKey};
                let secret_bytes: [u8; 32] = self.secret.as_slice().try_into().unwrap();
                let signing_key = SigningKey::from_bytes((&secret_bytes).into()).unwrap();
                let signature: Signature = signing_key.sign(message);
                signature.to_bytes().to_vec()
            }
        }
    }
}

impl Identity {
    /// Verify a signature
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), IdentityError> {
        match self.key_type() {
            KeyType::Ed25519 => {
                use ed25519_dalek::{Signature, Verifier, VerifyingKey};
                let public_bytes: [u8; 32] = self
                    .public_key_bytes()
                    .try_into()
                    .map_err(|_| IdentityError::SignatureVerificationFailed)?;
                let verifying_key = VerifyingKey::from_bytes(&public_bytes)
                    .map_err(|_| IdentityError::SignatureVerificationFailed)?;
                let sig_bytes: [u8; 64] = signature
                    .try_into()
                    .map_err(|_| IdentityError::SignatureVerificationFailed)?;
                let sig = Signature::from_bytes(&sig_bytes);
                verifying_key
                    .verify(message, &sig)
                    .map_err(|_| IdentityError::SignatureVerificationFailed)
            }
            KeyType::Secp256k1 => {
                use k256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
                let verifying_key = VerifyingKey::from_sec1_bytes(self.public_key_bytes())
                    .map_err(|_| IdentityError::SignatureVerificationFailed)?;
                let sig = Signature::from_slice(signature)
                    .map_err(|_| IdentityError::SignatureVerificationFailed)?;
                verifying_key
                    .verify(message, &sig)
                    .map_err(|_| IdentityError::SignatureVerificationFailed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_keypair_sign_verify() {
        let keypair = Keypair::generate_ed25519().unwrap();
        let message = b"hello world";
        let signature = keypair.sign(message);
        keypair.identity().verify(message, &signature).unwrap();
    }

    #[test]
    fn test_secp256k1_keypair_sign_verify() {
        let keypair = Keypair::generate_secp256k1().unwrap();
        let message = b"hello world";
        let signature = keypair.sign(message);
        keypair.identity().verify(message, &signature).unwrap();
    }

    #[test]
    fn test_identity_roundtrip() {
        let keypair = Keypair::generate_ed25519().unwrap();
        let identity = keypair.identity();
        let bytes = identity.as_bytes();
        let parsed = Identity::from_bytes(bytes).unwrap();
        assert_eq!(identity, &parsed);
    }
}
