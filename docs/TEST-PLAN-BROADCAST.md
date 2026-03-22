# MDRN Broadcast CLI Test Plan

This document outlines TDD test cases for the `mdrn broadcast` CLI command implementation.

## Overview

The broadcast command orchestrates:
1. **Keypair loading** - Read broadcaster identity from file
2. **Vouch loading** - Load admission credential
3. **Audio input** - Read from file or device
4. **Opus encoding** - Compress raw audio
5. **Chunking** - Segment into 20-60ms frames
6. **Encryption** - ChaCha20-Poly1305 per-chunk (optional)
7. **Stream announcement** - Publish to DHT
8. **Chunk publishing** - Send to gossipsub topic

CLI signature (from main.rs):
```
mdrn broadcast --stream-id <ID> [--input <FILE|DEVICE>] [--bitrate <KBPS>] [--encrypted]
```

---

## 1. Keypair Loading Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_load_keypair_ed25519_from_file` | Load Ed25519 keypair from CBOR file | - |
| `test_load_keypair_secp256k1_from_file` | Load secp256k1 keypair from CBOR file | - |
| `test_load_keypair_file_not_found` | Error when keypair file missing | Symlink to missing, permission denied |
| `test_load_keypair_invalid_cbor` | Error on corrupted/invalid CBOR | Truncated, wrong format |
| `test_load_keypair_wrong_format` | Error when file is valid CBOR but wrong structure | JSON instead of CBOR |
| `test_load_keypair_from_env_var` | Load from `MDRN_KEYPAIR` env var path | - |
| `test_load_keypair_default_location` | Load from `~/.mdrn/keypair.cbor` if no path given | - |
| `test_keypair_file_permissions_checked` | Warn/error if keypair file is world-readable | Mode 0644 vs 0600 |

### File Format Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_keypair_file_cbor_structure` | Validate expected CBOR schema | `{key_type, secret, identity}` |
| `test_keypair_file_roundtrip` | Save and reload keypair preserves data | - |

### Suggested Structure

```rust
#[cfg(test)]
mod keypair_loading_tests {
    use tempfile::NamedTempFile;
    use std::fs;

    #[test]
    fn test_load_keypair_ed25519_from_file() {
        // Create temp file with serialized keypair
        let keypair = Keypair::generate_ed25519().unwrap();
        let file = NamedTempFile::new().unwrap();
        save_keypair(&keypair, file.path()).unwrap();

        // Load it back
        let loaded = load_keypair(file.path()).unwrap();
        assert_eq!(loaded.identity(), keypair.identity());
    }

    #[test]
    fn test_load_keypair_file_not_found() {
        let result = load_keypair("/nonexistent/path/keypair.cbor");
        assert!(matches!(result, Err(BroadcastError::KeypairNotFound(_))));
    }

    #[test]
    fn test_load_keypair_invalid_cbor() {
        let file = NamedTempFile::new().unwrap();
        fs::write(file.path(), b"not valid cbor").unwrap();

        let result = load_keypair(file.path());
        assert!(matches!(result, Err(BroadcastError::InvalidKeypair(_))));
    }
}
```

---

## 2. Vouch Loading Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_load_vouch_from_file` | Load vouch credential from CBOR file | - |
| `test_load_vouch_file_not_found` | Error when vouch file missing | - |
| `test_load_vouch_invalid_cbor` | Error on corrupted CBOR | - |
| `test_load_vouch_signature_invalid` | Vouch fails verification | Tampered data |
| `test_load_vouch_expired` | Error when vouch has expired | - |
| `test_load_vouch_wrong_subject` | Error when vouch subject != broadcaster identity | - |
| `test_load_vouch_from_default_location` | Load from `~/.mdrn/vouch.cbor` | - |
| `test_load_vouch_from_env_var` | Load from `MDRN_VOUCH` env var path | - |

### Vouch Validation Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_vouch_subject_matches_keypair` | Vouch subject == broadcaster identity | Pass |
| `test_vouch_issuer_signature_valid` | Issuer signature verifies | Pass |
| `test_vouch_not_yet_valid` | Future `issued_at` timestamp | Fail (if checking) |
| `test_vouch_expires_during_broadcast` | Handle vouch expiring mid-stream | Graceful shutdown/warning |

### Suggested Structure

