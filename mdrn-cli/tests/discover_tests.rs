//! Discover Command Integration Tests
//!
//! TDD tests for MDRN stream discovery implementation.
//! These tests verify that the discover command can:
//! - Initialize a libp2p swarm and connect to DHT
//! - Query for StreamAnnouncements in the DHT
//! - Parse and display stream metadata
//! - Filter streams by tag and apply limits
//! - Handle empty results and errors gracefully
//!
//! Test Categories:
//! - Discovery Configuration: Config struct and defaults
//! - DHT Querying: Key scanning, result parsing
//! - Stream Parsing: CBOR deserialization, metadata extraction
//! - Filtering: Tag matching, limit enforcement
//! - Output Formatting: Table display, empty results
//!
//! To run these tests:
//! ```bash
//! cargo test --package mdrn-cli --test discover_tests
//! ```

use mdrn_core::identity::{Keypair, Vouch};
use mdrn_core::stream::{Codec, StreamAnnouncement};
use mdrn_core::transport::{MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a test keypair
fn create_test_keypair() -> Keypair {
    Keypair::generate_ed25519().expect("keypair generation should succeed")
}

/// Create a test swarm with random port
fn create_test_swarm(keypair: Keypair) -> MdrnSwarm {
    let config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    MdrnSwarm::new(keypair, config).expect("swarm creation should succeed")
}

/// Create a test vouch for a broadcaster
fn create_test_vouch(broadcaster: &Keypair) -> Vouch {
    let issuer = Keypair::generate_ed25519().expect("issuer keypair should generate");
    Vouch::create(broadcaster.identity().clone(), &issuer, None)
        .expect("vouch creation should succeed")
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

/// Create a test announcement with tags
fn create_test_announcement_with_tags(keypair: &Keypair, stream_id: &str, tags: Vec<String>) -> StreamAnnouncement {
    let mut announcement = create_test_announcement(keypair, stream_id);
    announcement.tags = tags;
    announcement
}

/// Serialize a value to CBOR bytes
fn to_cbor<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).expect("CBOR serialization should succeed");
    bytes
}

// ============================================================================
// Phase 1: Discovery Configuration Tests
// ============================================================================

/// Test DiscoverConfig struct with defaults
#[tokio::test]
async fn test_discover_config_defaults() {
    use mdrn_cli::discover::{DiscoverConfig, DiscoverResult};

    let config = DiscoverConfig::default();
    assert_eq!(config.limit, 10);
    assert!(config.tag.is_none());
    assert!(config.timeout_secs > 0);

    // Result type should exist
    let result = DiscoverResult {
        streams: vec![],
        total_found: 0,
        filtered_count: 0,
    };
    assert_eq!(result.streams.len(), 0);
}

/// Test DiscoverConfig with custom values
#[tokio::test]
async fn test_discover_config_custom() {
    use mdrn_cli::discover::DiscoverConfig;

    let config = DiscoverConfig {
        limit: 5,
        tag: Some("music".to_string()),
        timeout_secs: 30,
    };

    assert_eq!(config.limit, 5);
    assert_eq!(config.tag.as_ref().unwrap(), "music");
    assert_eq!(config.timeout_secs, 30);
}

// ============================================================================
// Phase 2: DHT Querying Tests
// ============================================================================

/// Test that discover can query local DHT store
#[tokio::test]
async fn test_discover_queries_local_dht() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // Store an announcement in local DHT
    let announcement = create_test_announcement(&keypair, "test-stream");
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&announcement.stream_addr)
    );
    let announcement_cbor = to_cbor(&announcement);
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), announcement_cbor)
        .expect("dht_put should succeed");

    // Discover should find the stream
    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 1);
    assert_eq!(result.streams.len(), 1);
    assert_eq!(result.streams[0].stream_id(), "test-stream");
}

