# Phase 3 Implementation Plan: Browser Client

Created: 2026-03-23
Author: architect-agent
Status: Planning
Target Duration: 4-6 weeks

## Overview

Build a browser-based MDRN client enabling web users to broadcast and listen to encrypted audio streams. This involves compiling mdrn-core to WebAssembly, integrating with Web Audio API for real-time audio processing, implementing browser-compatible transport (WebRTC via libp2p), and creating a minimal React-based UI.

## Requirements

- [ ] WASM compilation of mdrn-core protocol library
- [ ] Web Audio API integration for real-time audio capture/playback
- [ ] Browser-compatible libp2p transport (WebRTC)
- [ ] Minimal web UI for broadcast/listen functionality
- [ ] Opus codec support in browser (via opus.js or native WebCodecs)
- [ ] Stream encryption/decryption in WASM
- [ ] Payment commitment signing in browser (wallet integration)

## Design

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Browser Client                          │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────────────┐   │
│  │   React UI  │◄──│   JS/TS     │◄──│   mdrn-wasm         │   │
│  │             │   │   Bindings  │   │   (Rust → WASM)     │   │
│  └──────┬──────┘   └──────┬──────┘   └──────────┬──────────┘   │
│         │                 │                      │              │
│  ┌──────▼──────┐   ┌──────▼──────┐   ┌──────────▼──────────┐   │
│  │  Web Audio  │   │  libp2p     │   │  Crypto             │   │
│  │  Worklet    │   │  WebRTC     │   │  (ChaCha20,Ed25519) │   │
│  └─────────────┘   └─────────────┘   └─────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
         │                   │
         ▼                   ▼
    ┌─────────┐        ┌─────────────┐
    │ Mic/    │        │ MDRN Relay  │
    │ Speaker │        │ (native)    │
    └─────────┘        └─────────────┘
```

### New Crate Structure

```
mdrn-protocol/
├── mdrn-core/           # Existing - protocol library
├── mdrn-cli/            # Existing - CLI tool
├── mdrn-relay/          # Existing - relay daemon
├── mdrn-wasm/           # NEW - WASM bindings for mdrn-core
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs       # WASM entry point
│   │   ├── identity.rs  # Keypair WASM bindings
│   │   ├── crypto.rs    # Encryption WASM bindings
│   │   ├── protocol.rs  # Message WASM bindings
│   │   ├── stream.rs    # Chunk handling WASM bindings
│   │   └── transport.rs # WebRTC transport bindings
│   └── pkg/             # Generated JS/TS output
└── mdrn-web/            # NEW - Web client application
    ├── package.json
    ├── vite.config.ts
    ├── index.html
    ├── src/
    │   ├── main.tsx
    │   ├── App.tsx
    │   ├── lib/
    │   │   ├── mdrn-client.ts    # High-level API
    │   │   ├── audio-worklet.ts  # Web Audio processing
    │   │   ├── opus-codec.ts     # Opus encode/decode
    │   │   └── transport.ts      # WebRTC management
    │   ├── components/
    │   │   ├── BroadcastPanel.tsx
    │   │   ├── ListenPanel.tsx
    │   │   ├── StreamList.tsx
    │   │   └── PaymentStatus.tsx
    │   └── hooks/
    │       ├── useAudio.ts
    │       ├── useStream.ts
    │       └── usePayment.ts
    └── public/
        └── audio-processor.js   # AudioWorklet processor
```

### Interfaces

```typescript
// mdrn-web/src/lib/mdrn-client.ts

import init, { 
  Keypair, 
  StreamChunk, 
  PaymentCommitment,
  encrypt_chunk,
  decrypt_chunk,
  sign_message,
  verify_message
} from 'mdrn-wasm';

interface MdrnClient {
  // Identity
  generateKeypair(): Promise<Keypair>;
  loadKeypair(json: string): Promise<Keypair>;
  
  // Streaming
  startBroadcast(config: BroadcastConfig): Promise<BroadcastSession>;
  startListening(streamAddr: string): Promise<ListenSession>;
  
  // Discovery
  discoverStreams(): Promise<StreamAnnouncement[]>;
  
  // Payment
  createPaymentCommitment(amount: bigint): Promise<PaymentCommitment>;
}

interface BroadcastConfig {
  streamId: string;
  codec: 'opus';
  bitrate: number;
  sampleRate: 48000;
  encrypted: boolean;
  price?: bigint;
}