```rust
#[cfg(test)]
mod vouch_loading_tests {
    use tempfile::NamedTempFile;
    use crate::identity::{Keypair, Vouch};

    #[test]
    fn test_load_vouch_from_file() {
        let issuer = Keypair::generate_ed25519().unwrap();
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let vouch = Vouch::create(broadcaster.identity().clone(), &issuer, None).unwrap();

        let file = NamedTempFile::new().unwrap();
        save_vouch(&vouch, file.path()).unwrap();

        let loaded = load_vouch(file.path()).unwrap();
        loaded.verify().unwrap();
    }

    #[test]
    fn test_load_vouch_wrong_subject() {
        let issuer = Keypair::generate_ed25519().unwrap();
        let other = Keypair::generate_ed25519().unwrap();
        let broadcaster = Keypair::generate_ed25519().unwrap();

        // Vouch for `other`, not `broadcaster`
        let vouch = Vouch::create(other.identity().clone(), &issuer, None).unwrap();
        let file = NamedTempFile::new().unwrap();
        save_vouch(&vouch, file.path()).unwrap();

        let result = validate_vouch_for_broadcaster(file.path(), broadcaster.identity());
        assert!(matches!(result, Err(BroadcastError::VouchSubjectMismatch)));
    }

    #[test]
    fn test_load_vouch_expired() {
        let issuer = Keypair::generate_ed25519().unwrap();
        let broadcaster = Keypair::generate_ed25519().unwrap();
        // Expired 1 second ago
        let vouch = Vouch::create(broadcaster.identity().clone(), &issuer, Some(0)).unwrap();

        let file = NamedTempFile::new().unwrap();
        save_vouch(&vouch, file.path()).unwrap();

        let result = load_and_validate_vouch(file.path(), broadcaster.identity());
        assert!(matches!(result, Err(BroadcastError::VouchExpired)));
    }
}
```

---

## 3. Audio Input Tests

### Mock/Stub Strategy

Audio I/O requires careful mocking:

```rust
/// Trait for audio input sources
pub trait AudioSource: Send {
    /// Read next audio frame (returns PCM samples)
    fn read_frame(&mut self) -> Result<AudioFrame, AudioError>;

    /// Get sample rate
    fn sample_rate(&self) -> u32;

    /// Get number of channels
    fn channels(&self) -> u8;

    /// Check if source is exhausted (file) or still live (device)
    fn is_eof(&self) -> bool;
}

/// Mock audio source for testing
pub struct MockAudioSource {
    frames: VecDeque<AudioFrame>,
    sample_rate: u32,
    channels: u8,
}

impl MockAudioSource {
    pub fn from_sine_wave(freq: f32, duration_ms: u32, sample_rate: u32) -> Self { ... }
    pub fn from_silence(duration_ms: u32, sample_rate: u32) -> Self { ... }
    pub fn from_pcm_file(path: &Path) -> Self { ... }
}
```

### Unit Tests - File Input

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_read_wav_file` | Open and read WAV file | - |
| `test_read_ogg_file` | Open and read Ogg Vorbis file | - |
| `test_read_flac_file` | Open and read FLAC file | - |
| `test_audio_file_not_found` | Error on missing file | - |
| `test_audio_file_invalid_format` | Error on non-audio file | Text file, corrupted header |
| `test_audio_file_unsupported_codec` | Error on unsupported format | MP3 if not supported |
| `test_audio_file_sample_rate_detection` | Correctly detect sample rate | 8kHz, 44.1kHz, 48kHz, 96kHz |
| `test_audio_file_channel_count_detection` | Correctly detect mono/stereo | 1ch, 2ch, 5.1 (should error) |
| `test_audio_file_eof_handling` | Gracefully handle end of file | - |

### Unit Tests - Device Input (Stubbed)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_default_device_selection` | Select default input device | - |
| `test_named_device_selection` | Select device by name | - |
| `test_device_not_found` | Error on invalid device name | - |
| `test_device_sample_rate_mismatch` | Handle device with different sample rate | 44.1kHz device, 48kHz required |
| `test_device_permission_denied` | Error when no microphone access | macOS permission |

### Suggested Structure

```rust
#[cfg(test)]
mod audio_input_tests {
    use super::*;

    #[test]
    fn test_read_wav_file() {
        // Use embedded test WAV file
        let source = FileAudioSource::open("tests/fixtures/test_mono_48khz.wav").unwrap();
        assert_eq!(source.sample_rate(), 48000);
        assert_eq!(source.channels(), 1);

        let frame = source.read_frame().unwrap();
        assert!(!frame.samples.is_empty());
    }

    #[test]
    fn test_mock_audio_source_sine_wave() {
        let mut source = MockAudioSource::from_sine_wave(440.0, 1000, 48000);
        assert_eq!(source.sample_rate(), 48000);

        let frame = source.read_frame().unwrap();
        // Sine wave should have non-zero samples
        assert!(frame.samples.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn test_audio_file_not_found() {
        let result = FileAudioSource::open("/nonexistent/audio.wav");
        assert!(matches!(result, Err(AudioError::FileNotFound(_))));
    }
}
```

---