/// Test discover with multiple streams in DHT
#[tokio::test]
async fn test_discover_multiple_streams() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // Store multiple announcements
    for i in 0..5 {
        let announcement = create_test_announcement(&keypair, &format!("stream-{}", i));
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );
        let announcement_cbor = to_cbor(&announcement);
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), announcement_cbor)
            .expect("dht_put should succeed");
    }

    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 5);
    assert_eq!(result.streams.len(), 5);
}

/// Test discover returns empty result for empty DHT
#[tokio::test]
async fn test_discover_empty_dht() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let swarm = create_test_swarm(keypair);

    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 0);
    assert_eq!(result.streams.len(), 0);
}

// ============================================================================
// Phase 3: Stream Parsing Tests
// ============================================================================

/// Test announcement deserialization from CBOR
#[tokio::test]
async fn test_discover_parses_announcement_cbor() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    let announcement = create_test_announcement(&keypair, "parse-test");
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&announcement.stream_addr)
    );
    let announcement_cbor = to_cbor(&announcement);
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), announcement_cbor)
        .expect("dht_put should succeed");

    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.streams.len(), 1);
    let found = &result.streams[0];

    // Verify all metadata fields
    assert_eq!(found.stream_id(), "parse-test");
    assert_eq!(found.codec(), Codec::Opus);
    assert_eq!(found.bitrate(), 128);
    assert_eq!(found.sample_rate(), 48000);
    assert_eq!(found.channels(), 2);
    assert!(!found.encrypted());
}

/// Test discover skips invalid CBOR data
#[tokio::test]
async fn test_discover_skips_invalid_cbor() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // Store valid announcement
    let valid_announcement = create_test_announcement(&keypair, "valid-stream");
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&valid_announcement.stream_addr)
    );
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&valid_announcement))
        .expect("dht_put should succeed");

    // Store invalid CBOR data with stream namespace
    let invalid_key = format!("{}invalid-stream", DHT_STREAM_NAMESPACE);
    swarm
        .dht_put(invalid_key.as_bytes().to_vec(), vec![0xFF, 0xFF, 0xFF])
        .expect("dht_put should succeed");

    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    // Should only find the valid stream
    assert_eq!(result.total_found, 1);
    assert_eq!(result.streams[0].stream_id(), "valid-stream");
}

// ============================================================================
// Phase 4: Filtering Tests
// ============================================================================

/// Test discover respects limit parameter
#[tokio::test]
async fn test_discover_respects_limit() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // Store 10 announcements
    for i in 0..10 {
        let announcement = create_test_announcement(&keypair, &format!("stream-{}", i));
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&announcement))
            .expect("dht_put should succeed");
    }

    // Request only 3
    let config = DiscoverConfig {
        limit: 3,
        tag: None,
        timeout_secs: 10,
    };
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 10); // Total found before limit
    assert_eq!(result.streams.len(), 3); // Returned respects limit
}

/// Test discover filters by tag
#[tokio::test]
async fn test_discover_filters_by_tag() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // Store streams with different tags
    let music_stream = create_test_announcement_with_tags(
        &keypair,
        "music-stream",
        vec!["music".to_string(), "electronic".to_string()],
    );
    let podcast_stream = create_test_announcement_with_tags(
        &keypair,
        "podcast-stream",
        vec!["podcast".to_string(), "tech".to_string()],
    );
    let another_music = create_test_announcement_with_tags(
        &keypair,
        "another-music",
        vec!["music".to_string(), "jazz".to_string()],
    );

    for announcement in &[&music_stream, &podcast_stream, &another_music] {
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), to_cbor(*announcement))
            .expect("dht_put should succeed");
    }

    // Filter by "music" tag
    let config = DiscoverConfig {
        limit: 10,
        tag: Some("music".to_string()),
        timeout_secs: 10,
    };
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 3); // Total before filtering
    assert_eq!(result.filtered_count, 2); // Matches the tag
    assert_eq!(result.streams.len(), 2);

    // Verify both music streams are found
    let stream_ids: Vec<&str> = result.streams.iter().map(|s| s.stream_id()).collect();
    assert!(stream_ids.contains(&"music-stream"));
    assert!(stream_ids.contains(&"another-music"));
}

