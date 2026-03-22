//! MDRN CLI - Command-line broadcaster/listener/relay tool

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use tracing_subscriber::EnvFilter;

mod broadcast;

#[derive(Parser)]
#[command(name = "mdrn")]
#[command(about = "MDRN - Massively Distributed Radio Network CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start broadcasting a stream
    Broadcast {
        /// Stream identifier
        #[arg(short, long)]
        stream_id: String,

        /// Audio input device or file
        #[arg(short, long)]
        input: Option<String>,

        /// Bitrate in kbps
        #[arg(short, long, default_value = "128")]
        bitrate: u32,

        /// Encrypt the stream
        #[arg(short, long)]
        encrypted: bool,

        /// Broadcast to libp2p network (vs stdout mode)
        #[arg(short, long)]
        network: bool,
    },

    /// Listen to a stream
    Listen {
        /// Stream address (hex) or stream ID
        #[arg()]
        stream: String,

        /// Audio output device
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Run as a relay node
    Relay {
        /// Listen port
        #[arg(short, long, default_value = "9000")]
        port: u16,

        /// Price per minute (0 for free)
        #[arg(long, default_value = "0")]
        price: u64,
    },

    /// Discover available streams
    Discover {
        /// Filter by tag
        #[arg(short, long)]
        tag: Option<String>,

        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Generate a new identity keypair
    Keygen {
        /// Key type (ed25519 or secp256k1)
        #[arg(short, long, default_value = "ed25519")]
        key_type: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Create a vouch for another broadcaster
    Vouch {
        /// Subject's public key (hex)
        #[arg()]
        subject: String,

        /// Your keypair file
        #[arg(short, long)]
        keypair: String,

        /// Expiration in days (optional)
        #[arg(short, long)]
        expires: Option<u64>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.command {
        Commands::Broadcast {
            stream_id,
            input,
            bitrate,
            encrypted,
            network,
        } => {
            use broadcast::{load_keypair_default, load_vouch_default, run_broadcast, BroadcastConfig};

            tracing::info!(
                stream_id = %stream_id,
                input = ?input,
                bitrate = bitrate,
                encrypted = encrypted,
                network = network,
                "Starting broadcast..."
            );

            // Require input file for now (live capture not yet implemented)
            let input_path = input
                .ok_or_else(|| anyhow::anyhow!("--input <FILE> is required (live capture not yet implemented)"))?;
            let input_path = PathBuf::from(&input_path);

            if !input_path.exists() {
                anyhow::bail!("Audio file not found: {}", input_path.display());
            }

            // Load keypair from default location or env var
            let keypair = load_keypair_default()
                .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}. Generate one with: mdrn keygen -o ~/.mdrn/keypair.cbor", e))?;

            tracing::info!(
                identity = %hex::encode(keypair.identity().as_bytes()),
                "Loaded broadcaster identity"
            );

            // Load vouch from default location or env var
            let vouch = load_vouch_default()
                .map_err(|e| anyhow::anyhow!("Failed to load vouch: {}. Obtain a vouch from an existing broadcaster.", e))?;

            tracing::info!("Loaded vouch credential");

            // Run broadcast pipeline
            let config = BroadcastConfig {
                stream_id: &stream_id,
                input_path: &input_path,
                bitrate_kbps: bitrate,
                encrypted,
            };

            if network {
                // Network mode: broadcast to libp2p network
                use broadcast::broadcast_to_network;

                let rt = tokio::runtime::Runtime::new()?;
                let result = rt.block_on(broadcast_to_network(&keypair, &vouch, &config))?;

                println!("\n=== Network Broadcast Complete ===");
                println!("Stream ID: {}", stream_id);
                println!("Stream Address: {}", hex::encode(&result.announcement.stream_addr));
                println!("Broadcaster: {}", hex::encode(keypair.identity().as_bytes()));
                println!("Chunks Published: {}", result.chunks_published);
                println!("Announcement Stored: {}", result.announcement_stored);
            } else {
                // Stdout mode: just output results (original behavior)
                let result = run_broadcast(&keypair, &vouch, &config)?;

                // Output results
                println!("\n=== Broadcast Complete ===");
                println!("Stream ID: {}", stream_id);
                println!("Stream Address: {}", hex::encode(&result.announcement.stream_addr));
                println!("Broadcaster: {}", hex::encode(keypair.identity().as_bytes()));
                println!("Codec: {:?}", result.announcement.codec);
                println!("Bitrate: {} kbps", result.announcement.bitrate);
                println!("Sample Rate: {} Hz", result.announcement.sample_rate);
                println!("Channels: {}", result.announcement.channels);
                println!("Encrypted: {}", result.announcement.encrypted);
                println!("Chunks: {}", result.chunks.len());
                println!("Duration: {} ms", result.chunks.len() * 20);

                if let Some(key) = &result.stream_key {
                    println!("Stream Key: {}", hex::encode(key));
                }

                // Output first chunk info
                if let Some(first) = result.chunks.first() {
                    println!("\nFirst chunk:");
                    println!("  Seq: {}", first.seq);
                    println!("  Timestamp: {} us", first.timestamp);
                    println!("  Data size: {} bytes", first.data.len());
                    println!("  Keyframe: {}", first.is_keyframe());
                }

                // Output last chunk info
                if let Some(last) = result.chunks.last() {
                    println!("\nLast chunk:");
                    println!("  Seq: {}", last.seq);
                    println!("  Timestamp: {} us", last.timestamp);
                    println!("  Data size: {} bytes", last.data.len());
                }
            }
        }

        Commands::Listen { stream, output } => {
            tracing::info!(stream = %stream, output = ?output, "Connecting to stream...");
            use mdrn_core::identity::Keypair;
            use mdrn_core::transport::{stream_topic, MdrnSwarm, TransportConfig};
            use mdrn_core::stream::Chunk;

            let keypair = broadcast::load_keypair_default()
                .unwrap_or_else(|_| Keypair::generate_ed25519().expect("keygen failed"));

            let swarm_config = TransportConfig {
                listen_addrs: vec!["/ip4/0.0.0.0/tcp/0".to_string()],
                bootstrap_nodes: vec![],
                ..TransportConfig::default()
            };

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async move {
                let mut swarm = MdrnSwarm::new(keypair, swarm_config)
                    .map_err(|e| anyhow::anyhow!("Swarm error: {}", e))?;

                // Parse stream address from hex
                let stream_bytes = hex::decode(&stream)
                    .map_err(|_| anyhow::anyhow!("Invalid stream address (expected hex)"))?;
                if stream_bytes.len() != 32 {
                    anyhow::bail!("Stream address must be 32 bytes (64 hex chars)");
                }
                let mut addr = [0u8; 32];
                addr.copy_from_slice(&stream_bytes);
                let topic = stream_topic(&addr);
                swarm.subscribe(&topic).map_err(|e| anyhow::anyhow!("Subscribe error: {}", e))?;
                tracing::info!("Subscribed to stream {}", stream);
                tracing::info!("Waiting for audio chunks... (pipe to: ffplay -f f32le -ar 48000 -ac 1 -)");

                let output_path = output.map(PathBuf::from);
                let use_stdout = output_path.is_none();

                loop {
                    match swarm.inner_mut().select_next_some().await {
                        libp2p::swarm::SwarmEvent::Behaviour(
                            mdrn_core::transport::MdrnBehaviourEvent::Gossipsub(
                                libp2p::gossipsub::Event::Message { message, .. }
                            )
                        ) => {
                            match ciborium::from_reader::<Chunk, _>(&message.data[..]) {
                                Ok(chunk) => {
                                    tracing::debug!("Received chunk seq={} size={}", chunk.seq, chunk.data.len());
                                    if use_stdout {
                                        use std::io::Write;
                                        std::io::stdout().write_all(&chunk.data)?;
                                    }
                                }
                                Err(e) => tracing::warn!("Failed to decode chunk: {}", e),
                            }
                        }
                        libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                            tracing::info!("Listener addr: {}", address);
                        }
                        _ => {}
                    }
                }
                #[allow(unreachable_code)]
                Ok::<(), anyhow::Error>(())
            })?;
        }

        Commands::Relay { port, price } => {
            tracing::info!(port = port, price = price, "Starting relay node...");
            use mdrn_core::identity::Keypair;
            use mdrn_core::transport::{MdrnSwarm, TransportConfig};

            let keypair = Keypair::generate_ed25519()
                .map_err(|e| anyhow::anyhow!("Keygen failed: {}", e))?;

            let swarm_config = TransportConfig {
                listen_addrs: vec![
                    format!("/ip4/0.0.0.0/tcp/{}", port),
                    format!("/ip4/0.0.0.0/udp/{}/quic-v1", port),
                ],
                bootstrap_nodes: vec![],
                ..TransportConfig::default()
            };

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async move {
                let mut swarm = MdrnSwarm::new(keypair, swarm_config)
                    .map_err(|e| anyhow::anyhow!("Swarm error: {}", e))?;

                // Subscribe to mdrn/streams topic to relay all stream traffic
                let topic = libp2p::gossipsub::IdentTopic::new("mdrn/streams");
                swarm.subscribe(&topic).map_err(|e| anyhow::anyhow!("{}", e))?;

                tracing::info!("Relay running. Waiting for listen addresses...");

                // Wait for listen addrs, then print them
                let mut printed = false;
                loop {
                    tokio::select! {
                        event = swarm.inner_mut().select_next_some() => {
                            match event {
                                libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                                    let peer_id = swarm.local_peer_id();
                                    println!("Relay listening: {}/p2p/{}", address, peer_id);
                                    if !printed {
                                        println!("\nDial this address from broadcaster/listener:");
                                        println!("  {}/p2p/{}", address, peer_id);
                                        printed = true;
                                    }
                                }
                                libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                                    tracing::info!("Peer connected: {}", peer_id);
                                }
                                libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                                    tracing::info!("Peer disconnected: {} ({:?})", peer_id, cause);
                                }
                                libp2p::swarm::SwarmEvent::Behaviour(
                                    mdrn_core::transport::MdrnBehaviourEvent::Gossipsub(
                                        libp2p::gossipsub::Event::Message { propagation_source, message, .. }
                                    )
                                ) => {
                                    tracing::debug!("Relaying {} bytes from {}", message.data.len(), propagation_source);
                                }
                                _ => {}
                            }
                        }
                        _ = tokio::signal::ctrl_c() => {
                            tracing::info!("Relay shutting down...");
                            break;
                        }
                    }
                }
                Ok::<(), anyhow::Error>(())
            })?;
        }

        Commands::Discover { tag, limit } => {
            tracing::info!(tag = ?tag, limit = limit, "Discovering streams...");
            use mdrn_core::identity::Keypair;
            use mdrn_core::transport::{MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE};
            use mdrn_core::stream::StreamAnnouncement;

            let keypair = Keypair::generate_ed25519()
                .map_err(|e| anyhow::anyhow!("Keygen failed: {}", e))?;
            let swarm_config = TransportConfig::default();

            let mut swarm = MdrnSwarm::new(keypair, swarm_config)
                .map_err(|e| anyhow::anyhow!("Swarm error: {}", e))?;

            // Scan DHT store for stream announcements
            let prefix = DHT_STREAM_NAMESPACE.as_bytes();
            let mut found = 0;
            for (key, value) in swarm.dht_iter() {
                if key.starts_with(prefix) {
                    if let Ok(ann) = ciborium::from_reader::<StreamAnnouncement, _>(&value[..]) {
                        if let Some(ref t) = tag {
                            if !ann.tags.iter().any(|tag| tag.contains(t.as_str())) {
                                continue;
                            }
                        }
                        println!("Stream: {}", hex::encode(&ann.stream_addr));
                        println!("  Broadcaster: {}", hex::encode(ann.broadcaster.as_bytes()));
                        println!("  Codec: {:?} @ {}kbps", ann.codec, ann.bitrate);
                        println!("  Encrypted: {}", ann.encrypted);
                        println!();
                        found += 1;
                        if found >= limit { break; }
                    }
                }
            }
            if found == 0 {
                println!("No streams found. Make sure a broadcaster is running.");
            }
        }

        Commands::Keygen { key_type, output } => {
            tracing::info!(key_type = %key_type, output = ?output, "Generating keypair...");
            use mdrn_core::identity::{Keypair, Vouch};
            use std::path::PathBuf;

            let keypair = match key_type.as_str() {
                "secp256k1" => Keypair::generate_secp256k1(),
                _ => Keypair::generate_ed25519(),
            }.map_err(|e| anyhow::anyhow!("Key generation failed: {}", e))?;

            let cbor = keypair.to_cbor()
                .map_err(|e| anyhow::anyhow!("Serialization failed: {}", e))?;

            let out_path = output.map(PathBuf::from).unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(home).join(".mdrn").join("keypair.cbor")
            });

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, &cbor)?;

            // Auto-generate genesis self-vouch
            let vouch_path = out_path.with_extension("vouch.cbor");
            let self_vouch = Vouch::create(keypair.identity().clone(), &keypair, None)
                .map_err(|e| anyhow::anyhow!("Self-vouch failed: {}", e))?;
            let mut vouch_bytes = Vec::new();
            ciborium::into_writer(&self_vouch, &mut vouch_bytes)
                .map_err(|e| anyhow::anyhow!("Vouch serialization: {}", e))?;
            std::fs::write(&vouch_path, &vouch_bytes)?;

            println!("Identity: {}", hex::encode(keypair.identity().as_bytes()));
            println!("Saved to: {}", out_path.display());
            println!("Genesis vouch: {}", vouch_path.display());
            println!("\nHint: MDRN_KEYPAIR={} MDRN_VOUCH={} mdrn broadcast ...",
                out_path.display(), vouch_path.display());
        }

        Commands::Vouch { subject, keypair, expires } => {
            tracing::info!(subject = %subject, keypair = %keypair, expires = ?expires, "Creating vouch...");
            use mdrn_core::identity::{Identity, Keypair, Vouch};

            let kp_bytes = std::fs::read(&keypair)?;
            let kp = Keypair::from_cbor(&kp_bytes)
                .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))?;

            let subject_bytes = hex::decode(&subject)
                .map_err(|_| anyhow::anyhow!("Subject must be hex-encoded identity bytes"))?;
            let subject_id = Identity::from_bytes(&subject_bytes)
                .map_err(|e| anyhow::anyhow!("Invalid subject identity: {}", e))?;

            let expires_at = expires.map(|days| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                now + days * 86400
            });

            let vouch = Vouch::create(subject_id, &kp, expires_at)
                .map_err(|e| anyhow::anyhow!("Failed to create vouch: {}", e))?;

            let out_path = format!("{}.vouch.cbor", &subject[..16]);
            let mut vouch_bytes = Vec::new();
            ciborium::into_writer(&vouch, &mut vouch_bytes)
                .map_err(|e| anyhow::anyhow!("Serialization failed: {}", e))?;
            std::fs::write(&out_path, &vouch_bytes)?;

            println!("Vouch created for: {}", subject);
            println!("Saved to: {}", out_path);
        }
    }

    Ok(())
}