## 4. Opus Encoding Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_opus_encode_mono_48khz` | Encode mono 48kHz PCM | Standard case |
| `test_opus_encode_stereo_48khz` | Encode stereo 48kHz PCM | Standard case |
| `test_opus_encode_mono_24khz` | Encode at 24kHz (requires resampling) | - |
| `test_opus_encode_at_bitrate_64kbps` | Output respects 64kbps target | - |
| `test_opus_encode_at_bitrate_128kbps` | Output respects 128kbps target | - |
| `test_opus_encode_at_bitrate_256kbps` | Output respects 256kbps target | - |
| `test_opus_encode_20ms_frame` | Encode 20ms frame | 960 samples at 48kHz |
| `test_opus_encode_40ms_frame` | Encode 40ms frame | 1920 samples at 48kHz |
| `test_opus_encode_60ms_frame` | Encode 60ms frame | 2880 samples at 48kHz |
| `test_opus_encode_invalid_frame_size` | Error on non-standard frame size | 15ms, 100ms |
| `test_opus_encode_empty_frame` | Error or handle empty input | - |
| `test_opus_encode_silence` | Encodes silence efficiently | DTX behavior |
| `test_opus_encoder_reusable` | Same encoder handles multiple frames | - |
| `test_opus_decode_roundtrip` | Encode then decode recovers audio | - |

### Bitrate Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_opus_bitrate_produces_expected_size` | 128kbps produces ~320 bytes/20ms | +/- 20% |
| `test_opus_vbr_vs_cbr` | VBR mode output varies with content | - |

### Suggested Structure

```rust
#[cfg(test)]
mod opus_encoding_tests {
    use opus::{Encoder, Application};

    #[test]
    fn test_opus_encode_mono_48khz() {
        let encoder = OpusEncoder::new(48000, 1, 128).unwrap();

        // 20ms of samples at 48kHz mono = 960 samples
        let pcm: Vec<f32> = (0..960).map(|i| (i as f32 * 0.01).sin()).collect();

        let encoded = encoder.encode(&pcm).unwrap();
        assert!(!encoded.is_empty());
        assert!(encoded.len() < pcm.len() * 4); // Should be compressed
    }

    #[test]
    fn test_opus_encode_20ms_frame() {
        let encoder = OpusEncoder::new(48000, 2, 128).unwrap();

        // 20ms stereo = 960 samples * 2 channels = 1920 samples
        let pcm: Vec<f32> = vec![0.0; 1920];

        let encoded = encoder.encode(&pcm).unwrap();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_opus_encode_invalid_frame_size() {
        let encoder = OpusEncoder::new(48000, 1, 128).unwrap();

        // 15ms = 720 samples - invalid for Opus
        let pcm: Vec<f32> = vec![0.0; 720];

        let result = encoder.encode(&pcm);
        assert!(matches!(result, Err(OpusError::InvalidFrameSize)));
    }

    #[test]
    fn test_opus_decode_roundtrip() {
        let encoder = OpusEncoder::new(48000, 1, 128).unwrap();
        let decoder = OpusDecoder::new(48000, 1).unwrap();

        let original: Vec<f32> = (0..960).map(|i| (i as f32 * 0.01).sin()).collect();
        let encoded = encoder.encode(&original).unwrap();
        let decoded = decoder.decode(&encoded).unwrap();

        // Lossy codec - check approximate match
        assert_eq!(decoded.len(), original.len());
        // SNR should be reasonable
        let mse: f32 = original.iter()
            .zip(decoded.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>() / original.len() as f32;
        assert!(mse < 0.01, "MSE too high: {}", mse);
    }
}
```

---

## 5. Chunking Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_chunk_creation_unencrypted` | Create chunk with all required fields | - |
| `test_chunk_creation_encrypted` | Create chunk with nonce | - |
| `test_chunk_sequence_increments` | Each chunk has incrementing seq | - |
| `test_chunk_timestamp_increments` | Timestamps advance by duration | - |
| `test_chunk_duration_20ms` | Duration field set to 20000us | - |
| `test_chunk_duration_60ms` | Duration field set to 60000us | - |
| `test_chunk_stream_addr_set` | stream_addr matches announcement | - |
| `test_chunk_codec_set` | Codec field set correctly | - |
| `test_chunk_keyframe_flag` | First chunk marked as keyframe | - |
| `test_chunk_cbor_serialization` | Chunk serializes to CBOR | - |
| `test_chunk_cbor_deserialization` | Chunk deserializes from CBOR | - |
| `test_chunk_cbor_roundtrip` | Serialize/deserialize preserves data | - |
| `test_chunk_size_within_limits` | Serialized size under gossipsub max | < 1MB |

### ChunkBuilder Pattern Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_chunk_builder_minimal` | Build with required fields only | Valid chunk |
| `test_chunk_builder_with_encryption` | Build with encryption data | Encrypted flag set |
| `test_chunk_builder_seq_auto_increment` | Builder tracks sequence number | - |

