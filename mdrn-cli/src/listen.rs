//! Listen command implementation
//!
//! Handles:
//! - Stream address parsing (hex or stream_id)
//! - Chunk reception from stdin (MVP) or network (gossipsub)
//! - Opus decoding
//! - WAV file output
//! - Encryption handling (decryption with stream key)

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use mdrn_core::crypto;
use mdrn_core::stream::{Chunk, StreamAnnouncement};
use mdrn_core::transport::{stream_topic, MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE};

/// Configuration for listen operation
pub struct ListenConfig {
    /// Stream address (32 bytes)
    pub stream_addr: [u8; 32],
    /// Output WAV file path (None = stdout/speakers)
    pub output_path: Option<PathBuf>,
    /// Stream key for encrypted streams
    pub stream_key: Option<[u8; 32]>,
    /// Network mode (vs stdin mode)
    #[allow(dead_code)]
    pub network: bool,
}

/// Result of listen operation
pub struct ListenResult {
    /// Number of chunks received
    pub chunks_received: usize,
    /// Number of chunks decoded successfully
    pub chunks_decoded: usize,
    /// Total audio duration in milliseconds
    pub duration_ms: u64,
    /// Output file path if written
    pub output_path: Option<PathBuf>,
}

/// Parse stream address from CLI argument
///
/// Accepts:
/// - 64-char hex string (raw stream address)
/// - 0x-prefixed 64-char hex string
/// - Anything else is treated as stream_id (requires DHT lookup)
pub fn parse_stream_address(input: &str) -> Result<ParsedAddress> {
    // Strip 0x prefix if present
    let hex_str = input.strip_prefix("0x").unwrap_or(input);

    // Try to parse as hex
    if hex_str.len() == 64 {
        match hex::decode(hex_str) {
            Ok(bytes) if bytes.len() == 32 => {
                let mut addr = [0u8; 32];
                addr.copy_from_slice(&bytes);
                return Ok(ParsedAddress::Hex(addr));
            }
            _ => {}
        }
    }

    // Fall back to stream_id (requires broadcaster lookup)
    Ok(ParsedAddress::StreamId(input.to_string()))
}

/// Parsed stream address
#[derive(Debug)]
pub enum ParsedAddress {
    /// Direct hex stream address
    Hex([u8; 32]),
    /// Stream ID that requires DHT lookup
    StreamId(String),
}

/// Run listen in stdin mode (MVP)
///
/// Reads hex-encoded CBOR chunks from stdin, one per line.
/// Decodes Opus and writes to WAV file.
pub fn run_listen_stdin(config: &ListenConfig) -> Result<ListenResult> {
    tracing::info!(
        stream_addr = %hex::encode(&config.stream_addr),
        output = ?config.output_path,
        "Starting listen (stdin mode)"
    );

    // Create Opus decoder based on announcement (assume stereo for now)
    let mut decoder = opus::Decoder::new(48000, opus::Channels::Stereo)
        .map_err(|e| anyhow::anyhow!("Failed to create Opus decoder: {}", e))?;

    // Buffer for decoded audio
    let mut all_samples: Vec<i16> = Vec::new();
    let mut chunks_received = 0;
    let mut chunks_decoded = 0;

    // Read chunks from stdin
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("Error reading stdin: {}", e);
                break;
            }
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Decode hex to CBOR
        let cbor_bytes = match hex::decode(line) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to decode hex: {}", e);
                continue;
            }
        };

        // Parse CBOR to Chunk
        let chunk: Chunk = match ciborium::from_reader(&cbor_bytes[..]) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to parse CBOR chunk: {}", e);
                continue;
            }
        };

        chunks_received += 1;

        // Verify stream address matches
        if chunk.stream_addr != config.stream_addr {
            tracing::warn!(
                expected = %hex::encode(&config.stream_addr),
                got = %hex::encode(&chunk.stream_addr),
                "Chunk has wrong stream address, skipping"
            );
            continue;
        }

        // Decrypt if needed
        let audio_data = if chunk.is_encrypted() {
            let key = config.stream_key.ok_or_else(|| {
                anyhow::anyhow!("Stream is encrypted but no key provided")
            })?;
            let nonce = chunk.nonce.ok_or_else(|| {
                anyhow::anyhow!("Encrypted chunk missing nonce")
            })?;
            crypto::decrypt(&key, &chunk.data, &nonce)
                .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?
        } else {
            chunk.data.clone()
        };

        // Decode Opus
        let mut pcm_buffer = vec![0i16; 960 * 2]; // 20ms at 48kHz stereo
        match decoder.decode(&audio_data, &mut pcm_buffer, false) {
            Ok(samples) => {
                all_samples.extend_from_slice(&pcm_buffer[..samples * 2]); // stereo
                chunks_decoded += 1;
                tracing::debug!("Decoded chunk {} ({} samples)", chunk.seq, samples);
            }
            Err(e) => {
                tracing::warn!("Opus decode failed for chunk {}: {}", chunk.seq, e);
            }
        }
    }

    // Write WAV file if output specified
    if let Some(ref output_path) = config.output_path {
        write_wav_file(output_path, &all_samples, 48000, 2)?;
        tracing::info!("Wrote {} samples to {}", all_samples.len(), output_path.display());
    }

    let duration_ms = (chunks_decoded as u64) * 20; // 20ms per chunk

    Ok(ListenResult {
        chunks_received,
        chunks_decoded,
        duration_ms,
        output_path: config.output_path.clone(),
    })
}

