//! MDRN CLI - Command-line broadcaster/listener/relay tool

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod broadcast;
pub mod discover;
mod listen;
pub mod relay;

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

        /// Relay node address to connect to (e.g. /ip4/127.0.0.1/tcp/9000)
        #[arg(short, long)]
        relay: Option<String>,
    },

    /// Listen to a stream
    Listen {
        /// Stream address (hex) or stream ID
        #[arg()]
        stream: String,

        /// Output WAV file path (or audio device)
        #[arg(short, long)]
        output: Option<String>,

        /// Stream key for encrypted streams (hex)
        #[arg(short, long)]
        key: Option<String>,

        /// Listen via libp2p network (vs stdin mode)
        #[arg(short, long)]
        network: bool,

        /// Relay node address to connect to (e.g. /ip4/127.0.0.1/tcp/9000)
        #[arg(short, long)]
        relay: Option<String>,
    },

    /// Run as a relay node
    Relay {
        /// Listen port
        #[arg(short, long, default_value = "9000")]
        port: u16,

        /// Price per minute (0 for free)
        #[arg(long, default_value = "0")]
        price: u64,

        /// Run in daemon mode (disable signal handling for background operation)
        #[arg(short, long)]
        daemon: bool,
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
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        Commands::Broadcast {
            stream_id,
            input,
            bitrate,
            encrypted,
            network,
            relay,
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
                let result = rt.block_on(broadcast_to_network(&keypair, &vouch, &config, relay))?;

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

        Commands::Listen { stream, output, key, network, relay } => {
            use listen::{parse_stream_address, run_listen_stdin, ListenConfig, ParsedAddress};

            tracing::info!(
                stream = %stream,
                output = ?output,
                network = network,
                "Connecting to stream..."
            );

            // Parse stream address
            let stream_addr = match parse_stream_address(&stream)? {
                ParsedAddress::Hex(addr) => addr,
                ParsedAddress::StreamId(_id) => {
                    // For MVP, stream_id lookup requires DHT which requires network mode
                    anyhow::bail!(
                        "Stream ID lookup not implemented yet. Please use hex address.\n\
                         Hint: The stream address is SHA-256(broadcaster_identity || stream_id)"
                    );
                }
            };

            // Parse stream key if provided
            let stream_key = if let Some(key_hex) = key {
                let key_bytes = hex::decode(&key_hex)
                    .map_err(|e| anyhow::anyhow!("Invalid stream key hex: {}", e))?;
                if key_bytes.len() != 32 {
                    anyhow::bail!("Stream key must be 32 bytes (64 hex chars)");
                }
                let mut key_array = [0u8; 32];
                key_array.copy_from_slice(&key_bytes);
                Some(key_array)
            } else {
                None
            };

            let output_path = output.map(PathBuf::from);

            let config = ListenConfig {
                stream_addr,
                output_path: output_path.clone(),
                stream_key,
                network,
            };

            if network {
                // Network mode: subscribe to gossipsub topic
                use listen::run_listen_network;

                let rt = tokio::runtime::Runtime::new()?;
                let result = rt.block_on(run_listen_network(&config, None, relay))?;

                println!("\n=== Listen Complete ===");
                println!("Stream Address: {}", hex::encode(&config.stream_addr));
                println!("Chunks Received: {}", result.chunks_received);
                println!("Chunks Decoded: {}", result.chunks_decoded);
                println!("Duration: {} ms", result.duration_ms);
                if let Some(path) = result.output_path {
                    println!("Output: {}", path.display());
                }
            } else {
                // Stdin mode: read hex-encoded CBOR chunks from stdin
                let result = run_listen_stdin(&config)?;

                println!("\n=== Listen Complete ===");
                println!("Stream Address: {}", hex::encode(&config.stream_addr));
                println!("Chunks Received: {}", result.chunks_received);
                println!("Chunks Decoded: {}", result.chunks_decoded);
                println!("Duration: {} ms", result.duration_ms);
                if let Some(path) = result.output_path {
                    println!("Output: {}", path.display());
                }
            }
        }

        Commands::Relay { port, price, daemon } => {
            tracing::info!(
                port = port,
                price = price,
                "Starting relay node..."
            );

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                if let Err(e) = relay::run_relay(port, price, daemon).await {
                    tracing::error!("Relay error: {}", e);
                    anyhow::bail!("Relay failed: {}", e);
                }
                Ok::<(), anyhow::Error>(())
            })?;
        }

        Commands::Discover { tag, limit } => {
            tracing::info!(
                tag = ?tag,
                limit = limit,
                "Discovering streams..."
            );

            use discover::{run_discover, format_discover_output, DiscoverConfig};

            let config = DiscoverConfig {
                limit,
                tag,
                timeout_secs: 10,
            };

            let rt = tokio::runtime::Runtime::new()?;
            let result = rt.block_on(run_discover(None, &config))?;

            // Print formatted output
            println!("{}", format_discover_output(&result));

            // Print additional info
            if result.total_found > 0 {
                println!("\nTo listen to a stream, use:");
                println!("  mdrn listen <stream-address> --network");
            }
        }

        Commands::Keygen { key_type, output } => {
            tracing::info!(
                key_type = %key_type,
                output = ?output,
                "Generating keypair..."
            );

            // Generate keypair based on key type
            let keypair = match key_type.to_lowercase().as_str() {
                "ed25519" => mdrn_core::identity::Keypair::generate_ed25519()
                    .map_err(|e| anyhow::anyhow!("Failed to generate Ed25519 keypair: {}", e))?,
                "secp256k1" => mdrn_core::identity::Keypair::generate_secp256k1()
                    .map_err(|e| anyhow::anyhow!("Failed to generate secp256k1 keypair: {}", e))?,
                other => anyhow::bail!("Unsupported key type: '{}'. Use 'ed25519' or 'secp256k1'.", other),
            };

            // Determine output path
            let output_path = match output {
                Some(path) => PathBuf::from(path),
                None => {
                    // Default to ~/.mdrn/keypair.cbor
                    let home = std::env::var("HOME")
                        .map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
                    let mdrn_dir = PathBuf::from(home).join(".mdrn");
                    mdrn_dir.join("keypair.cbor")
                }
            };

            // Create parent directory if needed
            if let Some(parent) = output_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| anyhow::anyhow!("Failed to create directory {}: {}", parent.display(), e))?;
                }
            }

            // Serialize to CBOR
            let cbor_data = keypair.to_cbor()
                .map_err(|e| anyhow::anyhow!("Failed to serialize keypair: {}", e))?;

            // Write to file
            std::fs::write(&output_path, &cbor_data)
                .map_err(|e| anyhow::anyhow!("Failed to write keypair to {}: {}", output_path.display(), e))?;

            // Output identity
            let identity_hex = hex::encode(keypair.identity().as_bytes());
            println!("Keypair generated successfully!");
            println!("Key type: {:?}", keypair.key_type());
            println!("Identity: {}", identity_hex);
            println!("Saved to: {}", output_path.display());
        }

        Commands::Vouch {
            subject,
            keypair,
            expires,
        } => {
            tracing::info!(
                subject = %subject,
                keypair = %keypair,
                expires = ?expires,
                "Creating vouch..."
            );

            // Parse subject identity from hex
            let subject_bytes = hex::decode(&subject)
                .map_err(|e| anyhow::anyhow!("Invalid subject hex: {}", e))?;
            let subject_identity = mdrn_core::identity::Identity::from_bytes(&subject_bytes)
                .map_err(|e| anyhow::anyhow!("Invalid subject identity: {}", e))?;

            // Load issuer keypair from file
            let keypair_path = PathBuf::from(&keypair);
            let keypair_data = std::fs::read(&keypair_path)
                .map_err(|e| anyhow::anyhow!("Failed to read keypair file {}: {}", keypair_path.display(), e))?;
            let issuer_keypair = mdrn_core::identity::Keypair::from_cbor(&keypair_data)
                .map_err(|e| anyhow::anyhow!("Failed to parse keypair: {}", e))?;

            // Calculate expiration timestamp if provided
            let expires_at = expires.map(|days| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                now + (days * 24 * 60 * 60)
            });

            // Create the vouch
            let vouch = mdrn_core::identity::Vouch::create(
                subject_identity,
                &issuer_keypair,
                expires_at,
            ).map_err(|e| anyhow::anyhow!("Failed to create vouch: {}", e))?;

            // Verify the vouch before outputting
            vouch.verify()
                .map_err(|e| anyhow::anyhow!("Vouch verification failed: {}", e))?;

            // Serialize to CBOR and write to stdout
            let mut cbor_data = Vec::new();
            ciborium::into_writer(&vouch, &mut cbor_data)
                .map_err(|e| anyhow::anyhow!("Failed to serialize vouch: {}", e))?;

            // Write raw CBOR bytes to stdout
            use std::io::Write;
            std::io::stdout().write_all(&cbor_data)
                .map_err(|e| anyhow::anyhow!("Failed to write vouch: {}", e))?;

            tracing::info!("Vouch created successfully");
        }
    }

    Ok(())
}