### Suggested Structure

```rust
#[cfg(test)]
mod chunking_tests {
    use crate::stream::{Chunk, ChunkFlags, Codec};

    fn test_stream_addr() -> [u8; 32] {
        [0xAB; 32]
    }

    #[test]
    fn test_chunk_creation_unencrypted() {
        let opus_data = vec![0x00, 0x01, 0x02]; // Fake Opus frame
        let chunk = Chunk::new(
            test_stream_addr(),
            0,          // seq
            0,          // timestamp
            Codec::Opus,
            20_000,     // 20ms
            opus_data.clone(),
        );

        assert_eq!(chunk.seq, 0);
        assert_eq!(chunk.timestamp, 0);
        assert_eq!(chunk.duration_us, 20_000);
        assert_eq!(chunk.data, opus_data);
        assert!(!chunk.is_encrypted());
        assert!(chunk.nonce.is_none());
    }

    #[test]
    fn test_chunk_sequence_increments() {
        let mut chunker = ChunkBuilder::new(test_stream_addr(), Codec::Opus);

        let chunk0 = chunker.build_chunk(vec![0x00], 20_000);
        let chunk1 = chunker.build_chunk(vec![0x01], 20_000);
        let chunk2 = chunker.build_chunk(vec![0x02], 20_000);

        assert_eq!(chunk0.seq, 0);
        assert_eq!(chunk1.seq, 1);
        assert_eq!(chunk2.seq, 2);
    }

    #[test]
    fn test_chunk_timestamp_increments() {
        let mut chunker = ChunkBuilder::new(test_stream_addr(), Codec::Opus);

        let chunk0 = chunker.build_chunk(vec![0x00], 20_000); // 20ms
        let chunk1 = chunker.build_chunk(vec![0x01], 20_000);
        let chunk2 = chunker.build_chunk(vec![0x02], 40_000); // 40ms

        assert_eq!(chunk0.timestamp, 0);
        assert_eq!(chunk1.timestamp, 20_000);
        assert_eq!(chunk2.timestamp, 40_000);
    }

    #[test]
    fn test_chunk_cbor_roundtrip() {
        let original = Chunk::new(
            test_stream_addr(),
            42,
            1_000_000,
            Codec::Opus,
            20_000,
            vec![0xDE, 0xAD, 0xBE, 0xEF],
        );

        let mut cbor = Vec::new();
        ciborium::into_writer(&original, &mut cbor).unwrap();

        let decoded: Chunk = ciborium::from_reader(&cbor[..]).unwrap();

        assert_eq!(decoded.stream_addr, original.stream_addr);
        assert_eq!(decoded.seq, original.seq);
        assert_eq!(decoded.timestamp, original.timestamp);
        assert_eq!(decoded.data, original.data);
    }
}
```

---

## 6. Encryption Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_encrypt_chunk_data` | ChaCha20-Poly1305 encrypts chunk | - |
| `test_decrypt_chunk_data` | Decryption recovers plaintext | - |
| `test_encrypt_with_different_nonces` | Same data, different nonces = different ciphertext | - |
| `test_nonce_uniqueness` | Each chunk gets unique nonce | - |
| `test_nonce_size_is_12_bytes` | Nonce is exactly 12 bytes | - |
| `test_ciphertext_includes_auth_tag` | Output is 16 bytes longer than input | - |
| `test_decrypt_with_wrong_key_fails` | Wrong key returns error | - |
| `test_decrypt_with_wrong_nonce_fails` | Wrong nonce returns error | - |
| `test_decrypt_tampered_ciphertext_fails` | Modified ciphertext fails auth | - |
| `test_key_derivation_hkdf` | Stream key derived via HKDF-SHA256 | - |
| `test_key_derivation_deterministic` | Same inputs = same key | - |

### Key Management Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_generate_stream_key` | Random 32-byte key generated | - |
| `test_derive_stream_key_from_seed` | HKDF derivation from seed | Deterministic |
| `test_key_zeroization_on_drop` | Key material cleared from memory | Security |

### Suggested Structure

