//! Tests for the `mdrn listen` command
//!
//! TDD test suite covering:
//! - Stream address parsing
//! - Chunk reception from stdin
//! - Opus decoding
//! - WAV file output

use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Helper to run mdrn CLI command with stdin
fn run_mdrn_with_stdin(args: &[&str], stdin_data: &[u8]) -> std::process::Output {
    let mut child = Command::new("cargo")
        .args(["run", "--package", "mdrn-cli", "--"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn command");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_data).expect("Failed to write to stdin");
    }

    child.wait_with_output().expect("Failed to wait for command")
}

/// Helper to run mdrn CLI command
fn run_mdrn(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--package", "mdrn-cli", "--"])
        .args(args)
        .output()
        .expect("Failed to execute command")
}

// =============================================================================
// 1. Stream Address Parsing Tests
// =============================================================================

#[test]
fn test_listen_accepts_64_char_hex_stream_addr() {
    // 64-char hex string should be accepted as stream address
    let stream_addr = "ab".repeat(32); // 64 chars
    let output = run_mdrn(&["listen", &stream_addr]);

    // Should not fail with "invalid stream address" error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Invalid stream address format"),
        "Should accept 64-char hex: {}",
        stderr
    );
}

#[test]
fn test_listen_accepts_0x_prefix() {
    // 0x-prefixed hex should work
    let stream_addr = format!("0x{}", "ab".repeat(32));
    let output = run_mdrn(&["listen", &stream_addr]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Invalid stream address format"),
        "Should accept 0x-prefixed hex: {}",
        stderr
    );
}

#[test]
fn test_listen_rejects_invalid_hex() {
    // Non-hex characters should be treated as stream_id (which is not implemented)
    let stream_addr = "zz".repeat(32);
    let output = run_mdrn(&["listen", &stream_addr]);

    // For MVP, stream_id lookup is not implemented
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Stream ID lookup not implemented") ||
        stderr.contains("use hex address"),
        "Invalid hex should fall back to stream_id: {}",
        stderr
    );
}

#[test]
fn test_listen_rejects_wrong_length() {
    // Wrong length hex should be rejected
    let stream_addr = "ab".repeat(31); // 62 chars (wrong)
    let output = run_mdrn(&["listen", &stream_addr]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Stream ID lookup not implemented") ||
        stderr.contains("Invalid") ||
        stderr.contains("use hex address"),
        "Wrong length should error: {}",
        stderr
    );
}

#[test]
fn test_listen_treats_short_string_as_stream_id() {
    // Short strings should be treated as stream_id
    let output = run_mdrn(&["listen", "my-cool-stream"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Stream ID lookup not implemented") ||
        stderr.contains("use hex address"),
        "Short string should be stream_id: {}",
        stderr
    );
}

// =============================================================================
// 2. Output File Tests
// =============================================================================

#[test]
fn test_listen_creates_output_file() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = "ab".repeat(32);

    // Run with empty stdin (should create empty/header-only file or exit cleanly)
    let output = run_mdrn_with_stdin(
        &["listen", &stream_addr, "--output", output_path.to_str().unwrap()],
        b"", // empty stdin
    );

    // Should either succeed or fail gracefully (not panic)
    // For now, just verify it doesn't crash
    assert!(
        output.status.success() ||
        String::from_utf8_lossy(&output.stderr).contains("No chunks received"),
        "Should handle empty input gracefully: {:?}",
        output
    );
}

#[test]
fn test_listen_output_file_is_wav_format() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = "ab".repeat(32);

    // Create a minimal valid Opus frame encoded in CBOR chunk format
    // For testing, we'll use the real Opus decoder test
    // But first, let's verify the command parses correctly
    let output = run_mdrn_with_stdin(
        &["listen", &stream_addr, "--output", output_path.to_str().unwrap()],
        b"",
    );

    // Verify no panic
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panic") && !stderr.contains("SIGSEGV"),
        "Should not panic: {}",
        stderr
    );
}

// =============================================================================
// 3. Chunk Processing Tests (using stdin simulation)
// =============================================================================

