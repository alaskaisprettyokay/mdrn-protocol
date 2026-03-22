# MDRN Studio - Future Desktop Broadcaster

**Status:** Planned (post-Phase 1)

## Concept

Lightweight desktop app for casual broadcasters who just want to capture system audio and go live. No OBS, no Icecast config - just a "Go Live" button.

## Target Users

- DJs who want to stream their sets without server setup
- Podcasters doing live shows
- Anyone playing music from Spotify/iTunes/YouTube who wants to broadcast

## Features

### MVP
- Capture system audio (loopback)
- Capture specific app audio (where supported)
- Capture mic/line-in
- VU meter visualization
- Stream name input
- One-click go live
- Listener count display

### Later
- Chat/reactions overlay
- Tip notifications
- Recording (local archive)
- Scheduled broadcasts
- Multiple audio source mixing

## Tech Stack

- **Framework:** Tauri (Rust + webview, ~5MB binary)
- **Audio capture:** cpal (cross-platform)
- **UI:** Svelte or vanilla HTML/CSS
- **Protocol:** mdrn-core

## Platform Support

| Platform | System Audio Capture | Notes |
|----------|---------------------|-------|
| macOS | Via BlackHole/Loopback or ScreenCaptureKit | Need virtual audio device or macOS 13+ |
| Windows | WASAPI loopback | Native support |
| Linux | PulseAudio monitor | Native support |

## Wireframe

```
┌─────────────────────────────────────────────────────────┐
│  MDRN Studio                                            │
│  ┌─────────────────────────────────────────────────┐    │
│  │                                                 │    │
│  │   Audio Source: [System Audio      ▼]           │    │
│  │                                                 │    │
│  │   Stream Name:  [Friday Night Vibes   ]         │    │
│  │                                                 │    │
│  │   ┌─────────────────────────────────────────┐   │    │
│  │   │  ▁▂▃▅▆▇█▇▆▅▃▂▁▂▃▅▆▇█▇▆▅▃▂▁  -12dB      │   │    │
│  │   └─────────────────────────────────────────┘   │    │
│  │                                                 │    │
│  │            [ 🔴 GO LIVE ]                       │    │
│  │                                                 │    │
│  │   Listeners: 47    Duration: 01:23:45          │    │
│  │                                                 │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

## Dependencies

```toml
[dependencies]
mdrn-core = { path = "../mdrn-core" }
cpal = "0.15"          # Audio capture
opus = "0.3"           # Encoding
tauri = "2.0"          # Desktop framework
```

## Open Questions

1. How to handle system audio capture on macOS without requiring third-party virtual audio devices?
2. Should we bundle a virtual audio device installer?
3. Branding - "MDRN Studio" vs "Subcult Studio" vs something else?