/// Run listen in network mode
///
/// Discovers stream via DHT, subscribes to gossipsub topic,
/// receives chunks in real-time.
pub async fn run_listen_network(
    config: &ListenConfig,
    announcement: Option<StreamAnnouncement>,
    relay_addr: Option<String>,
) -> Result<ListenResult> {
    use futures::StreamExt;
    use libp2p::swarm::SwarmEvent;
    use mdrn_core::identity::Keypair;
    use mdrn_core::transport::MdrnBehaviourEvent;
    use std::time::Duration;
    use tokio::time::timeout;

    tracing::info!(
        stream_addr = %hex::encode(&config.stream_addr),
        output = ?config.output_path,
        "Starting listen (network mode)"
    );

    // Generate temporary keypair for listening
    let keypair = Keypair::generate_ed25519()
        .map_err(|e| anyhow::anyhow!("Failed to generate keypair: {}", e))?;

    // Create swarm
    let swarm_config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    let mut swarm = MdrnSwarm::new(keypair, swarm_config)
        .map_err(|e| anyhow::anyhow!("Failed to create swarm: {}", e))?;

    // Start listening for incoming connections
    swarm.listen("/ip4/127.0.0.1/tcp/0".parse()?)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start listening: {}", e))?;

    tracing::info!("Listener listening for connections");

    // Connect to relay node
    let relay_address = relay_addr
        .or_else(|| std::env::var("MDRN_RELAY").ok())
        .unwrap_or_else(|| "/ip4/127.0.0.1/tcp/9000".to_string());
    let relay_multiaddr: libp2p::Multiaddr = relay_address.parse()
        .map_err(|e| anyhow::anyhow!("Invalid relay address '{}': {}", relay_address, e))?;

    tracing::info!("Connecting to relay: {}", relay_multiaddr);
    swarm.dial(relay_multiaddr.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to dial relay: {}", e))?;

    // ── HOTFIX: Wait for Identify (not just ConnectionEstablished) before subscribing ──
    //
    // ConnectionEstablished fires when TCP handshake completes, but gossipsub protocol
    // negotiation (multistream-select + yamux) is still in progress. Calling subscribe()
    // immediately after ConnectionEstablished causes the subscription message to be
    // silently dropped because the gossipsub stream doesn't exist yet.
    //
    // Fix: wait for identify::Event::Received, which only fires after all protocols have
    // been negotiated and the peer info exchange is complete. Only THEN subscribe.
    // This ensures gossipsub is ready to accept our subscription.
    let mut connected = false;
    let mut protocols_ready = false;
    let connect_timeout = Duration::from_secs(15);
    let start_time = std::time::Instant::now();

    while (!connected || !protocols_ready) && start_time.elapsed() < connect_timeout {
        match tokio::time::timeout(Duration::from_millis(200), swarm.inner_mut().select_next_some()).await {
            Ok(SwarmEvent::ConnectionEstablished { peer_id, .. }) => {
                tracing::info!("Connected to relay peer: {}", peer_id);
                connected = true;
            }
            Ok(SwarmEvent::Behaviour(MdrnBehaviourEvent::Identify(
                libp2p::identify::Event::Received { peer_id, .. }
            ))) => {
                tracing::info!("Protocol negotiation complete with relay: {}", peer_id);
                protocols_ready = true;
            }
            Ok(SwarmEvent::OutgoingConnectionError { error, .. }) => {
                anyhow::bail!("Failed to connect to relay: {}", error);
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    if !connected {
        anyhow::bail!("Timeout connecting to relay after {}s", connect_timeout.as_secs());
    }
    if !protocols_ready {
        tracing::warn!("Identify not received — subscribing anyway (protocols may not be ready)");
    }
    // ── END HOTFIX ──

    // Now that protocols are negotiated, subscribe to the topic
    let topic = stream_topic(&config.stream_addr);
    swarm
        .subscribe(&topic)
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to topic: {}", e))?;

    tracing::info!("Subscribed to topic: {}", topic);

    // Determine channels from announcement or default to stereo
    let channels = announcement
        .as_ref()
        .map(|a| a.channels)
        .unwrap_or(2);

    let opus_channels = match channels {
        1 => opus::Channels::Mono,
        _ => opus::Channels::Stereo,
    };

    // Create Opus decoder
    let mut decoder = opus::Decoder::new(48000, opus_channels)
        .map_err(|e| anyhow::anyhow!("Failed to create Opus decoder: {}", e))?;

    // Buffer for decoded audio
    let mut all_samples: Vec<i16> = Vec::new();
    let mut chunks_received = 0;
    let mut chunks_decoded = 0;

    // Listen for chunks with timeout
    // HOTFIX: Extended from 10s to 60s — the listener must stay alive long enough
    // for the broadcaster to connect, form a mesh with the relay, and publish all chunks.
    // At 20ms/chunk × 250 chunks = 5s of audio + relay/mesh formation time (~15s worst case).
    // In production this would be infinite (stream runs until cancelled), but 60s
    // covers a full Phase 1 demo run end-to-end without ctrl-C management.
    let listen_duration = Duration::from_secs(60);
    let start = std::time::Instant::now();

    while start.elapsed() < listen_duration {
        let event = timeout(Duration::from_millis(100), swarm.inner_mut().select_next_some()).await;

        match event {
            Ok(SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                libp2p::gossipsub::Event::Message { message, .. },
            ))) => {
                // Parse chunk from CBOR
                let chunk: Chunk = match ciborium::from_reader(&message.data[..]) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to parse chunk: {}", e);
                        continue;
                    }
                };

                chunks_received += 1;

                // Verify stream address
                if chunk.stream_addr != config.stream_addr {
                    continue;
                }

                // Decrypt if needed
                let audio_data = if chunk.is_encrypted() {
                    let key = match config.stream_key {
                        Some(k) => k,
                        None => {
                            tracing::warn!("Encrypted chunk but no key, skipping");
                            continue;
                        }
                    };
                    let nonce = match chunk.nonce {
                        Some(n) => n,
                        None => {
                            tracing::warn!("Encrypted chunk missing nonce");
                            continue;
                        }
                    };
                    match crypto::decrypt(&key, &chunk.data, &nonce) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!("Decryption failed: {}", e);
                            continue;
                        }
                    }
                } else {
                    chunk.data.clone()
                };

                // Decode Opus
                let buffer_size = 960 * channels as usize;
                let mut pcm_buffer = vec![0i16; buffer_size];
                match decoder.decode(&audio_data, &mut pcm_buffer, false) {
                    Ok(samples) => {
                        let sample_count = samples * channels as usize;
                        all_samples.extend_from_slice(&pcm_buffer[..sample_count]);
                        chunks_decoded += 1;
                        tracing::debug!("Decoded chunk {} ({} samples)", chunk.seq, samples);
                    }
                    Err(e) => {
                        tracing::warn!("Opus decode failed: {}", e);
                    }
                }
            }
            Ok(SwarmEvent::NewListenAddr { address, .. }) => {
                tracing::info!("Listening on {}", address);
            }
            Ok(_) => {}
            Err(_) => {
                // Timeout, continue loop
            }
        }
    }

    // Write WAV file if output specified
    if let Some(ref output_path) = config.output_path {
        write_wav_file(output_path, &all_samples, 48000, channels)?;
        tracing::info!("Wrote {} samples to {}", all_samples.len(), output_path.display());
    }

    let duration_ms = (chunks_decoded as u64) * 20;

    Ok(ListenResult {
        chunks_received,
        chunks_decoded,
        duration_ms,
        output_path: config.output_path.clone(),
    })
}

