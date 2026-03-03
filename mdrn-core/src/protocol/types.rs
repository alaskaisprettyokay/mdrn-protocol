//! Protocol message type codes

use serde_repr::{Deserialize_repr, Serialize_repr};

/// Message type codes as defined in the protocol spec
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum MessageType {
    // Stream announcement
    Announce = 0x01,
    Withdraw = 0x02,

    // Discovery
    DiscoverReq = 0x10,
    DiscoverRes = 0x11,

    // Subscription
    Subscribe = 0x20,
    SubAck = 0x21,
    SubReject = 0x22,
    Unsubscribe = 0x23,

    // Audio chunks
    Chunk = 0x30,
    ChunkAck = 0x31,

    // Payment
    PayCommit = 0x40,
    PayReceipt = 0x41,

    // Backchannel
    BackMsg = 0x50,
    BackAck = 0x51,

    // Keepalive
    Ping = 0xF0,
    Pong = 0xF1,
}

impl MessageType {
    /// Get the message type from a u8 code
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0x01 => Some(Self::Announce),
            0x02 => Some(Self::Withdraw),
            0x10 => Some(Self::DiscoverReq),
            0x11 => Some(Self::DiscoverRes),
            0x20 => Some(Self::Subscribe),
            0x21 => Some(Self::SubAck),
            0x22 => Some(Self::SubReject),
            0x23 => Some(Self::Unsubscribe),
            0x30 => Some(Self::Chunk),
            0x31 => Some(Self::ChunkAck),
            0x40 => Some(Self::PayCommit),
            0x41 => Some(Self::PayReceipt),
            0x50 => Some(Self::BackMsg),
            0x51 => Some(Self::BackAck),
            0xF0 => Some(Self::Ping),
            0xF1 => Some(Self::Pong),
            _ => None,
        }
    }

    /// Get the u8 code for this message type
    pub fn code(&self) -> u8 {
        *self as u8
    }
}
