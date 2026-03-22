# MDRN Transport Layer Test Plan

This document outlines TDD test cases for the libp2p transport layer implementation.

## Overview

The transport layer wraps libp2p functionality for MDRN's peer-to-peer communication:
- **Swarm**: Manages connections with Noise encryption and Yamux multiplexing
- **gossipsub**: Pub/sub messaging for stream data distribution (one topic per stream)
- **Kademlia DHT**: Distributed storage for stream announcements and relay advertisements

Protocol ID: `/mdrn/1.0.0`

---

## 1. Swarm Creation Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_swarm_creates_with_default_config` | Swarm initializes with `TransportConfig::default()` | - |
| `test_swarm_creates_with_custom_listen_addrs` | Respects custom TCP/QUIC listen addresses | Empty list, invalid multiaddr |
| `test_swarm_noise_encryption_configured` | Verifies Noise IK/XX patterns are enabled | - |
| `test_swarm_yamux_multiplexing_configured` | Verifies Yamux is the multiplexer | - |
| `test_swarm_protocol_id_is_mdrn_1_0_0` | Protocol negotiation uses `/mdrn/1.0.0` | - |
| `test_swarm_kademlia_params` | DHT configured with k=20, alpha=3, SHA-256 IDs | k=0, alpha=0 |
| `test_swarm_gossipsub_heartbeat` | Heartbeat interval matches config | Zero duration |
| `test_swarm_creation_fails_invalid_keypair` | Rejects malformed Ed25519/secp256k1 keys | - |
| `test_swarm_local_peer_id_matches_identity` | libp2p PeerId derives from MDRN Identity | - |

### Suggested Structure

```rust
#[cfg(test)]
mod swarm_creation_tests {
    use super::*;
    use crate::identity::Keypair;

    #[test]
    fn test_swarm_creates_with_default_config() {
        let keypair = Keypair::generate_ed25519();
        let config = TransportConfig::default();
        let swarm = MdrnSwarm::new(keypair, config);
        assert!(swarm.is_ok());
    }

    #[test]
    fn test_swarm_kademlia_params() {
        let config = TransportConfig {
            kademlia_k: 20,
            kademlia_alpha: 3,
            ..Default::default()
        };
        let swarm = MdrnSwarm::new(Keypair::generate_ed25519(), config).unwrap();
        // Assert DHT config (requires accessor or behavior test)
    }
}
```

---

## 2. Peer Connection Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_dial_valid_multiaddr` | Connects to peer via multiaddr | - |
| `test_dial_invalid_multiaddr_fails` | Returns error for malformed address | Empty string, garbage |
| `test_dial_self_fails` | Cannot dial own peer ID | - |
| `test_connection_event_emitted_on_connect` | `ConnectionEstablished` event fires | - |
| `test_disconnection_event_emitted` | `ConnectionClosed` event fires | - |
| `test_idle_connection_times_out` | Connection closes after `idle_timeout` | - |
| `test_multiple_connections_same_peer` | Handles multiple transports to same peer | TCP + QUIC simultaneously |

### Integration Tests (two-node)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_two_nodes_connect_tcp` | Node A dials Node B over TCP | - |
| `test_two_nodes_connect_quic` | Node A dials Node B over QUIC | - |
| `test_connection_survives_idle` | Connection persists within timeout | - |
| `test_reconnect_after_disconnect` | Nodes can reconnect after clean close | - |

### Suggested Structure

```rust
#[cfg(test)]
mod connection_tests {
    use tokio::test;

    #[tokio::test]
    async fn test_dial_valid_multiaddr() {
        let (mut node_a, mut node_b) = setup_two_nodes().await;
        let addr = node_b.listen_addr();

        let result = node_a.dial(addr).await;
        assert!(result.is_ok());

        // Wait for connection event
        let event = node_a.next_event().await;
        assert!(matches!(event, SwarmEvent::ConnectionEstablished { .. }));
    }
}
```

---

## 3. gossipsub Topic Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_subscribe_creates_topic` | Subscribing to topic joins mesh | - |
| `test_subscribe_idempotent` | Double-subscribe is no-op | - |
| `test_unsubscribe_leaves_topic` | Unsubscribing removes from mesh | - |
| `test_unsubscribe_not_subscribed` | Unsubscribe on unknown topic is no-op | - |
| `test_publish_to_subscribed_topic` | Can publish data to joined topic | - |
| `test_publish_to_unsubscribed_topic_fails` | Publishing to unjoined topic errors | - |
| `test_topic_format_matches_spec` | Topic is `/mdrn/stream/{hex(stream_addr)}` | - |
| `test_stream_topic_helper_function` | `stream_topic()` produces correct IdentTopic | All-zeros addr, random addr |

