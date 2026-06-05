# BGMI Protocol Analysis

Notes from decompilation of `com.pubg.imobile` native libraries and network traffic capture.

## Overview

BGMI uses a custom binary protocol layered on top of UE4 networking primitives. The game connects via TCP to a gateway server for all game logic, and uses UDP for real-time communications (voice, spectator mode, anti-cheat telemetry).

## Libraries Analyzed

| Library | Purpose |
|---------|---------|
| `libUE4.so` | Core Unreal Engine 4 runtime, includes networking stack |
| `libhdmpvecore.so` | Upload/reporting module for telemetry data (match events, performance metrics) |
| `libTBlueData.so` | TDM (Team Deathmatch) specific telemetry handling |
| `libtersafe2.so` | Anti-cheat module (hooks into memory, checks integrity) |

## Packet Wire Format

All packets follow this structure:

```
┌────────────────┬────────────────┬───────────────────────┐
│  Type (4B BE)  │ Length (4B BE) │  Payload (N bytes)    │
└────────────────┴────────────────┴───────────────────────┘
```

- **Type**: `u32` big-endian. Maps to `PacketType` enum.
- **Length**: `u32` big-endian. Length of payload only (excludes header).
- **Payload**: Variable length. JSON for most packets, binary for telemetry/position.

Maximum packet size observed: 64KB. Server rejects anything larger.

## Packet Types (from decompilation)

### Authentication (0x0001 - 0x000F)

```
0x0001  LoginRequest        Client → Server
0x0002  LoginResponse       Server → Client
0x0003  TokenRefresh        Client → Server
0x0004  TokenRefreshAck     Server → Client
```

### Session (0x0010 - 0x001F)

```
0x0010  Heartbeat           Client → Server
0x0011  HeartbeatAck        Server → Client
0x0012  Disconnect          Client → Server (graceful)
0x0013  KickNotice          Server → Client
```

### Match (0x0030 - 0x003F)

```
0x0030  MatchJoinRequest    Client → Server
0x0031  MatchJoinResponse   Server → Client
0x0032  MatchStart          Server → Client (broadcast)
0x0033  MatchUpdate         Server → Client (zone, etc)
0x0034  MatchEnd            Server → Client
0x0035  MatchLeave          Client → Server
```

### Events (0x0040 - 0x004F)

```
0x0040  EventList           Client → Server (query)
0x0041  EventDetail         Server → Client (response)
0x0042  EventClaimRequest   Client → Server
0x0043  EventClaimResponse  Server → Client
0x0044  EventNotification   Server → Client (push)
0x0045  RewardGrant         Server → Client (push)
```

### Popularity (0x0050 - 0x005F)

```
0x0050  PopularityQuery     Client → Server
0x0051  PopularityClaim     Client → Server
0x0052  PopularityResult    Server → Client
```

### Telemetry (0x0060+)

```
0x0060  TelemetryReport     Client → Server
0x0061  TelemetryAck        Server → Client
```

## Authentication Flow

```
Client                              Server
  │                                    │
  │─── LoginRequest (plaintext) ──────▶│
  │    {open_id, token, device_id,     │
  │     client_version, os}            │
  │                                    │
  │◀── LoginResponse (plaintext) ──────│
  │    {code, session_token, lobby_url}│
  │                                    │
  │    *** All further packets are     │
  │    *** AES-256-GCM encrypted       │
  │                                    │
  │─── Heartbeat (encrypted) ─────────▶│
  │◀── HeartbeatAck (encrypted) ───────│
  │         ...                        │
```

### Token Format

Auth tokens are base64-encoded JSON:

```json
{
  "open_id": "5f3a8c...",
  "token": "eyJhbGciOi...",
  "ts": 1703275200,
  "sig": "a3f2b1c4..."
}
```

The `sig` field is `HMAC-SHA256(open_id + token + ts, APP_SECRET)` where `APP_SECRET` is derived from the app's signing key.

### Session Token

After successful login, the server returns a `session_token` (base64). This is combined with a static seed and the current timestamp to derive the AES-256-GCM key:

```
key = SHA256(STATIC_SEED || session_token_bytes || timestamp_le_8bytes)
```

The timestamp must be within a server-tolerance window (observed: ±60 seconds).

## Encryption Details

- Algorithm: AES-256-GCM
- Nonce: 12 bytes, prepended to ciphertext
- Tag: 16 bytes, appended to ciphertext
- Wire format of encrypted payload: `[nonce:12][ciphertext][tag:16]`

Login packets (type 0x0001, 0x0002) and error responses (0xFF00) are always plaintext.

## Server Infrastructure

From DNS and traffic analysis:

- **Gateway**: `bgmi-gateway.pubg.com:17500` (India region)
- **Lobby**: Dynamic, returned in LoginResponse as `lobby_url`
- **Match**: Separate server, connection details in MatchJoinResponse
- **Telemetry CDN**: HTTPS endpoint, reported by `libhdmpvecore.so`

## Heartbeat

- Client sends heartbeat every 15 seconds
- Server expects heartbeat within 30-second window
- Missing 2 consecutive heartbeats = server disconnect
- HeartbeatAck contains server timestamp (used for clock sync)

## Anti-Cheat Observations

`libtersafe2.so` performs:
- Memory integrity checks (CRC of game memory regions)
- Root detection (su binary, Magisk, etc.)
- Emulator detection (hardware fingerprinting)
- Speed hack detection (timestamp validation)

For the event bot, we don't need to bypass anti-cheat since we operate at the network level, not memory level. However, behavioral detection (unusual login patterns, instant claims, always-AFK matches) is a concern.

## Rate Limiting

Observed server-side rate limits:
- Event claims: Max 1 per event per 5 seconds
- Popularity gifts: Max 10 per day per account
- Match join: Max 1 concurrent, cooldown 10s between matches
- Heartbeat: Server ignores if sent more frequently than 5s

## Telemetry (libhdmpvecore.so)

This library handles:
- Match performance data upload
- Player behavior metrics
- Device telemetry
- Crash reporting

It communicates via HTTPS POST to a CDN endpoint. The bot should either:
1. Not send telemetry (risk: flagged as modified client)
2. Send minimal valid telemetry (safer but more complex)

Current approach: Send position telemetry via the game connection (0x0060) only. Skip HTTP telemetry upload.

## libTBlueData.so

Handles TDM-specific event tracking:
- Kill/death events in TDM mode
- TDM match statistics
- TDM-specific achievements

Not relevant for event farming (we use Classic mode for time accumulation).

## Open Questions

1. Exact format of `lobby_url` - is it a separate TCP connection or same socket?
2. Does the server validate client_version strictly? (need to track updates)
3. Token refresh mechanism - how long before tokens expire?
4. Are there additional packet types for seasonal events?
5. UDP channel details for spectate mode (some events may require spectating)
