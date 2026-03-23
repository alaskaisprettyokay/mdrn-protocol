//! Stream WASM bindings
//!
//! Provides browser-compatible chunk handling and payment commitments.

use wasm_bindgen::prelude::*;
use mdrn_core::stream::{Chunk, Codec, ChunkFlags};
use mdrn_core::payment::{PaymentCommitment, PaymentMethod};
use mdrn_core::identity::Identity;
use serde::Serialize;
use crate::WasmError;

/// WASM wrapper for audio chunk
#[wasm_bindgen]
pub struct WasmChunk {
    inner: Chunk,
}

#[wasm_bindgen]
impl WasmChunk {
    /// Create a new audio chunk
    #[wasm_bindgen(constructor)]
    pub fn new(
        stream_addr: &[u8],
        seq: u64,
        timestamp_us: u64,
        codec: u8,
        encrypted: bool,
        keyframe: bool,
        duration_us: u64,
        data: &[u8],
    ) -> Result<WasmChunk, WasmError> {
        if stream_addr.len() != 32 {
            return Err(WasmError::from("Stream address must be 32 bytes"));
        }

        let mut addr = [0u8; 32];
        addr.copy_from_slice(stream_addr);

        let codec = Codec::try_from(codec)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let mut flags = ChunkFlags::empty();
        if encrypted {
            flags |= ChunkFlags::ENCRYPTED;
        }
        if keyframe {
            flags |= ChunkFlags::KEYFRAME;
        }

        let nonce = if encrypted {
            Some([0u8; 12]) // Will be set by caller
        } else {
            None
        };

        let chunk = Chunk::new(
            addr,
            seq,
            timestamp_us,
            codec,
            flags,
            duration_us,
            data.to_vec(),
            nonce,
        );

        Ok(WasmChunk { inner: chunk })
    }

    /// Create chunk with random nonce for encryption
    #[wasm_bindgen]
    pub fn new_encrypted(
        stream_addr: &[u8],
        seq: u64,
        timestamp_us: u64,
        codec: u8,
        keyframe: bool,
        duration_us: u64,
        data: &[u8],
    ) -> Result<WasmChunk, WasmError> {
        use rand::RngCore;

        if stream_addr.len() != 32 {
            return Err(WasmError::from("Stream address must be 32 bytes"));
        }

        let mut addr = [0u8; 32];
        addr.copy_from_slice(stream_addr);

        let codec = Codec::try_from(codec)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let mut flags = ChunkFlags::ENCRYPTED;
        if keyframe {
            flags |= ChunkFlags::KEYFRAME;
        }

        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);

        let chunk = Chunk::new(
            addr,
            seq,
            timestamp_us,
            codec,
            flags,
            duration_us,
            data.to_vec(),
            Some(nonce),
        );

