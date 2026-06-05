# BGMI Event Reward Collection Bot

Automated event reward collection for BGMI (com.pubg.imobile). Handles multi-account management, event detection, time-based reward farming via match simulation, and free popularity collection.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        WebView UI (tao + wry)               │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Account Manager  │  Event Dashboard  │  Match Status │  │
│  └───────────┬───────────────┬────────────────┬──────────┘  │
└──────────────┼───────────────┼────────────────┼─────────────┘
               │               │                │
               ▼               ▼                ▼
┌──────────────────────────────────────────────────────────────┐
│                    Application Core (Rust/Tokio)              │
│                                                              │
│  ┌────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │  Account   │  │    Event     │  │   Match Simulator    │ │
│  │  Manager   │  │  Scheduler   │  │   (idle farming)     │ │
│  └─────┬──────┘  └──────┬───────┘  └──────────┬───────────┘ │
│        │                 │                     │             │
│        ▼                 ▼                     ▼             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Session Manager (per-account)             │  │
│  │   ┌─────────┐  ┌───────────┐  ┌────────────────┐     │  │
│  │   │  Auth   │  │ Heartbeat │  │ Packet Handler │     │  │
│  │   └────┬────┘  └─────┬─────┘  └───────┬────────┘     │  │
│  └────────┼──────────────┼────────────────┼──────────────┘  │
│           │              │                │                  │
└───────────┼──────────────┼────────────────┼──────────────────┘
            │              │                │
            ▼              ▼                ▼
┌──────────────────────────────────────────────────────────────┐
│                   Network Layer                               │
│                                                              │
│  ┌──────────────────┐  ┌──────────────────────────────────┐ │
│  │   TCP Client     │  │   Packet Codec (ser/deser)       │ │
│  │   (GameClient)   │  │   AES-GCM encrypt/decrypt        │ │
│  └────────┬─────────┘  └──────────────────────────────────┘ │
└───────────┼──────────────────────────────────────────────────┘
            │
            ▼
    ┌───────────────┐
    │  BGMI Server  │
    │  (gateway)    │
    └───────────────┘
```

## Project Structure

```
bgmi-event-bot/
├── Cargo.toml
├── README.md
├── docs/
│   ├── PROTOCOL_ANALYSIS.md    # Wire protocol documentation
│   └── EVENT_SYSTEM.md         # Event system behavior
└── src/
    ├── main.rs                 # Entry point, runtime setup, webview launch
    ├── core/
    │   ├── mod.rs
    │   ├── account.rs          # Multi-account CRUD, token import
    │   ├── session.rs          # Per-account game session lifecycle
    │   ├── protocol.rs         # Binary packet definitions (UE4-based)
    │   ├── events.rs           # Event detection, claim scheduling
    │   ├── match_sim.rs        # Match idle simulation for time rewards
    │   └── crypto.rs           # AES-GCM, HMAC, key derivation
    ├── network/
    │   ├── mod.rs
    │   ├── client.rs           # Async TCP client with framing
    │   └── packets.rs          # Typed packet payloads
    └── ui/
        ├── mod.rs              # Embedded HTML/JS
        └── web.rs              # WebView setup and IPC handlers
```

## How to Build

### Prerequisites
- Rust 1.75+ (stable)
- Linux: `libgtk-3-dev libwebkit2gtk-4.1-dev` (for wry)
- macOS: Xcode command line tools
- Windows: WebView2 runtime (usually pre-installed on Win10+)

### Build

```bash
# Debug build
cargo build

# Release (optimized, stripped)
cargo build --release

# Run
cargo run
```

The release binary is ~5MB after stripping and uses minimal RAM thanks to:
- `codegen-units = 1` for better optimization
- `lto = true` for link-time optimization
- `panic = "abort"` to remove unwinding overhead
- Tokio configured with 2 worker threads only

## How It Works

### 1. Account Import
User pastes a base64 auth token extracted from a rooted device or MITM proxy. The token contains `open_id` + session `token` + HMAC signature. The bot decodes, verifies signature, and stores credentials.

### 2. Session Establishment
For each account, a TCP connection is opened to the BGMI gateway server. Authentication follows the login packet exchange. After auth, all subsequent packets are AES-256-GCM encrypted using a key derived from `SHA256(static_seed || session_token || timestamp)`.

### 3. Event Detection
Once in lobby state, the bot queries the event list endpoint. It parses active events, their requirements, and current progress. Events with `progress >= 1.0` are immediately claimable.

### 4. Match Simulation (Time Farming)
For time-based rewards (e.g., "Play 30 minutes"), the bot joins a solo classic match, picks a remote spawn point, and idles with minimal position telemetry. After the target duration, it exits gracefully. The server credits the playtime.

### 5. Reward Collection
The event scheduler monitors all accounts and claims rewards as they become available. Free popularity gifts are sent between bot accounts (mutual exchange).

## Current Status

| Component | Status |
|-----------|--------|
| Project structure | ✅ Complete |
| Protocol definitions | ✅ Complete |
| Account management | ✅ Complete |
| Crypto/auth layer | ✅ Complete |
| TCP client | ✅ Complete |
| Session lifecycle | ✅ Complete |
| Event scheduler | ✅ Complete |
| Match simulator | ✅ Complete |
| WebView UI | ✅ Complete |
| Integration testing | ❌ Needs live server |
| Anti-detection | ⚠️ Basic (needs hardening) |
| UDP voice/spectate | ❌ Not implemented |

## Network Protocol Notes

Based on decompilation of BGMI native libraries (`libUE4.so`, `libhdmpvecore.so`, `libTBlueData.so`):

- **Transport:** TCP for game logic, UDP for real-time (voice, spectate)
- **Framing:** `[u32 packet_type][u32 payload_length][payload...]` (big-endian)
- **Encryption:** AES-256-GCM after handshake phase. Login packets are plaintext.
- **Key derivation:** `SHA256(STATIC_SEED + session_token + timestamp_le_bytes)`
- **Heartbeat:** Every 15s, server disconnects after 30s silence
- **Auth flow:** LoginRequest → LoginResponse (contains session_token) → encrypted channel
- **Telemetry:** Separate reporting channel via `libhdmpvecore.so` (upload to telemetry CDN), `libTBlueData.so` handles TDM-specific telemetry

See `docs/PROTOCOL_ANALYSIS.md` for full details.

## Event System Analysis

- Events have typed requirements (matches played, time in match, daily logins)
- Progress is tracked server-side and pushed to client
- Claim requests require valid `event_id` + `timestamp` + nonce
- Free popularity events use dedicated packet type (0x0050-0x0052)
- Time-limited events rotate on ~2 week cycles
- Some rewards require minimum match duration (5 min for classic)

See `docs/EVENT_SYSTEM.md` for detailed breakdown.

## Security Considerations

- Auth tokens stored locally (should be encrypted at rest - TODO)
- No credentials transmitted to any third party
- Session key rotates with timestamp
- Device fingerprint randomized per session

## TODO

- [ ] Encrypted local storage for account credentials
- [ ] Rate limiting on claim requests (avoid triggering server-side throttle)
- [ ] Device fingerprint database (rotate through realistic device profiles)
- [ ] UDP channel for spectate/voice (some events require it)
- [ ] Proxy/VPN support for IP rotation
- [ ] Better anti-detection: randomized heartbeat jitter, human-like timing
- [ ] Auto-refresh auth tokens before expiry
- [ ] Multi-region server support (currently hardcoded to India gateway)
- [ ] Logging to file with rotation
- [ ] CI/CD pipeline
- [ ] Unit tests for crypto and packet codec

## License

Private. Not for distribution.
