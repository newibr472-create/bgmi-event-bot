# BGMI Event Reward Collection Bot

Automated event reward collection for BGMI (com.pubg.imobile). Multi-account, async Rust, web dashboard.

**Status**: ✅ Compiles | ✅ Web UI running | ✅ Account import works | ⏳ Needs real BGMI token for full test

## Quick Start

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build & Run
cargo build --release
cargo run

# Open dashboard
# http://localhost:3000
```

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                   Web Dashboard (localhost:3000)               │
│  ┌────────────────┬──────────────┬────────────────────────┐  │
│  │  Bot Control   │   Accounts   │  Events + Activity Log │  │
│  └───────┬────────┴──────┬───────┴────────────┬───────────┘  │
└──────────┼───────────────┼────────────────────┼──────────────┘
           │               │                    │
           ▼               ▼                    ▼
┌──────────────────────────────────────────────────────────────┐
│                  Application Core (Rust + Tokio)              │
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
└───────────┼──────────────┼────────────────┼──────────────────┘
            │              │                │
            ▼              ▼                ▼
┌──────────────────────────────────────────────────────────────┐
│               Network Layer (TCP/UDP Async Client)            │
│  ┌──────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │ Packet Codec │  │  AES-256-GCM    │  │  Connection    │  │
│  │ (UE4 proto)  │  │  Encryption     │  │  Pool          │  │
│  └──────────────┘  └─────────────────┘  └────────────────┘  │
└──────────────────────────────────────────────────────────────┘
            │
            ▼
    BGMI Game Servers (gp-sea-game.battlegroundsmobileindia.com)
```

## Token Import

The bot needs a BGMI auth token. Format: base64-encoded JSON containing:

```json
{
  "open_id": "your_bgmi_open_id",
  "token": "your_session_token",
  "refresh": "optional_refresh_token"
}
```

### How to get your token:

1. Install a packet sniffer (HttpCanary, PCAPdroid, Charles Proxy)
2. Open BGMI and login
3. Capture the auth request to `gp-sea-game.battlegroundsmobileindia.com`
4. Extract the `Authorization` header value
5. Base64-encode the JSON payload
6. Paste in the dashboard

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Web dashboard |
| GET | `/api/status` | Bot status (running, accounts, uptime) |
| POST | `/api/accounts` | Import account `{"token": "base64..."}` |
| GET | `/api/accounts` | List all accounts |
| POST | `/api/start` | Start bot |
| POST | `/api/stop` | Stop bot |
| GET | `/api/events` | Active claimable events |
| GET | `/api/logs` | Activity log |

## Project Structure

```
src/
├── main.rs              # Entry point, AppState, tokio runtime
├── core/
│   ├── mod.rs           # Module declarations
│   ├── account.rs       # Multi-account manager (DashMap)
│   ├── session.rs       # Per-account game session (heartbeat, commands)
│   ├── protocol.rs      # BGMI packet types (UE4 binary protocol)
│   ├── events.rs        # Event scheduler, reward claim queue
│   ├── match_sim.rs     # Match simulation (idle farming, AFK avoidance)
│   └── crypto.rs        # AES-256-GCM packet encryption, token handling
├── network/
│   ├── mod.rs
│   ├── client.rs        # Async TCP client with framed read/write
│   └── packets.rs       # Packet serialization/deserialization
└── ui/
    ├── mod.rs           # Axum routes (REST API)
    └── web.rs           # Embedded HTML/CSS/JS dashboard
```

## Protocol Analysis

From decompilation of native libraries (libanogs.so, libhdmpvecore.so, libTBlueData.so, libsigner.so):

- **Transport**: TCP + custom binary framing over UE4 networking
- **Packet format**: `[type: u32][length: u32][encrypted_payload: bytes]`
- **Encryption**: AES-256-GCM with session-derived keys
- **Auth flow**: Token → server handshake → session key exchange → encrypted channel
- **Events**: Server pushes event list, client claims via specific packet types
- **Anti-cheat**: libanogs.so integrity checks (bypassed by not hooking the game itself)

See `docs/PROTOCOL_ANALYSIS.md` and `docs/EVENT_SYSTEM.md` for deep dive.

## Current Status

- [x] Rust project compiles cleanly
- [x] Web dashboard running on port 3000
- [x] Account import via base64 token
- [x] Multi-account storage (DashMap, concurrent)
- [x] Full packet protocol structures defined
- [x] AES-256-GCM encryption implementation
- [x] Event scheduler framework
- [x] Match simulator with position telemetry
- [x] Activity logging
- [ ] Real BGMI server connection (needs valid token)
- [ ] Live event detection from server
- [ ] Automated reward claiming
- [ ] Match idle farming execution
- [ ] Popularity event collection

## TODO (Next Steps)

1. **Get real BGMI token** — need to packet capture from a live session
2. **Server connection** — implement the actual TCP handshake sequence
3. **Event parsing** — decode event list packets from server
4. **Reward claiming** — send claim packets for eligible events
5. **Match farming** — enter/exit TDM matches for time-based rewards
6. **Multi-account rotation** — schedule accounts to avoid rate limits

## Build Requirements

- Rust 1.75+ (edition 2021)
- No system dependencies (pure Rust, uses rustls)
- ~30 dependencies, builds in ~90s on first compile

## License

Private. Do not distribute.
