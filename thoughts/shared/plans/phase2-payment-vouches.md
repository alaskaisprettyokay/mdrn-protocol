# Phase 2 Implementation Plan: Payment + Vouches

Generated: 2026-03-23
Author: Development Team
Status: Planning

## Overview

Build economic and admission layer on top of Phase 1 working transport.
Support both production payment mode and free testnet for development.

## Core Requirements

### Payment System
- **Paid Mode**: Relay operators charge for bandwidth usage
- **Free Mode**: Testnet operation for development/testing
- **Metering**: Track bandwidth usage and enforce limits
- **Settlement**: Off-chain commitments with on-chain settlement

### Vouch System
- **Genesis Bootstrapping**: Hardcoded initial trust anchors
- **Admission Control**: Relay nodes check vouch credentials
- **Social Trust**: Vouch chain verification and propagation

### Backward Compatibility
- Existing CLI commands continue working in free mode
- Optional payment flags for production deployment
- Seamless upgrade path from testnet to mainnet

---

## Technical Architecture

### Payment Flow
```
Listener ──► PaymentCommitment ──► Relay ──► BandwidthAccounting ──► Settlement
    │                                │
    └─── StreamChunks ◄──────────────┘
```

### Vouch Verification
```
Broadcaster ──► VouchCredential ──► Relay ──► TrustChainCheck ──► Admission
     │                                │
     └─── StreamAnnouncement ──────────┘
```

### Mode Detection
```rust
pub enum NetworkMode {
    Testnet,    // Free, no payments required
    Mainnet,    // Paid, vouch verification enforced
}
```

---

## Implementation Phases

### Phase 2.1: Payment Foundation (Week 1-2)

**Goal**: Basic payment tracking without enforcement

#### Core Types
```rust
// Already exists in mdrn-core/src/payment/
pub struct PaymentCommitment {
    relay_id: Identity,
    listener_id: Identity,
    stream_addr: [u8; 32],
    method: PaymentMethod,
    amount: u64,        // Cumulative, in base units
    currency: String,   // "USDC", "BTC", etc.
    seq: u64,
    timestamp: u64,
    signature: Vec<u8>,
}

pub enum PaymentMethod {
    Free,              // Testnet mode
    EvmL2,             // Base L2 USDC
    Lightning,         // Bitcoin Lightning
    Superfluid,        // Streaming payments
}
```

#### CLI Updates
```bash
# Relay pricing
mdrn relay --port 9000 --price-per-mb 100 --currency USDC --mode mainnet
mdrn relay --port 9000 --mode testnet  # Free mode

# Listener payment
mdrn listen --network --payment-method evm-l2 --max-spend 1000 stream-addr
mdrn listen --network --testnet stream-addr  # Free mode
```

#### Implementation Tasks
1. Add payment tracking to RelayNode
2. Implement PaymentCommitment creation/verification
3. Add CLI flags for payment configuration
4. Bandwidth metering in relay event loop

### Phase 2.2: Vouch Integration (Week 3-4)

**Goal**: Social trust admission control

#### Core Types
```rust
// Already exists in mdrn-core/src/identity/
pub struct Vouch {
    subject: Identity,
    issuer: Identity,
    issued_at: u64,
    expires_at: Option<u64>,
    signature: Vec<u8>,
}

pub struct TrustChain {
    vouches: Vec<Vouch>,
    genesis_keys: Vec<Identity>,
}
```

#### Verification Logic
```rust
impl RelayNode {
    async fn verify_broadcaster_admission(&self, announcement: &StreamAnnouncement) -> bool {
        match self.mode {
            NetworkMode::Testnet => true,  // Always allow in testnet
            NetworkMode::Mainnet => {
                // Verify vouch chain back to genesis
                self.verify_trust_chain(&announcement.vouch).await
            }
        }
    }
}
```

#### Implementation Tasks
1. Add genesis broadcaster keys to config
2. Implement trust chain verification
3. Add vouch checking to relay admission
4. DHT-based vouch discovery