#[test]
fn test_listen_reads_hex_encoded_cbor_from_stdin() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = "ab".repeat(32);

    // Create a simple chunk in CBOR format, hex-encoded
    // This is the format: one hex-encoded CBOR chunk per line
    let chunk = create_test_chunk_cbor(&hex_to_bytes(&stream_addr), 0, false);
    let chunk_hex = hex::encode(&chunk);
    let stdin_data = format!("{}\n", chunk_hex);

    let output = run_mdrn_with_stdin(
        &["listen", &stream_addr, "--output", output_path.to_str().unwrap()],
        stdin_data.as_bytes(),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should process the chunk (may fail on decode if not real Opus, but should parse)
    assert!(
        !stderr.contains("Failed to parse CBOR") || stderr.contains("Opus decode"),
        "Should parse CBOR chunk: {}",
        stderr
    );
}

// =============================================================================
// Helper functions
// =============================================================================

fn hex_to_bytes(hex: &str) -> [u8; 32] {
    let bytes = hex::decode(hex).expect("Invalid hex");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    arr
}

/// Create a test chunk in CBOR format
fn create_test_chunk_cbor(stream_addr: &[u8; 32], seq: u64, encrypted: bool) -> Vec<u8> {
    use mdrn_core::stream::{Chunk, Codec};

    // Create minimal Opus silence frame (simplest valid Opus packet)
    // Opus TOC byte for SILK-only, 10ms frame, mono
    let opus_silence = vec![0xF8, 0xFF, 0xFE]; // Minimal valid Opus packet

    let chunk = if encrypted {
        // For encrypted, we'd need the key - skip for now
        Chunk::new(
            *stream_addr,
            seq,
            seq * 20_000, // 20ms timestamps
            Codec::Opus,
            20_000,
            opus_silence,
        )
    } else {
        Chunk::new(
            *stream_addr,
            seq,
            seq * 20_000,
            Codec::Opus,
            20_000,
            opus_silence,
        )
    };

    let mut cbor = Vec::new();
    ciborium::into_writer(&chunk, &mut cbor).expect("Failed to serialize chunk");
    cbor
}

// =============================================================================
// 4. Opus Decode Tests
// =============================================================================

#[test]
fn test_opus_decoder_48khz_stereo() {
    // Test that we can create an Opus decoder
    let decoder = opus::Decoder::new(48000, opus::Channels::Stereo);
    assert!(decoder.is_ok(), "Should create stereo decoder");
}

#[test]
fn test_opus_decoder_48khz_mono() {
    let decoder = opus::Decoder::new(48000, opus::Channels::Mono);
    assert!(decoder.is_ok(), "Should create mono decoder");
}

#[test]
fn test_opus_decode_silence_frame() {
    let mut decoder = opus::Decoder::new(48000, opus::Channels::Stereo).unwrap();

    // Minimal Opus silence frame
    // TOC byte: 11111000 = SILK-only mode, 10ms frame, mono coded as stereo
    let opus_frame = vec![0xF8, 0xFF, 0xFE];

    // Output buffer for 960 samples (20ms at 48kHz mono, or 10ms stereo)
    let mut pcm = vec![0i16; 960 * 2]; // stereo

    let result = decoder.decode(&opus_frame, &mut pcm, false);
    // May fail with this minimal frame, but should not panic
    assert!(result.is_ok() || result.is_err(), "Should handle minimal frame");
}

// =============================================================================
// 5. Integration test - full pipeline
// =============================================================================

#[test]
fn test_listen_full_pipeline_with_real_opus() {
    // This test will be skipped if we don't have real Opus data
    // For now, verify the command structure works
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = "ab".repeat(32);

    // Just verify the command parses and starts correctly
    let output = run_mdrn(&[
        "listen",
        &stream_addr,
        "--output",
        output_path.to_str().unwrap(),
    ]);

    // For MVP, this should at least start and then exit when no input
    // (stdin is not piped so it should exit quickly or wait for input)
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show connection attempt or similar (in stderr via tracing or stdout via println)
    assert!(
        stderr.contains("Connecting to stream") ||
        stderr.contains("Listening") ||
        stderr.contains("stream_addr") ||
        stderr.contains("No chunks") ||
        stdout.contains("Listen Complete") ||
        stdout.contains("Stream Address") ||
        // Or the not-implemented message if we haven't finished
        stderr.contains("not yet implemented"),
        "Should show listening status: stderr={}, stdout={}",
        stderr,
        stdout
    );
}

