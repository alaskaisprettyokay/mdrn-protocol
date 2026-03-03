# MDRN — Massively Distributed Radio Network

## What This Is

MDRN is an encrypted peer-to-peer broadcast protocol for real-time audio distribution with integrated private payments and social trust-based admission. Apache-2.0 licensed. Clean-room implementation (we evaluated forking Streamr but their custom license blocks token/payment substitution).

## Architecture

Rust workspace with three crates:

- **mdrn-core**: Protocol library (identity, protocol messages, crypto, stream handling, payment, backchannel, transport)
- **mdrn-cli**: Command-line broadcaster/listener/relay tool (clap)
- **mdrn-relay**: Standalone relay node daemon

Plus a `contracts/` directory for Solidity settlement contracts (Base L2).

## Core Protocol Decisions (FINAL)

- **Wire encoding**: CBOR (RFC 8949), canonical encoding for signed messages
- **Identity**: Ed25519 + secp256k1 keypairs, multicodec-prefixed. Public key IS identity.
- **Admission**: Vouch system — broadcaster needs signed attestation from existing broadcaster. Genesis broadcasters bootstrap the network.
- **Transport encryption**: Noise Protocol Framework (IK for known peers, XX for dynamic discovery)
- **Stream encryption**: ChaCha20-Poly1305, HKDF-SHA256 key derivation, keys distributed via Noise channel during SUBSCRIBE
- **Audio codec**: Opus mandatory-to-implement (RFC 6716). FLAC and Codec2 optional.
- **Chunk format**: 20-60ms audio in CBOR-wrapped messages with seq numbers, timestamps, encryption nonces
- **DHT**: Kademlia (k=20, alpha=3, SHA-256 ID space) via libp2p-kad-dht
- **Fan-out**: gossipsub (one topic per stream)
- **Payment**: Listener pays relay. Signed cumulative CBOR commitments, lazy settlement on-chain. Payment-method-agnostic (FREE, EVM_L2, Lightning, Superfluid).
- **Backchannel**: E2E encrypted listener-to-broadcaster messages via ephemeral ECDH + ChaCha20-Poly1305
- **Transport**: libp2p primary (tcp, quic, yamux, noise, identify, autonat, relay, dcutr). WebRTC TBD for browsers.
- **Protocol ID**: `/mdrn/1.0.0`

## Message Types

| Code | Name | Description |
|------|------|-------------|
| 0x01 | ANNOUNCE | Broadcaster publishes stream metadata to DHT |
| 0x02 | WITHDRAW | Broadcaster removes stream from DHT |
| 0x10 | DISCOVER_REQ | Query DHT for active streams |
| 0x11 | DISCOVER_RES | DHT responds with matching streams/relays |
| 0x20 | SUBSCRIBE | Listener requests stream from relay |
| 0x21 | SUB_ACK | Relay accepts subscription |
| 0x22 | SUB_REJECT | Relay rejects subscription |
| 0x23 | UNSUBSCRIBE | Listener terminates subscription |
| 0x30 | CHUNK | Audio data chunk |
| 0x31 | CHUNK_ACK | Listener acknowledges receipt (optional QoS) |
| 0x40 | PAY_COMMIT | Listener sends payment commitment |
| 0x41 | PAY_RECEIPT | Relay acknowledges payment |
| 0x50 | BACK_MSG | Backchannel message (e2e encrypted) |
| 0x51 | BACK_ACK | Backchannel delivery ack |
| 0xF0 | PING | Keepalive |
| 0xF1 | PONG | Keepalive response |

## Message Envelope

```
Message = {
  version:  uint,        // 1 for this spec
  type:     uint,        // Message type code
  sender:   identity,    // Multicodec-prefixed public key
  nonce:    bytes(12),   // Unique per-message
  payload:  bytes,       // Type-specific CBOR
  sig:      bytes,       // Signature over (version || type || nonce || payload)
}
```

## Key Data Structures

### Identity Encoding
```
Ed25519:    0xED01 || <32-byte public key>  (34 bytes total)
secp256k1:  0xE701 || <33-byte compressed public key>  (35 bytes total)
```

### Vouch Credential
```
Vouch = {
  subject:    identity,     // New broadcaster
  issuer:     identity,     // Vouching broadcaster
  issued_at:  uint,         // Unix timestamp
  expires_at: uint | null,
  signature:  bytes,        // Over canonical CBOR of above
}
```

