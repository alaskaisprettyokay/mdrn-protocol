# MDRN Listen Command Test Plan

This document outlines TDD test cases for the `mdrn listen` CLI command implementation.

## Overview

The `listen` command connects to an MDRN stream and plays audio to the local output device. It orchestrates multiple subsystems:

1. **Stream Discovery**: Find stream via DHT (by `stream_addr` or `stream_id`)
2. **Relay Selection**: Query DHT for relays serving the stream
3. **Subscription**: SUBSCRIBE → SUB_ACK handshake with relay
4. **Chunk Reception**: Receive encrypted chunks via gossipsub
5. **Decryption**: ChaCha20-Poly1305 decryption of audio data
6. **Audio Decoding**: Opus decoding to PCM
7. **Playback**: Output to audio device
8. **Payment**: Periodic PAY_COMMIT messages to relay

CLI signature:
```
mdrn listen <stream> [--output <device>]
```

Where `<stream>` is either:
- 64-character hex `stream_addr`
- Human-readable `stream_id` (requires broadcaster lookup)

---

## 1. Stream Resolution Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_parse_stream_addr_hex` | Parses 64-char hex as stream_addr | - |
| `test_parse_stream_addr_hex_with_0x_prefix` | Accepts `0x` prefix | - |
| `test_parse_stream_addr_invalid_hex` | Rejects non-hex characters | Uppercase, mixed case |
| `test_parse_stream_addr_wrong_length` | Rejects != 64 chars | 63 chars, 65 chars |
| `test_parse_stream_id_fallback` | Non-hex treated as stream_id | - |
| `test_stream_id_to_addr_requires_broadcaster` | Cannot resolve without broadcaster identity | - |

### Integration Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_resolve_stream_id_via_dht` | Queries DHT for matching stream_id | - |
| `test_resolve_stream_id_not_found` | Returns error when stream doesn't exist | - |
| `test_resolve_stream_id_multiple_matches` | Handles multiple streams with same ID | - |
| `test_stream_addr_dht_lookup` | Direct lookup by stream_addr | - |
| `test_stream_announcement_validation` | Verifies vouch on retrieved announcement | Invalid vouch |

### Suggested Structure

```rust
#[cfg(test)]
mod stream_resolution_tests {
    use super::*;

    #[test]
    fn test_parse_stream_addr_hex() {
        let input = "ab".repeat(32); // 64-char hex
        let result = parse_stream_input(&input);
        assert!(matches!(result, StreamInput::Address(_)));
    }

    #[test]
    fn test_parse_stream_id_fallback() {
        let input = "my-cool-stream";
        let result = parse_stream_input(input);
        assert!(matches!(result, StreamInput::StreamId(_)));
    }

    #[test]
    fn test_parse_stream_addr_invalid_hex() {
        let input = "zz".repeat(32); // Invalid hex
        let result = parse_stream_input(&input);
        // Should fall back to stream_id, not panic
        assert!(matches!(result, StreamInput::StreamId(_)));
    }
}
```

---

## 2. Relay Discovery Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_relay_advertisement_deserialization` | Parse CBOR RelayAdvertisement | - |
| `test_relay_advertisement_cbor_roundtrip` | Serialize/deserialize preserves all fields | - |
| `test_relay_is_free_check` | `is_free()` returns true for price=0 | - |
| `test_relay_is_free_with_free_method` | `is_free()` for PaymentMethod::Free | - |
| `test_relay_endpoint_multiaddr_valid` | Validates endpoint multiaddr format | Invalid addr |

### Integration Tests (DHT)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_query_relays_for_stream` | DHT query returns relay advertisements | - |
| `test_query_relays_empty_result` | Handles no relays for stream | - |
| `test_query_relays_multiple` | Returns multiple relays | 1, 5, 20 relays |
| `test_query_relays_expired_filtered` | Expired advertisements not returned | TTL=0 |
| `test_relay_selection_by_price` | Can filter/sort relays by price | - |
| `test_relay_selection_by_latency` | Can filter/sort by latency_ms | - |
| `test_relay_selection_by_payment_method` | Filter by supported payment methods | - |

