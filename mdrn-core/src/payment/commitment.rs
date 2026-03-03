//! Payment commitment

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::{Identity, Keypair};

use super::PaymentMethod;

/// Payment commitment errors
#[derive(Debug, Error)]
pub enum CommitmentError {
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("serialization failed: {0}")]
    SerializationFailed(String),
    #[error("sequence number must increase")]
    InvalidSequence,
    #[error("amount must increase (cumulative)")]
    AmountNotCumulative,
}

/// Payment commitment (cumulative, signed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentCommitment {
    /// Relay being paid
    pub relay_id: Identity,
    /// Listener making payment
    pub listener_id: Identity,
    /// Stream address
    #[serde(with = "serde_bytes")]
    pub stream_addr: [u8; 32],
    /// Payment method
    pub method: PaymentMethod,
    /// Cumulative amount in base units
    pub amount: u64,
    /// Currency code (e.g., "USDC", "BTC")
    pub currency: String,
    /// Chain ID (for EVM methods)
    pub chain_id: Option<u64>,
    /// Sequence number (must increase)
    pub seq: u64,
    /// Unix timestamp
    pub timestamp: u64,
    /// Signature over commitment data
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

impl PaymentCommitment {
    /// Create and sign a new payment commitment
    pub fn create(
        relay_id: Identity,
        listener_keypair: &Keypair,
        stream_addr: [u8; 32],
        method: PaymentMethod,
        amount: u64,
        currency: String,
        chain_id: Option<u64>,
        seq: u64,
    ) -> Result<Self, CommitmentError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut commitment = Self {
            relay_id,
            listener_id: listener_keypair.identity().clone(),
            stream_addr,
            method,
            amount,
            currency,
            chain_id,
            seq,
            timestamp,
            signature: Vec::new(),
        };

        let sign_data = commitment.signing_data()?;
        commitment.signature = listener_keypair.sign(&sign_data);

        Ok(commitment)
    }

    /// Get the data to sign
    fn signing_data(&self) -> Result<Vec<u8>, CommitmentError> {
        #[derive(Serialize)]
        struct Signable<'a> {
            relay_id: &'a Identity,
            listener_id: &'a Identity,
            #[serde(with = "serde_bytes")]
            stream_addr: &'a [u8; 32],
            method: PaymentMethod,
            amount: u64,
            currency: &'a str,
            chain_id: Option<u64>,
            seq: u64,
            timestamp: u64,
        }

        let signable = Signable {
            relay_id: &self.relay_id,
            listener_id: &self.listener_id,
            stream_addr: &self.stream_addr,
            method: self.method,
            amount: self.amount,
            currency: &self.currency,
            chain_id: self.chain_id,
            seq: self.seq,
            timestamp: self.timestamp,
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&signable, &mut buf)
            .map_err(|e| CommitmentError::SerializationFailed(e.to_string()))?;
        Ok(buf)
    }

    /// Verify signature
    pub fn verify_signature(&self) -> Result<(), CommitmentError> {
        let sign_data = self.signing_data()?;
        self.listener_id
            .verify(&sign_data, &self.signature)
            .map_err(|_| CommitmentError::InvalidSignature)
    }

    /// Validate that this commitment supersedes a previous one
    pub fn validate_supersedes(&self, previous: &PaymentCommitment) -> Result<(), CommitmentError> {
        if self.seq <= previous.seq {
            return Err(CommitmentError::InvalidSequence);
        }
        if self.amount < previous.amount {
            return Err(CommitmentError::AmountNotCumulative);
        }
        Ok(())
    }
}
