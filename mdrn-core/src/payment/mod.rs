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

pub use commitment::PaymentCommitment;
pub use method::PaymentMethod;
pub use receipt::PaymentReceipt;
