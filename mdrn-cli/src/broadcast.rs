//! Broadcast command implementation
//!
//! Handles:
//! - Keypair loading from file or environment
//! - Vouch loading and verification
//! - Audio file reading and processing
//! - Opus encoding with configurable bitrate
//! - Stream encryption with ChaCha20-Poly1305
//! - Chunk generation with timestamps and sequence numbers
//! - Output stream announcement and chunks

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use mdrn_core::identity::{Keypair, Vouch};
use mdrn_core::stream::{Codec, StreamAnnouncement, Chunk};
use mdrn_core::crypto;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Audio data from file reading
#[derive(Debug)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u8,
}

/// Broadcast configuration
pub struct BroadcastConfig<'a> {
    pub stream_id: &'a str,
    pub input_path: &'a PathBuf,
    pub bitrate_kbps: u32,
    pub encrypted: bool,
}

/// Result of broadcast operation
pub struct BroadcastResult {
    pub announcement: StreamAnnouncement,
    pub chunks: Vec<Chunk>,
    pub stream_key: Option<Vec<u8>>,
}

/// Load keypair from file path
pub fn load_keypair_from_file(path: &PathBuf) -> Result<Keypair> {
    let bytes = std::fs::read(path)?;
    let keypair = Keypair::from_cbor(&bytes)?;
    Ok(keypair)
}

/// Load keypair from default location or environment variable
pub fn load_keypair_default() -> Result<Keypair> {
    // Check environment variable first
    if let Ok(env_path) = std::env::var("MDRN_KEYPAIR") {
        return load_keypair_from_file(&PathBuf::from(env_path));
    }

    // Check default location
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
    let default_path = PathBuf::from(home).join(".mdrn").join("keypair.cbor");

    load_keypair_from_file(&default_path)
}

/// Load vouch from file path
pub fn load_vouch_from_file(path: &PathBuf) -> Result<Vouch> {
    let bytes = std::fs::read(path)?;
    let vouch: Vouch = ciborium::from_reader(&bytes[..])?;
    vouch.verify()?; // Verify the vouch is valid
    Ok(vouch)
}

/// Load vouch from default location or environment variable
pub fn load_vouch_default() -> Result<Vouch> {
    // Check environment variable first
    if let Ok(env_path) = std::env::var("MDRN_VOUCH") {
        return load_vouch_from_file(&PathBuf::from(env_path));
    }

    // Check default location
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
    let default_path = PathBuf::from(home).join(".mdrn").join("vouch.cbor");

    load_vouch_from_file(&default_path)
}

/// Read and decode audio file using symphonia
pub fn read_audio_file(path: &PathBuf) -> Result<AudioData> {
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)?;
    let media_source = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension() {
        if let Some(ext_str) = extension.to_str() {
            hint.with_extension(ext_str);
        }
    }

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    let probed = symphonia::default::get_probe().format(&hint, media_source, &fmt_opts, &meta_opts)?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow::anyhow!("No audio track found"))?;

    let dec_opts: DecoderOptions = Default::default();
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &dec_opts)?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.ok_or_else(|| anyhow::anyhow!("Unknown sample rate"))?;
    let channels = track.codec_params.channels.ok_or_else(|| anyhow::anyhow!("Unknown channel count"))?.count();

    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::ResetRequired) => {
                unimplemented!("stream reset required");
            }
            Err(symphonia::core::errors::Error::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(err) => return Err(err.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let audio_buf = decoder.decode(&packet)?;

        let mut sample_buf = SampleBuffer::<f32>::new(audio_buf.capacity() as u64, *audio_buf.spec());
        sample_buf.copy_interleaved_ref(audio_buf);

        samples.extend_from_slice(sample_buf.samples());
    }

    Ok(AudioData {
        samples,
        sample_rate,
        channels: channels as u8,
    })
}