### Integration Tests (two-node)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_message_propagates_between_subscribers` | A publishes, B receives | - |
| `test_message_not_received_if_unsubscribed` | B doesn't get message after unsub | - |
| `test_multiple_subscribers_receive_message` | A, B, C all receive from publisher | - |
| `test_large_message_propagates` | Handles messages up to gossipsub limit | Near max size |
| `test_high_frequency_publishing` | Handles rapid message bursts | 100+ msg/sec |
| `test_message_has_correct_sender` | Received message includes source peer ID | - |

### Edge Cases to Cover

- **Empty message payload**: Should be allowed (keepalive use case)
- **Binary payload**: Audio chunks are binary, not text
- **Topic collision**: Two streams with same hash (astronomically unlikely but test handling)
- **Message ordering**: gossipsub doesn't guarantee order - test handling

### Suggested Structure

```rust
#[cfg(test)]
mod gossipsub_tests {
    use crate::transport::stream_topic;
    use crate::stream::StreamAnnouncement;

    #[test]
    fn test_topic_format_matches_spec() {
        let stream_addr = [0xAB; 32];
        let topic = stream_topic(&stream_addr);
        let expected = format!("/mdrn/stream/{}", hex::encode(stream_addr));
        assert_eq!(topic.to_string(), expected);
    }

    #[tokio::test]
    async fn test_message_propagates_between_subscribers() {
        let (mut node_a, mut node_b) = setup_two_connected_nodes().await;
        let stream_addr = [0x01; 32];
        let topic = stream_topic(&stream_addr);

        node_a.subscribe(&topic).unwrap();
        node_b.subscribe(&topic).unwrap();

        // Wait for mesh to form
        tokio::time::sleep(Duration::from_millis(100)).await;

        let payload = b"test audio chunk".to_vec();
        node_a.publish(&topic, payload.clone()).unwrap();

        let received = node_b.next_gossipsub_message().await;
        assert_eq!(received.data, payload);
    }
}
```

---

## 4. Kademlia DHT Tests

### Unit Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_put_record_succeeds` | Can store key-value in DHT | - |
| `test_put_record_with_ttl` | Record expires after TTL | - |
| `test_get_record_not_found` | Query for missing key returns None | - |
| `test_get_record_found` | Query for existing key returns value | - |
| `test_bootstrap_with_empty_nodes` | Gracefully handles no bootstrap peers | - |
| `test_bootstrap_with_valid_nodes` | Connects to bootstrap peers | - |
| `test_stream_namespace_key_format` | Keys use `/mdrn/streams/{stream_addr}` | - |
| `test_relay_namespace_key_format` | Keys use `/mdrn/relays/{stream_addr}` | - |

### Integration Tests (multi-node)

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_record_replicates_to_k_nodes` | Record stored on k closest peers | k=20 |
| `test_record_survives_node_churn` | Record persists when some nodes leave | - |
| `test_query_finds_record_after_bootstrap` | New node can find existing records | - |
| `test_concurrent_puts_same_key` | Handles race conditions | - |
| `test_get_closest_peers` | `get_closest_peers` returns correct set | - |

### StreamAnnouncement DHT Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_publish_stream_announcement` | StreamAnnouncement serialized and stored | - |
| `test_query_stream_announcement` | Retrieve and deserialize announcement | - |
| `test_announcement_ttl_respected` | Announcement expires after TTL | - |
| `test_announcement_refresh` | Re-publishing updates TTL | - |

### RelayAdvertisement DHT Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_publish_relay_advertisement` | RelayAdvertisement stored under stream | - |
| `test_query_relays_for_stream` | Find all relays for a stream_addr | 0 relays, 1 relay, many relays |
| `test_relay_advertisement_expires` | Old advertisements disappear | - |

### Suggested Structure

```rust
#[cfg(test)]
mod kademlia_tests {
    use crate::transport::{DHT_STREAM_NAMESPACE, DHT_RELAY_NAMESPACE};

    #[tokio::test]
    async fn test_put_and_get_record() {
        let mut swarm = setup_single_node().await;

        let key = format!("{}{}", DHT_STREAM_NAMESPACE, hex::encode([0x01; 32]));
        let value = b"test record".to_vec();

        swarm.dht_put(key.clone(), value.clone()).await.unwrap();

        let retrieved = swarm.dht_get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value));
    }

    #[tokio::test]
    async fn test_record_replicates_to_k_nodes() {
        let nodes = setup_n_connected_nodes(25).await; // > k=20

        let key = format!("{}{}", DHT_STREAM_NAMESPACE, hex::encode([0xAB; 32]));
        nodes[0].dht_put(key.clone(), b"data".to_vec()).await.unwrap();

        // Wait for replication
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Query from a different node
        let result = nodes[20].dht_get(&key).await.unwrap();
        assert!(result.is_some());
    }
}
```

---

