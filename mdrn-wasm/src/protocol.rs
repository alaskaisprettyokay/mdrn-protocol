//! Protocol WASM bindings
//!
//! Provides browser-compatible message encoding/decoding for MDRN protocol.

use wasm_bindgen::prelude::*;
use mdrn_core::protocol::{Message, MessageType};
use mdrn_core::stream::StreamAnnouncement;
use mdrn_core::identity::Identity;
use serde::{Deserialize, Serialize};
use crate::WasmError;

/// WASM wrapper for MDRN protocol message
#[wasm_bindgen]
pub struct WasmMessage {
    inner: Message,
}

#[wasm_bindgen]
impl WasmMessage {
    /// Create a new message
    #[wasm_bindgen(constructor)]
    pub fn new(
        message_type: u8,
        sender_hex: &str,
        payload: &[u8],
    ) -> Result<WasmMessage, WasmError> {
        let sender = Identity::from_bytes(
            hex::decode(sender_hex)
                .map_err(|e| WasmError::from(format!("Invalid sender hex: {}", e)))?
        ).map_err(|e| WasmError::from(e.to_string()))?;

        let msg_type = MessageType::try_from(message_type)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let message = Message::new(msg_type, sender, payload.to_vec())
            .map_err(|e| WasmError::from(e.to_string()))?;

        Ok(WasmMessage { inner: message })
    }

    /// Encode message to CBOR bytes
    #[wasm_bindgen]
    pub fn to_cbor(&self) -> Result<Vec<u8>, WasmError> {
        self.inner.to_cbor()
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Decode message from CBOR bytes
    #[wasm_bindgen]
    pub fn from_cbor(bytes: &[u8]) -> Result<WasmMessage, WasmError> {
        let message = Message::from_cbor(bytes)
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmMessage { inner: message })
    }

    /// Get message type as number
    #[wasm_bindgen]
    pub fn message_type(&self) -> u8 {
        self.inner.message_type() as u8
    }

    /// Get sender identity as hex string
    #[wasm_bindgen]
    pub fn sender_hex(&self) -> String {
        hex::encode(self.inner.sender().as_bytes())
    }

    /// Get message payload bytes
    #[wasm_bindgen]
    pub fn payload(&self) -> Vec<u8> {
        self.inner.payload().clone()
    }

    /// Get message nonce bytes
    #[wasm_bindgen]
    pub fn nonce(&self) -> Vec<u8> {
        self.inner.nonce().to_vec()
    }

    /// Get message signature bytes
    #[wasm_bindgen]
    pub fn signature(&self) -> Vec<u8> {
        self.inner.signature().clone()
    }

    /// Verify the message signature
    #[wasm_bindgen]
    pub fn verify(&self) -> bool {
        self.inner.verify().is_ok()
    }

    /// Convert to JSON for debugging
    #[wasm_bindgen]
    pub fn to_json(&self) -> Result<String, WasmError> {
        #[derive(Serialize)]
        struct MessageJson {
            message_type: u8,
            sender: String,
            nonce: String,
            payload_size: usize,
            signature: String,
        }

        let json = MessageJson {
            message_type: self.inner.message_type() as u8,
            sender: hex::encode(self.inner.sender().as_bytes()),
            nonce: hex::encode(self.inner.nonce()),
            payload_size: self.inner.payload().len(),
            signature: hex::encode(self.inner.signature()),
        };

        serde_json::to_string_pretty(&json)
            .map_err(|e| WasmError::from(e.to_string()))
    }
}

/// WASM wrapper for stream announcement
#[wasm_bindgen]
pub struct WasmStreamAnnouncement {
    inner: StreamAnnouncement,
}