/// Test discover with tag that matches no streams
#[tokio::test]
async fn test_discover_tag_no_matches() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    let stream = create_test_announcement_with_tags(
        &keypair,
        "music-stream",
        vec!["music".to_string()],
    );
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&stream.stream_addr)
    );
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&stream))
        .expect("dht_put should succeed");

    // Filter by non-existent tag
    let config = DiscoverConfig {
        limit: 10,
        tag: Some("sports".to_string()),
        timeout_secs: 10,
    };
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 1);
    assert_eq!(result.filtered_count, 0);
    assert_eq!(result.streams.len(), 0);
}

/// Test discover with case-insensitive tag matching
#[tokio::test]
async fn test_discover_tag_case_insensitive() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    let stream = create_test_announcement_with_tags(
        &keypair,
        "mixed-case-stream",
        vec!["Music".to_string(), "ELECTRONIC".to_string()],
    );
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&stream.stream_addr)
    );
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&stream))
        .expect("dht_put should succeed");

    // Search with different case
    let config = DiscoverConfig {
        limit: 10,
        tag: Some("music".to_string()),
        timeout_secs: 10,
    };
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.filtered_count, 1);
    assert_eq!(result.streams.len(), 1);
}

// ============================================================================
// Phase 5: Output Formatting Tests
// ============================================================================

/// Test DiscoveredStream display information
#[tokio::test]
async fn test_discovered_stream_display() {
    use mdrn_cli::discover::{discover_streams, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    let announcement = create_test_announcement_with_tags(
        &keypair,
        "display-test",
        vec!["music".to_string()],
    );
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&announcement.stream_addr)
    );
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&announcement))
        .expect("dht_put should succeed");

    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    let stream = &result.streams[0];

    // All display fields should be accessible
    assert!(!stream.stream_addr_hex().is_empty());
    assert!(!stream.broadcaster_hex().is_empty());
    assert_eq!(stream.codec_name(), "Opus");
    assert_eq!(stream.bitrate_display(), "128 kbps");
    assert_eq!(stream.channels_display(), "Stereo");
}

/// Test format_discover_output for table display
#[tokio::test]
async fn test_format_discover_output() {
    use mdrn_cli::discover::{format_discover_output, DiscoverResult, DiscoveredStream};

    // Create a mock result
    let keypair = create_test_keypair();
    let announcement = create_test_announcement(&keypair, "format-test");

    let result = DiscoverResult {
        streams: vec![DiscoveredStream::from(announcement)],
        total_found: 1,
        filtered_count: 1,
    };

    let output = format_discover_output(&result);

    // Output should contain stream information
    assert!(output.contains("format-test"));
    assert!(output.contains("Opus"));
    assert!(output.contains("128"));
}

/// Test format_discover_output for empty results
#[tokio::test]
async fn test_format_discover_output_empty() {
    use mdrn_cli::discover::{format_discover_output, DiscoverResult};

    let result = DiscoverResult {
        streams: vec![],
        total_found: 0,
        filtered_count: 0,
    };

    let output = format_discover_output(&result);

    // Should show friendly empty message
    assert!(output.contains("No streams found") || output.contains("no streams"));
}

/// Test format_discover_output with filtered results
#[tokio::test]
async fn test_format_discover_output_filtered() {
    use mdrn_cli::discover::{format_discover_output, DiscoverResult};

    let result = DiscoverResult {
        streams: vec![],
        total_found: 5,
        filtered_count: 0,
    };

    let output = format_discover_output(&result);

    // Should indicate filtering happened
    assert!(output.contains("5") || output.contains("filtered") || output.contains("found"));
}

// ============================================================================
// Phase 6: Network Integration Tests
// ============================================================================