interface BroadcastSession {
  streamAddr: Uint8Array;
  pushAudio(samples: Float32Array): void;
  stop(): Promise<void>;
}

interface ListenSession {
  onAudio: (callback: (samples: Float32Array) => void) => void;
  onPaymentRequired: (callback: (amount: bigint) => void) => void;
  stop(): Promise<void>;
}
```

```rust
// mdrn-wasm/src/lib.rs

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmKeypair {
    inner: mdrn_core::identity::Keypair,
}

#[wasm_bindgen]
impl WasmKeypair {
    #[wasm_bindgen(constructor)]
    pub fn generate() -> Result<WasmKeypair, JsValue>;
    
    pub fn from_json(json: &str) -> Result<WasmKeypair, JsValue>;
    pub fn to_json(&self) -> Result<String, JsValue>;
    pub fn identity_hex(&self) -> String;
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, JsValue>;
}

#[wasm_bindgen]
pub fn encrypt_chunk(
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8]
) -> Result<Vec<u8>, JsValue>;

#[wasm_bindgen]
pub fn decrypt_chunk(
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8]
) -> Result<Vec<u8>, JsValue>;
```

### Data Flow

#### Broadcast Flow
```
1. User grants microphone access
2. AudioWorklet captures PCM samples at 48kHz
3. Opus encoder (WebCodecs or opus.js) encodes to packets
4. mdrn-wasm encrypts packets (ChaCha20-Poly1305)
5. mdrn-wasm wraps in CBOR Chunk message
6. libp2p-webrtc publishes to gossipsub topic
7. Relay receives and forwards to subscribers
```

#### Listen Flow
```
1. User enters stream address or selects from discovery
2. libp2p-webrtc connects to relay via WebRTC
3. Relay sends encrypted chunks over gossipsub
4. mdrn-wasm decrypts and extracts audio data
5. Opus decoder converts to PCM
6. AudioWorklet plays through speakers
7. Payment commitments sent periodically
```

## Dependencies

| Dependency | Type | Reason |
|------------|------|--------|
| wasm-bindgen | External | Rust WASM bindings |
| wasm-pack | Build tool | WASM compilation |
| js-sys | External | JavaScript interop |
| web-sys | External | Web API bindings |
| getrandom/js | External | CSPRNG in browser |
| libp2p-webrtc | External | Browser transport |
| libp2p-gossipsub | Internal | Pub/sub (already used) |
| React 18 | External | UI framework |
| Vite | Build tool | Fast bundler with WASM support |
| @aspect-build/esbuild-plugin-wasm | Build | WASM bundling |
| opus-decoder (npm) | External | Opus decoding |
| AudioWorklet | Browser API | Low-latency audio |

## Implementation Phases

### Phase 3.1: WASM Foundation (Week 1-2)

**Goal**: Compile mdrn-core to WASM with working crypto

**Files to create:**
- `mdrn-wasm/Cargo.toml` - WASM crate configuration
- `mdrn-wasm/src/lib.rs` - WASM entry point
- `mdrn-wasm/src/identity.rs` - Keypair bindings
- `mdrn-wasm/src/crypto.rs` - ChaCha20/HKDF bindings
- `mdrn-wasm/src/protocol.rs` - Message encoding bindings

**Files to modify:**
- `Cargo.toml` - Add mdrn-wasm to workspace
- `mdrn-core/Cargo.toml` - Add WASM-compatible feature flags

**Technical Decisions:**

1. **Feature-gate native-only code**
```toml
# mdrn-core/Cargo.toml
[features]
default = ["native"]
native = ["libp2p/tcp", "libp2p/quic", "tokio/full"]
wasm = ["getrandom/js", "wasm-bindgen-futures"]
```

2. **Conditional compilation for transport**
```rust
// mdrn-core/src/transport/mod.rs
#[cfg(feature = "native")]
mod swarm;
#[cfg(feature = "native")]
pub use swarm::*;