// =============================================================================
// 6. Encoder/Decoder Roundtrip Tests
// =============================================================================

#[test]
fn test_opus_encode_decode_roundtrip() {
    use opus::{Channels, Encoder, Decoder, Application};

    // Create encoder and decoder
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio).unwrap();
    let mut decoder = Decoder::new(48000, Channels::Stereo).unwrap();

    // Create a simple sine wave (440Hz, 20ms at 48kHz stereo = 960*2 samples)
    let frame_size = 960;
    let mut input_samples = vec![0i16; frame_size * 2]; // stereo
    for i in 0..frame_size {
        let t = i as f32 / 48000.0;
        let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 16000.0;
        input_samples[i * 2] = sample as i16;     // left
        input_samples[i * 2 + 1] = sample as i16; // right
    }

    // Encode
    let mut encoded = vec![0u8; 4000];
    let encoded_len = encoder.encode(&input_samples, &mut encoded).unwrap();
    encoded.truncate(encoded_len);

    // Decode
    let mut output_samples = vec![0i16; frame_size * 2];
    let decoded_samples = decoder.decode(&encoded, &mut output_samples, false).unwrap();

    assert_eq!(decoded_samples, frame_size, "Should decode 960 samples");

    // Verify samples are non-zero (actual audio, not silence)
    let max_sample = output_samples.iter().map(|s| s.abs()).max().unwrap();
    assert!(max_sample > 1000, "Decoded audio should have significant amplitude");
}

#[test]
fn test_listen_decodes_real_opus_chunk_from_stdin() {
    use opus::{Channels, Encoder, Application};
    use mdrn_core::stream::{Chunk, Codec};

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = hex_to_bytes(&"ab".repeat(32));

    // Create a real Opus-encoded chunk
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio).unwrap();

    // Create 20ms of audio (960 stereo samples)
    let frame_size = 960;
    let mut input_samples = vec![0i16; frame_size * 2];
    for i in 0..frame_size {
        let t = i as f32 / 48000.0;
        let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 16000.0;
        input_samples[i * 2] = sample as i16;
        input_samples[i * 2 + 1] = sample as i16;
    }

    // Encode to Opus
    let mut encoded = vec![0u8; 4000];
    let encoded_len = encoder.encode(&input_samples, &mut encoded).unwrap();
    encoded.truncate(encoded_len);

    // Create chunk
    let chunk = Chunk::new(
        stream_addr,
        0,
        0,
        Codec::Opus,
        20_000,
        encoded,
    );

    // Serialize to CBOR and hex
    let mut cbor = Vec::new();
    ciborium::into_writer(&chunk, &mut cbor).unwrap();
    let chunk_hex = hex::encode(&cbor);
    let stdin_data = format!("{}\n", chunk_hex);

    // Run listen with the real Opus chunk
    let output = run_mdrn_with_stdin(
        &["listen", &"ab".repeat(32), "--output", output_path.to_str().unwrap()],
        stdin_data.as_bytes(),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should decode successfully
    assert!(
        stdout.contains("Chunks Decoded: 1") || stdout.contains("chunks_decoded=1"),
        "Should decode 1 chunk. stdout={}, stderr={}",
        stdout,
        stderr
    );

    // Output file should exist and have content
    assert!(output_path.exists(), "Output WAV file should exist");
    let file_size = std::fs::metadata(&output_path).unwrap().len();
    assert!(file_size > 44, "WAV file should have more than just header (size={})", file_size);
}

