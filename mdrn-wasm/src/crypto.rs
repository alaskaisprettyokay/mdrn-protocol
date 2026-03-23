//! Crypto WASM bindings
//!
//! Provides browser-compatible encryption, decryption, and key derivation.

use wasm_bindgen::prelude::*;
use mdrn_core::crypto::{encrypt_stream, decrypt_stream, derive_stream_key};
use crate::WasmError;

/// Encrypt a chunk of audio data with ChaCha20-Poly1305
#[wasm_bindgen]
pub fn encrypt_chunk(
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, WasmError> {
    if key.len() != 32 {
        return Err(WasmError::from("Key must be 32 bytes"));
    }
    if nonce.len() != 12 {
        return Err(WasmError::from("Nonce must be 12 bytes"));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(key);

    let mut nonce_array = [0u8; 12];
    nonce_array.copy_from_slice(nonce);

    encrypt_stream(&key_array, &nonce_array, plaintext)
        .map_err(|e| WasmError::from(e.to_string()))
}

/// Decrypt a chunk of audio data with ChaCha20-Poly1305
#[wasm_bindgen]
pub fn decrypt_chunk(
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, WasmError> {
    if key.len() != 32 {
        return Err(WasmError::from("Key must be 32 bytes"));
    }
    if nonce.len() != 12 {
        return Err(WasmError::from("Nonce must be 12 bytes"));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(key);

    let mut nonce_array = [0u8; 12];
    nonce_array.copy_from_slice(nonce);

    decrypt_stream(&key_array, &nonce_array, ciphertext)
        .map_err(|e| WasmError::from(e.to_string()))
}

/// Derive a stream encryption key from shared secret
#[wasm_bindgen]
pub fn derive_key(
    shared_secret: &[u8],
    stream_id: &str,
    info: &[u8],
) -> Result<Vec<u8>, WasmError> {
    if shared_secret.len() != 32 {
        return Err(WasmError::from("Shared secret must be 32 bytes"));
    }

    let mut secret_array = [0u8; 32];
    secret_array.copy_from_slice(shared_secret);

    let key = derive_stream_key(&secret_array, stream_id, info)
        .map_err(|e| WasmError::from(e.to_string()))?;

    Ok(key.to_vec())
}

/// Generate a random 12-byte nonce for encryption
#[wasm_bindgen]
pub fn generate_nonce() -> Vec<u8> {
    use rand::RngCore;
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce.to_vec()
}

/// Generate a random 32-byte key
#[wasm_bindgen]
pub fn generate_key() -> Vec<u8> {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key.to_vec()
}

/// Hash data with SHA-256
#[wasm_bindgen]
pub fn hash_sha256(data: &[u8]) -> Vec<u8> {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Compute HMAC-SHA256
#[wasm_bindgen]
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key)
        .expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Constant-time comparison of byte arrays
#[wasm_bindgen]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}