#[cfg(feature = "wasm")]
mod webrtc;
#[cfg(feature = "wasm")]
pub use webrtc::*;
```

3. **WASM-compatible random**
```rust
// mdrn-wasm/src/lib.rs
use getrandom::getrandom;  // Works in browser via js feature
```

**Acceptance Criteria:**
- [ ] `wasm-pack build` succeeds
- [ ] Keypair generation works in browser console
- [ ] ChaCha20 encryption/decryption works
- [ ] CBOR encoding/decoding works
- [ ] Unit tests pass with `wasm-pack test --headless --chrome`

**Estimated effort:** Medium

### Phase 3.2: Web Audio Integration (Week 2-3)

**Goal**: Real-time audio capture and playback in browser

**Files to create:**
- `mdrn-web/package.json` - Node dependencies
- `mdrn-web/vite.config.ts` - Build configuration
- `mdrn-web/src/lib/audio-worklet.ts` - Worklet management
- `mdrn-web/public/audio-processor.js` - AudioWorklet processor
- `mdrn-web/src/lib/opus-codec.ts` - Opus wrapper

**Technical Decisions:**

1. **AudioWorklet for low latency**
```javascript
// audio-processor.js (runs in audio thread)
class MdrnAudioProcessor extends AudioWorkletProcessor {
  process(inputs, outputs, parameters) {
    const input = inputs[0];
    if (input && input[0]) {
      // Send PCM to main thread
      this.port.postMessage({ pcm: input[0] });
    }
    return true;
  }
}
registerProcessor('mdrn-audio-processor', MdrnAudioProcessor);
```

2. **Opus codec choice**
   - Option A: `@aspect-build/opus-decoder` - Pure JS/WASM, well-maintained
   - Option B: WebCodecs API - Native but limited browser support
   - **Decision: Use WebCodecs with opus-decoder fallback**

```typescript
// opus-codec.ts
export async function createOpusEncoder(): Promise<AudioEncoder | OpusEncoderFallback> {
  if ('AudioEncoder' in window) {
    return new AudioEncoder({
      output: (chunk) => { /* handle encoded */ },
      error: (e) => console.error(e)
    });
  }
  // Fallback to JS implementation
  return new OpusEncoderFallback();
}
```

3. **Buffer management**
```typescript
// Collect 20ms frames (960 samples at 48kHz)
const FRAME_SIZE = 960;
const ringBuffer = new RingBuffer(FRAME_SIZE * 4);

worklet.port.onmessage = (e) => {
  ringBuffer.push(e.data.pcm);
  while (ringBuffer.available() >= FRAME_SIZE) {
    const frame = ringBuffer.read(FRAME_SIZE);
    encodeAndSend(frame);
  }
};
```

**Acceptance Criteria:**
- [ ] Microphone capture works with getUserMedia
- [ ] AudioWorklet processes audio in real-time
- [ ] Opus encoding produces valid packets
- [ ] Opus decoding plays audio without glitches
- [ ] Latency under 100ms end-to-end (local loopback)

**Estimated effort:** Medium-High

### Phase 3.3: Browser Transport (Week 3-4)

**Goal**: libp2p WebRTC transport connecting to native relays

**Files to create:**
- `mdrn-wasm/src/transport.rs` - WebRTC bindings
- `mdrn-web/src/lib/transport.ts` - Connection management
- `mdrn-web/src/lib/signaling.ts` - WebRTC signaling

**Technical Decisions:**

1. **libp2p-webrtc vs custom WebRTC**
   - libp2p-webrtc: Standardized, interop with native peers
   - Custom: More control, simpler
   - **Decision: Use rust-libp2p/webrtc compiled to WASM**

2. **Signaling approach**
```
Browser ──[WebSocket]──► Relay ──[libp2p]──► Other Peers
         signaling         native transport
```

3. **Relay discovery**
```typescript
// Bootstrap to known relays, then use DHT
const BOOTSTRAP_RELAYS = [
  '/dns4/relay1.mdrn.ai/tcp/443/wss/p2p/12D3Koo...',
  '/dns4/relay2.mdrn.ai/tcp/443/wss/p2p/12D3Koo...',
];
```

4. **Feature flags for mdrn-core**
```toml
# Workspace Cargo.toml additions
[workspace.dependencies]
libp2p = { version = "0.54", features = [...] }