### Relay Selection Algorithm Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_select_relay_prefers_lowest_price` | Chooses cheapest relay | - |
| `test_select_relay_prefers_lowest_latency_tie` | Latency breaks price tie | - |
| `test_select_relay_filters_unsupported_payment` | Excludes incompatible methods | - |
| `test_select_relay_with_capacity_check` | Respects relay capacity limits | capacity=0 |
| `test_select_relay_random_on_equal` | Randomizes among equivalent relays | - |

### Suggested Structure

```rust
#[cfg(test)]
mod relay_discovery_tests {
    use crate::stream::RelayAdvertisement;
    use crate::payment::PaymentMethod;

    fn test_relay(price: u64, latency: u32) -> RelayAdvertisement {
        RelayAdvertisement {
            relay_id: test_identity(),
            stream_addr: [0x01; 32],
            price_per_min: price,
            payment_methods: vec![PaymentMethod::Free],
            capacity: 100,
            latency_ms: latency,
            endpoints: vec![test_endpoint()],
            ttl: 300,
        }
    }

    #[test]
    fn test_select_relay_prefers_lowest_price() {
        let relays = vec![
            test_relay(100, 50),
            test_relay(50, 100),  // Cheaper, higher latency
            test_relay(200, 25),
        ];

        let selected = select_best_relay(&relays, &SelectionCriteria::default());
        assert_eq!(selected.unwrap().price_per_min, 50);
    }
}
```

---

## 3. Subscription State Machine Tests

### Unit Tests (Already Exist - Extend)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_subscription_lifecycle` | IDLE→PENDING→ACTIVE→CLOSING→IDLE | Already implemented |
| `test_subscription_reject` | IDLE→PENDING→IDLE on reject | Already implemented |
| `test_invalid_transition_idle_to_active` | Cannot skip PENDING | - |
| `test_invalid_transition_pending_to_closing` | Cannot skip ACTIVE | - |
| `test_invalid_transition_closing_to_active` | Cannot go backwards | - |
| `test_can_receive_chunks_only_active` | `can_receive_chunks()` only true in ACTIVE | All states |
| `test_timeout_transition` | ACTIVE→CLOSING on timeout | - |

### SUBSCRIBE Message Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_subscribe_message_creation` | Creates valid SUBSCRIBE payload | - |
| `test_subscribe_message_cbor_format` | Correct CBOR structure | - |
| `test_subscribe_message_signature_valid` | Signature verifies | - |
| `test_subscribe_includes_payment_method` | Contains listener's payment capability | - |
| `test_subscribe_includes_listener_identity` | Contains listener public key | - |

### SUB_ACK Message Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_sub_ack_contains_stream_key` | Encrypted stream key in payload | - |
| `test_sub_ack_signature_verifies` | Relay signature is valid | - |
| `test_sub_ack_decrypts_stream_key` | Can decrypt key using Noise channel | - |
| `test_sub_ack_wrong_relay_rejected` | Reject if sender != expected relay | - |

### SUB_REJECT Message Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_sub_reject_parse_reason` | Extracts rejection reason | - |
| `test_sub_reject_reason_capacity` | "CAPACITY_FULL" reason | - |
| `test_sub_reject_reason_payment` | "PAYMENT_REQUIRED" reason | - |
| `test_sub_reject_reason_unauthorized` | "UNAUTHORIZED" reason | - |

### Integration Tests (Two-Node)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_subscribe_ack_flow` | Full SUBSCRIBE→SUB_ACK handshake | - |
| `test_subscribe_reject_flow` | SUBSCRIBE→SUB_REJECT handling | - |
| `test_subscribe_timeout` | No response within timeout | - |
| `test_subscribe_retry_after_reject` | Can retry subscription | - |
| `test_subscribe_idempotent` | Double SUBSCRIBE doesn't break state | - |

### Suggested Structure

```rust
#[cfg(test)]
mod subscription_tests {
    use crate::stream::SubscriptionState;
    use crate::protocol::{Message, MessageType};

    #[test]
    fn test_invalid_transition_idle_to_active() {
        let state = SubscriptionState::Idle;
        assert!(state.on_sub_ack().is_none()); // Can't ACK from IDLE
    }

    #[test]
    fn test_can_receive_chunks_all_states() {
        assert!(!SubscriptionState::Idle.can_receive_chunks());
        assert!(!SubscriptionState::Pending.can_receive_chunks());
        assert!(SubscriptionState::Active.can_receive_chunks());
        assert!(!SubscriptionState::Closing.can_receive_chunks());
    }

    #[tokio::test]
    async fn test_subscribe_timeout() {
        let (mut listener, _relay) = setup_two_nodes().await;

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            listener.subscribe(stream_addr, relay_addr)
        ).await;

        assert!(matches!(result, Err(_) | Ok(Err(SubscribeError::Timeout))));
    }
}
```

