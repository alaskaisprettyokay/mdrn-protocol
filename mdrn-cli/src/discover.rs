//! Discover command implementation
//!
//! Handles:
//! - DHT querying for StreamAnnouncements
//! - Stream metadata parsing and display
//! - Tag filtering and result limiting
//! - Formatted output for CLI display

use anyhow::Result;
use mdrn_core::identity::Keypair;
use mdrn_core::stream::StreamAnnouncement;
use mdrn_core::transport::{MdrnSwarm, TransportConfig, DHT_STREAM_NAMESPACE};

// ============================================================================
// Configuration
// ============================================================================

/// Discovery configuration
#[derive(Debug, Clone)]
pub struct DiscoverConfig {
    /// Maximum number of results to return
    pub limit: usize,
    /// Optional tag filter
    pub tag: Option<String>,
    /// Timeout for network operations in seconds
    pub timeout_secs: u64,
}

impl Default for DiscoverConfig {
    fn default() -> Self {
        Self {
            limit: 10,
            tag: None,
            timeout_secs: 10,
        }
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// A discovered stream with display helpers
#[derive(Debug, Clone)]
pub struct DiscoveredStream {
    /// Original announcement
    pub announcement: StreamAnnouncement,
}

impl DiscoveredStream {
    /// Get stream address as hex string
    pub fn stream_addr_hex(&self) -> String {
        hex::encode(&self.announcement.stream_addr)
    }

    /// Get broadcaster identity as hex string
    pub fn broadcaster_hex(&self) -> String {
        hex::encode(self.announcement.broadcaster.as_bytes())
    }

    /// Get codec name
    pub fn codec_name(&self) -> &'static str {
        match self.announcement.codec {
            mdrn_core::stream::Codec::Opus => "Opus",
            mdrn_core::stream::Codec::Flac => "FLAC",
            mdrn_core::stream::Codec::Codec2 => "Codec2",
        }
    }

    /// Get bitrate display string
    pub fn bitrate_display(&self) -> String {
        format!("{} kbps", self.announcement.bitrate)
    }

    /// Get channels display string
    pub fn channels_display(&self) -> &'static str {
        match self.announcement.channels {
            1 => "Mono",
            2 => "Stereo",
            _ => "Multi",
        }
    }

    /// Get stream ID
    pub fn stream_id(&self) -> &str {
        &self.announcement.stream_id
    }

    /// Get codec
    pub fn codec(&self) -> mdrn_core::stream::Codec {
        self.announcement.codec
    }

    /// Get bitrate
    pub fn bitrate(&self) -> u32 {
        self.announcement.bitrate
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.announcement.sample_rate
    }

    /// Get channels
    pub fn channels(&self) -> u8 {
        self.announcement.channels
    }

    /// Check if encrypted
    pub fn encrypted(&self) -> bool {
        self.announcement.encrypted
    }

    /// Get tags
    pub fn tags(&self) -> &[String] {
        &self.announcement.tags
    }
}

impl From<StreamAnnouncement> for DiscoveredStream {
    fn from(announcement: StreamAnnouncement) -> Self {
        Self { announcement }
    }
}

/// Result of discovery operation
#[derive(Debug, Clone)]
pub struct DiscoverResult {
    /// Discovered streams (after filtering and limiting)
    pub streams: Vec<DiscoveredStream>,
    /// Total streams found in DHT (before filtering)
    pub total_found: usize,
    /// Streams matching filter (before limiting)
    pub filtered_count: usize,
}

// ============================================================================
// Discovery Functions
// ============================================================================

