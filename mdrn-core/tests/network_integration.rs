//! Network Integration Tests
//!
//! End-to-end tests for MDRN networking functionality. These tests verify that
//! multiple MdrnSwarm instances can discover, connect, and communicate with each other.
//!
//! These tests are marked #[ignore] because the full networking implementation is
//! still in progress. They define the expected behavior for:
//! - Node connection establishment
//! - Gossipsub message propagation
//! - Kademlia DHT record sharing
//! - Stream announcement discovery
//!
//! To run these tests once networking is implemented:
//! ```bash
//! cargo test --package mdrn-core --test network_integration -- --ignored
//! ```
//!
//! ## Required API Extensions for MdrnSwarm
//!
//! These tests expect the following methods to be added to MdrnSwarm:
//!
//! ```rust,ignore
//! impl MdrnSwarm {
//!     /// Start listening on a multiaddr
//!     pub async fn listen(&mut self, addr: Multiaddr) -> Result<(), SwarmError>;
//!
//!     /// Get an iterator over listening addresses
//!     pub fn listeners(&self) -> impl Iterator<Item = &Multiaddr>;
//!
//!     /// Dial a remote peer
//!     pub async fn dial(&mut self, addr: Multiaddr) -> Result<(), SwarmError>;
//!
//!     /// Get list of connected peer IDs
//!     pub fn connected_peers(&self) -> Vec<PeerId>;
//! }
//! ```

use std::time::Duration;

use libp2p::Multiaddr;

use mdrn_core::identity::Keypair;
use mdrn_core::transport::{stream_topic, MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE};