```rust
#[cfg(test)]
mod encryption_tests {
    use crate::crypto::{encrypt, decrypt, generate_stream_key, StreamCipher, NONCE_SIZE};

    #[test]
    fn test_encrypt_chunk_data() {
        let key = generate_stream_key();
        let plaintext = b"opus audio frame data";

        let (ciphertext, nonce) = encrypt(&key, plaintext).unwrap();

        // Ciphertext should be different from plaintext
        assert_ne!(ciphertext.as_slice(), plaintext);
        // Should be 16 bytes longer (auth tag)
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
    }

    #[test]
    fn test_decrypt_chunk_data() {
        let key = generate_stream_key();
        let plaintext = b"opus audio frame data";

        let (ciphertext, nonce) = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext, &nonce).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_with_different_nonces() {
        let key = generate_stream_key();
        let plaintext = b"same data";

        let (ct1, nonce1) = encrypt(&key, plaintext).unwrap();
        let (ct2, nonce2) = encrypt(&key, plaintext).unwrap();

        assert_ne!(nonce1, nonce2, "Nonces should be unique");
        assert_ne!(ct1, ct2, "Ciphertexts should differ");
    }

    #[test]
    fn test_nonce_size_is_12_bytes() {
        let key = generate_stream_key();
        let (_, nonce) = encrypt(&key, b"data").unwrap();
        assert_eq!(nonce.len(), NONCE_SIZE);
        assert_eq!(NONCE_SIZE, 12);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = generate_stream_key();
        let key2 = generate_stream_key();
        let plaintext = b"secret";

        let (ciphertext, nonce) = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &ciphertext, &nonce);

        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_tampered_ciphertext_fails() {
        let key = generate_stream_key();
        let plaintext = b"secret";

        let (mut ciphertext, nonce) = encrypt(&key, plaintext).unwrap();
        // Tamper with ciphertext
        ciphertext[0] ^= 0xFF;

        let result = decrypt(&key, &ciphertext, &nonce);
        assert!(result.is_err());
    }
}
```

---

## 7. Stream Announcement Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_create_stream_announcement` | Build announcement with all fields | - |
| `test_stream_addr_derivation` | SHA-256(identity \|\| stream_id) | - |
| `test_announcement_includes_vouch` | Vouch credential embedded | - |
| `test_announcement_cbor_serialization` | Serializes to CBOR | - |
| `test_announcement_cbor_roundtrip` | Serialize/deserialize preserves data | - |
| `test_announcement_ttl_default` | Default TTL is 300 seconds | - |
| `test_announcement_verify_passes` | Valid announcement verifies | - |
| `test_announcement_verify_fails_bad_vouch` | Invalid vouch fails verification | - |

### DHT Publishing Tests (Integration)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_publish_announcement_to_dht` | Announcement stored in DHT | - |
| `test_announcement_retrievable_from_dht` | Other nodes can query announcement | - |
| `test_announcement_expires_after_ttl` | Record disappears after TTL | - |
| `test_announcement_refresh_extends_ttl` | Re-publishing updates TTL | - |

### Suggested Structure

```rust
#[cfg(test)]
mod announcement_tests {
    use crate::stream::{StreamAnnouncement, Codec};
    use crate::identity::{Keypair, Vouch};
    use sha2::{Sha256, Digest};

    fn create_test_vouch(broadcaster: &Keypair) -> Vouch {
        let issuer = Keypair::generate_ed25519().unwrap();
        Vouch::create(broadcaster.identity().clone(), &issuer, None).unwrap()
    }

    #[test]
    fn test_create_stream_announcement() {
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let vouch = create_test_vouch(&broadcaster);

        let announcement = StreamAnnouncement::new(
            broadcaster.identity().clone(),
            "my-stream".to_string(),
            Codec::Opus,
            128,
            48000,
            2,
            true,
            vouch,
        );

        assert_eq!(announcement.stream_id, "my-stream");
        assert_eq!(announcement.bitrate, 128);
        assert!(announcement.encrypted);
        assert_eq!(announcement.ttl, 300);
    }

    #[test]
    fn test_stream_addr_derivation() {
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let stream_id = "test-stream";

        // Manual computation
        let mut hasher = Sha256::new();
        hasher.update(broadcaster.identity().as_bytes());
        hasher.update(stream_id.as_bytes());
        let expected: [u8; 32] = hasher.finalize().into();

        // Via function
        let computed = StreamAnnouncement::compute_stream_addr(
            broadcaster.identity(),
            stream_id
        );

        assert_eq!(computed, expected);
    }

    #[test]
    fn test_announcement_verify_passes() {
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let vouch = create_test_vouch(&broadcaster);

        let announcement = StreamAnnouncement::new(
            broadcaster.identity().clone(),
            "stream".to_string(),
            Codec::Opus,
            128,
            48000,
            2,
            false,
            vouch,
        );

        assert!(announcement.verify().is_ok());
    }
}
```

---

## 8. Gossipsub Publishing Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_topic_name_format` | Topic is `/mdrn/stream/{hex(stream_addr)}` | - |
| `test_subscribe_to_stream_topic` | Successfully subscribe to topic | - |
| `test_publish_chunk_to_topic` | Chunk CBOR published to topic | - |
| `test_publish_fails_not_subscribed` | Error if not subscribed to topic | - |
| `test_chunk_serialization_for_publish` | Chunk serializes correctly for gossipsub | - |

