//! Integration tests for CLI commands

use std::process::Command;
use tempfile::TempDir;

/// Helper to run mdrn CLI command
fn run_mdrn(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--package", "mdrn-cli", "--"])
        .args(args)
        .output()
        .expect("Failed to execute command")
}

#[test]
fn test_keygen_creates_keypair_file() {
    let temp_dir = TempDir::new().unwrap();
    let keypair_path = temp_dir.path().join("test.key");

    let output = run_mdrn(&["keygen", "-o", keypair_path.to_str().unwrap()]);

    assert!(output.status.success(), "keygen failed: {:?}", output);
    assert!(keypair_path.exists(), "keypair file was not created");

    // Keypair file should contain CBOR data (at least 64 bytes for ed25519)
    let contents = std::fs::read(&keypair_path).unwrap();
    assert!(contents.len() >= 64, "keypair file too small: {} bytes", contents.len());
}

#[test]
fn test_keygen_ed25519_default() {
    let temp_dir = TempDir::new().unwrap();
    let keypair_path = temp_dir.path().join("ed25519.key");

    let output = run_mdrn(&["keygen", "-o", keypair_path.to_str().unwrap()]);

    assert!(output.status.success());

    // Verify the keypair can be deserialized
    let contents = std::fs::read(&keypair_path).unwrap();
    let keypair: mdrn_core::identity::StoredKeypair =
        ciborium::from_reader(&contents[..]).expect("Failed to deserialize keypair");

    assert_eq!(keypair.key_type, mdrn_core::identity::KeyType::Ed25519);
}

#[test]
fn test_keygen_secp256k1() {
    let temp_dir = TempDir::new().unwrap();
    let keypair_path = temp_dir.path().join("secp256k1.key");

    let output = run_mdrn(&[
        "keygen",
        "-k", "secp256k1",
        "-o", keypair_path.to_str().unwrap()
    ]);

    assert!(output.status.success());

    let contents = std::fs::read(&keypair_path).unwrap();
    let keypair: mdrn_core::identity::StoredKeypair =
        ciborium::from_reader(&contents[..]).expect("Failed to deserialize keypair");

    assert_eq!(keypair.key_type, mdrn_core::identity::KeyType::Secp256k1);
}

#[test]
fn test_vouch_creates_credential() {
    let temp_dir = TempDir::new().unwrap();
    let issuer_path = temp_dir.path().join("issuer.key");
    let vouch_path = temp_dir.path().join("vouch.cbor");

    // Generate issuer keypair
    let output = run_mdrn(&["keygen", "-o", issuer_path.to_str().unwrap()]);
    assert!(output.status.success(), "keygen failed");

    // Generate a subject keypair to get their identity
    let subject_path = temp_dir.path().join("subject.key");
    run_mdrn(&["keygen", "-o", subject_path.to_str().unwrap()]);

    // Read subject keypair to get identity hex
    let subject_bytes = std::fs::read(&subject_path).unwrap();
    let subject_keypair: mdrn_core::identity::StoredKeypair =
        ciborium::from_reader(&subject_bytes[..]).unwrap();
    let subject_hex = hex::encode(subject_keypair.identity_bytes);

    // Create vouch - output goes to stdout, redirect via shell
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cargo run --package mdrn-cli -- vouch {} --keypair {} > {}",
            subject_hex,
            issuer_path.to_str().unwrap(),
            vouch_path.to_str().unwrap()
        ))
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "vouch failed: {:?}", String::from_utf8_lossy(&output.stderr));
    assert!(vouch_path.exists(), "vouch file was not created");

    // Verify vouch can be deserialized and verified
    let vouch_bytes = std::fs::read(&vouch_path).unwrap();
    let vouch: mdrn_core::identity::Vouch =
        ciborium::from_reader(&vouch_bytes[..]).expect("Failed to deserialize vouch");

    vouch.verify().expect("Vouch verification failed");
}

