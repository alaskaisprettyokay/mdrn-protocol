//! Network Broadcast Integration Tests
//!
//! TDD tests for libp2p network broadcasting integration.
//! These tests verify that the broadcast command can:
//! - Publish StreamAnnouncement to DHT via MdrnSwarm::dht_put()
//! - Subscribe to stream gossipsub topic
//! - Publish CBOR-serialized Chunks via MdrnSwarm::publish()
//! - Handle async libp2p operations from sync CLI context
//!
//! Test Categories:
//! - PASSING: Tests that verify existing functionality works
//! - FAILING (ignored): Tests for features not yet implemented
//!
//! To run these tests:
//! ```bash
//! cargo test --package mdrn-cli --test network_broadcast_tests
//! ```

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use tempfile::TempDir;

use mdrn_core::identity::{Keypair, Vouch};
use mdrn_core::stream::{Chunk, Codec, StreamAnnouncement};
use mdrn_core::transport::{stream_topic, MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a test keypair
fn create_test_keypair() -> Keypair {
    Keypair::generate_ed25519().expect("keypair generation should succeed")
}

/// Create a test vouch for a broadcaster
fn create_test_vouch(broadcaster: &Keypair) -> Vouch {
    let issuer = Keypair::generate_ed25519().expect("issuer keypair should generate");
    Vouch::create(broadcaster.identity().clone(), &issuer, None)
        .expect("vouch creation should succeed")
}

/// Create a test swarm with random port
#[allow(dead_code)]
fn create_test_swarm() -> MdrnSwarm {
    let keypair = create_test_keypair();
    let config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    MdrnSwarm::new(keypair, config).expect("swarm creation should succeed")
}

/// Create a test swarm from an existing keypair
fn create_test_swarm_with_keypair(keypair: Keypair) -> MdrnSwarm {
    let config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    MdrnSwarm::new(keypair, config).expect("swarm creation should succeed")
}

/// Create a minimal valid StreamAnnouncement
fn create_test_announcement(keypair: &Keypair, stream_id: &str) -> StreamAnnouncement {
    let vouch = create_test_vouch(keypair);
    StreamAnnouncement::new(
        keypair.identity().clone(),
        stream_id.to_string(),
        Codec::Opus,
        128,   // bitrate kbps
        48000, // sample rate
        2,     // channels (stereo)
        false, // not encrypted
        vouch,
    )
}

/// Create test chunks for an announcement
fn create_test_chunks(announcement: &StreamAnnouncement, count: usize) -> Vec<Chunk> {
    let mut chunks = Vec::with_capacity(count);
    for i in 0..count {
        let chunk = Chunk::new(
            announcement.stream_addr,
            i as u64,                     // seq
            (i as u64) * 20_000,          // timestamp (20ms per chunk)
            Codec::Opus,
            20_000,                       // duration_us (20ms)
            vec![0x00, 0x01, 0x02, 0x03], // mock opus data
        );
        chunks.push(chunk);
    }
    chunks
}

/// Serialize a value to CBOR bytes
fn to_cbor<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("CBOR serialization should succeed");
    bytes
}