### Phase 2.3: Economic Enforcement (Week 5-6)

**Goal**: Working payment enforcement and settlement

#### Payment Enforcement
```rust
impl RelayNode {
    async fn enforce_payment_limits(&mut self, listener_id: &Identity) -> bool {
        match self.mode {
            NetworkMode::Testnet => true,  // No limits in testnet
            NetworkMode::Mainnet => {
                let usage = self.get_listener_usage(listener_id);
                let payments = self.get_listener_payments(listener_id);
                payments.amount >= usage.required_payment()
            }
        }
    }
}
```

#### Implementation Tasks
1. Payment commitment verification
2. Bandwidth accounting and enforcement
3. Rate limiting based on payment balance
4. Settlement contract integration (Base L2)

---

## Configuration Design

### Network Mode Detection
```rust
// In mdrn-core/src/transport/config.rs
#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub network_mode: NetworkMode,
    pub genesis_keys: Vec<Identity>,
    pub payment_config: Option<PaymentConfig>,
    // ... existing fields
}

#[derive(Debug, Clone)]
pub struct PaymentConfig {
    pub method: PaymentMethod,
    pub currency: String,
    pub price_per_mb: u64,
    pub settlement_contract: Option<String>,  // Contract address
}
```

### CLI Configuration
```bash
# Testnet mode (default for development)
export MDRN_NETWORK=testnet
mdrn relay --port 9000
mdrn listen stream-addr --network
mdrn broadcast --network stream-id --input audio.wav

# Mainnet mode (production)
export MDRN_NETWORK=mainnet
mdrn relay --port 9000 --price-per-mb 100 --currency USDC
mdrn listen stream-addr --network --payment-method evm-l2 --max-spend 1000
mdrn broadcast --network stream-id --input audio.wav --vouch vouch.cbor
```

---

## Genesis Bootstrapping

### Initial Trust Anchors
```rust
// Hardcoded genesis broadcaster keys for mainnet
pub const GENESIS_BROADCASTERS: &[&str] = &[
    "ed01_alice_key_hex...",    // Founder key
    "ed01_bob_key_hex...",      // Early adopter
    "ed01_subcult_key_hex...",  // First client
];

// Testnet genesis (development keys)
pub const TESTNET_GENESIS: &[&str] = &[
    "ed01_test_key_1...",
    "ed01_test_key_2...",
];
```

### Self-Vouch for Genesis
```rust
impl Keypair {
    pub fn create_genesis_vouch(&self) -> Result<Vouch> {
        // Genesis broadcasters can vouch for themselves
        Vouch::create(self.identity().clone(), self, None)
    }
}
```

---

## Success Criteria

### Phase 2.1 Complete
- [ ] Payment tracking works in relay
- [ ] CLI accepts payment configuration flags
- [ ] Testnet mode bypasses all payment checks
- [ ] Bandwidth metering is accurate

### Phase 2.2 Complete
- [ ] Vouch verification blocks invalid broadcasters in mainnet
- [ ] Genesis self-vouching works
- [ ] Trust chain verification is correct
- [ ] Testnet mode bypasses vouch checks

### Phase 2.3 Complete
- [ ] Payment enforcement prevents unpaid usage
- [ ] Rate limiting works based on payment balance
- [ ] Settlement contracts handle payment finalization
- [ ] Full economic flow works end-to-end

### Overall Phase 2 Success
- [ ] Relay operators can earn revenue in mainnet mode
- [ ] Developers can test freely in testnet mode
- [ ] Admission system prevents abuse
- [ ] Payment flows work with multiple methods
- [ ] Trust network bootstraps from genesis keys

---

## Next Steps

1. **Start with Phase 2.1**: Add basic payment tracking
2. **Implement mode detection**: Testnet vs mainnet configuration
3. **Add CLI flags**: Payment and mode configuration
4. **Test both modes**: Ensure testnet stays free, mainnet requires payment

Ready to begin implementation! 🚀