/// Default timeout for network operations in tests
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Short timeout for connection establishment
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Helper to create a test swarm with a fresh keypair
fn create_test_swarm() -> MdrnSwarm {
    let keypair = Keypair::generate_ed25519().expect("keypair generation should succeed");
    let config = TransportConfig {
        // Use localhost TCP only for integration tests
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    MdrnSwarm::new(keypair, config).expect("swarm creation should succeed")
}

/// Helper to create a test swarm and return the keypair's identity bytes
fn create_test_swarm_with_identity() -> (MdrnSwarm, Vec<u8>) {
    let keypair = Keypair::generate_ed25519().expect("keypair generation should succeed");
    let identity_bytes = keypair.identity().as_bytes().to_vec();
    let config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    let swarm = MdrnSwarm::new(keypair, config).expect("swarm creation should succeed");
    (swarm, identity_bytes)
}

// =============================================================================
// TEST 1: Two Nodes Connect
// =============================================================================
// Verifies that two MdrnSwarm instances can establish a connection.
//
// Expected behavior:
// 1. Node A listens on a local address
// 2. Node B dials Node A's address
// 3. Connection is established
// 4. Both nodes see each other as connected peers
//
// Required MdrnSwarm API:
// - listen(addr: Multiaddr) -> Result<(), SwarmError>
// - listeners() -> impl Iterator<Item = &Multiaddr>
// - dial(addr: Multiaddr) -> Result<(), SwarmError>
// - connected_peers() -> Vec<PeerId>
// =============================================================================

/// Test that two nodes can connect to each other.
///
/// IGNORED: Requires MdrnSwarm.listen() and MdrnSwarm.dial() to be implemented.
#[tokio::test]
#[ignore = "Full networking not yet implemented - MdrnSwarm needs listen/dial methods"]
async fn test_two_nodes_connect() {
    // Create two nodes
    let _node_a = create_test_swarm();
    let _node_b = create_test_swarm();

    // Node A listens on localhost
    let _listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();

    // Start listening - this should return the actual bound address
    // REQUIRED API: node_a.listen(listen_addr).await
    // node_a.listen(listen_addr).await.expect("Node A should start listening");

    // Get the actual listening address (with the assigned port)
    // REQUIRED API: node_a.listeners()
    // let node_a_addr = node_a.listeners().next().expect("Node A should have a listener");

    // Node B dials Node A
    // REQUIRED API: node_b.dial(addr).await
    // let dial_result = tokio::time::timeout(
    //     CONNECT_TIMEOUT,
    //     node_b.dial(node_a_addr.clone())
    // ).await;
    //
    // assert!(dial_result.is_ok(), "dial should complete within timeout");
    // assert!(dial_result.unwrap().is_ok(), "dial should succeed");

    // Wait a moment for connection to be established
    // tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify Node B sees Node A as a connected peer
    // REQUIRED API: node_b.connected_peers()
    // let node_a_peer_id = node_a.local_peer_id();
    // let node_b_connected_peers = node_b.connected_peers();
    // assert!(
    //     node_b_connected_peers.contains(&node_a_peer_id),
    //     "Node B should see Node A as a connected peer"
    // );

    // Placeholder assertion until API is implemented
    assert!(true, "Test structure defined - implementation pending");
}

/// Test that dialing a non-existent address fails appropriately.
///
/// IGNORED: Requires MdrnSwarm.dial() to be implemented.
#[tokio::test]
#[ignore = "Full networking not yet implemented - MdrnSwarm needs dial method"]
async fn test_dial_nonexistent_address_fails() {
    let _node = create_test_swarm();

    // Try to dial an address where nothing is listening
    let _fake_addr: Multiaddr = "/ip4/127.0.0.1/tcp/65534".parse().unwrap();

    // REQUIRED API: node.dial(fake_addr).await
    // let dial_result = tokio::time::timeout(CONNECT_TIMEOUT, node.dial(fake_addr)).await;
    //
    // // Should either timeout or fail explicitly
    // match dial_result {
    //     Ok(inner_result) => {
    //         assert!(inner_result.is_err(), "dial to nonexistent address should fail");
    //     }
    //     Err(_timeout) => {
    //         // Timeout is also acceptable - no peer at that address
    //     }
    // }

    assert!(true, "Test structure defined - implementation pending");
}

// =============================================================================
// TEST 2: Gossipsub Message Propagation
// =============================================================================
// Verifies that messages published to a gossipsub topic are received by
// subscribers on other nodes.
//
// Expected behavior:
// 1. Node A and Node B connect
// 2. Both subscribe to the same topic
// 3. Node A publishes a message
// 4. Node B receives the message
//
// Required MdrnSwarm API:
// - Event stream or message callback for receiving gossipsub messages
// - Full libp2p gossipsub behavior (not just in-memory tracking)
// =============================================================================

/// Test that gossipsub messages propagate between connected nodes.
///
/// IGNORED: Requires full libp2p gossipsub integration.
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real gossipsub"]
async fn test_gossipsub_message_propagation() {
    // Create two connected nodes
    let mut node_a = create_test_swarm();
    let mut node_b = create_test_swarm();

    // SETUP: Connect the nodes
    // let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
    // node_a.listen(listen_addr).await.unwrap();
    // let node_a_addr = node_a.listeners().next().unwrap();
    // node_b.dial(node_a_addr).await.unwrap();
    // tokio::time::sleep(Duration::from_millis(200)).await;

    // Create a test topic
    let stream_addr: [u8; 32] = [0xAB; 32];
    let topic = stream_topic(&stream_addr);

    // Both nodes subscribe to the topic
    node_a.subscribe(&topic).expect("Node A subscribe should succeed");
    node_b.subscribe(&topic).expect("Node B subscribe should succeed");

    // Wait for gossipsub mesh to form
    // tokio::time::sleep(Duration::from_millis(500)).await;

    // Node A publishes a message
    let test_message = b"Hello from Node A!".to_vec();
    node_a
        .publish(&topic, test_message.clone())
        .expect("publish should succeed");

    // REQUIRED: Message receiving mechanism
    // In a real implementation, Node B should receive the message through:
    // - An event stream: node_b.next_event().await
    // - A message callback: node_b.on_message(topic, |msg| { ... })
    // - A channel: node_b.message_receiver()

    // Verify message was received (implementation-specific)
    // let received = tokio::time::timeout(TEST_TIMEOUT, node_b.recv_message()).await;
    // assert!(received.is_ok(), "should receive message within timeout");
    // assert_eq!(received.unwrap(), test_message);

    assert!(true, "Test structure defined - implementation pending");
}

/// Test that unsubscribed nodes don't receive messages.
///
/// IGNORED: Requires full libp2p gossipsub integration.
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real gossipsub"]
async fn test_gossipsub_unsubscribed_node_does_not_receive() {
    let mut node_a = create_test_swarm();
    let node_b = create_test_swarm();

    // Create a test topic
    let stream_addr: [u8; 32] = [0xCD; 32];
    let topic = stream_topic(&stream_addr);

    // Only Node A subscribes (Node B does NOT subscribe)
    node_a.subscribe(&topic).expect("Node A subscribe should succeed");
    // node_b does NOT subscribe

    // Node A publishes
    let test_message = b"This should not reach Node B".to_vec();
    node_a
        .publish(&topic, test_message.clone())
        .expect("publish should succeed");

    // Verify Node B did not receive anything for this topic
    assert!(
        !node_b.is_subscribed(&topic),
        "Node B should not be subscribed"
    );

    assert!(true, "Test structure defined - implementation pending");
}

/// Test that messages propagate through multiple subscribers in a mesh.
///
/// IGNORED: Requires full libp2p gossipsub integration.
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real gossipsub"]
async fn test_gossipsub_multiple_subscribers() {
    // Create three nodes in a chain: A - B - C
    let mut node_a = create_test_swarm();
    let mut node_b = create_test_swarm();
    let mut node_c = create_test_swarm();

    // All three subscribe to the same topic
    let stream_addr: [u8; 32] = [0xEF; 32];
    let topic = stream_topic(&stream_addr);

    node_a.subscribe(&topic).unwrap();
    node_b.subscribe(&topic).unwrap();
    node_c.subscribe(&topic).unwrap();

    // Node A publishes
    let test_message = b"Message from A to all".to_vec();
    node_a.publish(&topic, test_message.clone()).unwrap();

    // Both B and C should receive the message through gossipsub fan-out

    assert!(true, "Test structure defined - implementation pending");
}

// =============================================================================
// TEST 3: DHT Record Sharing
// =============================================================================
// Verifies that Kademlia DHT records can be shared between nodes.
//
// Expected behavior:
// 1. Node A and Node B connect
// 2. Node A puts a record in the DHT
// 3. Node B queries the DHT and retrieves the record
//
// Current status: MdrnSwarm has in-memory dht_put/dht_get.
// These tests verify the expected behavior when real Kademlia is integrated.
// =============================================================================

/// Test that DHT records are shared between connected nodes.
///
/// IGNORED: Requires full Kademlia DHT (current implementation is in-memory only).
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real Kademlia DHT"]
async fn test_dht_record_sharing() {
    // Create two nodes
    let mut node_a = create_test_swarm();
    let node_b = create_test_swarm();

    // SETUP: Connect the nodes
    // let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
    // node_a.listen(listen_addr).await.unwrap();
    // let node_a_addr = node_a.listeners().next().unwrap();
    // node_b.dial(node_a_addr).await.unwrap();
    // tokio::time::sleep(Duration::from_millis(300)).await;

    // Node A puts a record in the DHT
    let key = b"/mdrn/test/key123";
    let value = b"test_record_value";

    node_a
        .dht_put(key, value)
        .expect("DHT put should succeed");

    // With real Kademlia:
    // - The record would be stored on the k closest nodes
    // - Node B would query the DHT network to retrieve it

    // Give time for DHT to propagate
    // tokio::time::sleep(Duration::from_millis(500)).await;

    // Node B retrieves the record
    // In real implementation, this would involve network round-trips
    let get_result = node_b.dht_get(key);

    // CURRENT BEHAVIOR: In-memory, so node_b won't find node_a's record
    // EXPECTED BEHAVIOR: node_b.dht_get should find the record

    // This assertion documents expected behavior (will fail with current stub)
    // assert!(get_result.is_ok());
    // assert_eq!(get_result.unwrap(), Some(value.to_vec()));

    // Verify current in-memory behavior
    assert!(get_result.is_ok(), "dht_get should not error");
    // Note: With real Kademlia, this would return Some(value)
    // Current in-memory implementation returns None (different stores)

    assert!(true, "Test structure defined - implementation pending");
}

/// Test that querying a non-existent DHT key returns None.
#[tokio::test]
async fn test_dht_record_not_found() {
    let node = create_test_swarm();

    // Query for a key that was never stored
    let nonexistent_key = b"/mdrn/nonexistent/key";

    let get_result = node.dht_get(nonexistent_key);

    // Should return Ok(None) for nonexistent key
    assert!(get_result.is_ok(), "DHT get should not error for missing key");
    assert!(
        get_result.unwrap().is_none(),
        "DHT get for nonexistent key should return None"
    );
}

/// Test that DHT records can be updated.
///
/// IGNORED: Requires full Kademlia DHT for cross-node verification.
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real Kademlia DHT"]
async fn test_dht_record_update() {
    let mut node_a = create_test_swarm();
    let _node_b = create_test_swarm();

    // SETUP: Connect nodes
    // ...

    let key = b"/mdrn/test/updateable";

    // Put initial value
    let initial_value = b"version_1";
    node_a.dht_put(key, initial_value).unwrap();
    // tokio::time::sleep(Duration::from_millis(300)).await;

    // Update the value
    let updated_value = b"version_2";
    node_a.dht_put(key, updated_value).unwrap();
    // tokio::time::sleep(Duration::from_millis(300)).await;

    // With real Kademlia, node_b should see the updated value
    // let retrieved_v2 = node_b.dht_get(key).unwrap();
    // assert_eq!(retrieved_v2, Some(updated_value.to_vec()));

    assert!(true, "Test structure defined - implementation pending");
}

// =============================================================================
// TEST 4: Stream Announcement Discovery
// =============================================================================
// Verifies the complete flow of a broadcaster announcing a stream and a
// listener discovering it through the DHT.
//
// Expected behavior:
// 1. Broadcaster announces stream to DHT
// 2. Listener queries DHT for streams
// 3. Listener discovers the announced stream
// =============================================================================

/// Test the complete stream announcement and discovery flow.
///
/// IGNORED: Requires full networking (listen/dial + Kademlia DHT).
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real networking"]
async fn test_stream_announcement_discovery() {
    // Create broadcaster and listener nodes
    let (mut broadcaster, broadcaster_identity_bytes) = create_test_swarm_with_identity();
    let _listener = create_test_swarm();

    // SETUP: Connect the nodes
    // let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
    // broadcaster.listen(listen_addr).await.unwrap();
    // let broadcaster_addr = broadcaster.listeners().next().unwrap();
    // listener.dial(broadcaster_addr).await.unwrap();
    // tokio::time::sleep(Duration::from_millis(300)).await;

    // Broadcaster creates a stream announcement
    let stream_id = "my_cool_stream";

    // Compute stream address: SHA-256(broadcaster_identity || stream_id)
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(&broadcaster_identity_bytes);
    hasher.update(stream_id.as_bytes());
    let stream_addr: [u8; 32] = hasher.finalize().into();

    // Create DHT key for the stream announcement
    let dht_key = format!("{}{}", DHT_STREAM_NAMESPACE, hex::encode(&stream_addr));

    // Create a simple stream announcement payload
    // In production, this would be a CBOR-encoded StreamAnnouncement
    let announcement = format!(
        "{{\"stream_addr\":\"{}\",\"stream_id\":\"{}\",\"broadcaster\":\"{}\"}}",
        hex::encode(&stream_addr),
        stream_id,
        hex::encode(&broadcaster_identity_bytes)
    );

    // Broadcaster publishes announcement to DHT
    broadcaster
        .dht_put(dht_key.as_bytes(), announcement.as_bytes())
        .expect("DHT put should succeed");

    // With real Kademlia: Give time for DHT to propagate
    // tokio::time::sleep(Duration::from_millis(500)).await;

    // Listener discovers the stream via DHT
    // With real networking, this would find the record
    // let discovered = listener.dht_get(dht_key.as_bytes()).expect("DHT get should succeed");
    // assert!(discovered.is_some(), "Listener should discover the stream announcement");

    assert!(true, "Test structure defined - implementation pending");
}

/// Test the full flow: announce, subscribe, and receive chunks.
///
/// IGNORED: Requires full networking stack.
#[tokio::test]
#[ignore = "Full networking not yet implemented - needs real networking"]
async fn test_stream_announcement_with_subscription() {
    // Create broadcaster, relay, and listener nodes
    let (mut broadcaster, broadcaster_identity_bytes) = create_test_swarm_with_identity();
    let mut relay = create_test_swarm();
    let mut listener = create_test_swarm();

    // Set up network topology: broadcaster <-> relay <-> listener
    // ...

    // Create stream address
    let stream_id = "live_broadcast";
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(&broadcaster_identity_bytes);
    hasher.update(stream_id.as_bytes());
    let stream_addr: [u8; 32] = hasher.finalize().into();

    // Create topic for the stream
    let topic = stream_topic(&stream_addr);

    // 1. Broadcaster announces to DHT
    let dht_key = format!("{}{}", DHT_STREAM_NAMESPACE, hex::encode(&stream_addr));
    broadcaster
        .dht_put(dht_key.as_bytes(), b"stream_announcement_data")
        .unwrap();

    // 2. Broadcaster subscribes to stream topic (to publish chunks)
    broadcaster.subscribe(&topic).unwrap();

    // 3. Relay subscribes to stream topic (to relay chunks)
    relay.subscribe(&topic).unwrap();

    // 4. Listener discovers stream via DHT (requires real networking)
    // let discovered = listener.dht_get(dht_key.as_bytes()).unwrap();
    // assert!(discovered.is_some(), "Listener should discover stream");

    // 5. Listener subscribes to receive stream chunks
    listener.subscribe(&topic).unwrap();

    // 6. Broadcaster publishes a test chunk
    let test_chunk = b"audio_chunk_data_here".to_vec();
    broadcaster.publish(&topic, test_chunk.clone()).unwrap();

    // 7. Verify subscriptions
    assert!(broadcaster.is_subscribed(&topic));
    assert!(relay.is_subscribed(&topic));
    assert!(listener.is_subscribed(&topic));

    // With real gossipsub: listener should receive the chunk via relay
    // let received = listener.recv_message().await;
    // assert_eq!(received, test_chunk);

    assert!(true, "Test structure defined - implementation pending");
}

// =============================================================================
// Additional Integration Tests
// =============================================================================

/// Test that connections persist after idle periods.
///
/// IGNORED: Requires full networking with keepalive support.
#[tokio::test]
#[ignore = "Full networking not yet implemented"]
async fn test_connection_persistence_after_idle() {
    let _node_a = create_test_swarm();
    let _node_b = create_test_swarm();

    // SETUP: Connect nodes
    // ...

    // Wait longer than idle timeout to test keepalive
    // tokio::time::sleep(Duration::from_secs(2)).await;

    // Connection should still be alive
    // let node_a_peer_id = node_a.local_peer_id();
    // assert!(
    //     node_b.connected_peers().contains(&node_a_peer_id),
    //     "Connection should persist after idle period"
    // );

    assert!(true, "Test structure defined - implementation pending");
}

/// Test reconnection after disconnect.
///
/// IGNORED: Requires full networking with disconnect support.
#[tokio::test]
#[ignore = "Full networking not yet implemented"]
async fn test_reconnection_after_disconnect() {
    let _node_a = create_test_swarm();
    let _node_b = create_test_swarm();

    // SETUP: Connect, disconnect, reconnect
    // ...

    assert!(true, "Test structure defined - implementation pending");
}

/// Test multiple gossipsub topics over a single connection.
///
/// IGNORED: Requires full gossipsub networking.
#[tokio::test]
#[ignore = "Full networking not yet implemented"]
async fn test_multiple_topics_same_connection() {
    let mut node_a = create_test_swarm();
    let mut node_b = create_test_swarm();

    // Subscribe to multiple topics
    let stream_addr_1: [u8; 32] = [0x01; 32];
    let stream_addr_2: [u8; 32] = [0x02; 32];
    let stream_addr_3: [u8; 32] = [0x03; 32];

    let topic_1 = stream_topic(&stream_addr_1);
    let topic_2 = stream_topic(&stream_addr_2);
    let topic_3 = stream_topic(&stream_addr_3);

    node_a.subscribe(&topic_1).unwrap();
    node_a.subscribe(&topic_2).unwrap();
    node_a.subscribe(&topic_3).unwrap();

    node_b.subscribe(&topic_1).unwrap();
    node_b.subscribe(&topic_2).unwrap();
    node_b.subscribe(&topic_3).unwrap();

    // Verify all topics are subscribed
    assert!(node_a.is_subscribed(&topic_1));
    assert!(node_a.is_subscribed(&topic_2));
    assert!(node_a.is_subscribed(&topic_3));
    assert!(node_b.is_subscribed(&topic_1));
    assert!(node_b.is_subscribed(&topic_2));
    assert!(node_b.is_subscribed(&topic_3));
}

// =============================================================================
// Stress / Performance Tests
// =============================================================================

/// Test rapid publish/subscribe throughput.
///
/// IGNORED: Requires full gossipsub networking.
#[tokio::test]
#[ignore = "Full networking not yet implemented - stress test"]
async fn test_rapid_publish_subscribe() {
    let mut node_a = create_test_swarm();

    let stream_addr: [u8; 32] = [0xAA; 32];
    let topic = stream_topic(&stream_addr);

    node_a.subscribe(&topic).unwrap();

    // Rapidly publish 100 messages
    for i in 0..100u32 {
        let data = format!("chunk_{}", i).into_bytes();
        node_a.publish(&topic, data).expect("publish should succeed");
    }

    // All publishes should complete without error
}

/// Test many DHT records.
///
/// IGNORED: Requires full Kademlia DHT networking.
#[tokio::test]
#[ignore = "Full networking not yet implemented - stress test"]
async fn test_many_dht_records() {
    let mut node = create_test_swarm();

    // Store 50 records
    for i in 0..50u32 {
        let key = format!("/mdrn/test/record_{}", i);
        let value = format!("value_{}", i);
        node.dht_put(key.as_bytes(), value.as_bytes()).unwrap();
    }

    // Retrieve all 50 records
    for i in 0..50u32 {
        let key = format!("/mdrn/test/record_{}", i);
        let expected_value = format!("value_{}", i);

        let retrieved = node.dht_get(key.as_bytes()).unwrap();
        assert_eq!(
            retrieved,
            Some(expected_value.into_bytes()),
            "Record {} should be retrievable",
            i
        );
    }
}

// =============================================================================
// Local (non-ignored) tests that verify current stub behavior
// =============================================================================

/// Verify that in-memory DHT works within a single node.
#[tokio::test]
async fn test_single_node_dht_put_get() {
    let mut node = create_test_swarm();

    let key = b"/mdrn/local/test";
    let value = b"local_value";

    // Put and get within the same node should work
    node.dht_put(key, value).unwrap();
    let retrieved = node.dht_get(key).unwrap();

    assert_eq!(retrieved, Some(value.to_vec()));
}

/// Verify that topic subscription tracking works locally.
#[tokio::test]
async fn test_local_subscription_tracking() {
    let mut node = create_test_swarm();

    let stream_addr: [u8; 32] = [0x42; 32];
    let topic = stream_topic(&stream_addr);

    // Initially not subscribed
    assert!(!node.is_subscribed(&topic));

    // Subscribe
    node.subscribe(&topic).unwrap();
    assert!(node.is_subscribed(&topic));

    // Unsubscribe
    node.unsubscribe(&topic).unwrap();
    assert!(!node.is_subscribed(&topic));
}

/// Verify that publish to unsubscribed topic fails.
#[tokio::test]
async fn test_publish_to_unsubscribed_topic_fails() {
    let mut node = create_test_swarm();

    let stream_addr: [u8; 32] = [0x99; 32];
    let topic = stream_topic(&stream_addr);

    // Not subscribed - publish should fail
    let result = node.publish(&topic, b"data".to_vec());
    assert!(result.is_err(), "publish to unsubscribed topic should fail");
}

/// Verify swarm creation and peer ID derivation.
#[tokio::test]
async fn test_swarm_has_peer_id() {
    let node = create_test_swarm();

    // Swarm should have a valid peer ID
    let peer_id = node.local_peer_id();
    assert!(!peer_id.to_string().is_empty(), "peer ID should not be empty");
}

/// Verify protocol ID is correct.
#[tokio::test]
async fn test_protocol_id_is_correct() {
    let node = create_test_swarm();

    assert_eq!(node.protocol_id(), "/mdrn/1.0.0");
}

// =============================================================================
// Unused variable/import allowances for ignored tests
// =============================================================================

#[allow(unused)]
const _: () = {
    // These constants are used in ignored tests and help document expected timeouts
    let _ = TEST_TIMEOUT;
    let _ = CONNECT_TIMEOUT;
};
