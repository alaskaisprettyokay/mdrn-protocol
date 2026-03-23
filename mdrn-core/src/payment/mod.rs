//! Payment module
//!
//! Handles:
//! - PaymentCommitment (signed cumulative commitments)
//! - PaymentReceipt
//! - Payment method codes
//! - Commitment validation

mod commitment;
mod method;
mod receipt;
mod settlement;

pub use commitment::PaymentCommitment;
pub use method::PaymentMethod;
pub use receipt::PaymentReceipt;
pub use settlement::{SettlementContract, SettlementError, SettlementResult};
