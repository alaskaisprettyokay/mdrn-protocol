//! WASM bindings for MDRN protocol
//!
//! Provides JavaScript bindings for:
//! - Identity management (keypair generation, signing)
//! - Crypto operations (ChaCha20 encryption, HKDF key derivation)
//! - Protocol messages (CBOR encoding/decoding)
//! - Stream handling (chunk creation, parsing)

use wasm_bindgen::prelude::*;
use console_error_panic_hook;

// Import modules
mod identity;
mod crypto;
mod protocol;
mod stream;

// Re-export main types for JavaScript
pub use identity::{WasmKeypair, WasmIdentity};
pub use crypto::{encrypt_chunk, decrypt_chunk, derive_key};
pub use protocol::{WasmMessage, WasmStreamAnnouncement};
pub use stream::{WasmChunk, WasmPaymentCommitment};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global allocator
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// Initialize the WASM module
#[wasm_bindgen(start)]
pub fn init() {
    // Set up panic hook for better error messages
    console_error_panic_hook::set_once();
}

/// Log a message to browser console (for debugging)
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[allow(unused_macros)]
macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

// Utility functions for JavaScript interop
#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen]
pub fn get_description() -> String {
    "MDRN WASM bindings for browser clients".to_string()
}

// Error handling for JavaScript
#[wasm_bindgen]
pub struct WasmError {
    message: String,
}

#[wasm_bindgen]
impl WasmError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

impl From<anyhow::Error> for WasmError {
    fn from(err: anyhow::Error) -> Self {
        WasmError {
            message: err.to_string(),
        }
    }
}

impl From<String> for WasmError {
    fn from(message: String) -> Self {
        WasmError { message }
    }
}

impl From<&str> for WasmError {
    fn from(message: &str) -> Self {
        WasmError {
            message: message.to_string(),
        }
    }
}