# MDRN Settlement Contracts

Solidity smart contracts for on-chain payment settlement on Base L2.

## Overview

These contracts handle:
- Payment channel settlement (cumulative signed commitments)
- Dispute resolution
- Relay registration and staking (TBD)

## Status

**TBD** — Contract ABI and implementation to be designed.

## Target Chain

- **Base L2** (Ethereum L2)

## Development

Contracts will be developed using Foundry.

```bash
# Install foundry
curl -L https://foundry.paradigm.xyz | bash
foundryup

# Build
forge build

# Test
forge test
```
