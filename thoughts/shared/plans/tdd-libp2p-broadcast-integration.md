# TDD Test Plan: libp2p Network Broadcasting Integration

Generated: 2026-03-22
Author: Plan Agent (Opus)

## Goal

Transform `run_broadcast()` from an "audio processor that outputs to stdout" into a "network broadcaster" that:
1. Publishes `StreamAnnouncement` to DHT via `MdrnSwarm::dht_put()`
2. Subscribes to the stream's gossipsub topic
3. Publishes CBOR-serialized `Chunk` messages via `MdrnSwarm::publish()`
4. Handles async libp2p operations from synchronous CLI context

## Existing Components Analysis

### MdrnSwarm (mdrn-core/src/transport/swarm.rs)
- Full libp2p implementation with TCP/QUIC, Noise, Kademlia, Gossipsub
- Key methods for broadcasting:
  - `new(keypair, config)` - Creates swarm
  - `listen(addr)` - Async, starts listening
  - `subscribe(topic)` - Subscribes to gossipsub topic
  - `publish(topic, data)` - Publishes to topic (requires subscription)
  - `dht_put(key, value)` - Stores in local DHT + Kademlia network
  - `run()` - Async event loop

### run_broadcast() (mdrn-cli/src/broadcast.rs)
- Currently synchronous
- Returns `BroadcastResult { announcement, chunks, stream_key }`
- Does NOT touch networking - just audio processing

### Protocol Types
- `StreamAnnouncement` - derives `Serialize/Deserialize` (ciborium compatible)
- `Chunk` - derives `Serialize/Deserialize` (ciborium compatible)
- Topic format: `/mdrn/stream/{hex(stream_addr)}`
- DHT namespace: `/mdrn/streams/`

---

## Test Categories

### Category 1: Unit Tests (No Network)

These test the integration logic without requiring actual network operations.

#### 1.1 CBOR Serialization Tests

```
test_stream_announcement_cbor_roundtrip
test_chunk_cbor_roundtrip
test_encrypted_chunk_cbor_roundtrip
test_serialized_chunk_size_reasonable (< 2KB for 20ms Opus)
```

**Purpose:** Ensure `StreamAnnouncement` and `Chunk` serialize/deserialize correctly for network transmission.

**Edge Cases:**
- Empty stream_id
- Maximum bitrate values
- All codec variants
- Encrypted vs unencrypted chunks
- Keyframe flag variations

#### 1.2 Topic Construction Tests

```
test_stream_topic_format
test_stream_topic_from_announcement
test_dht_key_construction
```

**Purpose:** Verify topic/key strings match protocol spec.

**Assertions:**
- Topic matches `/mdrn/stream/{64-char-hex}`
- DHT key matches `/mdrn/streams/{64-char-hex}`

#### 1.3 Chunk Timing Tests

```
test_chunk_timestamps_are_monotonic
test_chunk_sequence_numbers_increment
test_chunk_duration_matches_frame_size
test_chunk_timestamp_spacing_is_20ms
```

**Purpose:** Ensure chunk metadata is correct before network transmission.

---

### Category 2: Swarm Integration Tests (Mock Network)

These test MdrnSwarm behavior without requiring peer connections.

#### 2.1 Swarm Initialization Tests

```
test_swarm_creates_from_mdrn_keypair
test_swarm_protocol_id_is_mdrn_1_0_0
test_swarm_can_listen_on_random_port
test_swarm_config_applies_kademlia_params
```

**Already exist:** Tests at swarm.rs:379-611 cover basic swarm creation.

#### 2.2 Topic Subscription Tests

```
test_subscribe_to_stream_topic_succeeds
test_publish_requires_prior_subscription
test_unsubscribe_prevents_publish
test_multiple_topic_subscriptions
```

**Already partially exist:** Tests at swarm.rs:492-560.

**New tests needed:**
- `test_subscribe_to_own_stream_topic` - Broadcaster must subscribe before publishing

#### 2.3 DHT Storage Tests

```
test_dht_put_stores_locally
test_dht_put_serialized_announcement
test_dht_get_returns_stored_value
test_dht_key_uses_stream_namespace
```

**Already partially exist:** dht_put/dht_get tested implicitly.

**New tests needed:**
- `test_dht_stores_cbor_announcement` - Full announcement roundtrip

#### 2.4 Publish Tests

```
test_publish_to_subscribed_topic_succeeds
test_publish_returns_error_without_subscription
test_publish_accepts_cbor_bytes
```

**Already exist:** test_publish_requires_subscription at swarm.rs:560.

---

### Category 3: Async/Sync Bridge Tests

Critical for CLI integration - the CLI is sync, but libp2p is async.

#### 3.1 Runtime Creation Tests