        Ok(WasmChunk { inner: chunk })
    }

    /// Encode chunk to CBOR bytes
    #[wasm_bindgen]
    pub fn to_cbor(&self) -> Result<Vec<u8>, WasmError> {
        self.inner.to_cbor()
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Decode chunk from CBOR bytes
    #[wasm_bindgen]
    pub fn from_cbor(bytes: &[u8]) -> Result<WasmChunk, WasmError> {
        let chunk = Chunk::from_cbor(bytes)
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmChunk { inner: chunk })
    }

    /// Get stream address bytes
    #[wasm_bindgen]
    pub fn stream_addr(&self) -> Vec<u8> {
        self.inner.stream_addr().to_vec()
    }

    /// Get stream address as hex string
    #[wasm_bindgen]
    pub fn stream_addr_hex(&self) -> String {
        hex::encode(self.inner.stream_addr())
    }

    /// Get sequence number
    #[wasm_bindgen]
    pub fn seq(&self) -> u64 {
        self.inner.seq()
    }

    /// Get timestamp in microseconds
    #[wasm_bindgen]
    pub fn timestamp_us(&self) -> u64 {
        self.inner.timestamp_us()
    }

    /// Get codec type
    #[wasm_bindgen]
    pub fn codec(&self) -> u8 {
        self.inner.codec() as u8
    }

    /// Check if chunk is encrypted
    #[wasm_bindgen]
    pub fn encrypted(&self) -> bool {
        self.inner.flags().contains(ChunkFlags::ENCRYPTED)
    }

    /// Check if chunk is a keyframe
    #[wasm_bindgen]
    pub fn keyframe(&self) -> bool {
        self.inner.flags().contains(ChunkFlags::KEYFRAME)
    }

    /// Get duration in microseconds
    #[wasm_bindgen]
    pub fn duration_us(&self) -> u64 {
        self.inner.duration_us()
    }

    /// Get audio data bytes
    #[wasm_bindgen]
    pub fn data(&self) -> Vec<u8> {
        self.inner.data().clone()
    }

    /// Get nonce bytes (for encrypted chunks)
    #[wasm_bindgen]
    pub fn nonce(&self) -> Option<Vec<u8>> {
        self.inner.nonce().map(|n| n.to_vec())
    }

    /// Convert to JSON for debugging
    #[wasm_bindgen]
    pub fn to_json(&self) -> Result<String, WasmError> {
        #[derive(Serialize)]
        struct ChunkJson {
            stream_addr: String,
            seq: u64,
            timestamp_us: u64,
            codec: u8,
            encrypted: bool,
            keyframe: bool,
            duration_us: u64,
            data_size: usize,
            nonce: Option<String>,
        }

        let json = ChunkJson {
            stream_addr: hex::encode(self.inner.stream_addr()),
            seq: self.inner.seq(),
            timestamp_us: self.inner.timestamp_us(),
            codec: self.inner.codec() as u8,
            encrypted: self.inner.flags().contains(ChunkFlags::ENCRYPTED),
            keyframe: self.inner.flags().contains(ChunkFlags::KEYFRAME),
            duration_us: self.inner.duration_us(),
            data_size: self.inner.data().len(),
            nonce: self.inner.nonce().map(|n| hex::encode(n)),
        };

        serde_json::to_string_pretty(&json)
            .map_err(|e| WasmError::from(e.to_string()))
    }
}

/// WASM wrapper for payment commitment
#[wasm_bindgen]
pub struct WasmPaymentCommitment {
    inner: PaymentCommitment,
}

#[wasm_bindgen]
impl WasmPaymentCommitment {
    /// Create a new payment commitment
    #[wasm_bindgen(constructor)]
    pub fn new(
        relay_id_hex: &str,
        listener_id_hex: &str,
        stream_addr: &[u8],
        method: u8,
        amount: u64,
        currency: &str,
        chain_id: Option<u64>,
        seq: u64,
    ) -> Result<WasmPaymentCommitment, WasmError> {
        let relay_id = Identity::from_bytes(
            hex::decode(relay_id_hex)
                .map_err(|e| WasmError::from(format!("Invalid relay ID hex: {}", e)))?
        ).map_err(|e| WasmError::from(e.to_string()))?;

        let listener_id = Identity::from_bytes(
            hex::decode(listener_id_hex)
                .map_err(|e| WasmError::from(format!("Invalid listener ID hex: {}", e)))?
        ).map_err(|e| WasmError::from(e.to_string()))?;

        if stream_addr.len() != 32 {
            return Err(WasmError::from("Stream address must be 32 bytes"));
        }
        let mut addr = [0u8; 32];
        addr.copy_from_slice(stream_addr);

        let method = PaymentMethod::try_from(method)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let commitment = PaymentCommitment::create_raw(
            relay_id,
            listener_id,
            addr,
            method,
            amount,
            currency.to_string(),
            chain_id,
            seq,
        ).map_err(|e| WasmError::from(e.to_string()))?;

        Ok(WasmPaymentCommitment { inner: commitment })
    }

