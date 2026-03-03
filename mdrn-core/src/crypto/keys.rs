//! Key generation and derivation

use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use thiserror::Error;

use super::KEY_SIZE;

/// Key derivation errors
#[derive(Debug, Error)]
pub enum KeyError {
    #[error("key derivation failed")]
    DerivationFailed,
}

/// Generate a new random stream key
pub fn generate_stream_key() -> [u8; KEY_SIZE] {
    let mut key = [0u8; KEY_SIZE];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

/// Derive a stream key using HKDF-SHA256
///
/// # Arguments
/// * `ikm` - Input key material (e.g., shared secret from key exchange)
/// * `salt` - Optional salt (use stream identifier)
/// * `info` - Context info (e.g., "mdrn-stream-key")
pub fn derive_stream_key(
    ikm: &[u8],
    salt: Option<&[u8]>,
    info: &[u8],
) -> Result<[u8; KEY_SIZE], KeyError> {
    let hkdf = Hkdf::<Sha256>::new(salt, ikm);
    let mut key = [0u8; KEY_SIZE];
    hkdf.expand(info, &mut key)
        .map_err(|_| KeyError::DerivationFailed)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_stream_key() {
        let key1 = generate_stream_key();
        let key2 = generate_stream_key();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_stream_key() {
        let ikm = b"shared secret";
        let salt = b"stream-123";
        let info = b"mdrn-stream-key";

        let key1 = derive_stream_key(ikm, Some(salt), info).unwrap();
        let key2 = derive_stream_key(ikm, Some(salt), info).unwrap();
        assert_eq!(key1, key2);

        let key3 = derive_stream_key(ikm, Some(b"different-salt"), info).unwrap();
        assert_ne!(key1, key3);
    }
}
