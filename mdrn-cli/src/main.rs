//! MDRN CLI - Command-line broadcaster/listener/relay tool

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
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
        } => {
            use broadcast::{load_keypair_default, load_vouch_default, run_broadcast, BroadcastConfig};

            tracing::info!(
                stream_id = %stream_id,
                input = ?input,
                bitrate = bitrate,
                encrypted = encrypted,
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

        Commands::Listen { stream, output } => {
            tracing::info!(
                stream = %stream,
                output = ?output,
                "Connecting to stream..."
            );
            // TODO: Implement listen
            tracing::warn!("Listen not yet implemented");
        }

        Commands::Relay { port, price } => {
            tracing::info!(
                port = port,
                price = price,
                "Starting relay node..."
            );
            // TODO: Implement relay
            tracing::warn!("Relay not yet implemented");
        }

        Commands::Discover { tag, limit } => {
            tracing::info!(
                tag = ?tag,
                limit = limit,
                "Discovering streams..."
            );
            // TODO: Implement discovery
            tracing::warn!("Discovery not yet implemented");
        }

        Commands::Keygen { key_type, output } => {
            tracing::info!(
                key_type = %key_type,
                output = ?output,
                "Generating keypair..."
            );
            // TODO: Implement keygen
            tracing::warn!("Keygen not yet implemented");
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
            // TODO: Implement vouch creation
            tracing::warn!("Vouch creation not yet implemented");
        }
    }

    Ok(())
}
