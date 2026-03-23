//! Identity WASM bindings
//!
//! Provides browser-compatible keypair generation, signing, and identity management.

use wasm_bindgen::prelude::*;
use mdrn_core::identity::{Keypair, Identity, KeyType};
use serde::{Deserialize, Serialize};
use crate::WasmError;

/// WASM wrapper for MDRN Keypair
#[wasm_bindgen]
pub struct WasmKeypair {
    inner: Keypair,
}

#[wasm_bindgen]
impl WasmKeypair {
    /// Get the inner keypair (for internal use)
    pub(crate) fn inner(&self) -> &Keypair {
        &self.inner
    }
    /// Generate a new Ed25519 keypair
    #[wasm_bindgen(constructor)]
    pub fn generate() -> Result<WasmKeypair, WasmError> {
        let keypair = Keypair::generate_ed25519()
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmKeypair { inner: keypair })
    }

    /// Generate a secp256k1 keypair
    #[wasm_bindgen]
    pub fn generate_secp256k1() -> Result<WasmKeypair, WasmError> {
        let keypair = Keypair::generate_secp256k1()
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmKeypair { inner: keypair })
    }

    /// Load keypair from CBOR bytes
    #[wasm_bindgen]
    pub fn from_cbor(bytes: &[u8]) -> Result<WasmKeypair, WasmError> {
        let keypair = Keypair::from_cbor(bytes)
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmKeypair { inner: keypair })
    }

    /// Export keypair to CBOR bytes
    #[wasm_bindgen]
    pub fn to_cbor(&self) -> Result<Vec<u8>, WasmError> {
        self.inner.to_cbor()
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Load keypair from JSON string (for browser localStorage)
    #[wasm_bindgen]
    pub fn from_json(json: &str) -> Result<WasmKeypair, WasmError> {
        #[derive(Deserialize)]
        struct JsonKeypair {
            key_type: String,
            private_key: String,
        }

        let json_kp: JsonKeypair = serde_json::from_str(json)
            .map_err(|e| WasmError::from(format!("Invalid JSON: {}", e)))?;

        let private_bytes = hex::decode(&json_kp.private_key)
            .map_err(|e| WasmError::from(format!("Invalid hex: {}", e)))?;

        let keypair = match json_kp.key_type.as_str() {
            "Ed25519" => {
                if private_bytes.len() != 32 {
                    return Err(WasmError::from("Ed25519 private key must be 32 bytes"));
                }
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&private_bytes);
                Keypair::from_ed25519_bytes(bytes)
            }
            "Secp256k1" => {
                if private_bytes.len() != 32 {
                    return Err(WasmError::from("secp256k1 private key must be 32 bytes"));
                }
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&private_bytes);
                Keypair::from_secp256k1_bytes(bytes)
            }
            _ => return Err(WasmError::from("Unknown key type")),
        };

        keypair.map(|kp| WasmKeypair { inner: kp })
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Export keypair to JSON string (for browser localStorage)
    #[wasm_bindgen]
    pub fn to_json(&self) -> Result<String, WasmError> {
        #[derive(Serialize)]
        struct JsonKeypair {
            key_type: String,
            private_key: String,
        }

        let key_type = match self.inner.key_type() {
            KeyType::Ed25519 => "Ed25519",
            KeyType::Secp256k1 => "Secp256k1",
        }.to_string();

        let private_key = hex::encode(self.inner.private_key_bytes());

        let json_kp = JsonKeypair { key_type, private_key };

        serde_json::to_string(&json_kp)
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Get the identity (public key) as hex string
    #[wasm_bindgen]
    pub fn identity_hex(&self) -> String {
        hex::encode(self.inner.identity().as_bytes())
    }

    /// Get the identity as a WASM wrapper
    #[wasm_bindgen]
    pub fn identity(&self) -> WasmIdentity {
        WasmIdentity {
            inner: self.inner.identity().clone(),
        }
    }

    /// Sign a message and return signature bytes
    #[wasm_bindgen]
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, WasmError> {
        self.inner.sign(message)
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Get the key type as string
    #[wasm_bindgen]
    pub fn key_type(&self) -> String {
        match self.inner.key_type() {
            KeyType::Ed25519 => "Ed25519",
            KeyType::Secp256k1 => "Secp256k1",
        }.to_string()
    }
}

/// WASM wrapper for MDRN Identity
#[wasm_bindgen]
pub struct WasmIdentity {
    inner: Identity,
}

#[wasm_bindgen]
impl WasmIdentity {
    /// Get the inner identity (for internal use)
    pub(crate) fn inner(&self) -> &Identity {
        &self.inner
    }
    /// Create identity from hex string
    #[wasm_bindgen]
    pub fn from_hex(hex: &str) -> Result<WasmIdentity, WasmError> {
        let bytes = hex::decode(hex)
            .map_err(|e| WasmError::from(format!("Invalid hex: {}", e)))?;
        let identity = Identity::from_bytes(bytes)
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmIdentity { inner: identity })
    }

    /// Get identity as hex string
    #[wasm_bindgen]
    pub fn to_hex(&self) -> String {
        hex::encode(self.inner.as_bytes())
    }

    /// Get raw identity bytes
    #[wasm_bindgen]
    pub fn bytes(&self) -> Vec<u8> {
        self.inner.as_bytes().to_vec()
    }

    /// Verify a signature against this identity
    #[wasm_bindgen]
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        self.inner.verify(message, signature).is_ok()
    }

    /// Get the key type of this identity
    #[wasm_bindgen]
    pub fn key_type(&self) -> String {
        match self.inner.key_type() {
            KeyType::Ed25519 => "Ed25519",
            KeyType::Secp256k1 => "Secp256k1",
        }.to_string()
    }
}