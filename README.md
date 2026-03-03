# MDRN — Massively Distributed Radio Network

An encrypted peer-to-peer broadcast protocol for real-time audio distribution with integrated private payments and social trust-based admission.

## Overview

MDRN enables:
- **Encrypted live audio streaming** via peer-to-peer relay networks
- **Payment integration** for monetized streams (EVM L2, Lightning, Superfluid)
- **Vouch-based admission** — broadcasters must be vouched for by existing broadcasters
- **Backchannel messaging** — E2E encrypted listener-to-broadcaster communication

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ Broadcaster │────▶│    Relay    │────▶│  Listener   │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       │                   │                   │
       ▼                   ▼                   ▼
   ┌───────────────────────────────────────────────┐
   │              Kademlia DHT + gossipsub         │
   └───────────────────────────────────────────────┘
```

### Crates

- **mdrn-core**: Protocol library (identity, messages, crypto, streams, payments, transport)
- **mdrn-cli**: Command-line broadcaster/listener/relay tool
- **mdrn-relay**: Standalone relay node daemon

## Protocol Highlights

| Component | Technology |
|-----------|------------|
| Wire encoding | CBOR (RFC 8949) |
| Identity | Ed25519 + secp256k1, multicodec-prefixed |
| Transport encryption | Noise Protocol Framework |
| Stream encryption | ChaCha20-Poly1305, HKDF-SHA256 |
| Audio codec | Opus (mandatory), FLAC/Codec2 (optional) |
| DHT | Kademlia (k=20, alpha=3) via libp2p |
| Fan-out | gossipsub (one topic per stream) |
| Payment | Signed cumulative commitments, lazy settlement |

## Building

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Build release
cargo build --release
```

## CLI Usage

```bash
# Generate a keypair
mdrn keygen -o my-key.json

# Start broadcasting
mdrn broadcast --stream-id "my-stream" --encrypted

# Listen to a stream
mdrn listen <stream-addr>

# Run as relay
mdrn relay --port 9000

# Discover streams
mdrn discover --tag music
```

## Development Status

**Phase 1 (Current)**: CLI demo — encrypted audio through relay over libp2p.

See `CLAUDE.md` for full protocol specification.

## License

Apache-2.0

## Author

Christopher Rose <chris@mdrn.ai>