/// Test discover initializes swarm correctly
#[tokio::test]
async fn test_discover_swarm_initialization() {
    use mdrn_cli::discover::{run_discover, DiscoverConfig};

    let keypair = create_test_keypair();
    let config = DiscoverConfig::default();

    // Should not panic when running discovery
    let result = run_discover(Some(keypair), &config).await;

    // Result should be Ok even with no peers/streams
    assert!(result.is_ok());
    let discover_result = result.unwrap();
    assert_eq!(discover_result.total_found, 0);
}

/// Test discover with pre-populated local DHT
#[tokio::test]
async fn test_discover_with_local_data() {
    use mdrn_cli::discover::{run_discover_with_swarm, DiscoverConfig};

    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // Pre-populate DHT
    let announcement = create_test_announcement(&keypair, "local-stream");
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&announcement.stream_addr)
    );
    swarm
        .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&announcement))
        .expect("dht_put should succeed");

    let config = DiscoverConfig::default();
    let result = run_discover_with_swarm(swarm, &config).await;

    assert!(result.is_ok());
    let discover_result = result.unwrap();
    assert_eq!(discover_result.total_found, 1);
    assert_eq!(discover_result.streams[0].stream_id(), "local-stream");
}

// ============================================================================
// Phase 7: Error Handling Tests
// ============================================================================

/// Test discover handles timeout gracefully
#[tokio::test]
async fn test_discover_timeout_handling() {
    use mdrn_cli::discover::{run_discover, DiscoverConfig};

    let keypair = create_test_keypair();
    let config = DiscoverConfig {
        limit: 10,
        tag: None,
        timeout_secs: 1, // Very short timeout
    };

    // Should complete without error even with short timeout
    let result = run_discover(Some(keypair), &config).await;
    assert!(result.is_ok());
}

/// Test discover generates new keypair if none provided
#[tokio::test]
async fn test_discover_generates_keypair() {
    use mdrn_cli::discover::{run_discover, DiscoverConfig};

    let config = DiscoverConfig::default();

    // Should work even without providing keypair
    let result = run_discover(None, &config).await;
    assert!(result.is_ok());
}

// ============================================================================
// Integration Test: Full Discovery Workflow
// ============================================================================

/// Test complete discovery workflow
#[tokio::test]
async fn test_discover_full_workflow() {
    use mdrn_cli::discover::{
        discover_streams, format_discover_output, DiscoverConfig,
    };

    // 1. Create swarm
    let keypair = create_test_keypair();
    let mut swarm = create_test_swarm(keypair.clone());

    // 2. Simulate other broadcasters having announced streams
    for i in 0..3 {
        let broadcaster_keypair = create_test_keypair();
        let tags = if i == 0 {
            vec!["music".to_string(), "live".to_string()]
        } else if i == 1 {
            vec!["podcast".to_string()]
        } else {
            vec!["music".to_string()]
        };

        let announcement = create_test_announcement_with_tags(
            &broadcaster_keypair,
            &format!("stream-{}", i),
            tags,
        );
        let dht_key = format!(
            "{}{}",
            DHT_STREAM_NAMESPACE,
            hex::encode(&announcement.stream_addr)
        );
        swarm
            .dht_put(dht_key.as_bytes().to_vec(), to_cbor(&announcement))
            .expect("dht_put should succeed");
    }

    // 3. Discover all streams
    let config = DiscoverConfig::default();
    let result = discover_streams(&swarm, &config);

    assert_eq!(result.total_found, 3);
    assert_eq!(result.streams.len(), 3);

    // 4. Filter by tag
    let music_config = DiscoverConfig {
        limit: 10,
        tag: Some("music".to_string()),
        timeout_secs: 10,
    };
    let music_result = discover_streams(&swarm, &music_config);

    assert_eq!(music_result.filtered_count, 2);
    assert_eq!(music_result.streams.len(), 2);

    // 5. Format output
    let output = format_discover_output(&music_result);
    assert!(!output.is_empty());

    // 6. Verify stream addresses are usable
    for stream in &music_result.streams {
        let addr_hex = stream.stream_addr_hex();
        assert_eq!(addr_hex.len(), 64); // 32 bytes = 64 hex chars
    }
}