/// Resample audio to 48kHz if needed
pub fn resample_to_48k(audio_data: &AudioData) -> Result<AudioData> {
    if audio_data.sample_rate == 48000 {
        return Ok(AudioData {
            samples: audio_data.samples.clone(),
            sample_rate: audio_data.sample_rate,
            channels: audio_data.channels,
        });
    }

    use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 160,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        48000.0 / audio_data.sample_rate as f64,
        2.0,
        params,
        audio_data.samples.len(),
        audio_data.channels as usize,
    )?;

    // Convert interleaved to channels
    let channels = audio_data.channels as usize;
    let frames = audio_data.samples.len() / channels;
    let mut channel_data = vec![vec![0.0f32; frames]; channels];

    for (i, &sample) in audio_data.samples.iter().enumerate() {
        let channel = i % channels;
        let frame = i / channels;
        channel_data[channel][frame] = sample;
    }

    let resampled_data = resampler.process(&channel_data, None)?;

    // Convert back to interleaved
    let mut samples = Vec::new();
    let resampled_frames = resampled_data[0].len();
    for frame in 0..resampled_frames {
        for channel in 0..channels {
            samples.push(resampled_data[channel][frame]);
        }
    }

    Ok(AudioData {
        samples,
        sample_rate: 48000,
        channels: audio_data.channels,
    })
}

/// Encode audio to Opus with specified bitrate
pub fn encode_opus(audio_data: &AudioData, bitrate_kbps: u32) -> Result<Vec<Vec<u8>>> {
    use opus::{Encoder, Application, Channels};

    let opus_channels = match audio_data.channels {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        _ => return Err(anyhow::anyhow!("Unsupported channel count: {}", audio_data.channels)),
    };

    let mut encoder = Encoder::new(48000, opus_channels, Application::Audio)?;
    encoder.set_bitrate(opus::Bitrate::Bits(bitrate_kbps as i32 * 1000))?;

    const FRAME_SIZE: usize = 960; // 20ms at 48kHz
    let frame_samples = FRAME_SIZE * audio_data.channels as usize;

    let mut encoded_frames = Vec::new();
    let mut output_buffer = vec![0u8; 4000]; // Max Opus frame size

    for chunk in audio_data.samples.chunks(frame_samples) {
        if chunk.len() < frame_samples {
            // Pad the last frame with zeros if needed
            let mut padded = chunk.to_vec();
            padded.resize(frame_samples, 0.0);

            let len = encoder.encode_float(&padded, &mut output_buffer)?;
            encoded_frames.push(output_buffer[..len].to_vec());
        } else {
            let len = encoder.encode_float(chunk, &mut output_buffer)?;
            encoded_frames.push(output_buffer[..len].to_vec());
        }
    }

    Ok(encoded_frames)
}

/// Generate stream key for encryption
pub fn generate_stream_key() -> [u8; 32] {
    crypto::generate_stream_key()
}