---

## 4. Chunk Reception Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_chunk_deserialization` | Parse CBOR Chunk | Already have some |
| `test_chunk_encrypted_flag` | `is_encrypted()` checks flag | - |
| `test_chunk_keyframe_flag` | `is_keyframe()` checks flag | - |
| `test_chunk_nonce_present_when_encrypted` | Nonce field populated | - |
| `test_chunk_nonce_absent_when_unencrypted` | Nonce is None | - |
| `test_chunk_sequence_number_u64_max` | Handles large seq numbers | u64::MAX |
| `test_chunk_timestamp_microseconds` | Timestamp is in microseconds | - |

### gossipsub Reception Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_receive_chunk_on_subscribed_topic` | Chunk arrives via gossipsub | - |
| `test_ignore_chunk_wrong_stream_addr` | Filters chunks for other streams | - |
| `test_chunk_signature_verification` | Verify chunk came from relay/broadcaster | Invalid sig |
| `test_chunk_message_envelope_valid` | Full Message envelope parses | - |

### Chunk Ordering Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_chunks_out_of_order_handling` | Handles seq 1, 3, 2 arrival | - |
| `test_chunk_gap_detection` | Detects missing seq numbers | - |
| `test_chunk_duplicate_detection` | Ignores duplicate seq numbers | - |
| `test_chunk_late_arrival_handling` | Handles very delayed chunks | Gap > buffer |
| `test_jitter_buffer_reorder` | Jitter buffer reorders chunks | - |

### Integration Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_receive_100_chunks_in_order` | Handles sustained reception | - |
| `test_receive_chunks_high_frequency` | 50 chunks/sec (20ms each) | - |
| `test_chunk_reception_survives_reconnect` | Continues after brief disconnect | - |

### Suggested Structure

```rust
#[cfg(test)]
mod chunk_reception_tests {
    use crate::stream::{Chunk, ChunkFlags, Codec};

    fn test_chunk(seq: u64, encrypted: bool) -> Chunk {
        if encrypted {
            Chunk::new_encrypted(
                [0x01; 32],
                seq,
                seq * 20_000, // 20ms per chunk
                Codec::Opus,
                20_000,
                vec![0xDE, 0xAD, 0xBE, 0xEF],
                [0u8; 12],
            )
        } else {
            Chunk::new([0x01; 32], seq, seq * 20_000, Codec::Opus, 20_000, vec![0xDE, 0xAD])
        }
    }

    #[test]
    fn test_chunk_gap_detection() {
        let mut buffer = ChunkBuffer::new(10); // 10-chunk buffer

        buffer.push(test_chunk(1, false));
        buffer.push(test_chunk(3, false)); // Gap!

        assert!(buffer.has_gap());
        assert_eq!(buffer.missing_seqs(), vec![2]);
    }
}
```

---

## 5. Decryption Tests

### Unit Tests (Extend Existing)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_decrypt_chunk_with_valid_key` | Decrypts chunk data | Already have encrypt/decrypt |
| `test_decrypt_chunk_wrong_key_fails` | Returns error on wrong key | Already have |
| `test_decrypt_chunk_wrong_nonce_fails` | Returns error on tampered nonce | - |
| `test_decrypt_chunk_tampered_ciphertext` | Detects data tampering | Flip one bit |
| `test_decrypt_chunk_truncated_ciphertext` | Handles truncated data | - |

### Stream Key Management Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_stream_key_derived_from_sub_ack` | Key from SUB_ACK usable | - |
| `test_stream_key_rotation` | Handle key change mid-stream | - |
| `test_stream_key_storage_secure` | Key not logged or leaked | - |
| `test_decrypt_without_key_fails` | Error before SUB_ACK received | - |

### Chunk Decryption Flow Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_decrypt_opus_chunk` | Full decrypt → Opus bytes | - |
| `test_unencrypted_chunk_passthrough` | Unencrypted chunks not decrypted | - |
| `test_decrypt_uses_chunk_nonce` | Each chunk's nonce used | - |
| `test_concurrent_decryption` | Thread-safe cipher usage | - |