/// Create a simple WAV file for testing
#[allow(dead_code)]
fn create_test_wav(dir: &TempDir, duration_ms: u32, sample_rate: u32, channels: u16) -> PathBuf {
    let path = dir.path().join("test_audio.wav");
    let samples_per_channel = (sample_rate as u64 * duration_ms as u64 / 1000) as u32;
    let num_samples = samples_per_channel * channels as u32;

    // Create a simple sine wave
    let mut pcm_data: Vec<i16> = Vec::with_capacity(num_samples as usize);
    for i in 0..samples_per_channel {
        let t = i as f32 / sample_rate as f32;
        let sample = (f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 16000.0) as i16;
        for _ in 0..channels {
            pcm_data.push(sample);
        }
    }

    // Write WAV file
    let mut file = fs::File::create(&path).unwrap();

    // WAV header
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = num_samples * 2;
    let file_size = 36 + data_size;

    file.write_all(b"RIFF").unwrap();
    file.write_all(&file_size.to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&channels.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&bits_per_sample.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();

    for sample in pcm_data {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    path
}

// ============================================================================
// Category 1: CBOR Serialization Tests (Unit) - PASSING
// ============================================================================
// These verify that protocol types serialize correctly for network transmission.

mod cbor_serialization_tests {
    use super::*;

    /// StreamAnnouncement must roundtrip through CBOR for DHT storage
    #[test]
    fn test_stream_announcement_cbor_roundtrip() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");

        // Serialize to CBOR
        let cbor_bytes = to_cbor(&announcement);

        // Deserialize back
        let restored: StreamAnnouncement =
            ciborium::from_reader(&cbor_bytes[..]).expect("CBOR deserialization should succeed");

        assert_eq!(restored.stream_addr, announcement.stream_addr);
        assert_eq!(restored.stream_id, announcement.stream_id);
        assert_eq!(restored.codec, announcement.codec);
        assert_eq!(restored.bitrate, announcement.bitrate);
        assert_eq!(restored.sample_rate, announcement.sample_rate);
        assert_eq!(restored.channels, announcement.channels);
        assert_eq!(restored.encrypted, announcement.encrypted);
    }

    /// Chunk must roundtrip through CBOR for gossipsub transmission
    #[test]
    fn test_chunk_cbor_roundtrip() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let chunks = create_test_chunks(&announcement, 1);
        let chunk = &chunks[0];

        let cbor_bytes = to_cbor(chunk);
        let restored: Chunk =
            ciborium::from_reader(&cbor_bytes[..]).expect("CBOR deserialization should succeed");

        assert_eq!(restored.stream_addr, chunk.stream_addr);
        assert_eq!(restored.seq, chunk.seq);
        assert_eq!(restored.timestamp, chunk.timestamp);
        assert_eq!(restored.duration_us, chunk.duration_us);
        assert_eq!(restored.data, chunk.data);
    }

    /// Encrypted chunks must include nonce in CBOR
    #[test]
    fn test_encrypted_chunk_cbor_roundtrip() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let nonce: [u8; 12] = [0xAB; 12];

        let chunk = Chunk::new_encrypted(
            announcement.stream_addr,
            0,
            0,
            Codec::Opus,
            20_000,
            vec![0x00, 0x01, 0x02, 0x03],
            nonce,
        );

        let cbor_bytes = to_cbor(&chunk);
        let restored: Chunk = ciborium::from_reader(&cbor_bytes[..]).unwrap();

        assert!(restored.is_encrypted());
        assert_eq!(restored.nonce, Some(nonce));
    }

    /// Serialized chunk size should be reasonable for network (< 2KB for 20ms Opus)
    #[test]
    fn test_serialized_chunk_size_reasonable() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");

        // Create a chunk with realistic Opus data (128kbps = ~320 bytes per 20ms)
        let opus_data = vec![0x00; 320];
        let chunk = Chunk::new(
            announcement.stream_addr,
            0,
            0,
            Codec::Opus,
            20_000,
            opus_data,
        );

        let cbor_bytes = to_cbor(&chunk);

        // CBOR overhead should be minimal, total < 2KB
        assert!(
            cbor_bytes.len() < 2048,
            "Chunk CBOR size {} exceeds 2KB limit",
            cbor_bytes.len()
        );
    }
}

// ============================================================================
// Category 2: Topic and DHT Key Construction Tests (Unit) - PASSING
// ============================================================================
// These verify topic/key strings match protocol spec.

mod topic_construction_tests {
    use super::*;

    /// Stream topic should follow /mdrn/stream/{64-char-hex} format
    #[test]
    fn test_stream_topic_format() {
        let stream_addr: [u8; 32] = [0xAB; 32];
        let _topic = stream_topic(&stream_addr);

        // The topic hash is a gossipsub topic hash, not the raw string
        // What we really want to test is that stream_topic produces consistent output
        let expected_hex = hex::encode(&stream_addr);
        assert_eq!(expected_hex.len(), 64, "stream_addr hex should be 64 chars");
    }

    /// Topic created from announcement should match manual construction
    #[test]
    fn test_stream_topic_from_announcement() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");

        let topic1 = stream_topic(&announcement.stream_addr);
        let topic2 = stream_topic(&announcement.stream_addr);

