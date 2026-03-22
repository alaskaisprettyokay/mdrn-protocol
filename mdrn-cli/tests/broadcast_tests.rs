//! Unit tests for broadcast CLI functionality
//!
//! Tests cover:
//! - Keypair loading from file
//! - Audio file reading (via symphonia)
//! - Opus encoding
//! - Chunk creation
//! - Optional encryption

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a temp keypair file
fn create_test_keypair(dir: &TempDir) -> PathBuf {
    use mdrn_core::identity::Keypair;

    let keypair = Keypair::generate_ed25519().unwrap();
    let cbor = keypair.to_cbor().unwrap();

    let path = dir.path().join("test_keypair.cbor");
    fs::write(&path, &cbor).unwrap();
    path
}

/// Helper to create a simple WAV file for testing
fn create_test_wav(dir: &TempDir, duration_ms: u32, sample_rate: u32, channels: u16) -> PathBuf {
    let path = dir.path().join("test_audio.wav");
    let samples_per_channel = (sample_rate as u64 * duration_ms as u64 / 1000) as u32;
    let num_samples = samples_per_channel * channels as u32;

    // Create a simple sine wave
    let mut pcm_data: Vec<i16> = Vec::with_capacity(num_samples as usize);
    for i in 0..samples_per_channel {
        // 440 Hz sine wave
        let t = i as f32 / sample_rate as f32;
        let sample = (f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 16000.0) as i16;
        for _ in 0..channels {
            pcm_data.push(sample);
        }
    }

    // Write WAV file
    let mut file = fs::File::create(&path).unwrap();

    // WAV header
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = num_samples * 2; // 16-bit samples = 2 bytes each
    let file_size = 36 + data_size;

    // RIFF header
    file.write_all(b"RIFF").unwrap();
    file.write_all(&file_size.to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    // fmt subchunk
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap(); // subchunk size
    file.write_all(&1u16.to_le_bytes()).unwrap(); // audio format (PCM)
    file.write_all(&channels.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&bits_per_sample.to_le_bytes()).unwrap();

    // data subchunk
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();

    // Write PCM samples
    for sample in pcm_data {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }

    path
}

// ============================================================================
// PHASE 1: Keypair Loading Tests
// ============================================================================

mod keypair_loading_tests {
    use super::*;
    use mdrn_core::identity::{Keypair, KeyType};

    #[test]
    fn test_load_keypair_ed25519_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let keypair_path = create_test_keypair(&temp_dir);

        // Load keypair from file
        let loaded = load_keypair_from_file(&keypair_path).unwrap();
        assert_eq!(loaded.key_type(), KeyType::Ed25519);
    }

    #[test]
    fn test_load_keypair_file_not_found() {
        let result = load_keypair_from_file(&PathBuf::from("/nonexistent/path/keypair.cbor"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_keypair_invalid_cbor() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("invalid.cbor");
        fs::write(&path, b"not valid cbor").unwrap();

        let result = load_keypair_from_file(&path);
        assert!(result.is_err());
    }

    // Helper function to load keypair - this is what we're testing
    fn load_keypair_from_file(path: &PathBuf) -> anyhow::Result<Keypair> {
        let bytes = fs::read(path)?;
        let keypair = Keypair::from_cbor(&bytes)?;
        Ok(keypair)
    }
}

// ============================================================================
// PHASE 2: Audio Input Tests
// ============================================================================

mod audio_input_tests {
    use super::*;

    #[test]
    fn test_read_wav_file_mono_48khz() {
        let temp_dir = TempDir::new().unwrap();
        let wav_path = create_test_wav(&temp_dir, 1000, 48000, 1); // 1 second mono 48kHz

        let audio_data = read_audio_file(&wav_path).unwrap();

        assert_eq!(audio_data.sample_rate, 48000);
        assert_eq!(audio_data.channels, 1);
        assert!(!audio_data.samples.is_empty());
        // 1 second at 48kHz = 48000 samples
        assert!(audio_data.samples.len() >= 48000, "Expected at least 48000 samples, got {}", audio_data.samples.len());
    }

    #[test]
    fn test_read_wav_file_stereo_48khz() {
        let temp_dir = TempDir::new().unwrap();
        let wav_path = create_test_wav(&temp_dir, 1000, 48000, 2); // 1 second stereo 48kHz

        let audio_data = read_audio_file(&wav_path).unwrap();

        assert_eq!(audio_data.sample_rate, 48000);
        assert_eq!(audio_data.channels, 2);
        // Stereo: samples are interleaved, so total samples = 48000 * 2
        assert!(audio_data.samples.len() >= 96000);
    }

    #[test]
    fn test_audio_file_not_found() {
        let result = read_audio_file(&PathBuf::from("/nonexistent/audio.wav"));
        assert!(result.is_err());
    }

    /// Decoded audio data
    #[derive(Debug)]
    struct AudioData {
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u8,
    }

    /// Read and decode an audio file
    fn read_audio_file(path: &PathBuf) -> anyhow::Result<AudioData> {
        use symphonia::core::audio::SampleBuffer;
        use symphonia::core::codecs::DecoderOptions;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let file = std::fs::File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let hint = Hint::new();
        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)?;

        let mut format = probed.format;
        let track = format.default_track().ok_or_else(|| anyhow::anyhow!("No audio track"))?;

        let sample_rate = track.codec_params.sample_rate.ok_or_else(|| anyhow::anyhow!("No sample rate"))?;
        let channels = track.codec_params.channels.map(|c| c.count() as u8).unwrap_or(1);

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &decoder_opts)?;

        let mut samples = Vec::new();

        loop {
            match format.next_packet() {
                Ok(packet) => {
                    let decoded = decoder.decode(&packet)?;
                    let spec = *decoded.spec();
                    let duration = decoded.capacity() as u64;

                    let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
                    sample_buf.copy_interleaved_ref(decoded);
                    samples.extend_from_slice(sample_buf.samples());
                }
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(AudioData {
            samples,
            sample_rate,
            channels,
        })
    }
}

// ============================================================================
// PHASE 3: Opus Encoding Tests
// ============================================================================

mod opus_encoding_tests {
    use super::*;

    #[test]
    fn test_opus_encode_mono_48khz() {
        // 20ms of samples at 48kHz mono = 960 samples
        let pcm: Vec<f32> = (0..960).map(|i| (i as f32 * 0.01).sin()).collect();

        let encoded = encode_opus_frame(&pcm, 48000, 1, 128).unwrap();

        assert!(!encoded.is_empty());
        // Opus compressed should be smaller than raw PCM
        assert!(encoded.len() < pcm.len() * 4);
    }

    #[test]
    fn test_opus_encode_stereo_48khz() {
        // 20ms stereo = 960 samples * 2 channels = 1920 samples
        let pcm: Vec<f32> = (0..1920).map(|i| (i as f32 * 0.01).sin()).collect();

        let encoded = encode_opus_frame(&pcm, 48000, 2, 128).unwrap();

        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_opus_encode_20ms_frame() {
        // 20ms at 48kHz mono = 960 samples
        let pcm: Vec<f32> = vec![0.0; 960];

        let encoded = encode_opus_frame(&pcm, 48000, 1, 128).unwrap();

        assert!(!encoded.is_empty());
    }

    /// Encode a single Opus frame
    fn encode_opus_frame(pcm: &[f32], sample_rate: u32, channels: u8, bitrate_kbps: u32) -> anyhow::Result<Vec<u8>> {
        use opus::{Application, Channels, Encoder};

        let opus_channels = if channels == 1 { Channels::Mono } else { Channels::Stereo };
        let mut encoder = Encoder::new(sample_rate, opus_channels, Application::Audio)?;
        encoder.set_bitrate(opus::Bitrate::Bits(bitrate_kbps as i32 * 1000))?;

        // Convert f32 to i16 for Opus
        let pcm_i16: Vec<i16> = pcm.iter().map(|&s| (s * 32767.0) as i16).collect();

        // Output buffer - Opus max frame size is ~1276 bytes
        let mut output = vec![0u8; 4000];
        let len = encoder.encode(&pcm_i16, &mut output)?;

        output.truncate(len);
        Ok(output)
    }
}

// ============================================================================
// PHASE 4: Chunk Creation Tests
// ============================================================================

mod chunk_creation_tests {
    use super::*;
    use mdrn_core::stream::{Chunk, ChunkFlags, Codec};

    #[test]
    fn test_chunk_creation_unencrypted() {
        let stream_addr = [0xAB; 32];
        let opus_data = vec![0x00, 0x01, 0x02]; // Fake Opus frame

        let chunk = Chunk::new(
            stream_addr,
            0,      // seq
            0,      // timestamp
            Codec::Opus,
            20000,  // duration_us (20ms)
            opus_data.clone(),
        );

        assert_eq!(chunk.stream_addr, stream_addr);
        assert_eq!(chunk.seq, 0);
        assert_eq!(chunk.codec, Codec::Opus);
        assert_eq!(chunk.duration_us, 20000);
        assert_eq!(chunk.data, opus_data);
        assert!(!chunk.is_encrypted());
    }

    #[test]
    fn test_chunk_creation_encrypted() {
        let stream_addr = [0xAB; 32];
        let opus_data = vec![0x00, 0x01, 0x02];
        let nonce = [0x12; 12];

        let chunk = Chunk::new_encrypted(
            stream_addr,
            0,
            0,
            Codec::Opus,
            20000,
            opus_data,
            nonce,
        );

        assert!(chunk.is_encrypted());
        assert_eq!(chunk.nonce, Some(nonce));
    }

    #[test]
    fn test_chunk_sequence_increments() {
        let stream_addr = [0xAB; 32];

        let chunk0 = Chunk::new(stream_addr, 0, 0, Codec::Opus, 20000, vec![0]);
        let chunk1 = Chunk::new(stream_addr, 1, 20000, Codec::Opus, 20000, vec![0]);
        let chunk2 = Chunk::new(stream_addr, 2, 40000, Codec::Opus, 20000, vec![0]);

        assert_eq!(chunk0.seq, 0);
        assert_eq!(chunk1.seq, 1);
        assert_eq!(chunk2.seq, 2);
    }

    #[test]
    fn test_chunk_cbor_roundtrip() {
        let stream_addr = [0xAB; 32];
        let opus_data = vec![0x00, 0x01, 0x02, 0x03];

        let chunk = Chunk::new(
            stream_addr,
            42,
            84000,
            Codec::Opus,
            20000,
            opus_data,
        );

        // Serialize to CBOR
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&chunk, &mut cbor_bytes).unwrap();

        // Deserialize back
        let restored: Chunk = ciborium::from_reader(&cbor_bytes[..]).unwrap();

        assert_eq!(restored.stream_addr, chunk.stream_addr);
        assert_eq!(restored.seq, chunk.seq);
        assert_eq!(restored.timestamp, chunk.timestamp);
        assert_eq!(restored.duration_us, chunk.duration_us);
        assert_eq!(restored.data, chunk.data);
    }
}

// ============================================================================
// PHASE 5: Stream Announcement Tests
// ============================================================================

mod announcement_tests {
    use super::*;
    use mdrn_core::identity::{Keypair, Vouch};
    use mdrn_core::stream::{Codec, StreamAnnouncement};

    #[test]
    fn test_stream_announcement_creation() {
        let broadcaster = Keypair::generate_ed25519().unwrap();
        let issuer = Keypair::generate_ed25519().unwrap();
        let vouch = Vouch::create(broadcaster.identity().clone(), &issuer, None).unwrap();

        let announcement = StreamAnnouncement::new(
            broadcaster.identity().clone(),
            "test-stream".to_string(),
            Codec::Opus,
            128,   // bitrate kbps
            48000, // sample rate
            2,     // channels (stereo)
            false, // not encrypted
            vouch,
        );

        assert_eq!(announcement.stream_id, "test-stream");
        assert_eq!(announcement.codec, Codec::Opus);
        assert_eq!(announcement.bitrate, 128);
        assert_eq!(announcement.sample_rate, 48000);
        assert_eq!(announcement.channels, 2);
        assert!(!announcement.encrypted);
    }

    #[test]
    fn test_stream_addr_computation() {
        let broadcaster = Keypair::generate_ed25519().unwrap();

        let stream_addr = StreamAnnouncement::compute_stream_addr(
            broadcaster.identity(),
            "my-stream"
        );

        // Stream addr should be 32 bytes (SHA-256)
        assert_eq!(stream_addr.len(), 32);

        // Same inputs should produce same output
        let stream_addr2 = StreamAnnouncement::compute_stream_addr(
            broadcaster.identity(),
            "my-stream"
        );
        assert_eq!(stream_addr, stream_addr2);
    }
}

// ============================================================================
// PHASE 6: Encryption Tests
// ============================================================================

mod encryption_tests {
    use super::*;
    use mdrn_core::crypto::{StreamCipher, generate_stream_key};

    #[test]
    fn test_encrypt_opus_frame() {
        let key = generate_stream_key();
        let cipher = StreamCipher::new(&key);

        let opus_data = vec![0x00, 0x01, 0x02, 0x03, 0x04];
        let (ciphertext, nonce) = cipher.encrypt(&opus_data).unwrap();

        // Ciphertext should include auth tag (16 bytes)
        assert!(ciphertext.len() > opus_data.len());

        // Should be decryptable
        let decrypted = cipher.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, opus_data);
    }
}

// ============================================================================
// PHASE 7: End-to-End Broadcast Pipeline Tests
// ============================================================================

mod broadcast_pipeline_tests {
    use super::*;
    use mdrn_core::identity::{Keypair, Vouch};
    use mdrn_core::stream::{Chunk, Codec, StreamAnnouncement};
    use mdrn_core::crypto::{StreamCipher, generate_stream_key};

    #[test]
    fn test_broadcast_pipeline_unencrypted() {
        let temp_dir = TempDir::new().unwrap();

        // 1. Create keypair
        let keypair = Keypair::generate_ed25519().unwrap();

        // 2. Create vouch
        let issuer = Keypair::generate_ed25519().unwrap();
        let vouch = Vouch::create(keypair.identity().clone(), &issuer, None).unwrap();

        // 3. Create test audio file (100ms, enough for 5 20ms chunks)
        let wav_path = create_test_wav(&temp_dir, 100, 48000, 1);

        // 4. Run pipeline
        let result = run_broadcast_pipeline(
            &keypair,
            &vouch,
            &wav_path,
            "test-stream",
            128,
            false, // not encrypted
        ).unwrap();

        // Should produce at least 4 chunks (100ms / 20ms = 5, minus some for rounding)
        assert!(result.chunks.len() >= 4, "Expected at least 4 chunks, got {}", result.chunks.len());

        // All chunks should have correct stream_addr
        let expected_addr = StreamAnnouncement::compute_stream_addr(keypair.identity(), "test-stream");
        for chunk in &result.chunks {
            assert_eq!(chunk.stream_addr, expected_addr);
            assert!(!chunk.is_encrypted());
        }

        // Sequences should be monotonically increasing
        for (i, chunk) in result.chunks.iter().enumerate() {
            assert_eq!(chunk.seq, i as u64);
        }
    }

    #[test]
    fn test_broadcast_pipeline_encrypted() {
        let temp_dir = TempDir::new().unwrap();

        let keypair = Keypair::generate_ed25519().unwrap();
        let issuer = Keypair::generate_ed25519().unwrap();
        let vouch = Vouch::create(keypair.identity().clone(), &issuer, None).unwrap();

        let wav_path = create_test_wav(&temp_dir, 100, 48000, 1);

        let result = run_broadcast_pipeline(
            &keypair,
            &vouch,
            &wav_path,
            "encrypted-stream",
            128,
            true, // encrypted
        ).unwrap();

        // All chunks should be encrypted
        for chunk in &result.chunks {
            assert!(chunk.is_encrypted());
            assert!(chunk.nonce.is_some());
        }
    }

    /// Broadcast pipeline result
    #[derive(Debug)]
    struct BroadcastResult {
        announcement: StreamAnnouncement,
        chunks: Vec<Chunk>,
    }

    /// Run the broadcast pipeline
    fn run_broadcast_pipeline(
        keypair: &Keypair,
        vouch: &Vouch,
        audio_path: &std::path::Path,
        stream_id: &str,
        bitrate_kbps: u32,
        encrypted: bool,
    ) -> anyhow::Result<BroadcastResult> {
        use symphonia::core::audio::SampleBuffer;
        use symphonia::core::codecs::DecoderOptions;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;
        use opus::{Application, Channels, Encoder};

        // 1. Create stream announcement
        let file = std::fs::File::open(audio_path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let probed = symphonia::default::get_probe()
            .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())?;

        let mut format = probed.format;
        let track = format.default_track().ok_or_else(|| anyhow::anyhow!("No track"))?;

        let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);
        let channels = track.codec_params.channels.map(|c| c.count() as u8).unwrap_or(1);

        let announcement = StreamAnnouncement::new(
            keypair.identity().clone(),
            stream_id.to_string(),
            Codec::Opus,
            bitrate_kbps,
            sample_rate,
            channels,
            encrypted,
            vouch.clone(),
        );

        // 2. Decode audio
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;

        let mut all_samples: Vec<f32> = Vec::new();

        loop {
            match format.next_packet() {
                Ok(packet) => {
                    let decoded = decoder.decode(&packet)?;
                    let spec = *decoded.spec();
                    let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    sample_buf.copy_interleaved_ref(decoded);
                    all_samples.extend_from_slice(sample_buf.samples());
                }
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }

        // 3. Resample to 48kHz if needed (Opus requires 48kHz)
        // For simplicity, assume audio is already 48kHz in tests

        // 4. Create Opus encoder
        let opus_channels = if channels == 1 { Channels::Mono } else { Channels::Stereo };
        let mut opus_encoder = Encoder::new(48000, opus_channels, Application::Audio)?;
        opus_encoder.set_bitrate(opus::Bitrate::Bits(bitrate_kbps as i32 * 1000))?;

        // 5. Create cipher if encrypted
        let cipher = if encrypted {
            Some(StreamCipher::new(&generate_stream_key()))
        } else {
            None
        };

        // 6. Chunk audio into 20ms frames
        let samples_per_frame = 960 * channels as usize; // 20ms at 48kHz
        let mut chunks = Vec::new();
        let mut seq = 0u64;
        let mut timestamp = 0u64;

        for frame in all_samples.chunks(samples_per_frame) {
            if frame.len() < samples_per_frame {
                break; // Skip incomplete final frame
            }

            // Convert to i16
            let pcm_i16: Vec<i16> = frame.iter().map(|&s| (s * 32767.0) as i16).collect();

            // Encode
            let mut output = vec![0u8; 4000];
            let len = opus_encoder.encode(&pcm_i16, &mut output)?;
            output.truncate(len);

            // Optionally encrypt
            let (data, nonce) = if let Some(ref c) = cipher {
                let (ciphertext, n) = c.encrypt(&output)?;
                (ciphertext, Some(n))
            } else {
                (output, None)
            };

            // Create chunk
            let chunk = if let Some(n) = nonce {
                Chunk::new_encrypted(
                    announcement.stream_addr,
                    seq,
                    timestamp,
                    Codec::Opus,
                    20000, // 20ms
                    data,
                    n,
                )
            } else {
                Chunk::new(
                    announcement.stream_addr,
                    seq,
                    timestamp,
                    Codec::Opus,
                    20000,
                    data,
                )
            };

            chunks.push(chunk);
            seq += 1;
            timestamp += 20000; // 20ms in microseconds
        }

        Ok(BroadcastResult {
            announcement,
            chunks,
        })
    }
}