## 5. Transport Protocol Negotiation Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_protocol_negotiation_succeeds` | Peers agree on `/mdrn/1.0.0` | - |
| `test_protocol_negotiation_fails_mismatch` | Rejects incompatible protocols | `/mdrn/2.0.0` |
| `test_identify_protocol_exchanges_info` | libp2p identify shares peer info | - |
| `test_noise_handshake_completes` | Noise encryption established | - |
| `test_noise_ik_for_known_peer` | Uses IK pattern when peer is known | - |
| `test_noise_xx_for_unknown_peer` | Uses XX pattern for discovery | - |

---

## 6. NAT Traversal Tests (Integration)

These require more complex network setups (simulated NAT or Docker).

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_autonat_detects_public_address` | AutoNAT identifies reachability | - |
| `test_relay_connection_when_nat_blocked` | Uses relay when direct fails | - |
| `test_dcutr_hole_punching` | DCUtR establishes direct connection | - |
| `test_relay_fallback_on_punch_failure` | Falls back to relay if punch fails | - |

---

## 7. Error Handling Tests

| Test Name | Description | Edge Cases |
|-----------|-------------|------------|
| `test_swarm_error_creation_failed` | `SwarmError::CreationFailed` message | - |
| `test_swarm_error_dial_failed` | `SwarmError::DialFailed` on bad dial | - |
| `test_swarm_error_publish_failed` | `SwarmError::PublishFailed` on pub error | - |
| `test_graceful_shutdown` | Swarm closes cleanly | - |
| `test_handle_peer_misbehavior` | gossipsub scores/bans bad peers | Spam, invalid messages |

---

## 8. Test Infrastructure Requirements

### Helper Functions Needed

```rust
/// Create a single swarm for unit tests
async fn setup_single_node() -> MdrnSwarm;

/// Create two connected nodes for integration tests
async fn setup_two_nodes() -> (MdrnSwarm, MdrnSwarm);

/// Create two nodes and establish connection
async fn setup_two_connected_nodes() -> (MdrnSwarm, MdrnSwarm);

/// Create N connected nodes for DHT tests
async fn setup_n_connected_nodes(n: usize) -> Vec<MdrnSwarm>;

/// Wait for specific swarm event with timeout
async fn wait_for_event<F>(swarm: &mut MdrnSwarm, predicate: F, timeout: Duration) -> Option<SwarmEvent>
where F: Fn(&SwarmEvent) -> bool;

/// Generate random stream address
fn random_stream_addr() -> [u8; 32];

/// Create test StreamAnnouncement
fn test_stream_announcement(broadcaster: &Keypair) -> StreamAnnouncement;
```

### Test Configuration

```rust
/// Test-specific config with fast timeouts
fn test_transport_config() -> TransportConfig {
    TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        kademlia_k: 3,        // Smaller for tests
        kademlia_alpha: 2,    // Smaller for tests
        gossipsub_heartbeat: Duration::from_millis(100),
        idle_timeout: Duration::from_secs(5),
    }
}
```

---

## 9. Test Organization

### Directory Structure

```
mdrn-core/src/transport/
    mod.rs
    config.rs
    swarm.rs
    tests/                      # Integration tests
        mod.rs
        connection_tests.rs
        gossipsub_tests.rs
        kademlia_tests.rs
        helpers.rs

mdrn-core/tests/                # External integration tests
    transport_integration.rs    # Multi-crate tests
```

### Unit vs Integration Split

| Category | Location | #[cfg(test)] | async |
|----------|----------|--------------|-------|
| Swarm creation | `swarm.rs` inline | Yes | No |
| Topic helpers | `mod.rs` inline | Yes | No |
| Connection (2-node) | `tests/connection_tests.rs` | No | Yes |
| gossipsub (2-node) | `tests/gossipsub_tests.rs` | No | Yes |
| Kademlia (N-node) | `tests/kademlia_tests.rs` | No | Yes |
| End-to-end | `mdrn-core/tests/` | No | Yes |

---

## 10. Acceptance Criteria Summary

The transport layer is complete when:

1. **Swarm Creation**
   - [x] Creates with Noise + Yamux
   - [x] Uses `/mdrn/1.0.0` protocol
   - [x] Kademlia k=20, alpha=3
   - [x] Supports TCP + QUIC

2. **Peer Connections**
   - [x] Dial peers by multiaddr
   - [x] Connection events fire correctly
   - [x] Idle timeout works

3. **gossipsub**
   - [x] Subscribe/unsubscribe topics
   - [x] Messages propagate between subscribers
   - [x] Topic format `/mdrn/stream/{hex}`

4. **Kademlia DHT**
   - [x] Put/get records
   - [x] Namespace prefixes correct
   - [x] Records replicate to k nodes
   - [x] TTL expiration works

5. **Error Handling**
   - [x] All error variants tested
   - [x] Graceful shutdown

---

## Notes

- **libp2p version**: 0.54 (per Cargo.toml)
- **Async runtime**: tokio with `test-util` feature for time manipulation
- **Deterministic tests**: Use seeded RNG where randomness is involved
- **Timeout tests**: Use `tokio::time::pause()` to avoid wall-clock waits
