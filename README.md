# BGMI Event Bot

Automated event reward collection bot for BGMI (Battlegrounds Mobile India).

Built with **real protocol analysis** from captured network traffic — not guesswork.

## Architecture

```
┌─────────────────────────────────────────────┐
│  Web Dashboard (localhost:3000)              │
│  - Account management                       │
│  - Collection triggers                      │
│  - Real-time result log                     │
├─────────────────────────────────────────────┤
│  Session Manager                            │
│  - Multi-account orchestration              │
│  - Periodic collection scheduler            │
│  - Rate limiting + anti-detection           │
├─────────────────────────────────────────────┤
│  BGMI Client (HTTPS)                        │
│  - ITOP SDK auth (in-sdkapi.globh.com)      │
│  - Payment/reward API (min-pay.globh.com)   │
│  - Telemetry replication                    │
├─────────────────────────────────────────────┤
│  Crypto Layer                               │
│  - sValidKey MD5 signatures                 │
│  - AES-128-ECB payload encryption           │
│  - Session key management                   │
└─────────────────────────────────────────────┘
```

## Protocol

Based on real HttpCanary HTTPS traffic captures of BGMI v4.4.0:

| Endpoint | Purpose |
|----------|---------|
| `in-sdkapi.globh.com/v1.0/user/login` | Authentication |
| `in-sdkapi.globh.com/v1.0/user/getTicket` | Session ticket |
| `min-pay.globh.com/v1/r/1450025957/mobile_overseas_common` | Rewards/payment |
| `in-notice.globh.com/v1.0/notice/getNotice` | Event notifications |
| `in-cloudctrl.globh.com/cfgpush/getConfig` | Dynamic config |

See [docs/PROTOCOL_ANALYSIS.md](docs/PROTOCOL_ANALYSIS.md) for full protocol documentation.

## Features

- Multi-account support (Twitter, Facebook, Google, Guest login)
- Daily login reward collection
- Popularity reward collection
- Extra event reward collection
- Anti-detection (telemetry replication, human-like delays)
- Web dashboard with real-time status
- Periodic auto-collection

## Setup

```bash
# Build
cargo build --release

# Run
./target/release/bgmi-event-bot
# Dashboard at http://localhost:3000
```

## API

```bash
# Add account (Twitter example)
curl -X POST http://localhost:3000/api/accounts \
  -H "Content-Type: application/json" \
  -d '{
    "label": "main",
    "credential_type": "twitter",
    "oauth_token": "2059972254298206209-qcUz8RcfqJVWAP7gPMcByu007GpSDC",
    "oauth_token_secret": "5Lpa3xOvxLxgSISgjJNudb2NGn9IXYjAbSrFjDD0LOa4o"
  }'

# Collect all accounts
curl -X POST http://localhost:3000/api/collect-all

# Check results
curl http://localhost:3000/api/results
```

## Stack

- **Language**: Rust
- **Web**: axum (port 3000)
- **HTTP Client**: reqwest + rustls (no OpenSSL dependency)
- **Crypto**: md5, aes, hex
- **Async**: tokio

## Status

- [x] Real protocol reverse-engineered from captures
- [x] Authentication flow implemented
- [x] Payment session + key exchange
- [x] Encrypted command system
- [x] Multi-account management
- [x] Web dashboard
- [ ] Full event catalog discovery
- [ ] Token refresh mechanism
- [ ] Proxy support for multi-IP