/// Main broadcast function
pub fn run_broadcast(keypair: &Keypair, vouch: &Vouch, config: &BroadcastConfig) -> Result<BroadcastResult> {
    // Read and process audio
    tracing::info!("Reading audio file: {}", config.input_path.display());
    let audio_data = read_audio_file(config.input_path)?;

    tracing::info!("Original: {}Hz, {} channels", audio_data.sample_rate, audio_data.channels);

    // Resample to 48kHz if needed
    let audio_48k = resample_to_48k(&audio_data)?;
    tracing::info!("Resampled: {}Hz, {} channels", audio_48k.sample_rate, audio_48k.channels);

    // Encode to Opus
    tracing::info!("Encoding to Opus at {}kbps", config.bitrate_kbps);
    let encoded_frames = encode_opus(&audio_48k, config.bitrate_kbps)?;
    tracing::info!("Encoded {} frames", encoded_frames.len());

    // Generate stream key for encryption if needed
    let stream_key = if config.encrypted {
        Some(generate_stream_key())
    } else {
        None
    };

    // Create stream announcement
    let announcement = StreamAnnouncement::new(
        keypair.identity().clone(),
        config.stream_id.to_string(),
        Codec::Opus,
        config.bitrate_kbps,
        48000, // sample_rate
        audio_48k.channels,
        config.encrypted,
        vouch.clone(),
    );

    // Generate chunks
    let mut chunks = Vec::new();
    let start_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;

    for (i, frame_data) in encoded_frames.iter().enumerate() {
        let timestamp = start_time + (i as u64 * 20_000); // 20ms per frame

        let (data, nonce) = if let Some(ref key) = stream_key {
            // Encrypt the frame
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(key);

            let (encrypted_data, nonce_bytes) = crypto::encrypt(&key_array, frame_data)?;
            (encrypted_data, Some(nonce_bytes))
        } else {
            (frame_data.clone(), None)
        };

        let chunk = if let Some(nonce_bytes) = nonce {
            Chunk::new_encrypted(
                announcement.stream_addr,
                i as u64,
                timestamp,
                Codec::Opus,
                20_000, // 20ms in microseconds
                data,
                nonce_bytes,
            )
        } else {
            Chunk::new(
                announcement.stream_addr,
                i as u64,
                timestamp,
                Codec::Opus,
                20_000, // 20ms in microseconds
                data,
            )
        };

        chunks.push(chunk);
    }

    Ok(BroadcastResult {
        announcement,
        chunks,
        stream_key: stream_key.map(|k| k.to_vec()),
    })
}
/// Result of network broadcast operation
pub struct NetworkBroadcastResult {
    pub announcement: StreamAnnouncement,
    pub chunks_published: usize,
    pub announcement_stored: bool,
}