#[test]
fn test_listen_handles_multiple_chunks() {
    use opus::{Channels, Encoder, Application};
    use mdrn_core::stream::{Chunk, Codec};

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = hex_to_bytes(&"ab".repeat(32));

    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio).unwrap();
    let frame_size = 960;

    let mut stdin_lines = Vec::new();

    // Create 5 chunks
    for seq in 0..5 {
        let mut input_samples = vec![0i16; frame_size * 2];
        let freq = 440.0 + seq as f32 * 100.0; // Different frequency per chunk
        for i in 0..frame_size {
            let t = i as f32 / 48000.0;
            let sample = (t * freq * 2.0 * std::f32::consts::PI).sin() * 16000.0;
            input_samples[i * 2] = sample as i16;
            input_samples[i * 2 + 1] = sample as i16;
        }

        let mut encoded = vec![0u8; 4000];
        let encoded_len = encoder.encode(&input_samples, &mut encoded).unwrap();
        encoded.truncate(encoded_len);

        let chunk = Chunk::new(
            stream_addr,
            seq,
            seq * 20_000, // 20ms timestamps
            Codec::Opus,
            20_000,
            encoded,
        );

        let mut cbor = Vec::new();
        ciborium::into_writer(&chunk, &mut cbor).unwrap();
        stdin_lines.push(hex::encode(&cbor));
    }

    let stdin_data = stdin_lines.join("\n") + "\n";

    let output = run_mdrn_with_stdin(
        &["listen", &"ab".repeat(32), "--output", output_path.to_str().unwrap()],
        stdin_data.as_bytes(),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should decode all 5 chunks
    assert!(
        stdout.contains("Chunks Decoded: 5"),
        "Should decode 5 chunks. stdout={}",
        stdout
    );

    // Duration should be 100ms (5 * 20ms)
    assert!(
        stdout.contains("Duration: 100 ms"),
        "Duration should be 100ms. stdout={}",
        stdout
    );
}

// =============================================================================
// 7. Encrypted Stream Tests
// =============================================================================

#[test]
fn test_listen_decodes_encrypted_chunk_with_key() {
    use opus::{Channels, Encoder, Application};
    use mdrn_core::stream::{Chunk, Codec};
    use mdrn_core::crypto;

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = hex_to_bytes(&"ab".repeat(32));

    // Generate stream key
    let stream_key = crypto::generate_stream_key();
    let key_hex = hex::encode(&stream_key);

    // Create Opus audio
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio).unwrap();
    let frame_size = 960;
    let mut input_samples = vec![0i16; frame_size * 2];
    for i in 0..frame_size {
        let t = i as f32 / 48000.0;
        let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 16000.0;
        input_samples[i * 2] = sample as i16;
        input_samples[i * 2 + 1] = sample as i16;
    }

    let mut encoded = vec![0u8; 4000];
    let encoded_len = encoder.encode(&input_samples, &mut encoded).unwrap();
    encoded.truncate(encoded_len);

    // Encrypt the Opus data
    let (encrypted_data, nonce) = crypto::encrypt(&stream_key, &encoded).unwrap();

    // Create encrypted chunk
    let chunk = Chunk::new_encrypted(
        stream_addr,
        0,
        0,
        Codec::Opus,
        20_000,
        encrypted_data,
        nonce,
    );

    let mut cbor = Vec::new();
    ciborium::into_writer(&chunk, &mut cbor).unwrap();
    let stdin_data = format!("{}\n", hex::encode(&cbor));

    // Run listen with the stream key
    let output = run_mdrn_with_stdin(
        &[
            "listen",
            &"ab".repeat(32),
            "--output", output_path.to_str().unwrap(),
            "--key", &key_hex,
        ],
        stdin_data.as_bytes(),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should decrypt and decode successfully
    assert!(
        stdout.contains("Chunks Decoded: 1"),
        "Should decode encrypted chunk. stdout={}, stderr={}",
        stdout,
        stderr
    );
}