/// Write samples to WAV file
fn write_wav_file(path: &PathBuf, samples: &[i16], sample_rate: u32, channels: u8) -> Result<()> {
    use std::fs::File;

    let file = File::create(path)?;
    let mut writer = io::BufWriter::new(file);

    // WAV header
    let data_size = (samples.len() * 2) as u32;
    let file_size = 36 + data_size;
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels as u16 * 2;

    // RIFF header
    writer.write_all(b"RIFF")?;
    writer.write_all(&file_size.to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    // fmt chunk
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?; // chunk size
    writer.write_all(&1u16.to_le_bytes())?; // PCM format
    writer.write_all(&(channels as u16).to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&16u16.to_le_bytes())?; // bits per sample

    // data chunk
    writer.write_all(b"data")?;
    writer.write_all(&data_size.to_le_bytes())?;

    // Write samples
    for sample in samples {
        writer.write_all(&sample.to_le_bytes())?;
    }

    writer.flush()?;
    Ok(())
}

/// Lookup stream announcement in DHT
#[allow(dead_code)]
pub async fn lookup_stream_announcement(
    swarm: &MdrnSwarm,
    stream_addr: &[u8; 32],
) -> Option<StreamAnnouncement> {
    let dht_key = format!("{}{}", DHT_STREAM_NAMESPACE, hex::encode(stream_addr));
    let value = swarm.dht_get(dht_key.as_bytes())?;
    ciborium::from_reader(&value[..]).ok()
}