### Suggested Structure

```rust
#[cfg(test)]
mod decryption_tests {
    use crate::crypto::{StreamCipher, generate_stream_key, decrypt};
    use crate::stream::{Chunk, ChunkFlags};

    #[test]
    fn test_decrypt_chunk_tampered_ciphertext() {
        let key = generate_stream_key();
        let plaintext = b"opus frame data";

        let (mut ciphertext, nonce) = encrypt(&key, plaintext).unwrap();
        ciphertext[0] ^= 0xFF; // Tamper with first byte

        let result = decrypt(&key, &ciphertext, &nonce);
        assert!(matches!(result, Err(EncryptionError::DecryptionFailed)));
    }

    #[test]
    fn test_unencrypted_chunk_passthrough() {
        let chunk = Chunk::new(
            [0x01; 32], 1, 0, Codec::Opus, 20_000,
            b"raw opus".to_vec(),
        );

        assert!(!chunk.is_encrypted());
        // Should return data as-is, not attempt decryption
        let audio = process_chunk(&chunk, None).unwrap();
        assert_eq!(audio, b"raw opus");
    }
}
```

---

## 6. Audio Decoding Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_opus_decoder_creation` | Creates decoder with sample rate/channels | - |
| `test_opus_decoder_48khz_stereo` | Standard config | - |
| `test_opus_decoder_48khz_mono` | Mono config | - |
| `test_opus_decode_valid_frame` | Decodes to PCM samples | - |
| `test_opus_decode_invalid_frame` | Returns error on garbage | - |
| `test_opus_decode_truncated_frame` | Handles truncated data | - |
| `test_opus_decode_packet_loss_concealment` | PLC on missing frame | - |

### Sample Rate/Channel Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_decoder_matches_announcement` | Decoder config from StreamAnnouncement | - |
| `test_decoder_sample_rate_48000` | 48kHz output | - |
| `test_decoder_sample_rate_24000` | 24kHz output | - |
| `test_decoder_channels_1` | Mono decode | - |
| `test_decoder_channels_2` | Stereo decode | - |
| `test_decoder_mismatch_handled` | Error if config doesn't match | - |

### Decode Pipeline Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_decode_continuous_stream` | 100 frames sequential | - |
| `test_decode_with_gaps` | Handle missing frames (PLC) | 1 gap, 5 gap |
| `test_decode_output_pcm_s16` | Output is signed 16-bit PCM | - |
| `test_decode_output_samples_count` | Correct sample count per frame | - |
| `test_decode_latency_measurement` | Decode < 5ms per frame | - |

### Mock Decoder for Testing

```rust
/// Mock Opus decoder for tests that don't need real audio
struct MockOpusDecoder {
    sample_rate: u32,
    channels: u8,
    frames_decoded: usize,
}

impl MockOpusDecoder {
    fn decode(&mut self, _opus_data: &[u8]) -> Result<Vec<i16>, DecodeError> {
        self.frames_decoded += 1;
        // Return silence (20ms at 48kHz stereo = 1920 samples)
        let samples = (self.sample_rate as usize / 50) * self.channels as usize;
        Ok(vec![0i16; samples])
    }
}
```

---

## 7. Audio Output Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_audio_output_device_enumeration` | Lists available devices | - |
| `test_audio_output_default_device` | Gets system default | No default |
| `test_audio_output_device_by_name` | Selects specific device | Name not found |
| `test_audio_output_config_48khz_stereo` | Opens with correct config | - |
| `test_audio_output_config_mismatch_resampling` | Handles rate mismatch | - |

### Playback Buffer Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_playback_buffer_underrun` | Handles empty buffer gracefully | - |
| `test_playback_buffer_overrun` | Handles full buffer (drop old) | - |
| `test_playback_buffer_latency` | Buffer adds ~100ms latency | - |
| `test_playback_buffer_drain_on_stop` | Flushes on stream end | - |

### Mock Audio Output for Tests

```rust
/// Mock audio output that records samples instead of playing
struct MockAudioOutput {
    samples: Vec<i16>,
    underruns: usize,
    overruns: usize,
}

impl MockAudioOutput {
    fn write(&mut self, samples: &[i16]) -> Result<(), AudioError> {
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn written_duration(&self, sample_rate: u32, channels: u8) -> Duration {
        let total_samples = self.samples.len();
        let frames = total_samples / channels as usize;
        Duration::from_secs_f64(frames as f64 / sample_rate as f64)
    }
}
```