# For WASM target
[target.'cfg(target_arch = "wasm32")'.dependencies]
libp2p = { version = "0.54", features = [
    "wasm-bindgen",
    "webrtc-websys",
    "gossipsub",
    "kad",
    "noise",
    "yamux",
] }
```

**Acceptance Criteria:**
- [ ] Browser connects to native relay via WebRTC
- [ ] Gossipsub subscription works
- [ ] DHT queries return results
- [ ] Reconnection handles network changes
- [ ] Works on Chrome, Firefox, Safari

**Estimated effort:** High

### Phase 3.4: Minimal UI (Week 4-5)

**Goal**: Functional web interface for broadcast/listen

**Files to create:**
- `mdrn-web/src/main.tsx` - React entry
- `mdrn-web/src/App.tsx` - Main application
- `mdrn-web/src/components/BroadcastPanel.tsx`
- `mdrn-web/src/components/ListenPanel.tsx`
- `mdrn-web/src/components/StreamList.tsx`
- `mdrn-web/src/components/PaymentStatus.tsx`
- `mdrn-web/src/hooks/useAudio.ts`
- `mdrn-web/src/hooks/useStream.ts`

**UI Design:**

```
┌────────────────────────────────────────────┐
│  MDRN                           [Connect]  │
├────────────────────────────────────────────┤
│                                            │
│  ┌──────────────┐  ┌──────────────┐       │
│  │  BROADCAST   │  │    LISTEN    │       │
│  │              │  │              │       │
│  │  [🎤 Start]  │  │  Stream ID:  │       │
│  │              │  │  [________]  │       │
│  │  Status:     │  │              │       │
│  │  Ready       │  │  [▶ Join]    │       │
│  │              │  │              │       │
│  │  Listeners:  │  │  Status:     │       │
│  │  0           │  │  Idle        │       │
│  └──────────────┘  └──────────────┘       │
│                                            │
│  ┌──────────────────────────────────────┐ │
│  │  Active Streams                      │ │
│  │  ─────────────────────────────────── │ │
│  │  • jazz-radio (3 listeners)    [▶]  │ │
│  │  • news-live (12 listeners)    [▶]  │ │
│  │  • ambient-music (1 listener)  [▶]  │ │
│  └──────────────────────────────────────┘ │
│                                            │
│  Payment: Free tier │ [Upgrade]           │
└────────────────────────────────────────────┘
```

**Component Hierarchy:**
```
App
├── Header (connection status, wallet)
├── BroadcastPanel
│   ├── AudioMeter (input level)
│   └── StatusDisplay
├── ListenPanel
│   ├── StreamInput
│   ├── AudioMeter (output level)
│   └── StatusDisplay
├── StreamList
│   └── StreamCard[]
└── Footer (payment status)
```

**State Management:**
```typescript
// Simple React context, no Redux needed
interface MdrnState {
  keypair: Keypair | null;
  connected: boolean;
  broadcasting: BroadcastSession | null;
  listening: ListenSession | null;
  streams: StreamAnnouncement[];
}
```

**Acceptance Criteria:**
- [ ] Broadcast panel captures and streams audio
- [ ] Listen panel plays incoming streams
- [ ] Stream discovery shows active broadcasts
- [ ] Connection status visible
- [ ] Works on mobile browsers

**Estimated effort:** Medium

### Phase 3.5: Payment & Polish (Week 5-6)

**Goal**: Payment integration and production readiness

**Files to create:**
- `mdrn-web/src/lib/wallet.ts` - Wallet connection
- `mdrn-web/src/hooks/usePayment.ts` - Payment state
- `mdrn-web/src/components/WalletConnect.tsx`

**Files to modify:**
- All components - Error handling, loading states
- `mdrn-wasm/src/lib.rs` - Payment commitment signing

**Technical Decisions:**

1. **Wallet integration**
```typescript
// Support MetaMask + WalletConnect
import { createWeb3Modal, defaultConfig } from '@web3modal/ethers';

const projectId = 'YOUR_WALLETCONNECT_PROJECT_ID';
const metadata = {
  name: 'MDRN',
  description: 'Decentralized Radio Network',
  url: 'https://mdrn.ai',
  icons: ['https://mdrn.ai/icon.png']
};
```

2. **Payment flow**
```typescript
// Lazy settlement - sign commitments locally
async function createCommitment(amount: bigint): Promise<Uint8Array> {
  const commitment = wasmClient.createPaymentCommitment(
    relayId,
    streamAddr,
    amount,
    'USDC'
  );
  // Sign with wallet
  const signature = await signer.signMessage(commitment);
  return wasmClient.finalizeCommitment(commitment, signature);
}
```

3. **Free tier for testing**
```typescript
// Default to free mode, payment optional
const paymentMode = localStorage.getItem('mdrn_payment_mode') || 'free';
```

**Acceptance Criteria:**
- [ ] Wallet connection works (MetaMask)
- [ ] Payment commitments are signed correctly
- [ ] Free tier works without wallet
- [ ] Error messages are user-friendly
- [ ] Loading states show progress
- [ ] Mobile responsive

**Estimated effort:** Medium

---

## Testing Strategy

### Unit Tests (WASM)
```bash
# Run in headless browser
wasm-pack test --headless --chrome mdrn-wasm