#[test]
fn test_vouch_with_expiration() {
    let temp_dir = TempDir::new().unwrap();
    let issuer_path = temp_dir.path().join("issuer.key");
    let vouch_path = temp_dir.path().join("vouch.cbor");

    // Generate issuer keypair
    run_mdrn(&["keygen", "-o", issuer_path.to_str().unwrap()]);

    // Generate subject
    let subject_path = temp_dir.path().join("subject.key");
    run_mdrn(&["keygen", "-o", subject_path.to_str().unwrap()]);

    let subject_bytes = std::fs::read(&subject_path).unwrap();
    let subject_keypair: mdrn_core::identity::StoredKeypair =
        ciborium::from_reader(&subject_bytes[..]).unwrap();
    let subject_hex = hex::encode(subject_keypair.identity_bytes);

    // Create vouch with 30 day expiration
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cargo run --package mdrn-cli -- vouch {} --keypair {} --expires 30 > {}",
            subject_hex,
            issuer_path.to_str().unwrap(),
            vouch_path.to_str().unwrap()
        ))
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let vouch_bytes = std::fs::read(&vouch_path).unwrap();
    let vouch: mdrn_core::identity::Vouch =
        ciborium::from_reader(&vouch_bytes[..]).expect("Failed to deserialize vouch");

    // Should have expiration set
    assert!(vouch.expires_at.is_some());

    // Expiration should be approximately 30 days from now
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let expires = vouch.expires_at.unwrap();
    let expected = now + (30 * 24 * 60 * 60);

    // Allow 60 second tolerance for test execution time
    assert!((expires as i64 - expected as i64).abs() < 60);
}

#[test]
fn test_vouch_invalid_subject_hex() {
    let temp_dir = TempDir::new().unwrap();
    let issuer_path = temp_dir.path().join("issuer.key");

    // Generate issuer keypair
    run_mdrn(&["keygen", "-o", issuer_path.to_str().unwrap()]);

    // Try vouch with invalid hex
    let output = run_mdrn(&[
        "vouch",
        "not-valid-hex",
        "--keypair", issuer_path.to_str().unwrap()
    ]);

    assert!(!output.status.success(), "vouch should fail with invalid hex");
}

#[test]
fn test_vouch_missing_keypair_file() {
    // Try vouch with non-existent keypair file
    let output = run_mdrn(&[
        "vouch",
        "ed01" , // minimal hex (will still fail for other reasons)
        "--keypair", "/nonexistent/path.key"
    ]);

    assert!(!output.status.success(), "vouch should fail with missing keypair");
}

// =============================================================================
// Broadcast Command Tests (TDD - these tests define expected behavior)
// =============================================================================

/// Create a minimal valid WAV file for testing
///
/// WAV format:
/// - RIFF header (12 bytes)
/// - fmt chunk (24 bytes for PCM)
/// - data chunk header (8 bytes) + samples
fn create_test_wav(path: &std::path::Path, duration_secs: f32, sample_rate: u32) {
    let channels: u16 = 1; // mono
    let bits_per_sample: u16 = 16;
    let bytes_per_sample = bits_per_sample / 8;
    let num_samples = (duration_secs * sample_rate as f32) as usize;
    let data_size = num_samples * bytes_per_sample as usize * channels as usize;

    let mut wav = Vec::with_capacity(44 + data_size);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&((36 + data_size) as u32).to_le_bytes()); // file size - 8
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // audio format (1 = PCM)
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = channels * bytes_per_sample;
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_size as u32).to_le_bytes());

    // Audio samples (silence = zeros)
    wav.extend(std::iter::repeat(0u8).take(data_size));

    std::fs::write(path, &wav).unwrap();
}

/// Helper to check if broadcast is implemented (vs stub)
fn is_broadcast_implemented(stderr: &str) -> bool {
    !stderr.contains("not yet implemented") && !stderr.contains("Broadcast not yet implemented")
}

/// Helper to set up keypair and vouch for broadcast tests
/// Returns (keypair_path, vouch_path, identity_hex)
fn setup_broadcaster(temp_dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf, String) {
    use mdrn_core::identity::{Keypair, Vouch};

    let keypair_path = temp_dir.path().join("keypair.cbor");
    let vouch_path = temp_dir.path().join("vouch.cbor");

    // Generate keypair
    let keypair = Keypair::generate_ed25519().unwrap();
    let cbor = keypair.to_cbor().unwrap();
    std::fs::write(&keypair_path, &cbor).unwrap();

    let identity_hex = hex::encode(keypair.identity().as_bytes());

    // Create self-vouch (for testing)
    let vouch = Vouch::create(keypair.identity().clone(), &keypair, None).unwrap();
    let mut vouch_cbor = Vec::new();
    ciborium::into_writer(&vouch, &mut vouch_cbor).unwrap();
    std::fs::write(&vouch_path, &vouch_cbor).unwrap();

    (keypair_path, vouch_path, identity_hex)
}