    /// Create and sign a payment commitment
    #[wasm_bindgen]
    pub fn create_signed(
        relay_id_hex: &str,
        stream_addr: &[u8],
        method: u8,
        amount: u64,
        currency: &str,
        chain_id: Option<u64>,
        seq: u64,
        keypair: &crate::identity::WasmKeypair,
    ) -> Result<WasmPaymentCommitment, WasmError> {
        let relay_id = Identity::from_bytes(
            hex::decode(relay_id_hex)
                .map_err(|e| WasmError::from(format!("Invalid relay ID hex: {}", e)))?
        ).map_err(|e| WasmError::from(e.to_string()))?;

        if stream_addr.len() != 32 {
            return Err(WasmError::from("Stream address must be 32 bytes"));
        }
        let mut addr = [0u8; 32];
        addr.copy_from_slice(stream_addr);

        let method = PaymentMethod::try_from(method)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let commitment = PaymentCommitment::create(
            keypair.identity().inner().clone(),
            keypair.inner(),
            addr,
            method,
            amount,
            currency.to_string(),
            seq,
        ).map_err(|e| WasmError::from(e.to_string()))?;

        Ok(WasmPaymentCommitment { inner: commitment })
    }

    /// Encode to CBOR bytes
    #[wasm_bindgen]
    pub fn to_cbor(&self) -> Result<Vec<u8>, WasmError> {
        self.inner.to_cbor()
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Decode from CBOR bytes
    #[wasm_bindgen]
    pub fn from_cbor(bytes: &[u8]) -> Result<WasmPaymentCommitment, WasmError> {
        let commitment = PaymentCommitment::from_cbor(bytes)
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmPaymentCommitment { inner: commitment })
    }

    /// Get relay ID as hex
    #[wasm_bindgen]
    pub fn relay_id_hex(&self) -> String {
        hex::encode(self.inner.relay_id().as_bytes())
    }

    /// Get listener ID as hex
    #[wasm_bindgen]
    pub fn listener_id_hex(&self) -> String {
        hex::encode(self.inner.listener_id().as_bytes())
    }

    /// Get stream address as hex
    #[wasm_bindgen]
    pub fn stream_addr_hex(&self) -> String {
        hex::encode(self.inner.stream_addr())
    }

    /// Get payment method
    #[wasm_bindgen]
    pub fn method(&self) -> u8 {
        self.inner.method() as u8
    }

    /// Get amount
    #[wasm_bindgen]
    pub fn amount(&self) -> u64 {
        self.inner.amount()
    }

    /// Get currency
    #[wasm_bindgen]
    pub fn currency(&self) -> String {
        self.inner.currency().clone()
    }

    /// Get chain ID (for on-chain methods)
    #[wasm_bindgen]
    pub fn chain_id(&self) -> Option<u64> {
        self.inner.chain_id()
    }

    /// Get sequence number
    #[wasm_bindgen]
    pub fn seq(&self) -> u64 {
        self.inner.seq()
    }

    /// Get timestamp
    #[wasm_bindgen]
    pub fn timestamp(&self) -> u64 {
        self.inner.timestamp()
    }

    /// Verify the payment commitment signature
    #[wasm_bindgen]
    pub fn verify(&self) -> bool {
        self.inner.verify().is_ok()
    }

    /// Convert to JSON
    #[wasm_bindgen]
    pub fn to_json(&self) -> Result<String, WasmError> {
        #[derive(Serialize)]
        struct PaymentJson {
            relay_id: String,
            listener_id: String,
            stream_addr: String,
            method: u8,
            amount: u64,
            currency: String,
            chain_id: Option<u64>,
            seq: u64,
            timestamp: u64,
        }

        let json = PaymentJson {
            relay_id: hex::encode(self.inner.relay_id().as_bytes()),
            listener_id: hex::encode(self.inner.listener_id().as_bytes()),
            stream_addr: hex::encode(self.inner.stream_addr()),
            method: self.inner.method() as u8,
            amount: self.inner.amount(),
            currency: self.inner.currency().clone(),
            chain_id: self.inner.chain_id(),
            seq: self.inner.seq(),
            timestamp: self.inner.timestamp(),
        };

        serde_json::to_string_pretty(&json)
            .map_err(|e| WasmError::from(e.to_string()))
    }
}