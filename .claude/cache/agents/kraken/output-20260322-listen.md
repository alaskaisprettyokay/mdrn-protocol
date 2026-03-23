# Implementation Report: Listen Command for MDRN CLI
Generated: 2026-03-22T17:00:00Z

## Task
Implement the `mdrn listen` command for the MDRN CLI, enabling stream reception, Opus decoding, and WAV file output with encryption support.

## TDD Summary

### Tests Written (18 total)
| Test | Description |
|------|-------------|
| `test_listen_accepts_64_char_hex_stream_addr` | Accepts valid 64-char hex stream address |
| `test_listen_accepts_0x_prefix` | Accepts 0x-prefixed hex address |
| `test_listen_rejects_invalid_hex` | Falls back to stream_id for invalid hex |
| `test_listen_rejects_wrong_length` | Falls back to stream_id for wrong length |
| `test_listen_treats_short_string_as_stream_id` | Short strings treated as stream_id |
| `test_listen_creates_output_file` | Creates output WAV file |
| `test_listen_output_file_is_wav_format` | Output is valid WAV |
| `test_listen_reads_hex_encoded_cbor_from_stdin` | Reads CBOR chunks from stdin |
| `test_opus_decoder_48khz_stereo` | Creates stereo Opus decoder |
| `test_opus_decoder_48khz_mono` | Creates mono Opus decoder |
| `test_opus_decode_silence_frame` | Handles minimal Opus frames |
| `test_opus_encode_decode_roundtrip` | Full encode/decode cycle |
| `test_listen_decodes_real_opus_chunk_from_stdin` | Decodes real Opus audio |
| `test_listen_handles_multiple_chunks` | Handles multi-chunk streams |
| `test_listen_full_pipeline_with_real_opus` | End-to-end CLI test |
| `test_listen_decodes_encrypted_chunk_with_key` | Decrypts with correct key |
| `test_listen_fails_encrypted_chunk_without_key` | Fails gracefully without key |
| `test_listen_fails_with_wrong_key` | Fails gracefully with wrong key |

### Implementation Files
| File | Changes |
|------|---------|
| `mdrn-cli/src/listen.rs` | New module (394 lines) - listen command implementation |
| `mdrn-cli/src/main.rs` | Updated Listen command handler (added ~60 lines) |
| `mdrn-cli/Cargo.toml` | Added futures and libp2p dependencies |
| `mdrn-cli/tests/listen_command.rs` | Added 6 new tests (~200 lines) |

## Test Results
```
running 18 tests
test test_listen_accepts_0x_prefix ... ok
test test_listen_decodes_real_opus_chunk_from_stdin ... ok
test test_listen_accepts_64_char_hex_stream_addr ... ok
test test_listen_output_file_is_wav_format ... ok
test test_listen_full_pipeline_with_real_opus ... ok
test test_opus_decode_silence_frame ... ok
test test_opus_decoder_48khz_mono ... ok
test test_opus_decoder_48khz_stereo ... ok
test test_opus_encode_decode_roundtrip ... ok
test test_listen_fails_encrypted_chunk_without_key ... ok
test test_listen_handles_multiple_chunks ... ok
test test_listen_fails_with_wrong_key ... ok
test test_listen_decodes_encrypted_chunk_with_key ... ok
test test_listen_creates_output_file ... ok
test test_listen_treats_short_string_as_stream_id ... ok
test test_listen_rejects_wrong_length ... ok
test test_listen_rejects_invalid_hex ... ok
test test_listen_reads_hex_encoded_cbor_from_stdin ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Changes Made

### 1. Created `mdrn-cli/src/listen.rs`
New module implementing:
- `ListenConfig` - Configuration struct for listen operation
- `ListenResult` - Result struct with statistics
- `ParsedAddress` - Enum for hex address vs stream_id
- `parse_stream_address()` - Parses CLI stream argument
- `run_listen_stdin()` - Stdin mode: reads hex CBOR chunks, decodes Opus, writes WAV
- `run_listen_network()` - Network mode: subscribes to gossipsub, receives chunks
- `write_wav_file()` - Writes PCM samples to WAV format
- `lookup_stream_announcement()` - DHT lookup (for future use)

### 2. Updated `mdrn-cli/src/main.rs`
- Added `mod listen` import
- Updated `Commands::Listen` to include `--key` and `--network` flags
- Implemented listen command handler with:
  - Stream address parsing
  - Stream key parsing (hex to 32-byte array)
  - Stdin mode vs network mode dispatch
  - Result output to stdout

### 3. Updated `mdrn-cli/Cargo.toml`
Added dependencies:
```toml
futures.workspace = true
libp2p.workspace = true
```

### 4. Enhanced `mdrn-cli/tests/listen_command.rs`
Added comprehensive tests for:
- Opus encode/decode roundtrip
- Real Opus chunk decoding from stdin
- Multiple chunk handling
- Encrypted stream with correct key
- Encrypted stream without key (graceful failure)
- Encrypted stream with wrong key (graceful failure)

## Usage Examples

### Stdin Mode (Default)
```bash
# Listen to stream from stdin (hex-encoded CBOR chunks)
cat chunks.hex | mdrn listen <stream_addr> --output output.wav

# With encryption key
cat chunks.hex | mdrn listen <stream_addr> -o output.wav -k <32_byte_hex_key>
```

### Network Mode
```bash
# Listen via libp2p network
mdrn listen <stream_addr> --network --output output.wav

# Encrypted stream on network
mdrn listen <stream_addr> -n -o output.wav -k <stream_key_hex>
```

## Architecture

```
CLI Arguments
    |
    v
parse_stream_address() --> Hex([u8; 32]) or StreamId(String)
    |
    v
[stdin mode]          [network mode]
run_listen_stdin()    run_listen_network()
    |                     |
    +---------------------+
    v
For each chunk:
  1. Parse CBOR
  2. Verify stream_addr matches
  3. Decrypt if encrypted (ChaCha20-Poly1305)
  4. Decode Opus (48kHz, stereo)
  5. Accumulate PCM samples
    |
    v
write_wav_file() --> output.wav
```

## Notes

### Stream ID Lookup
Stream ID lookup (non-hex strings) requires DHT network access. Currently returns an error message directing users to use hex addresses. The infrastructure for DHT lookup exists in `lookup_stream_announcement()` but requires connected peers.

### Network Mode Limitations
Network mode uses a 10-second timeout and requires connected peers to receive gossipsub messages. For local testing, stdin mode is recommended.

### Encrypted Streams
- Uses ChaCha20-Poly1305 (same as broadcast)
- Key must be provided via `--key` flag (64 hex chars = 32 bytes)
- Graceful failure on missing/wrong key (logs warning, skips chunk)

### WAV Output
- 16-bit PCM
- 48kHz sample rate (matches Opus)
- Stereo by default (mono support exists but defaults to stereo without announcement)

## Follow-up Items
1. Add `--mono` flag for mono output
2. Implement stream ID lookup via DHT when network mode is active
3. Add audio playback support (speakers) in addition to file output
4. Add reconnection/retry logic for network mode