/// Discover streams from local DHT store
///
/// This function scans the local DHT store for StreamAnnouncement records,
/// parses them, applies filters, and returns results.
pub fn discover_streams(swarm: &MdrnSwarm, config: &DiscoverConfig) -> DiscoverResult {
    let mut all_announcements: Vec<StreamAnnouncement> = Vec::new();

    // Iterate over local DHT entries
    for (key, value) in swarm.dht_iter() {
        // Check if key starts with stream namespace
        let key_str = String::from_utf8_lossy(key);
        if !key_str.starts_with(DHT_STREAM_NAMESPACE) {
            continue;
        }

        // Try to deserialize as StreamAnnouncement
        match ciborium::from_reader::<StreamAnnouncement, _>(&value[..]) {
            Ok(announcement) => {
                all_announcements.push(announcement);
            }
            Err(e) => {
                tracing::debug!("Failed to parse announcement from DHT: {}", e);
                // Skip invalid entries
            }
        }
    }

    let total_found = all_announcements.len();

    // Apply tag filter if specified
    let filtered: Vec<StreamAnnouncement> = if let Some(ref tag) = config.tag {
        let tag_lower = tag.to_lowercase();
        all_announcements
            .into_iter()
            .filter(|a| {
                a.tags
                    .iter()
                    .any(|t| t.to_lowercase() == tag_lower)
            })
            .collect()
    } else {
        all_announcements
    };

    let filtered_count = filtered.len();

    // Apply limit
    let limited: Vec<DiscoveredStream> = filtered
        .into_iter()
        .take(config.limit)
        .map(DiscoveredStream::from)
        .collect();

    DiscoverResult {
        streams: limited,
        total_found,
        filtered_count,
    }
}

/// Format discovery results for CLI output
pub fn format_discover_output(result: &DiscoverResult) -> String {
    if result.streams.is_empty() {
        if result.total_found == 0 {
            return "No streams found in the network.".to_string();
        } else {
            return format!(
                "No streams found matching filter (found {} total).",
                result.total_found
            );
        }
    }

    let mut output = String::new();

    // Header
    output.push_str(&format!(
        "Found {} stream(s)",
        result.filtered_count
    ));
    if result.filtered_count < result.total_found {
        output.push_str(&format!(" (of {} total)", result.total_found));
    }
    if result.streams.len() < result.filtered_count {
        output.push_str(&format!(", showing {}", result.streams.len()));
    }
    output.push_str(":\n\n");

    // Table header
    output.push_str(&format!(
        "{:<20} {:<16} {:<6} {:<10} {:<8} {:<10}\n",
        "Stream ID", "Stream Address", "Codec", "Bitrate", "Channels", "Encrypted"
    ));
    output.push_str(&format!(
        "{:-<20} {:-<16} {:-<6} {:-<10} {:-<8} {:-<10}\n",
        "", "", "", "", "", ""
    ));

    // Stream rows
    for stream in &result.streams {
        let addr_short = &stream.stream_addr_hex()[..16]; // First 16 chars
        let encrypted = if stream.encrypted() { "Yes" } else { "No" };

        output.push_str(&format!(
            "{:<20} {:<16} {:<6} {:<10} {:<8} {:<10}\n",
            truncate(&stream.announcement.stream_id, 20),
            format!("{}...", addr_short),
            stream.codec_name(),
            stream.bitrate_display(),
            stream.channels_display(),
            encrypted
        ));

        // Show tags if present
        if !stream.tags().is_empty() {
            output.push_str(&format!(
                "  Tags: {}\n",
                stream.tags().join(", ")
            ));
        }
    }

    output
}

/// Truncate a string with ellipsis if too long
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

// ============================================================================
// Async Entry Points
// ============================================================================

/// Run discovery with new swarm
///
/// Creates a new swarm, optionally connects to bootstrap nodes,
/// and queries for streams.
pub async fn run_discover(
    keypair: Option<Keypair>,
    config: &DiscoverConfig,
) -> Result<DiscoverResult> {
    // Generate or use provided keypair
    let keypair = keypair.unwrap_or_else(|| {
        Keypair::generate_ed25519().expect("keypair generation should succeed")
    });

    // Create swarm
    let swarm_config = TransportConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        bootstrap_nodes: vec![],
        ..TransportConfig::default()
    };

    let swarm = MdrnSwarm::new(keypair, swarm_config)
        .map_err(|e| anyhow::anyhow!("Failed to create swarm: {}", e))?;

    // For local discovery, just scan local DHT
    let result = discover_streams(&swarm, config);

    Ok(result)
}

/// Run discovery with an existing swarm
///
/// Uses the provided swarm's local DHT store.
pub async fn run_discover_with_swarm(
    swarm: MdrnSwarm,
    config: &DiscoverConfig,
) -> Result<DiscoverResult> {
    let result = discover_streams(&swarm, config);
    Ok(result)
}