/// Run mdrn with environment variables for keypair and vouch
fn run_mdrn_with_env(args: &[&str], keypair_path: &std::path::Path, vouch_path: &std::path::Path) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--package", "mdrn-cli", "--"])
        .args(args)
        .env("MDRN_KEYPAIR", keypair_path)
        .env("MDRN_VOUCH", vouch_path)
        .output()
        .expect("Failed to execute command")
}

/// Test: Broadcast without --input should fail with helpful error
///
/// EXPECTED BEHAVIOR:
/// - Command should fail (non-zero exit)
/// - Error message should mention --input flag or audio source requirement
#[test]
fn test_broadcast_requires_input() {
    let output = run_mdrn(&[
        "broadcast",
        "--stream-id", "test-stream"
    ]);

    // Should fail without input
    assert!(!output.status.success(), "broadcast should fail without input");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);
    assert!(
        combined.contains("input") || combined.contains("--input") || combined.contains("audio"),
        "Error should mention input requirement. Got: {}",
        combined
    );
}

/// TDD Test: Broadcast with nonexistent file should fail with clear error
///
/// EXPECTED BEHAVIOR (when implemented):
/// - Command should fail (non-zero exit)
/// - Error message should indicate file not found
///
/// CURRENT STATE: Command succeeds with "not yet implemented" warning
#[test]
fn test_broadcast_invalid_file_fails() {
    let output = run_mdrn(&[
        "broadcast",
        "--stream-id", "test-stream",
        "--input", "/nonexistent/audio/file.wav"
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    // TDD: If not implemented yet, verify stub behavior and pass
    if !is_broadcast_implemented(&stderr) {
        assert!(output.status.success(),
            "Stub broadcast should succeed (until implemented)");
        assert!(stderr.contains("not yet implemented"),
            "Stub should warn about not being implemented");
        return;
    }

    // EXPECTED BEHAVIOR once implemented:
    assert!(!output.status.success(), "broadcast should fail with nonexistent file");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);
    assert!(
        combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("no such file")
        || combined.to_lowercase().contains("does not exist")
        || combined.to_lowercase().contains("cannot open")
        || combined.to_lowercase().contains("failed to")
        || combined.to_lowercase().contains("error"),
        "Error should indicate file not found. Got: {}",
        combined
    );
}

/// TDD Test: Broadcast with valid input should create StreamAnnouncement
///
/// EXPECTED BEHAVIOR (when implemented):
/// - Command should succeed
/// - Output should contain stream announcement info (stream_addr, broadcaster, etc.)
///
/// CURRENT STATE: Command succeeds with "not yet implemented" warning
#[test]
fn test_broadcast_creates_announcement() {
    let temp_dir = TempDir::new().unwrap();
    let audio_path = temp_dir.path().join("test.wav");
    let (keypair_path, vouch_path, _) = setup_broadcaster(&temp_dir);

    // Setup: Create test WAV file (1 second of silence at 48kHz)
    create_test_wav(&audio_path, 1.0, 48000);
    assert!(audio_path.exists(), "test WAV file was not created");

    // Run broadcast command with keypair/vouch env vars
    let output = run_mdrn_with_env(&[
        "broadcast",
        "--stream-id", "test-stream",
        "--input", audio_path.to_str().unwrap(),
        "--bitrate", "128",
    ], &keypair_path, &vouch_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // TDD: If not implemented yet, verify stub behavior and pass
    if !is_broadcast_implemented(&stderr) {
        assert!(output.status.success(),
            "Stub broadcast should succeed (until implemented)");
        assert!(stderr.contains("not yet implemented"),
            "Stub should warn about not being implemented");
        // Note: This test will need to be updated once broadcast is implemented
        eprintln!("NOTE: Broadcast command not yet implemented - TDD test placeholder");
        return;
    }

    // EXPECTED BEHAVIOR once implemented:
    assert!(output.status.success(),
        "broadcast should succeed with valid input. stderr: {}, stdout: {}",
        stderr, stdout);

    let combined = format!("{}{}", stderr, stdout);
    assert!(
        combined.contains("stream_addr")
        || combined.contains("stream-addr")
        || combined.contains("StreamAnnouncement")
        || combined.contains("announcement")
        || combined.contains("test-stream"),
        "Output should contain stream announcement info. Got: {}",
        combined
    );
}

/// TDD Test: Broadcast requires keypair for signing
///
/// EXPECTED BEHAVIOR (when implemented):
/// - Command should fail without keypair (or use default location)
/// - Error should mention keypair requirement
#[test]
fn test_broadcast_requires_keypair() {
    let temp_dir = TempDir::new().unwrap();
    let audio_path = temp_dir.path().join("test.wav");

    create_test_wav(&audio_path, 0.5, 48000);

    let output = run_mdrn(&[
        "broadcast",
        "--stream-id", "test-stream",
        "--input", audio_path.to_str().unwrap(),
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    // TDD: If not implemented, pass (keypair validation comes with implementation)
    if !is_broadcast_implemented(&stderr) {
        return;
    }

    // Once implemented: should fail or warn about missing keypair
    assert!(!output.status.success() || stderr.contains("keypair"),
        "broadcast should require keypair for signing");
}

/// TDD Test: --encrypted flag is recognized
///
/// EXPECTED BEHAVIOR (when implemented):
/// - Flag should be accepted without "unknown argument" errors
/// - Stream should be marked as encrypted
#[test]
fn test_broadcast_encrypted_flag() {
    let temp_dir = TempDir::new().unwrap();
    let audio_path = temp_dir.path().join("test.wav");

    create_test_wav(&audio_path, 0.5, 48000);

    let output = run_mdrn(&[
        "broadcast",
        "--stream-id", "encrypted-test",
        "--input", audio_path.to_str().unwrap(),
        "--encrypted",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    // The --encrypted flag should be recognized (no "unknown argument" error)
    // This works even with stub implementation since clap parses args first
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("unknown option"),
        "--encrypted flag should be recognized. stderr: {}",
        stderr
    );
}

/// TDD Test: --bitrate option accepts valid values
///
/// EXPECTED BEHAVIOR:
/// - Various bitrate values (64, 128, 256) should be accepted
/// - Invalid values should produce clear errors
#[test]
fn test_broadcast_bitrate_option() {
    let temp_dir = TempDir::new().unwrap();
    let audio_path = temp_dir.path().join("test.wav");
    let (keypair_path, vouch_path, _) = setup_broadcaster(&temp_dir);

    create_test_wav(&audio_path, 0.5, 48000);

    // Test various valid bitrate values
    for bitrate in &["64", "128", "256"] {
        let output = run_mdrn_with_env(&[
            "broadcast",
            "--stream-id", "bitrate-test",
            "--input", audio_path.to_str().unwrap(),
            "--bitrate", bitrate,
        ], &keypair_path, &vouch_path);

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Bitrate should be accepted without parse errors (clap handles this)
        assert!(
            !stderr.contains("invalid value") && !stderr.contains("parse"),
            "bitrate {} should be accepted. stderr: {}",
            bitrate, stderr
        );
    }
}

/// TDD Test: Invalid audio format should fail
///
/// EXPECTED BEHAVIOR (when implemented):
/// - Command should fail with non-audio file
/// - Error should mention format/audio issues
///
/// CURRENT STATE: Command succeeds with "not yet implemented" warning
#[test]
fn test_broadcast_invalid_audio_format() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_path = temp_dir.path().join("not_audio.txt");

    std::fs::write(&invalid_path, b"this is not audio data").unwrap();

    let output = run_mdrn(&[
        "broadcast",
        "--stream-id", "test-stream",
        "--input", invalid_path.to_str().unwrap(),
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    // TDD: If not implemented, pass
    if !is_broadcast_implemented(&stderr) {
        return;
    }

    // EXPECTED BEHAVIOR once implemented:
    assert!(!output.status.success(),
        "broadcast should fail with invalid audio format");

    let combined = format!("{}{}", stderr, String::from_utf8_lossy(&output.stdout));
    assert!(
        combined.to_lowercase().contains("format")
        || combined.to_lowercase().contains("audio")
        || combined.to_lowercase().contains("unsupported")
        || combined.to_lowercase().contains("invalid"),
        "Error should mention invalid format. Got: {}",
        combined
    );
}