```
test_tokio_runtime_creates_for_broadcast
test_swarm_runs_in_tokio_block_on
test_multiple_sequential_broadcasts
```

**Purpose:** Verify tokio runtime management doesn't leak or conflict.

#### 3.2 Blocking Operation Tests

```
test_dht_put_blocks_until_local_store
test_publish_blocks_until_sent
test_broadcast_completes_all_chunks
```

**Purpose:** Ensure async ops complete before returning to sync caller.

#### 3.3 Cancellation Tests

```
test_broadcast_can_be_interrupted
test_partial_broadcast_cleanup
```

**Purpose:** Handle Ctrl+C gracefully.

---

### Category 4: Network Broadcaster Integration Tests

Full end-to-end tests with mock or real network.

#### 4.1 Broadcast Pipeline Tests

```
test_broadcast_publishes_announcement_to_dht
test_broadcast_subscribes_to_stream_topic
test_broadcast_publishes_all_chunks
test_broadcast_chunks_are_valid_cbor
test_broadcast_respects_chunk_timing
```

**Setup:**
1. Create keypair and vouch
2. Create test WAV file
3. Call new `broadcast_to_network()` function
4. Verify DHT contains announcement
5. Verify topic received all chunks

#### 4.2 Multi-Peer Tests (Integration)

```
test_two_swarms_can_gossip_chunks
test_listener_receives_chunks_from_broadcaster
test_dht_announcement_propagates_to_peer
```

**Setup:**
1. Create two MdrnSwarm instances
2. Connect them via dial/listen
3. One broadcasts, one listens
4. Verify chunk reception

---

### Category 5: Error Handling Tests

#### 5.1 Network Failure Tests

```
test_broadcast_handles_no_listeners
test_broadcast_continues_on_publish_error
test_dht_put_failure_is_recoverable
test_gossipsub_mesh_failure_handling
```

**Behavior:**
- DHT put failure: Log warning, continue (local store still works)
- Publish failure: Log error, optionally retry or skip chunk
- No listeners: Still succeeds (gossipsub allows publishing without peers)

#### 5.2 Resource Exhaustion Tests

```
test_large_file_broadcast_memory_bounded
test_many_chunks_dont_exhaust_buffers
```

**Purpose:** Streaming chunks, not buffering entire file.

#### 5.3 Invalid State Tests

```
test_broadcast_without_keypair_fails
test_broadcast_without_vouch_fails
test_publish_to_unknown_topic_fails
```

---

### Category 6: Timing and Performance Tests

#### 6.1 Real-Time Pacing Tests

```
test_chunk_publish_rate_matches_audio_duration
test_20ms_chunks_publish_every_20ms
test_broadcast_duration_matches_audio_length
```

**Purpose:** Chunks must be published at playback rate for live streaming.

**Implementation note:** For file-based broadcast, may batch initially. For live, must pace.

#### 6.2 Latency Tests

```
test_dht_put_latency_acceptable (< 100ms local)
test_publish_latency_acceptable (< 10ms local)
```

---

## Test Structure Recommendations

### File Organization

```
mdrn-cli/tests/
  broadcast_tests.rs        # Existing - audio pipeline tests
  broadcast_network_tests.rs # NEW - network integration tests

mdrn-core/src/transport/
  swarm.rs                  # Has inline #[cfg(test)] tests

mdrn-core/tests/
  integration/
    two_peer_broadcast.rs   # NEW - multi-peer integration tests
```

### Test Helpers Needed

```rust
/// Create a test swarm with random port
fn create_test_swarm() -> MdrnSwarm

/// Create connected pair of swarms
async fn create_connected_swarms() -> (MdrnSwarm, MdrnSwarm)

/// Create minimal valid StreamAnnouncement
fn test_announcement(keypair: &Keypair) -> StreamAnnouncement

/// Create test chunks from announcement
fn test_chunks(announcement: &StreamAnnouncement, count: usize) -> Vec<Chunk>

/// Serialize to CBOR bytes
fn to_cbor<T: Serialize>(value: &T) -> Vec<u8>
```

### Async Test Pattern

```rust
#[tokio::test]
async fn test_broadcast_publishes_to_network() {
    let keypair = Keypair::generate_ed25519().unwrap();
    let mut swarm = MdrnSwarm::new(keypair.clone(), TransportConfig::default()).unwrap();

    // Start listening
    swarm.listen("/ip4/127.0.0.1/tcp/0".parse().unwrap()).await.unwrap();

    // Create announcement
    let announcement = test_announcement(&keypair);
    let topic = stream_topic(&announcement.stream_addr);

    // Subscribe and publish
    swarm.subscribe(&topic).unwrap();

    let cbor = ciborium::to_vec(&announcement).unwrap();
    swarm.dht_put(
        format!("{}{}", DHT_STREAM_NAMESPACE, hex::encode(&announcement.stream_addr)).into_bytes(),
        cbor.clone()
    ).unwrap();

    // Verify local DHT has it
    let stored = swarm.dht_get(/* key */).unwrap();
    assert_eq!(stored, cbor);
}
```