### Stream Announcement (DHT record)
```
StreamAnnouncement = {
  stream_addr:   bytes(32),    // SHA-256(broadcaster_identity || stream_id)
  broadcaster:   identity,
  stream_id:     text,
  codec:         uint,         // 1=Opus, 2=FLAC, 3=Codec2
  bitrate:       uint,         // kbps
  sample_rate:   uint,         // Hz
  channels:      uint,         // 1=mono, 2=stereo
  encrypted:     bool,
  price_min:     uint | null,
  vouch:         Vouch,
  tags:          [text],
  started_at:    uint,
  ttl:           uint,         // seconds (default 300)
}
```

### Relay Advertisement (DHT record)
```
RelayAdvertisement = {
  relay_id:        identity,
  stream_addr:     bytes(32),
  price_per_min:   uint,       // 0 = free
  payment_methods: [uint],
  capacity:        uint,
  latency_ms:      uint,
  endpoints:       [Endpoint],
  ttl:             uint,
}
```

### Audio Chunk
```
Chunk = {
  stream_addr:  bytes(32),
  seq:          uint,          // Monotonically increasing
  timestamp:    uint,          // Presentation timestamp (microseconds)
  codec:        uint,
  flags:        uint,          // 0x01=encrypted, 0x02=keyframe
  duration_us:  uint,
  data:         bytes,         // Encoded audio
  nonce:        bytes(12),     // Present only if encrypted
}
```

### Payment Commitment
```
PaymentCommitment = {
  relay_id:     identity,
  listener_id:  identity,
  stream_addr:  bytes(32),
  method:       uint,          // 0=FREE, 1=EVM_L2, 2=LIGHTNING, 3=SUPERFLUID
  amount:       uint,          // Cumulative, in base units
  currency:     text,          // "USDC", "BTC", etc.
  chain_id:     uint | null,
  seq:          uint,
  timestamp:    uint,
  signature:    bytes,
}
```

### Subscription Lifecycle State Machine
```
IDLE --[SUBSCRIBE]--> PENDING
  <--[SUB_REJECT]-- PENDING
PENDING --[SUB_ACK]--> ACTIVE
ACTIVE --[CHUNK]--> ACTIVE  (steady state)
ACTIVE --[UNSUBSCRIBE]--> CLOSING
ACTIVE --[timeout]--> CLOSING
CLOSING --[settled]--> IDLE
```

## Module Map (mdrn-core/src/)

- `identity/` — KeyType enum, Identity struct, multicodec encoding/decoding, Vouch struct, signature verification, genesis broadcaster list
- `protocol/` — MessageType enum, Message envelope, CBOR serialization/deserialization for all message types, canonical encoding
- `crypto/` — ChaCha20-Poly1305 stream encryption, HKDF key derivation, ephemeral ECDH for backchannel, stream key generation
- `stream/` — StreamAnnouncement, Chunk format, codec identifiers, subscription state machine
- `payment/` — PaymentCommitment, PaymentReceipt, payment method codes, commitment validation
- `backchannel/` — BackchannelMessage, payload types (TEXT, REACTION, TIP), encryption/decryption
- `transport/` — libp2p swarm configuration, gossipsub topic management, DHT record publishing/querying, relay behavior

## Build Phase Plan

**Phase 1 (Current — CLI Demo, 6-8 weeks):**
Two machines, encrypted audio through relay over libp2p. Prove it works.

**Phase 2 (Payment + Vouches, 3-4 weeks):**
Economic and admission layer on top of working transport.

**Phase 3 (Browser Client, 4-6 weeks):**
WASM + Web Audio, minimal web UI.

**Phase 4 (Production Hardening, 8-12 weeks):**
Edge cases, mobile, reconnection, NAT traversal testing.

## TBD Items

- Genesis broadcaster public keys
- Naming layer (ENS vs custom vs DNS TXT)
- WebRTC transport binding
- Raw QUIC transport binding
- Settlement contract ABI (Solidity on Base)
- Private payment integration (RAILGUN/Lightning)
- CDDL schemas (RFC 8610)
- Test vectors
- Bootstrap node addresses

## Context

Author: Christopher Rose <chris@mdrn.ai>
GitHub: alaskaisprettyokay
License: Apache-2.0
First client: Subcult (music streaming platform)
Relationship to Subcult: MDRN is standalone protocol infrastructure; Subcult is the first application built on it.