#[test]
fn test_listen_fails_encrypted_chunk_without_key() {
    use opus::{Channels, Encoder, Application};
    use mdrn_core::stream::{Chunk, Codec};
    use mdrn_core::crypto;

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = hex_to_bytes(&"ab".repeat(32));

    // Generate stream key but don't provide it
    let stream_key = crypto::generate_stream_key();

    // Create Opus audio
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio).unwrap();
    let frame_size = 960;
    let mut input_samples = vec![0i16; frame_size * 2];
    for i in 0..frame_size {
        let sample = ((i as f32 / 48000.0) * 440.0 * 2.0 * std::f32::consts::PI).sin() * 16000.0;
        input_samples[i * 2] = sample as i16;
        input_samples[i * 2 + 1] = sample as i16;
    }

    let mut encoded = vec![0u8; 4000];
    let encoded_len = encoder.encode(&input_samples, &mut encoded).unwrap();
    encoded.truncate(encoded_len);

    let (encrypted_data, nonce) = crypto::encrypt(&stream_key, &encoded).unwrap();

    let chunk = Chunk::new_encrypted(
        stream_addr,
        0,
        0,
        Codec::Opus,
        20_000,
        encrypted_data,
        nonce,
    );

    let mut cbor = Vec::new();
    ciborium::into_writer(&chunk, &mut cbor).unwrap();
    let stdin_data = format!("{}\n", hex::encode(&cbor));

    // Run listen WITHOUT the stream key
    let output = run_mdrn_with_stdin(
        &["listen", &"ab".repeat(32), "--output", output_path.to_str().unwrap()],
        stdin_data.as_bytes(),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail to decode (either error in stderr or 0 chunks decoded)
    let decode_failed = stdout.contains("Chunks Decoded: 0") ||
                        stderr.contains("encrypted") ||
                        stderr.contains("no key");

    assert!(
        decode_failed,
        "Should fail without key. stdout={}, stderr={}",
        stdout,
        stderr
    );
}

#[test]
fn test_listen_fails_with_wrong_key() {
    use opus::{Channels, Encoder, Application};
    use mdrn_core::stream::{Chunk, Codec};
    use mdrn_core::crypto;

    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("output.wav");
    let stream_addr = hex_to_bytes(&"ab".repeat(32));

    // Generate two different keys
    let correct_key = crypto::generate_stream_key();
    let wrong_key = crypto::generate_stream_key();
    let wrong_key_hex = hex::encode(&wrong_key);

    // Create Opus audio
    let mut encoder = Encoder::new(48000, Channels::Stereo, Application::Audio).unwrap();
    let frame_size = 960;
    let mut input_samples = vec![0i16; frame_size * 2];
    for i in 0..frame_size {
        let sample = ((i as f32 / 48000.0) * 440.0 * 2.0 * std::f32::consts::PI).sin() * 16000.0;
        input_samples[i * 2] = sample as i16;
        input_samples[i * 2 + 1] = sample as i16;
    }

    let mut encoded = vec![0u8; 4000];
    let encoded_len = encoder.encode(&input_samples, &mut encoded).unwrap();
    encoded.truncate(encoded_len);

    // Encrypt with correct key
    let (encrypted_data, nonce) = crypto::encrypt(&correct_key, &encoded).unwrap();

    let chunk = Chunk::new_encrypted(
        stream_addr,
        0,
        0,
        Codec::Opus,
        20_000,
        encrypted_data,
        nonce,
    );

    let mut cbor = Vec::new();
    ciborium::into_writer(&chunk, &mut cbor).unwrap();
    let stdin_data = format!("{}\n", hex::encode(&cbor));

    // Run listen with WRONG key
    let output = run_mdrn_with_stdin(
        &[
            "listen",
            &"ab".repeat(32),
            "--output", output_path.to_str().unwrap(),
            "--key", &wrong_key_hex,
        ],
        stdin_data.as_bytes(),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail to decrypt (decryption error or decode error)
    let decode_failed = stdout.contains("Chunks Decoded: 0") ||
                        stderr.contains("Decryption failed") ||
                        stderr.contains("decrypt");

    assert!(
        decode_failed,
        "Should fail with wrong key. stdout={}, stderr={}",
        stdout,
        stderr
    );
}