### Integration Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_decode_to_output_pipeline` | Chunks → decode → play | - |
| `test_playback_timing_accuracy` | Plays at real-time rate | - |
| `test_playback_pause_resume` | Can pause/resume stream | - |
| `test_playback_stop_graceful` | Clean shutdown | - |

---

## 8. Payment Commitment Tests

### Unit Tests (Extend Existing)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_payment_commitment_creation` | Creates signed commitment | Already have |
| `test_payment_commitment_signature_valid` | Signature verifies | Already have |
| `test_payment_commitment_cumulative` | Amount must increase | Already have |
| `test_payment_commitment_sequence` | Seq must increase | Already have |
| `test_payment_commitment_free_method` | Free method amount=0 | - |

### Payment Scheduling Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_pay_commit_interval_default` | Sends every 60 seconds | - |
| `test_pay_commit_interval_configurable` | Respects custom interval | - |
| `test_pay_commit_on_stream_start` | First commit after SUB_ACK | - |
| `test_pay_commit_on_stream_end` | Final commit on UNSUBSCRIBE | - |
| `test_pay_commit_amount_calculation` | Amount = rate * time_listened | - |

### Amount Calculation Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_amount_free_stream` | Amount always 0 | - |
| `test_amount_paid_stream_per_minute` | price_per_min * minutes | - |
| `test_amount_partial_minute` | Pro-rated for partial minutes | - |
| `test_amount_cumulative_over_session` | Increases monotonically | - |
| `test_amount_overflow_handling` | Handles very long streams | u64 overflow |

### PAY_COMMIT Message Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_pay_commit_message_format` | Correct CBOR structure | - |
| `test_pay_commit_envelope_signature` | Outer message signed | - |
| `test_pay_commit_inner_signature` | Commitment itself signed | - |
| `test_pay_commit_relay_id_correct` | Matches subscribed relay | - |
| `test_pay_commit_stream_addr_correct` | Matches current stream | - |

### PAY_RECEIPT Response Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_pay_receipt_parsing` | Parse relay's receipt | - |
| `test_pay_receipt_verifies` | Receipt signature valid | - |
| `test_pay_receipt_amount_matches` | Receipt acknowledges sent amount | - |
| `test_pay_receipt_missing_handled` | Continue without receipt | - |

### Integration Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_full_payment_flow` | COMMIT → RECEIPT cycle | - |
| `test_payment_persists_across_reconnect` | Resume from last commit | - |
| `test_final_settlement_on_disconnect` | Last commit on close | - |

### Suggested Structure

```rust
#[cfg(test)]
mod payment_commitment_tests {
    use crate::payment::{PaymentCommitment, PaymentMethod};
    use crate::identity::Keypair;

    struct PaymentTracker {
        relay_id: Identity,
        stream_addr: [u8; 32],
        listener_keypair: Keypair,
        price_per_min: u64,
        last_seq: u64,
        total_amount: u64,
        listen_start: Instant,
    }

    impl PaymentTracker {
        fn create_commitment(&mut self) -> Result<PaymentCommitment, CommitmentError> {
            let elapsed = self.listen_start.elapsed();
            let minutes = elapsed.as_secs_f64() / 60.0;
            let new_amount = (minutes * self.price_per_min as f64) as u64;

            self.last_seq += 1;
            self.total_amount = new_amount;

            PaymentCommitment::create(
                self.relay_id.clone(),
                &self.listener_keypair,
                self.stream_addr,
                PaymentMethod::EvmL2,
                new_amount,
                "USDC".to_string(),
                Some(8453), // Base chain ID
                self.last_seq,
            )
        }
    }

    #[test]
    fn test_amount_cumulative_over_session() {
        let mut tracker = test_payment_tracker(100); // 100 units/min

        // Simulate 30 seconds
        std::thread::sleep(Duration::from_millis(50)); // In real test, mock time
        let c1 = tracker.create_commitment().unwrap();

        // Simulate another 30 seconds
        std::thread::sleep(Duration::from_millis(50));
        let c2 = tracker.create_commitment().unwrap();

        assert!(c2.amount >= c1.amount);
        assert!(c2.seq > c1.seq);
    }
}
```