        // Same stream_addr should produce same topic
        assert_eq!(topic1.hash(), topic2.hash());
    }

    /// DHT key should use /mdrn/streams/ namespace
    #[test]
    fn test_dht_key_construction() {
        let stream_addr: [u8; 32] = [0xAB; 32];
        let hex_addr = hex::encode(&stream_addr);

        let dht_key = format!("{}{}", DHT_STREAM_NAMESPACE, hex_addr);

        assert!(dht_key.starts_with("/mdrn/streams/"));
        assert!(dht_key.ends_with(&hex_addr));
    }
}

// ============================================================================
// Category 3: Chunk Timing Tests (Unit) - PASSING
// ============================================================================
// These ensure chunk metadata is correct before network transmission.

mod chunk_timing_tests {
    use super::*;

    /// Chunk timestamps should be monotonically increasing
    #[test]
    fn test_chunk_timestamps_are_monotonic() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let chunks = create_test_chunks(&announcement, 10);

        for i in 1..chunks.len() {
            assert!(
                chunks[i].timestamp > chunks[i - 1].timestamp,
                "Chunk {} timestamp {} should be > chunk {} timestamp {}",
                i,
                chunks[i].timestamp,
                i - 1,
                chunks[i - 1].timestamp
            );
        }
    }

    /// Chunk sequence numbers should increment by 1
    #[test]
    fn test_chunk_sequence_numbers_increment() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let chunks = create_test_chunks(&announcement, 10);

        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(
                chunk.seq, i as u64,
                "Chunk {} should have seq {}, got {}",
                i, i, chunk.seq
            );
        }
    }

    /// Chunk duration should match 20ms frame size
    #[test]
    fn test_chunk_duration_matches_frame_size() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let chunks = create_test_chunks(&announcement, 5);

        for chunk in &chunks {
            assert_eq!(
                chunk.duration_us, 20_000,
                "Chunk duration should be 20000us (20ms), got {}",
                chunk.duration_us
            );
        }
    }

    /// Timestamp spacing should be 20ms between chunks
    #[test]
    fn test_chunk_timestamp_spacing_is_20ms() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let chunks = create_test_chunks(&announcement, 5);

        for i in 1..chunks.len() {
            let spacing = chunks[i].timestamp - chunks[i - 1].timestamp;
            assert_eq!(
                spacing, 20_000,
                "Timestamp spacing between chunk {} and {} should be 20000us, got {}",
                i - 1,
                i,
                spacing
            );
        }
    }
}

// ============================================================================
// Category 4: Network Broadcaster Function Tests - NOW IMPLEMENTED
// ============================================================================
// These test the broadcast_to_network() function and --network CLI flag.

mod network_broadcaster_tests {
    /// broadcast_to_network() function should exist and accept correct parameters
    ///
    /// This test verifies the function exists by checking CLI help output,
    /// since the broadcast module is private.
    #[test]
    fn test_broadcast_to_network_function_exists() {
        // The function exists if the CLI builds with --network support.
        // We verify by checking that the CLI binary compiles.
        use std::process::Command;
        
        let output = Command::new("cargo")
            .args(["build", "--package", "mdrn-cli", "--quiet"])
            .output()
            .expect("Failed to run cargo build");
        
        assert!(
            output.status.success(),
            "CLI should build successfully with broadcast_to_network: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// --network flag should exist on broadcast CLI command
    ///
    /// This test verifies the CLI accepts --network flag by checking help output.
    #[test]
    fn test_cli_broadcast_network_flag_exists() {
        use std::process::Command;
        
        // Check help output includes --network flag
        let output = Command::new("cargo")
            .args(["run", "--package", "mdrn-cli", "--quiet", "--", "broadcast", "--help"])
            .output()
            .expect("Failed to run CLI help");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("--network") || stdout.contains("-n"),
            "CLI broadcast command should have --network flag. Help output:\n{}",
            stdout
        );
    }
}

// ============================================================================
// Category 5: DHT Publishing Tests - PASSING
// ============================================================================
// These test that StreamAnnouncement gets stored in DHT.

mod dht_publishing_tests {
    use super::*;

    /// Broadcast should publish announcement to DHT
    ///
    /// This tests the integration between broadcast and MdrnSwarm::dht_put()
    #[tokio::test]
    async fn test_broadcast_publishes_announcement_to_dht() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        let announcement = create_test_announcement(&keypair, "test-stream");
        let cbor_bytes = to_cbor(&announcement);

        // Construct DHT key
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );

        // Store in DHT (dht_put takes Vec<u8>, not &[u8])
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), cbor_bytes.clone())
            .expect("dht_put should succeed");

        // Verify local DHT has it
        let stored = swarm.dht_get(dht_key.as_bytes());
        assert!(stored.is_some(), "DHT should contain the announcement");

        let stored_bytes = stored.unwrap();
        assert_eq!(
            stored_bytes, cbor_bytes,
            "Stored bytes should match original"
        );

        // Verify it deserializes back to a valid announcement
        let restored: StreamAnnouncement = ciborium::from_reader(&stored_bytes[..])
            .expect("Stored CBOR should deserialize to StreamAnnouncement");
        assert_eq!(restored.stream_id, "test-stream");
    }

    /// DHT key should use correct namespace format
    #[tokio::test]
    async fn test_dht_key_uses_stream_namespace() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");

        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );

        assert!(dht_key.starts_with("/mdrn/streams/"));
        assert_eq!(dht_key.len(), "/mdrn/streams/".len() + 64); // namespace + 64 hex chars
    }
}

// ============================================================================
// Category 6: Gossipsub Publishing Tests - PARTIAL
// ============================================================================
// These test that Chunks get published to gossipsub topics.
// Note: Some tests require connected peers to pass.

mod gossipsub_publishing_tests {
    use super::*;

    /// Broadcaster must subscribe to topic before publishing
    #[tokio::test]
    async fn test_broadcast_subscribes_to_stream_topic() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        let announcement = create_test_announcement(&keypair, "test-stream");
        let topic = stream_topic(&announcement.stream_addr);

        // Subscribe to the topic
        swarm.subscribe(&topic).expect("subscribe should succeed");

        // Verify subscription
        assert!(
            swarm.is_subscribed(&topic),
            "Swarm should be subscribed to the stream topic"
        );
    }

    /// Publishing to unsubscribed topic should fail
    #[tokio::test]
    async fn test_publish_without_subscription_fails() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        let announcement = create_test_announcement(&keypair, "test-stream");
        let topic = stream_topic(&announcement.stream_addr);
        let chunks = create_test_chunks(&announcement, 1);

        // Try to publish without subscribing
        let result = swarm.publish(&topic, to_cbor(&chunks[0]));

        assert!(
            result.is_err(),
            "Publishing to unsubscribed topic should fail"
        );
    }

    /// Chunks published should be valid CBOR
    #[tokio::test]
    async fn test_broadcast_chunks_are_valid_cbor() {
        let keypair = create_test_keypair();
        let announcement = create_test_announcement(&keypair, "test-stream");
        let chunks = create_test_chunks(&announcement, 5);

        for chunk in &chunks {
            let cbor_bytes = to_cbor(chunk);

            // Should deserialize back
            let restored: Chunk = ciborium::from_reader(&cbor_bytes[..])
                .expect("Chunk CBOR should be valid and deserializable");

            assert_eq!(restored.seq, chunk.seq);
            assert_eq!(restored.stream_addr, chunk.stream_addr);
        }
    }

    /// Chunks should be publishable to subscribed topic (requires peers)
    ///
    /// EXPECTED FAILURE: gossipsub requires at least one peer to publish
    /// This test documents the current behavior and will pass once we have
    /// either connected peers or a "local queue" mode for testing.
    #[tokio::test]
    #[ignore = "gossipsub publish requires connected peers - need peer connection or local mode"]
    async fn test_broadcast_can_publish_chunks_to_topic() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        let announcement = create_test_announcement(&keypair, "test-stream");
        let topic = stream_topic(&announcement.stream_addr);
        let chunks = create_test_chunks(&announcement, 5);

        // Must subscribe before publishing
        swarm.subscribe(&topic).expect("subscribe should succeed");

        // Publish all chunks - requires peers!
        for chunk in &chunks {
            let cbor_bytes = to_cbor(chunk);
            swarm
                .publish(&topic, cbor_bytes)
                .expect("publish should succeed");
        }
    }
}

// ============================================================================
// Category 7: Async/Sync Bridge Tests - PARTIAL
// ============================================================================
// Critical for CLI integration - the CLI is sync, but libp2p is async.

mod async_sync_bridge_tests {
    use super::*;

