# Kraken: Listen Command Implementation

## Task
Implement the `mdrn listen` command for the MDRN CLI using TDD approach.

## Checkpoints
**Task:** Implement listen command with stream discovery, chunk reception, Opus decoding, and WAV output
**Started:** 2026-03-22T16:30:00Z
**Last Updated:** 2026-03-22T17:00:00Z

### Phase Status
- Phase 1 (Tests Written): VALIDATED (18 tests)
- Phase 2 (Implementation): VALIDATED (all tests green)
- Phase 3 (Refactoring): VALIDATED (clean code, no warnings)
- Phase 4 (Documentation): VALIDATED (module docs added)

### Validation State
```json
{
  "test_count": 18,
  "tests_passing": 18,
  "tests_failing": 0,
  "files_modified": [
    "mdrn-cli/src/listen.rs",
    "mdrn-cli/src/main.rs",
    "mdrn-cli/Cargo.toml",
    "mdrn-cli/tests/listen_command.rs"
  ],
  "last_test_command": "cargo test --package mdrn-cli --test listen_command",
  "last_test_exit_code": 0
}
```

### Resume Context
- Current focus: Implementation complete
- Next action: None - task finished
- Blockers: None

## Implementation Summary

### Phase 1: Stream Address Parsing + Stdin Mode (MVP)
- Created `mdrn-cli/src/listen.rs` module
- Implemented `parse_stream_address()` - handles hex and 0x-prefixed addresses
- Implemented `run_listen_stdin()` - reads hex-encoded CBOR chunks from stdin
- Implemented Opus decoding with proper stereo support
- Implemented `write_wav_file()` for WAV output

### Phase 2: Network Mode
- Implemented `run_listen_network()` with gossipsub subscription
- Uses MdrnSwarm with temporary keypair for listening
- Handles gossipsub message events for chunk reception
- 10-second timeout for network listening

### Phase 3: Encryption Support
- Full encryption support with `--key` flag
- Decrypts ChaCha20-Poly1305 encrypted chunks
- Proper error handling for missing key or wrong key

### Tests Added
- Stream address parsing (hex, 0x prefix, invalid)
- Opus encode/decode roundtrip
- Single chunk decoding
- Multiple chunk handling
- Encrypted chunk with correct key
- Encrypted chunk without key (fails gracefully)
- Encrypted chunk with wrong key (fails gracefully)
