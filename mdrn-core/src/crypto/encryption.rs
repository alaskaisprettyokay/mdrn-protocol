//! Stream encryption with ChaCha20-Poly1305

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use thiserror::Error;

use super::{KEY_SIZE, NONCE_SIZE};

/// Encryption errors
#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid key size")]
    InvalidKeySize,
    #[error("invalid nonce size")]
    InvalidNonceSize,
}

/// Stream cipher for encrypting audio chunks
pub struct StreamCipher {
    cipher: ChaCha20Poly1305,
}

impl StreamCipher {
    /// Create a new stream cipher with the given key
    pub fn new(key: &[u8; KEY_SIZE]) -> Self {
        let cipher = ChaCha20Poly1305::new(key.into());
        Self { cipher }
    }

    /// Encrypt data with a random nonce, returning (ciphertext, nonce)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_SIZE]), EncryptionError> {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        Ok((ciphertext, nonce_bytes))
    }

    /// Encrypt data with a specific nonce
    pub fn encrypt_with_nonce(
        &self,
        plaintext: &[u8],
        nonce: &[u8; NONCE_SIZE],
    ) -> Result<Vec<u8>, EncryptionError> {
        let nonce = Nonce::from_slice(nonce);
        self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptionFailed)
    }

    /// Decrypt data with the given nonce
    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        nonce: &[u8; NONCE_SIZE],
    ) -> Result<Vec<u8>, EncryptionError> {
        let nonce = Nonce::from_slice(nonce);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::DecryptionFailed)
    }
}

/// Convenience function to encrypt data
pub fn encrypt(
    key: &[u8; KEY_SIZE],
    plaintext: &[u8],
) -> Result<(Vec<u8>, [u8; NONCE_SIZE]), EncryptionError> {
    StreamCipher::new(key).encrypt(plaintext)
}

/// Convenience function to decrypt data
pub fn decrypt(
    key: &[u8; KEY_SIZE],
    ciphertext: &[u8],
    nonce: &[u8; NONCE_SIZE],
) -> Result<Vec<u8>, EncryptionError> {
    StreamCipher::new(key).decrypt(ciphertext, nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_stream_key;

    #[test]
    fn test_encrypt_decrypt() {
        let key = generate_stream_key();
        let plaintext = b"hello world";

        let (ciphertext, nonce) = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext, &nonce).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_stream_cipher() {
        let key = generate_stream_key();
        let cipher = StreamCipher::new(&key);

        let plaintext = b"audio chunk data";
        let (ciphertext, nonce) = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&ciphertext, &nonce).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = generate_stream_key();
        let key2 = generate_stream_key();
        let plaintext = b"secret data";

        let (ciphertext, nonce) = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &ciphertext, &nonce);

        assert!(result.is_err());
    }
}