---

## 9. End-to-End Listen Flow Tests

### Integration Tests (Full Pipeline)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_listen_full_flow_unencrypted` | Discovery → Subscribe → Receive → Play | - |
| `test_listen_full_flow_encrypted` | Full flow with decryption | - |
| `test_listen_full_flow_with_payment` | Full flow with PAY_COMMIT | - |
| `test_listen_graceful_disconnect` | UNSUBSCRIBE → settlement | - |
| `test_listen_relay_disconnect_recovery` | Reconnect on relay drop | - |
| `test_listen_stream_end_handling` | Handle broadcaster stop | - |

### Error Recovery Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_listen_relay_unreachable` | Falls back to another relay | - |
| `test_listen_all_relays_unreachable` | Graceful error message | - |
| `test_listen_dht_timeout` | Discovery timeout handling | - |
| `test_listen_stream_not_found` | Clear error for missing stream | - |
| `test_listen_vouch_invalid` | Reject stream with bad vouch | - |

### Reconnection Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_listen_auto_reconnect` | Automatic reconnection | - |
| `test_listen_reconnect_same_relay` | Prefer last relay | - |
| `test_listen_reconnect_different_relay` | Fall back on failure | - |
| `test_listen_reconnect_maintains_payment` | Payment state preserved | - |
| `test_listen_reconnect_max_attempts` | Give up after N failures | - |

### Performance Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_listen_latency_under_500ms` | Glass-to-glass < 500ms | - |
| `test_listen_cpu_usage_reasonable` | < 10% CPU on decode | - |
| `test_listen_memory_stable` | No memory leaks over time | 1 hour run |

---

## 10. CLI Argument Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_cli_listen_parse_stream_arg` | Required stream argument | - |
| `test_cli_listen_parse_output_flag` | Optional --output | - |
| `test_cli_listen_missing_stream_fails` | Error without stream | - |
| `test_cli_listen_verbose_flag` | -v enables debug logging | - |

### Suggested Structure

```rust
#[cfg(test)]
mod cli_tests {
    use clap::Parser;
    use super::Cli;

    #[test]
    fn test_cli_listen_parse_stream_arg() {
        let cli = Cli::try_parse_from(["mdrn", "listen", "abc123"]).unwrap();
        match cli.command {
            Commands::Listen { stream, .. } => assert_eq!(stream, "abc123"),
            _ => panic!("Wrong command"),
        }
    }

    #[test]
    fn test_cli_listen_missing_stream_fails() {
        let result = Cli::try_parse_from(["mdrn", "listen"]);
        assert!(result.is_err());
    }
}
```

---

## 11. Test Infrastructure Requirements

### Helper Functions

```rust
/// Create a mock relay that serves a test stream
async fn setup_mock_relay(stream: &StreamAnnouncement) -> MockRelay;

/// Create a mock DHT with pre-populated records
fn setup_mock_dht(streams: Vec<StreamAnnouncement>, relays: Vec<RelayAdvertisement>) -> MockDht;

/// Generate test Opus frames (silence or tone)
fn generate_test_opus_frames(count: usize) -> Vec<Vec<u8>>;

/// Create encrypted test chunks
fn generate_encrypted_chunks(
    key: &[u8; 32],
    count: usize,
    codec: Codec,
) -> Vec<Chunk>;

/// Mock audio output that captures samples
fn mock_audio_output() -> (MockAudioOutput, AudioOutputHandle);

/// Wait for subscription state with timeout
async fn wait_for_state(
    sub: &Subscription,
    expected: SubscriptionState,
    timeout: Duration,
) -> Result<(), TimeoutError>;
```

### Test Fixtures