### Integration Tests (Two-Node)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_chunk_propagates_to_subscriber` | Publisher chunk reaches subscriber | - |
| `test_multiple_subscribers_receive_chunk` | All subscribers get chunk | - |
| `test_unsubscribed_node_misses_chunk` | Non-subscriber doesn't receive | - |
| `test_chunk_ordering_not_guaranteed` | Handle out-of-order chunks | - |
| `test_high_frequency_chunk_publishing` | Handle 50 chunks/sec (20ms frames) | - |

### Suggested Structure

```rust
#[cfg(test)]
mod gossipsub_tests {
    use crate::transport::stream_topic;
    use crate::stream::Chunk;

    #[test]
    fn test_topic_name_format() {
        let stream_addr = [0xAB; 32];
        let topic = stream_topic(&stream_addr);
        let expected = format!("/mdrn/stream/{}", hex::encode(stream_addr));
        assert_eq!(topic.to_string(), expected);
    }

    #[tokio::test]
    async fn test_chunk_propagates_to_subscriber() {
        let (mut publisher, mut subscriber) = setup_two_connected_nodes().await;
        let stream_addr = [0x01; 32];
        let topic = stream_topic(&stream_addr);

        publisher.subscribe(&topic).unwrap();
        subscriber.subscribe(&topic).unwrap();

        // Wait for mesh
        tokio::time::sleep(Duration::from_millis(100)).await;

        let chunk = Chunk::new(
            stream_addr,
            0,
            0,
            Codec::Opus,
            20_000,
            vec![0xDE, 0xAD],
        );
        let cbor = chunk.to_cbor().unwrap();

        publisher.publish(&topic, cbor.clone()).unwrap();

        let received = subscriber.next_message().await;
        assert_eq!(received.data, cbor);
    }
}
```

---

## 9. End-to-End Broadcast Pipeline Tests

### Integration Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_broadcast_pipeline_file_input` | Full pipeline with WAV file input | Chunks published |
| `test_broadcast_pipeline_mock_audio` | Full pipeline with mock audio source | Chunks published |
| `test_broadcast_pipeline_encrypted` | Pipeline with encryption enabled | Encrypted chunks |
| `test_broadcast_pipeline_unencrypted` | Pipeline without encryption | Unencrypted chunks |
| `test_broadcast_publishes_announcement_first` | Announcement before first chunk | Correct order |
| `test_broadcast_chunk_rate_matches_duration` | 50 chunks/sec for 20ms frames | Timing |
| `test_broadcast_graceful_shutdown` | Ctrl+C stops cleanly | No lost chunks |
| `test_broadcast_file_eof_stops` | Broadcast stops when file ends | Clean exit |

### CLI Argument Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_cli_parse_broadcast_minimal` | `mdrn broadcast --stream-id foo` | Valid parse |
| `test_cli_parse_broadcast_with_input` | `--input audio.wav` | Path parsed |
| `test_cli_parse_broadcast_with_bitrate` | `--bitrate 256` | Bitrate=256 |
| `test_cli_parse_broadcast_encrypted` | `--encrypted` | encrypted=true |
| `test_cli_parse_broadcast_all_options` | All flags together | All parsed |
| `test_cli_missing_stream_id_fails` | No `--stream-id` | Error |
| `test_cli_invalid_bitrate_fails` | `--bitrate abc` | Parse error |

### Suggested Structure

```rust
#[cfg(test)]
mod e2e_broadcast_tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_broadcast_pipeline_mock_audio() {
        // Setup
        let keypair = Keypair::generate_ed25519().unwrap();
        let vouch = create_test_vouch(&keypair);
        let audio = MockAudioSource::from_sine_wave(440.0, 1000, 48000); // 1 second

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let mock_publisher = MockPublisher::new(tx);

        // Run broadcast
        let config = BroadcastConfig {
            stream_id: "test".to_string(),
            bitrate: 128,
            encrypted: false,
        };

        let handle = tokio::spawn(async move {
            broadcast_pipeline(&keypair, &vouch, audio, mock_publisher, config).await
        });

        // Collect published messages
        let mut messages = Vec::new();
        while let Ok(Some(msg)) = timeout(Duration::from_secs(2), rx.recv()).await {
            messages.push(msg);
        }

        handle.await.unwrap().unwrap();

        // Verify announcement came first
        assert!(matches!(messages[0], PublishedMessage::Announcement(_)));

        // Verify chunks followed
        let chunks: Vec<_> = messages.iter()
            .filter_map(|m| match m {
                PublishedMessage::Chunk(c) => Some(c),
                _ => None,
            })
            .collect();

        // ~50 chunks for 1 second at 20ms
        assert!(chunks.len() >= 45 && chunks.len() <= 55);

        // Verify sequence numbers
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.seq, i as u64);
        }
    }

    #[tokio::test]
    async fn test_broadcast_pipeline_encrypted() {
        let keypair = Keypair::generate_ed25519().unwrap();
        let vouch = create_test_vouch(&keypair);
        let audio = MockAudioSource::from_silence(100, 48000); // 100ms

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let mock_publisher = MockPublisher::new(tx);

        let config = BroadcastConfig {
            stream_id: "encrypted-test".to_string(),
            bitrate: 128,
            encrypted: true,
        };

        broadcast_pipeline(&keypair, &vouch, audio, mock_publisher, config).await.unwrap();

        while let Some(msg) = rx.recv().await {
            if let PublishedMessage::Chunk(chunk) = msg {
                assert!(chunk.is_encrypted());
                assert!(chunk.nonce.is_some());
            }
        }
    }
}
```