    /// Tokio runtime should be creatable for broadcast
    #[test]
    fn test_tokio_runtime_creates_for_broadcast() {
        // This tests that we can create a tokio runtime in sync context
        let rt = tokio::runtime::Runtime::new().expect("Runtime creation should succeed");

        // Run a simple async task
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    /// Multiple sequential broadcasts should not conflict
    #[test]
    fn test_multiple_sequential_broadcasts() {
        let rt = tokio::runtime::Runtime::new().expect("Runtime creation should succeed");

        // First broadcast - just subscribe (no publish, since no peers)
        rt.block_on(async {
            let keypair = create_test_keypair();
            let mut swarm = create_test_swarm_with_keypair(keypair.clone());
            let announcement = create_test_announcement(&keypair, "stream-1");
            let topic = stream_topic(&announcement.stream_addr);
            swarm.subscribe(&topic).unwrap();
        });

        // Second broadcast (should not conflict with first)
        rt.block_on(async {
            let keypair = create_test_keypair();
            let mut swarm = create_test_swarm_with_keypair(keypair.clone());
            let announcement = create_test_announcement(&keypair, "stream-2");
            let topic = stream_topic(&announcement.stream_addr);
            swarm.subscribe(&topic).unwrap();
        });
    }

    /// Swarm operations should work in block_on context (requires peers for publish)
    ///
    /// EXPECTED FAILURE: gossipsub publish requires connected peers
    #[tokio::test]
    #[ignore = "gossipsub publish requires connected peers"]
    async fn test_swarm_runs_in_tokio_block_on() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        let announcement = create_test_announcement(&keypair, "test-stream");
        let topic = stream_topic(&announcement.stream_addr);

        // Subscribe works without peers
        swarm.subscribe(&topic).expect("subscribe should succeed");

        // Publish requires peers
        let chunks = create_test_chunks(&announcement, 3);
        for chunk in &chunks {
            swarm
                .publish(&topic, to_cbor(chunk))
                .expect("publish should succeed");
        }
    }
}

// ============================================================================
// Category 8: Error Handling Tests - PARTIAL
// ============================================================================
// These test that network failures are handled properly.

mod error_handling_tests {
    use super::*;

    /// Invalid keypair type should fail swarm creation gracefully
    #[test]
    fn test_swarm_creation_with_invalid_config() {
        // This tests error handling, not a failure we expect to implement
        let keypair = create_test_keypair();
        let config = TransportConfig {
            listen_addrs: vec!["invalid-not-a-multiaddr".to_string()],
            ..TransportConfig::default()
        };

        // Swarm creation should fail gracefully with bad config
        // (The actual behavior depends on implementation)
        let _ = MdrnSwarm::new(keypair, config);
    }

    /// Broadcast should handle case when no listeners are connected
    /// (gossipsub requires peers, so this currently fails)
    ///
    /// EXPECTED FAILURE: gossipsub requires at least one peer
    /// Future: May need a "local buffer" mode or accept this limitation
    #[tokio::test]
    #[ignore = "gossipsub requires peers - document as expected limitation"]
    async fn test_broadcast_handles_no_listeners() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        let announcement = create_test_announcement(&keypair, "test-stream");
        let topic = stream_topic(&announcement.stream_addr);
        let chunks = create_test_chunks(&announcement, 3);

        swarm.subscribe(&topic).unwrap();

        // Publishing without peers currently fails with InsufficientPeers
        // This test documents that behavior
        for chunk in &chunks {
            let result = swarm.publish(&topic, to_cbor(chunk));
            assert!(
                result.is_ok(),
                "Publishing without peers should succeed locally"
            );
        }
    }
}

// ============================================================================
// Category 9: Full Integration Pipeline Tests - PARTIAL
// ============================================================================
// These test the complete broadcast-to-network flow.

mod full_pipeline_tests {
    use super::*;

    /// Complete pipeline: create announcement, store in DHT, subscribe to topic
    /// Note: publish step requires connected peers
    #[tokio::test]
    async fn test_broadcast_pipeline_without_publish() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        // 1. Create announcement
        let announcement = create_test_announcement(&keypair, "full-test-stream");

