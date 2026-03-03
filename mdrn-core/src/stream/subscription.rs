//! Subscription lifecycle state machine

use serde::{Deserialize, Serialize};

/// Subscription state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionState {
    /// No active subscription
    Idle,
    /// SUBSCRIBE sent, waiting for SUB_ACK or SUB_REJECT
    Pending,
    /// Subscription active, receiving CHUNKs
    Active,
    /// UNSUBSCRIBE sent or timeout, waiting for settlement
    Closing,
}

impl SubscriptionState {
    /// Transition on SUBSCRIBE sent
    pub fn on_subscribe(self) -> Option<Self> {
        match self {
            Self::Idle => Some(Self::Pending),
            _ => None,
        }
    }

    /// Transition on SUB_ACK received
    pub fn on_sub_ack(self) -> Option<Self> {
        match self {
            Self::Pending => Some(Self::Active),
            _ => None,
        }
    }

    /// Transition on SUB_REJECT received
    pub fn on_sub_reject(self) -> Option<Self> {
        match self {
            Self::Pending => Some(Self::Idle),
            _ => None,
        }
    }

    /// Transition on UNSUBSCRIBE sent or timeout
    pub fn on_unsubscribe(self) -> Option<Self> {
        match self {
            Self::Active => Some(Self::Closing),
            _ => None,
        }
    }

    /// Transition on settlement complete
    pub fn on_settled(self) -> Option<Self> {
        match self {
            Self::Closing => Some(Self::Idle),
            _ => None,
        }
    }

    /// Check if in a state where chunks can be received
    pub fn can_receive_chunks(&self) -> bool {
        matches!(self, Self::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_lifecycle() {
        let mut state = SubscriptionState::Idle;

        // IDLE -> PENDING
        state = state.on_subscribe().unwrap();
        assert_eq!(state, SubscriptionState::Pending);

        // PENDING -> ACTIVE
        state = state.on_sub_ack().unwrap();
        assert_eq!(state, SubscriptionState::Active);
        assert!(state.can_receive_chunks());

        // ACTIVE -> CLOSING
        state = state.on_unsubscribe().unwrap();
        assert_eq!(state, SubscriptionState::Closing);

        // CLOSING -> IDLE
        state = state.on_settled().unwrap();
        assert_eq!(state, SubscriptionState::Idle);
    }

    #[test]
    fn test_subscription_reject() {
        let mut state = SubscriptionState::Idle;
        state = state.on_subscribe().unwrap();
        state = state.on_sub_reject().unwrap();
        assert_eq!(state, SubscriptionState::Idle);
    }
}