```rust
/// Standard test stream announcement
fn test_stream_announcement() -> StreamAnnouncement {
    let broadcaster = Keypair::generate_ed25519().unwrap();
    let issuer = Keypair::generate_ed25519().unwrap();
    let vouch = Vouch::create(broadcaster.identity().clone(), &issuer, None).unwrap();

    StreamAnnouncement::new(
        broadcaster.identity().clone(),
        "test-stream".to_string(),
        Codec::Opus,
        128,
        48000,
        2,
        true, // encrypted
        vouch,
    )
}

/// Standard test relay advertisement
fn test_relay_advertisement(stream_addr: [u8; 32], free: bool) -> RelayAdvertisement {
    let relay = Keypair::generate_ed25519().unwrap();
    RelayAdvertisement {
        relay_id: relay.identity().clone(),
        stream_addr,
        price_per_min: if free { 0 } else { 100 },
        payment_methods: vec![if free { PaymentMethod::Free } else { PaymentMethod::EvmL2 }],
        capacity: 100,
        latency_ms: 50,
        endpoints: vec![Endpoint {
            addr: "/ip4/127.0.0.1/tcp/9000".to_string(),
            transport: Transport::Tcp,
        }],
        ttl: 300,
    }
}
```

### Test Configuration

```rust
/// Test config with fast timeouts
fn test_listen_config() -> ListenConfig {
    ListenConfig {
        subscribe_timeout: Duration::from_secs(1),
        chunk_timeout: Duration::from_millis(100),
        reconnect_delay: Duration::from_millis(100),
        max_reconnect_attempts: 3,
        payment_interval: Duration::from_secs(5), // Fast for tests
        jitter_buffer_size: 5,
    }
}
```

---

## 12. Test Organization

### Directory Structure

```
mdrn-cli/src/
    commands/
        listen/
            mod.rs              # Listen command entry
            resolver.rs         # Stream resolution
            subscription.rs     # Subscription management
            receiver.rs         # Chunk reception
            player.rs           # Audio playback
            payment.rs          # Payment tracking
            tests/              # Unit tests
                mod.rs
                resolver_tests.rs
                subscription_tests.rs
                receiver_tests.rs
                player_tests.rs
                payment_tests.rs

mdrn-cli/tests/                 # Integration tests
    listen_integration.rs       # End-to-end tests
    helpers.rs                  # Test infrastructure
```

### Test Categories

| Category | Location | Async | Mock Level |
|----------|----------|-------|------------|
| CLI parsing | inline `#[cfg(test)]` | No | None |
| Stream resolution | `tests/resolver_tests.rs` | Yes | Mock DHT |
| Subscription FSM | `tests/subscription_tests.rs` | Yes | Mock relay |
| Chunk reception | `tests/receiver_tests.rs` | Yes | Mock gossipsub |
| Decryption | inline + `tests/` | No | None |
| Audio decode | `tests/player_tests.rs` | No | Mock decoder |
| Audio output | `tests/player_tests.rs` | Yes | Mock output |
| Payment | `tests/payment_tests.rs` | Yes | Mock time |
| Full pipeline | `mdrn-cli/tests/` | Yes | Full mock |

---

## 13. Acceptance Criteria Summary

The `listen` command is complete when:

1. **Stream Resolution**
   - [ ] Parses hex stream_addr
   - [ ] Falls back to stream_id lookup
   - [ ] Validates stream announcement vouch

2. **Relay Discovery**
   - [ ] Queries DHT for relays
   - [ ] Selects relay by price/latency
   - [ ] Filters by payment method

3. **Subscription**
   - [ ] SUBSCRIBE → SUB_ACK handshake works
   - [ ] Handles SUB_REJECT gracefully
   - [ ] State machine transitions correct
   - [ ] Receives stream key from SUB_ACK

4. **Chunk Reception**
   - [ ] Receives chunks via gossipsub
   - [ ] Handles out-of-order chunks
   - [ ] Detects and handles gaps

5. **Decryption**
   - [ ] Decrypts chunks with stream key
   - [ ] Handles unencrypted streams
   - [ ] Detects tampered data

6. **Audio Playback**
   - [ ] Decodes Opus to PCM
   - [ ] Plays to output device
   - [ ] Handles buffer underruns

7. **Payment**
   - [ ] Sends PAY_COMMIT periodically
   - [ ] Calculates cumulative amount
   - [ ] Handles free streams (no payment)

8. **Error Handling**
   - [ ] Graceful reconnection
   - [ ] Clear error messages
   - [ ] Clean shutdown on Ctrl+C

---

## Notes

- **Opus library**: Use `audiopus` or `opus` crate for decoding
- **Audio output**: Use `cpal` for cross-platform audio
- **Async runtime**: tokio with `test-util` for time manipulation in tests
- **Mock strategy**: Start with mocks, graduate to real implementations
- **Deterministic tests**: Seed RNG, mock time, avoid wall-clock waits