/// Broadcast to libp2p network
///
/// This function:
/// 1. Processes audio using run_broadcast() 
/// 2. Creates MdrnSwarm
/// 3. Stores StreamAnnouncement in DHT
/// 4. Subscribes to stream topic
/// 5. Publishes chunks via gossipsub (with 20ms pacing)
pub async fn broadcast_to_network(
    keypair: &Keypair,
    vouch: &Vouch,
    config: &BroadcastConfig<'_>,
    relay_addr: Option<String>,
) -> Result<NetworkBroadcastResult> {
    use mdrn_core::transport::{stream_topic, MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE, MdrnBehaviourEvent};
    use std::time::Duration;
    use tokio::time::sleep;
    use futures::StreamExt;
    use libp2p::swarm::SwarmEvent;

    // First, process audio using existing pipeline
    let broadcast_result = run_broadcast(keypair, vouch, config)?;

    // Create swarm with same keypair
    let swarm_config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };
    let mut swarm = MdrnSwarm::new(keypair.clone(), swarm_config)
        .map_err(|e| anyhow::anyhow!("Failed to create swarm: {}", e))?;

    // Start listening for incoming connections
    swarm.listen("/ip4/127.0.0.1/tcp/0".parse()?)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start listening: {}", e))?;

    tracing::info!("Broadcaster listening for connections");

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

    // ── HOTFIX: Wait for Identify before DHT/subscribe (same fix as listen.rs) ──
    // ConnectionEstablished fires before multistream-select negotiates gossipsub.
    // Waiting for identify::Event::Received ensures all protocols are ready.
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
        tracing::warn!("Identify not received — proceeding anyway");
    }
    // ── END HOTFIX ──

    // Now that protocols are ready, proceed with DHT and gossipsub

    // Store announcement in DHT
    let dht_key = format!(
        "{}{}",
        DHT_STREAM_NAMESPACE,
        hex::encode(&broadcast_result.announcement.stream_addr)
    );
    let mut announcement_cbor = Vec::new();
    ciborium::into_writer(&broadcast_result.announcement, &mut announcement_cbor)?;

    swarm
        .dht_put(dht_key.as_bytes().to_vec(), announcement_cbor)
        .map_err(|e| anyhow::anyhow!("Failed to store announcement in DHT: {}", e))?;

    tracing::info!("Stored announcement in DHT: {}", dht_key);

    // Subscribe to stream topic
    let topic = stream_topic(&broadcast_result.announcement.stream_addr);
    swarm
        .subscribe(&topic)
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to topic: {}", e))?;

    tracing::info!("Subscribed to topic: {}", topic);

    // ── HOTFIX: Wait for at least one mesh peer to subscribe before publishing ──
    //
    // Gossipsub requires direct peer-to-peer mesh connections. When the broadcaster
    // and listener both connect to the same relay, they are NOT automatically meshed —
    // gossipsub needs to discover each other and form a direct connection. Publishing
    // immediately after subscribing results in InsufficientPeers errors.
    //
    // Fix: poll swarm events until we see a GossipsubEvent::Subscribed from another
    // peer on our stream topic (meaning they joined the mesh), then publish.
    // 15-second timeout with a fallback to publish anyway (for single-node demos).
    {
        use mdrn_core::transport::MdrnBehaviourEvent;
        use libp2p::gossipsub;

        tracing::info!("Waiting for listener to join gossipsub mesh (up to 15s)...");
        let topic_str = topic.to_string();
        let mesh_wait = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match swarm.inner_mut().select_next_some().await {
                    SwarmEvent::Behaviour(MdrnBehaviourEvent::Gossipsub(
                        gossipsub::Event::Subscribed { peer_id, topic }
                    )) => {
                        tracing::info!("Peer {} joined mesh on topic {}", peer_id, topic);
                        break;
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        tracing::debug!("New connection from {}", peer_id);
                    }
                    _ => {}
                }
            }
        }).await;

        match mesh_wait {
            Ok(_) => tracing::info!("Mesh peer found — starting broadcast"),
            Err(_) => tracing::warn!(
                "No listener joined mesh after 15s on topic {} — broadcasting anyway (no subscribers will receive)",
                topic_str
            ),
        }
    }
    // ── END HOTFIX ──

    // Publish chunks with real-time pacing (20ms between chunks)
    let mut chunks_published = 0;
    for chunk in &broadcast_result.chunks {
        let mut chunk_cbor = Vec::new();
        ciborium::into_writer(chunk, &mut chunk_cbor)?;

        // Publish to connected relay peers
        match swarm.publish(&topic, chunk_cbor) {
            Ok(_) => {
                chunks_published += 1;
                tracing::debug!("Published chunk {}", chunk.seq);
            }
            Err(e) => {
                tracing::warn!("Failed to publish chunk {}: {}", chunk.seq, e);
                // Continue trying to publish remaining chunks
            }
        }

        // Real-time pacing: 20ms between chunks
        sleep(Duration::from_millis(20)).await;
    }

    tracing::info!("Published {} chunks to network", chunks_published);

    // ── HOTFIX: Drain period — keep event loop alive after publishing ──
    //
    // Gossipsub is async: publish() queues messages but delivery happens in the
    // background event loop. If the process exits immediately after the publish
    // loop, all queued messages are dropped before they reach the relay.
    //
    // Fix: continue draining the swarm event loop for 5 seconds after the last
    // chunk is published. This gives gossipsub time to flush its send queue and
    // confirm delivery to all mesh peers.
    tracing::info!("Draining gossipsub queue (5s)...");
    let drain_start = std::time::Instant::now();
    while drain_start.elapsed() < Duration::from_secs(5) {
        tokio::select! {
            _ = swarm.inner_mut().select_next_some() => {}
            _ = sleep(Duration::from_millis(50)) => {}
        }
    }
    tracing::info!("Drain complete");
    // ── END HOTFIX ──

    Ok(NetworkBroadcastResult {
        announcement: broadcast_result.announcement,
        chunks_published,
        announcement_stored: true,
    })
}