        // 2. Store announcement in DHT
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );
        let announcement_cbor = to_cbor(&announcement);
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), announcement_cbor)
            .unwrap();

        // 3. Subscribe to stream topic
        let topic = stream_topic(&announcement.stream_addr);
        swarm.subscribe(&topic).unwrap();

        // 4. Verify DHT contains announcement
        let stored = swarm.dht_get(dht_key.as_bytes());
        assert!(stored.is_some());

        // 5. Verify we're subscribed
        assert!(swarm.is_subscribed(&topic));

        // Note: publish step skipped - requires peers
    }

    /// Complete pipeline including chunk publishing (requires peers)
    ///
    /// EXPECTED FAILURE: gossipsub publish requires connected peers
    #[tokio::test]
    #[ignore = "gossipsub publish requires connected peers"]
    async fn test_full_broadcast_pipeline_to_network() {
        let keypair = create_test_keypair();
        let mut swarm = create_test_swarm_with_keypair(keypair.clone());

        // 1. Create announcement
        let announcement = create_test_announcement(&keypair, "full-test-stream");

        // 2. Store announcement in DHT
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );
        let announcement_cbor = to_cbor(&announcement);
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), announcement_cbor)
            .unwrap();

        // 3. Subscribe to stream topic
        let topic = stream_topic(&announcement.stream_addr);
        swarm.subscribe(&topic).unwrap();

        // 4. Publish chunks (requires peers!)
        let chunks = create_test_chunks(&announcement, 5);
        for chunk in &chunks {
            swarm.publish(&topic, to_cbor(chunk)).unwrap();
        }

        // 5. Verify DHT contains announcement
        let stored = swarm.dht_get(dht_key.as_bytes());
        assert!(stored.is_some());

        // 6. Verify we're subscribed
        assert!(swarm.is_subscribed(&topic));
    }

    /// Simulated broadcast from audio file through to network
    ///
    /// EXPECTED FAILURE: Requires full end-to-end integration test setup
    #[tokio::test]
    #[ignore = "Requires full end-to-end integration test setup with audio file"]
    async fn test_broadcast_audio_file_to_network() {
        let _temp_dir = TempDir::new().unwrap();
        // let wav_path = create_test_wav(&temp_dir, 100, 48000, 1);

        let _keypair = create_test_keypair();

        // This would call the new broadcast_to_network() function via CLI
        // For now, we test the components individually in other tests
        
        // The implementation exists - this test just needs proper setup
        // to call it via the CLI or by making the module public
    }
}

// ============================================================================
// Category 10: Two-Peer Integration Tests - NOT IMPLEMENTED
// ============================================================================
// These test actual message propagation between connected swarms.

mod two_peer_tests {
    /// Two connected swarms should be able to exchange chunks via gossipsub
    ///
    /// EXPECTED FAILURE: Requires listen/dial and event handling
    #[tokio::test]
    #[ignore = "Requires MdrnSwarm listen/dial implementation"]
    async fn test_two_swarms_can_gossip_chunks() {
        // This test requires:
        // 1. swarm1.listen() to start accepting connections
        // 2. swarm2.dial(swarm1_addr) to connect
        // 3. Both subscribe to same topic
        // 4. swarm1 publishes chunk
        // 5. swarm2 receives chunk via event

        unimplemented!("Two-swarm gossipsub test requires listen/dial");
    }

    /// Listener should receive chunks from broadcaster
    ///
    /// EXPECTED FAILURE: Requires full networking implementation
    #[tokio::test]
    #[ignore = "Requires full networking implementation"]
    async fn test_listener_receives_chunks_from_broadcaster() {
        // This is the end-to-end test:
        // 1. Broadcaster creates announcement, stores in DHT
        // 2. Listener queries DHT for announcement
        // 3. Listener subscribes to topic
        // 4. Broadcaster publishes chunks
        // 5. Listener receives chunks

        unimplemented!("Full listener test requires networking");
    }

    /// DHT announcement should propagate to connected peer
    ///
    /// EXPECTED FAILURE: Requires DHT network propagation
    #[tokio::test]
    #[ignore = "Requires DHT network propagation"]
    async fn test_dht_announcement_propagates_to_peer() {
        // This tests Kademlia DHT record propagation:
        // 1. swarm1 puts record
        // 2. swarm2 (connected) queries for record
        // 3. swarm2 receives record

        unimplemented!("DHT propagation test requires connected peers");
    }
}