---

## 10. Error Handling Tests

### Error Types

```rust
#[derive(Debug, Error)]
pub enum BroadcastError {
    #[error("keypair not found: {0}")]
    KeypairNotFound(String),

    #[error("invalid keypair: {0}")]
    InvalidKeypair(String),

    #[error("vouch not found: {0}")]
    VouchNotFound(String),

    #[error("invalid vouch: {0}")]
    InvalidVouch(String),

    #[error("vouch expired")]
    VouchExpired,

    #[error("vouch subject mismatch")]
    VouchSubjectMismatch,

    #[error("audio input error: {0}")]
    AudioInputError(String),

    #[error("encoding error: {0}")]
    EncodingError(String),

    #[error("encryption error: {0}")]
    EncryptionError(String),

    #[error("network error: {0}")]
    NetworkError(String),

    #[error("dht error: {0}")]
    DhtError(String),

    #[error("gossipsub error: {0}")]
    GossipsubError(String),
}
```

### Error Test Cases

| Test Name | Description | Expected Error |
|-----------|-------------|----------------|
| `test_error_keypair_not_found` | Missing keypair file | `KeypairNotFound` |
| `test_error_vouch_not_found` | Missing vouch file | `VouchNotFound` |
| `test_error_vouch_expired` | Expired vouch | `VouchExpired` |
| `test_error_vouch_subject_mismatch` | Wrong identity in vouch | `VouchSubjectMismatch` |
| `test_error_audio_file_not_found` | Missing audio file | `AudioInputError` |
| `test_error_audio_invalid_format` | Non-audio file | `AudioInputError` |
| `test_error_encoding_failed` | Opus encoding failure | `EncodingError` |
| `test_error_encryption_failed` | Encryption failure | `EncryptionError` |
| `test_error_dht_publish_failed` | DHT unreachable | `DhtError` |
| `test_error_gossipsub_publish_failed` | No peers connected | `GossipsubError` |

### Error Recovery Tests

| Test Name | Description | Expected |
|-----------|-------------|----------|
| `test_transient_network_error_retries` | Retry on temporary failure | Eventually succeeds |
| `test_persistent_network_error_fails` | Fail after max retries | Error returned |
| `test_graceful_shutdown_on_error` | Clean up on fatal error | Resources released |

---

## 11. Mock/Stub Strategies Summary

### Audio I/O Mocking

```rust
/// Trait-based audio source for testability
pub trait AudioSource: Send {
    fn read_frame(&mut self) -> Result<AudioFrame, AudioError>;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u8;
    fn is_eof(&self) -> bool;
}

/// Production implementation
pub struct CpalAudioSource { /* cpal device */ }

/// File-based implementation
pub struct FileAudioSource { /* symphonia decoder */ }

/// Test mock
pub struct MockAudioSource {
    frames: VecDeque<AudioFrame>,
    // ...
}
```

### Network Mocking

```rust
/// Trait-based publisher for testability
pub trait ChunkPublisher: Send {
    async fn publish_announcement(&self, ann: &StreamAnnouncement) -> Result<(), PublishError>;
    async fn publish_chunk(&self, chunk: &Chunk) -> Result<(), PublishError>;
}

/// Production implementation
pub struct GossipsubPublisher {
    swarm: MdrnSwarm,
}

/// Test mock
pub struct MockPublisher {
    tx: mpsc::Sender<PublishedMessage>,
}
```

### Time Mocking

```rust
// Use tokio::time::pause() for timing tests
#[tokio::test]
async fn test_chunk_timing() {
    tokio::time::pause();

    // Test timing without wall-clock waits
    tokio::time::advance(Duration::from_millis(20)).await;
}
```

---

## 12. Test Fixtures

### Required Test Files