# Test crypto
#[wasm_bindgen_test]
fn test_encrypt_decrypt_roundtrip() {
    let key = [0u8; 32];
    let nonce = [0u8; 12];
    let plaintext = b"hello world";
    let ciphertext = encrypt_chunk(&key, &nonce, plaintext).unwrap();
    let decrypted = decrypt_chunk(&key, &nonce, &ciphertext).unwrap();
    assert_eq!(plaintext.to_vec(), decrypted);
}
```

### Integration Tests (Browser)
```typescript
// Playwright tests
test('broadcast and listen roundtrip', async ({ page }) => {
  await page.goto('/');
  
  // Start broadcast
  await page.click('[data-testid="broadcast-start"]');
  await page.waitForSelector('[data-testid="broadcast-active"]');
  
  // Open second tab, listen
  const page2 = await browser.newPage();
  await page2.goto('/');
  await page2.fill('[data-testid="stream-input"]', streamAddr);
  await page2.click('[data-testid="listen-start"]');
  
  // Verify audio received
  await page2.waitForSelector('[data-testid="audio-meter-active"]');
});
```

### Manual Testing Checklist
- [ ] Chrome desktop - broadcast
- [ ] Chrome desktop - listen
- [ ] Firefox desktop - broadcast
- [ ] Firefox desktop - listen
- [ ] Safari desktop - listen only (no getUserMedia without HTTPS)
- [ ] Chrome Android - listen
- [ ] Safari iOS - listen
- [ ] Cross-browser: Chrome broadcast → Firefox listen
- [ ] Native CLI broadcast → Browser listen
- [ ] Browser broadcast → Native CLI listen

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| libp2p-webrtc not stable | High | Fallback to WebSocket relay; start testing early |
| AudioWorklet browser support | Medium | Use ScriptProcessorNode fallback (deprecated but works) |
| WASM bundle size too large | Medium | Tree-shaking, code splitting, lazy loading |
| Opus codec complexity | Medium | Use proven npm package (opus-decoder) |
| Browser crypto performance | Low | ChaCha20 is fast; profile and optimize if needed |
| Safari WebRTC quirks | Medium | Test early, document limitations |
| Mobile battery drain | Medium | Throttle when backgrounded, document power usage |

---

## Open Questions

- [ ] Should we support recording streams to file?
- [ ] What's the minimum viable discovery UX?
- [ ] Do we need offline/PWA support?
- [ ] How do we handle CORS for relay connections?
- [ ] Should payment use EIP-712 typed signatures?
- [ ] What analytics/telemetry (if any)?

---

## Success Criteria

1. **Functional**: Browser user can discover, listen to, and create streams
2. **Performance**: Audio latency under 200ms end-to-end
3. **Reliability**: No audio glitches during 1-hour listening session
4. **Compatibility**: Works on Chrome, Firefox, Safari (latest versions)
5. **Size**: Initial bundle under 500KB gzipped
6. **Security**: All stream encryption works correctly in browser

---

## Dependencies on Prior Phases

| Dependency | Source | Status |
|------------|--------|--------|
| Protocol messages | mdrn-core/protocol | Complete |
| Crypto (ChaCha20, Ed25519) | mdrn-core/crypto | Complete |
| Stream chunks | mdrn-core/stream | Complete |
| Payment commitments | mdrn-core/payment | Complete (Phase 2) |
| Vouch verification | mdrn-core/identity | Complete (Phase 2) |
| Native relay | mdrn-relay | Complete |

---

## Next Steps

1. **Week 1**: Set up mdrn-wasm crate, get basic WASM build working
2. **Week 1**: Audit mdrn-core for WASM compatibility issues
3. **Week 2**: Implement crypto bindings, test in browser
4. **Week 2**: Start Web Audio worklet prototype
5. **Week 3**: WebRTC transport integration begins
6. **Week 4**: UI development in parallel with transport
7. **Week 5**: Integration testing, payment flow
8. **Week 6**: Polish, browser compatibility, documentation

Ready to begin implementation.