#[wasm_bindgen]
impl WasmStreamAnnouncement {
    /// Create a new stream announcement
    #[wasm_bindgen(constructor)]
    pub fn new(
        stream_id: &str,
        broadcaster_hex: &str,
        codec: u8,
        bitrate: u32,
        sample_rate: u32,
        channels: u8,
        encrypted: bool,
        vouch_cbor: &[u8],
    ) -> Result<WasmStreamAnnouncement, WasmError> {
        use mdrn_core::stream::Codec;
        use mdrn_core::identity::Vouch;

        let broadcaster = Identity::from_bytes(
            hex::decode(broadcaster_hex)
                .map_err(|e| WasmError::from(format!("Invalid broadcaster hex: {}", e)))?
        ).map_err(|e| WasmError::from(e.to_string()))?;

        let codec = Codec::try_from(codec)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let vouch = Vouch::from_cbor(vouch_cbor)
            .map_err(|e| WasmError::from(e.to_string()))?;

        let announcement = StreamAnnouncement::new(
            stream_id.to_string(),
            broadcaster,
            codec,
            bitrate,
            sample_rate,
            channels,
            encrypted,
            None, // price_min
            vouch,
            Vec::new(), // tags
        ).map_err(|e| WasmError::from(e.to_string()))?;

        Ok(WasmStreamAnnouncement { inner: announcement })
    }

    /// Encode to CBOR bytes
    #[wasm_bindgen]
    pub fn to_cbor(&self) -> Result<Vec<u8>, WasmError> {
        self.inner.to_cbor()
            .map_err(|e| WasmError::from(e.to_string()))
    }

    /// Decode from CBOR bytes
    #[wasm_bindgen]
    pub fn from_cbor(bytes: &[u8]) -> Result<WasmStreamAnnouncement, WasmError> {
        let announcement = StreamAnnouncement::from_cbor(bytes)
            .map_err(|e| WasmError::from(e.to_string()))?;
        Ok(WasmStreamAnnouncement { inner: announcement })
    }

    /// Get stream address (SHA-256 of broadcaster + stream_id)
    #[wasm_bindgen]
    pub fn stream_addr(&self) -> Vec<u8> {
        self.inner.stream_addr().to_vec()
    }

    /// Get stream address as hex string
    #[wasm_bindgen]
    pub fn stream_addr_hex(&self) -> String {
        hex::encode(self.inner.stream_addr())
    }

    /// Get stream ID
    #[wasm_bindgen]
    pub fn stream_id(&self) -> String {
        self.inner.stream_id().clone()
    }

    /// Get broadcaster identity as hex
    #[wasm_bindgen]
    pub fn broadcaster_hex(&self) -> String {
        hex::encode(self.inner.broadcaster().as_bytes())
    }

    /// Get codec type
    #[wasm_bindgen]
    pub fn codec(&self) -> u8 {
        self.inner.codec() as u8
    }

    /// Get bitrate in kbps
    #[wasm_bindgen]
    pub fn bitrate(&self) -> u32 {
        self.inner.bitrate()
    }

    /// Get sample rate in Hz
    #[wasm_bindgen]
    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    /// Get number of channels
    #[wasm_bindgen]
    pub fn channels(&self) -> u8 {
        self.inner.channels()
    }

    /// Check if stream is encrypted
    #[wasm_bindgen]
    pub fn encrypted(&self) -> bool {
        self.inner.encrypted()
    }

    /// Get tags as JSON array
    #[wasm_bindgen]
    pub fn tags_json(&self) -> String {
        serde_json::to_string(self.inner.tags()).unwrap_or_else(|_| "[]".to_string())
    }

    /// Convert to JSON for JavaScript consumption
    #[wasm_bindgen]
    pub fn to_json(&self) -> Result<String, WasmError> {
        #[derive(Serialize)]
        struct AnnouncementJson {
            stream_id: String,
            stream_addr: String,
            broadcaster: String,
            codec: u8,
            bitrate: u32,
            sample_rate: u32,
            channels: u8,
            encrypted: bool,
            tags: Vec<String>,
        }

        let json = AnnouncementJson {
            stream_id: self.inner.stream_id().clone(),
            stream_addr: hex::encode(self.inner.stream_addr()),
            broadcaster: hex::encode(self.inner.broadcaster().as_bytes()),
            codec: self.inner.codec() as u8,
            bitrate: self.inner.bitrate(),
            sample_rate: self.inner.sample_rate(),
            channels: self.inner.channels(),
            encrypted: self.inner.encrypted(),
            tags: self.inner.tags().clone(),
        };

        serde_json::to_string_pretty(&json)
            .map_err(|e| WasmError::from(e.to_string()))
    }
}