---

## Implementation Order (TDD Red-Green-Refactor)

### Phase 1: Serialization (Unit Tests First)

1. Write `test_stream_announcement_cbor_roundtrip` - RED
2. Verify `StreamAnnouncement` already derives Serialize/Deserialize - GREEN
3. Write `test_chunk_cbor_roundtrip` - Should already pass

### Phase 2: Network API (Integration Tests)

1. Write `test_broadcast_publishes_announcement_to_dht` - RED
2. Create `broadcast_to_network()` function skeleton - RED
3. Add `MdrnSwarm` creation - still RED
4. Add `dht_put()` call with CBOR announcement - GREEN
5. Write `test_broadcast_subscribes_to_stream_topic` - RED
6. Add `subscribe()` call - GREEN
7. Write `test_broadcast_publishes_all_chunks` - RED
8. Add chunk iteration with `publish()` - GREEN

### Phase 3: CLI Integration

1. Write `test_cli_broadcast_with_network_flag` - RED
2. Add `--network` flag to CLI - RED
3. Add tokio runtime wrapper - GREEN
4. Write `test_broadcast_output_includes_peer_id` - RED
5. Add peer ID to output - GREEN

### Phase 4: Async Bridge

1. Write `test_tokio_runtime_creates_for_broadcast` - RED
2. Add `#[tokio::main]` or `Runtime::new()` pattern - GREEN
3. Write `test_multiple_sequential_broadcasts` - verify no runtime conflicts

### Phase 5: Multi-Peer (End-to-End)

1. Write `test_two_swarms_can_gossip_chunks` - RED
2. Implement peer connection in test setup - GREEN
3. Write `test_listener_receives_chunks_from_broadcaster` - RED
4. This validates the full pipeline works

---

## Edge Cases Checklist

### Network Edge Cases
- [ ] No bootstrap nodes available
- [ ] All peers disconnect mid-broadcast
- [ ] DHT put times out
- [ ] Gossipsub mesh not formed yet
- [ ] Topic has no subscribers
- [ ] Publish queue full

### Audio Edge Cases
- [ ] Zero-length audio file
- [ ] Single chunk broadcast
- [ ] Very long broadcast (10,000+ chunks)
- [ ] Encrypted with no key distribution yet

### Timing Edge Cases
- [ ] System clock jumps during broadcast
- [ ] Slow consumer backpressure
- [ ] Fast producer overwhelms network

### State Edge Cases
- [ ] Broadcast interrupted mid-stream
- [ ] Same stream_id broadcast twice
- [ ] Keypair/identity mismatch with vouch

---

## Expected Test Count

| Category | Test Count |
|----------|------------|
| CBOR Serialization | 4 |
| Topic/Key Construction | 3 |
| Chunk Timing | 4 |
| Swarm Integration | 8 |
| Async Bridge | 5 |
| Broadcast Pipeline | 5 |
| Multi-Peer | 3 |
| Error Handling | 6 |
| Timing/Performance | 4 |
| **Total** | **42** |

---

## Dependencies

### Test Dependencies (add to Cargo.toml)

```toml
[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
tempfile = "3"
```

### Runtime Dependencies (already present)

- `ciborium` - CBOR serialization
- `libp2p` - Networking
- `tokio` - Async runtime

---

## Success Criteria

The integration is complete when:

1. `mdrn broadcast --input test.wav --stream-id test --network` successfully:
   - Creates MdrnSwarm with broadcaster identity
   - Publishes StreamAnnouncement to DHT
   - Subscribes to stream topic
   - Publishes all chunks to gossipsub

2. A second CLI instance can:
   - Query DHT for the announcement
   - Subscribe to the stream topic
   - Receive chunks via gossipsub

3. All 42 tests pass with:
   - No network flakiness
   - < 5 second total test runtime
   - No resource leaks

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Gossipsub mesh not forming in tests | Tests fail/flaky | Use short heartbeat, add delay or mock |
| DHT propagation slow | Tests timeout | Test local store first, network optional |
| Tokio runtime conflicts | Panic | Use single runtime, or `#[tokio::test]` |
| Large test audio files | Slow CI | Use 100ms test files |
| libp2p version changes | API breaks | Pin version in workspace |

---

## Notes

- The existing `MdrnSwarm` is well-tested for basic operations
- CBOR serialization uses `ciborium` consistently
- The async/sync bridge is the main complexity
- Consider adding `--dry-run` flag that skips network but validates pipeline