```
mdrn-cli/tests/fixtures/
    audio/
        test_mono_48khz.wav          # 1 second mono 48kHz
        test_stereo_48khz.wav        # 1 second stereo 48kHz
        test_mono_44100hz.wav        # For resampling tests
        test_silence_1sec.wav        # Silence for DTX tests
        invalid.wav                  # Corrupted/invalid
    keypairs/
        test_ed25519.cbor           # Valid Ed25519 keypair
        test_secp256k1.cbor         # Valid secp256k1 keypair
        invalid.cbor                # Corrupted keypair
    vouches/
        test_valid.cbor             # Valid vouch
        test_expired.cbor           # Expired vouch
        test_wrong_subject.cbor     # Subject != test keypair
```

### Fixture Generation Script

```rust
// tests/generate_fixtures.rs
fn main() {
    // Generate keypairs
    let kp = Keypair::generate_ed25519().unwrap();
    save_keypair(&kp, "fixtures/keypairs/test_ed25519.cbor");

    // Generate vouches
    let issuer = Keypair::generate_ed25519().unwrap();
    let vouch = Vouch::create(kp.identity().clone(), &issuer, None).unwrap();
    save_vouch(&vouch, "fixtures/vouches/test_valid.cbor");

    // Generate audio (use hound crate)
    generate_wav_file(
        "fixtures/audio/test_mono_48khz.wav",
        48000, 1, Duration::from_secs(1)
    );
}
```

---

## 13. Test Organization

### Directory Structure

```
mdrn-cli/
    src/
        main.rs
        broadcast/
            mod.rs              # broadcast subcommand
            config.rs           # BroadcastConfig
            pipeline.rs         # Main broadcast pipeline
            audio.rs            # AudioSource trait + impls
            encoder.rs          # Opus encoding wrapper
            chunker.rs          # ChunkBuilder
            publisher.rs        # ChunkPublisher trait + impls
            error.rs            # BroadcastError
    tests/
        fixtures/               # Test files (audio, keypairs, vouches)
        broadcast_tests.rs      # Integration tests
        helpers.rs              # Test utilities

mdrn-core/
    src/
        ...                     # Existing code
    tests/
        crypto_tests.rs         # Encryption unit tests (if not inline)
```

### Test Categories

| Category | Location | Async | Network |
|----------|----------|-------|---------|
| Keypair loading | inline `#[cfg(test)]` | No | No |
| Vouch loading | inline `#[cfg(test)]` | No | No |
| Audio parsing | inline `#[cfg(test)]` | No | No |
| Opus encoding | inline `#[cfg(test)]` | No | No |
| Chunk building | inline `#[cfg(test)]` | No | No |
| Encryption | `mdrn-core` inline | No | No |
| Announcement | `mdrn-core` inline | No | No |
| Pipeline (mock) | `tests/broadcast_tests.rs` | Yes | No |
| Pipeline (real network) | `tests/broadcast_tests.rs` | Yes | Yes |
| CLI parsing | inline `#[cfg(test)]` | No | No |

---

## 14. Acceptance Criteria Summary

The broadcast command is complete when:

1. **Keypair Loading**
   - [ ] Load from file (CBOR)
   - [ ] Default location support
   - [ ] Error on invalid/missing

2. **Vouch Loading**
   - [ ] Load from file (CBOR)
   - [ ] Validate signature
   - [ ] Validate expiration
   - [ ] Validate subject matches

3. **Audio Input**
   - [ ] Read WAV files
   - [ ] Detect sample rate/channels
   - [ ] Handle EOF gracefully
   - [ ] (Future) Device input

4. **Opus Encoding**
   - [ ] Encode 20/40/60ms frames
   - [ ] Respect bitrate setting
   - [ ] Handle mono and stereo

5. **Chunking**
   - [ ] Increment sequence numbers
   - [ ] Set correct timestamps
   - [ ] CBOR serialization

6. **Encryption**
   - [ ] ChaCha20-Poly1305 per-chunk
   - [ ] Unique nonces
   - [ ] Optional (--encrypted flag)

7. **Announcement**
   - [ ] Publish to DHT
   - [ ] Correct stream_addr derivation
   - [ ] Include vouch

8. **Chunk Publishing**
   - [ ] Publish to correct gossipsub topic
   - [ ] Maintain real-time rate
   - [ ] Handle network errors

9. **Error Handling**
   - [ ] All error types defined
   - [ ] Graceful shutdown
   - [ ] Clear error messages

---

## Notes

- **Audio library**: Consider `symphonia` for decoding, `cpal` for device input, `opus` crate for encoding
- **Test audio generation**: Use `hound` crate to generate WAV fixtures
- **Timing tests**: Use `tokio::time::pause()` to avoid flaky timing-dependent tests
- **Real network tests**: Mark with `#[ignore]` for CI, run manually or in dedicated test suite
- **Bitrate verification**: Opus VBR makes exact size prediction difficult; test ranges
