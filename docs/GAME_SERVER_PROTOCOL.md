# BGMI Game Server Protocol - Complete Deep Analysis

## Architecture Overview

```
┌─────────────┐          ┌──────────────────────────────┐
│   BGMI App  │          │    Server Infrastructure     │
│             │          │                              │
│  ┌───────┐  │  HTTPS   │  ┌─────────────────────────┐│
│  │ ITOP  │──┼──────────┼──│ in-sdkapi.globh.com     ││
│  │  SDK  │  │          │  │ (Auth: login/ticket)     ││
│  └───────┘  │          │  └─────────────────────────┘│
│             │          │                              │
│  ┌───────┐  │  HTTPS   │  ┌─────────────────────────┐│
│  │MidasPay│──┼──────────┼──│ min-pay.globh.com       ││
│  │  SDK  │  │          │  │ (Payment/key exchange)   ││
│  └───────┘  │          │  └─────────────────────────┘│
│             │          │                              │
│  ┌───────┐  │  UDP     │  ┌─────────────────────────┐│
│  │GCloud │  │  8700    │  │ 20.193.140.198:8700     ││
│  │Voice  │──┼──────────┼──│ (Voice config server)   ││
│  │  SDK  │  │  8011    │  │ voice-qos.globh.com:8011││
│  └───────┘  │          │  └─────────────────────────┘│
│             │          │                              │
│  ┌───────┐  │  UDP     │  ┌─────────────────────────┐│
│  │ Game  │  │  9030    │  │ 20.41.230.28:9030       ││
│  │Engine │──┼──────────┼──│ (Gateway/Director)      ││
│  │(UE4)  │  │  9031    │  │ 3x Lobby Servers:9031   ││
│  └───────┘  │          │  │ Match Servers (dynamic) ││
│             │          │  └─────────────────────────┘│
└─────────────┘          └──────────────────────────────┘
```

## Connection Sequence (from capture analysis)

### Phase 1: Authentication (HTTPS) ✅ FULLY IMPLEMENTED
```
1. POST in-sdkapi.globh.com/v1.0/user/login (Twitter OAuth)
   → Returns: openid, innerToken, guid, expireTime
   
2. GET in-sdkapi.globh.com/v1.0/user/getTicket
   → Returns: ticket (for game server auth)
   
3. POST min-pay.globh.com/v1/r/1450025957/mobile_overseas_common
   cmd=get_key|get_ip
   → Returns: key_info, server IPs, h5_host
```

### Phase 2: Gateway Connection (UDP:9030)
```
4. Client → 20.41.230.28:9030 (single packet, 22 bytes)
   Magic: 74 AC 00 00 [timestamp] [session_init]
   Purpose: Request lobby server assignment
   
   Response: (likely contains lobby server list - not captured separately)
```

### Phase 3: Lobby Keepalive (UDP:9031)
```
5. Client → 3 lobby servers simultaneously every ~2.5s
   IPs: 20.219.78.62, 34.0.11.52, 20.204.17.153 (all port 9031)
   
   Packet: 22 bytes fixed format
   [74 AC] [00] [SeqN] [Timestamp:5B] [TTL] [33] [C3 DE 08] [AAAAAAAA] [BBBBBBBB]
   
   SeqN: Global counter, round-robins across 3 servers
   Timestamp: 5 bytes, server-specific offset in byte[5]
   TTL: Decrements over time (70→56 in 23 sec)
   Session: "33 C3 DE 08" - identifies this login session
```

### Phase 4: Voice Config (UDP:8700)
```
6. Client → 20.193.140.198:8700
   GCloud protocol, magic: 75 75
   
   Request (97 bytes):
   [75 75] [00 44] [00 16] [session] [app_id:1375135419] [openid] [auth_hash:md5]
   
   Response (418 bytes):
   [75 75] [01 7E] [00 17] [session] [JSON config]
   
   Config contains:
   - tqos_url: "udp://voice-qos.globh.com:8011"
   - gvofs: "gvoffline.globh.com"  
   - bit_rate: 32000
   - fec: 1
```

### Phase 5: Match Start (NOT CAPTURED - theoretical)
```
7. Client → Lobby Server (larger packet, same 74AC magic or different cmd)
   Contains: game_mode, map_id, team_size, region
   
8. Lobby → Client: MATCH_ASSIGNED
   Contains: match_server_ip, match_port, session_ticket, match_id
   
9. Client → Match Server (new UDP connection)
   Handshake with session_ticket
   
10. Match Server → Client: GAME_STATE stream
    Encrypted protobuf game state (positions, entities, events)
```

## Protocol Details

### 0x74AC Keepalive Format
```
Offset  Size  Field           Description
------  ----  -----           -----------
0       2     Magic           Always 0x74AC
2       1     Version         Always 0x00  
3       1     Sequence        Global packet counter (round-robin across servers)
4       5     Timestamp       Per-server clock value
9       1     TTL             Time-to-live or RTT counter (decrements)
10      1     SessionMarker   0x33 (constant per session)
11      3     SessionToken    C3 DE 08 (identifies lobby session)
14      4     PaddingA        AA AA AA AA
18      4     PaddingB        BB BB BB BB
------
Total: 22 bytes
```

### GCloud SDK Protocol (0x7575)
```
Offset  Size  Field           Description
------  ----  -----           -----------
0       2     Magic           Always 0x7575
2       2     Length          Payload length (big-endian)
4       2     Command         0x0016=GetConfig, 0x0017=ConfigResp
6       2     Flags           0x0000
8       2     SessionHi       0x00DE (session identifier)
10      6     Reserved        All zeros
16      1     StrLen          Length of next string
17      N     String          Length-prefixed string (app_id, openid, hash...)
...     ...   ...             More length-prefixed strings
```

### Voice QOS Protocol (0x7572)  
```
Magic: "ur" (0x7572)
303 bytes total
Contains device info, app_id, openid, SDK version, room_id
Used for voice quality telemetry
```

## Game Server IPs (Captured)

| Server | IP | Port | Role |
|--------|-----|------|------|
| Gateway | 20.41.230.28 | 9030 | Initial director/load balancer |
| Lobby 1 | 20.219.78.62 | 9031 | Primary lobby keepalive |
| Lobby 2 | 34.0.11.52 | 9031 | Secondary lobby keepalive |
| Lobby 3 | 20.204.17.153 | 9031 | Tertiary lobby keepalive |
| GCloud Config | 20.193.140.198 | 8700 | Voice configuration |
| Voice QOS | 4.187.187.9 | 8011 | Voice quality reporting |

All on Azure India (South India / Central India regions).

## Encryption

### HTTPS Layer
- Standard TLS 1.2/1.3 for SDK API and payment
- Certificate pinning in libsigner.so

### UDP Game Protocol
- The keepalive packets (22 bytes) appear UNENCRYPTED (or very simple XOR)
- The AA/BB padding suggests placeholder or encryption marker
- Actual game state packets (match) use **AES-128-GCM** (confirmed by PUBG PC research)
- Key exchange happens during match join via the session ticket

### Payment Encryption
- AES-ECB with key from `get_key` response
- Key is per-session, rotates on each `get_key` call
- encrypt_msg contains the actual game command in encrypted JSON

## Match Architecture (from PUBG/UE4 knowledge)

PUBG Mobile uses Unreal Engine 4's networking stack:
- **Lobby**: Lightweight UDP keepalive + command/response
- **Match**: Full UE4 replication with custom serialization
- **Matchmaking**: Request → Wait → Assigned (all through lobby connection)
- **Protocol**: Custom reliable UDP (NOT standard KCP/ENet - Tencent's own)

### Match Join Flow
```
1. MATCH_REQUEST (lobby cmd)
   → game_mode: 1=Classic, 2=Arcade, 3=Arena
   → perspective: 1=TPP, 2=FPP  
   → map: 0=Random, 1=Erangel, 2=Miramar, etc.
   → team_size: 1=Solo, 2=Duo, 4=Squad
   → region: 91=India

2. MATCH_QUEUED (lobby response)
   → queue_id, estimated_wait

3. MATCH_FOUND (lobby push)
   → match_id, server_addr, port, ticket
   → When enough players found

4. JOIN_MATCH (to match server)
   → ticket, player_info
   → Starts game session

5. GAME_STATE (continuous)
   → Player positions, zone, items, etc.
   → Encrypted with per-match AES key
```

## What's Needed for Auto-Play Bot

### Option 1: Protocol Implementation (HARD - 3-6 months)
- Reverse engineer the FULL lobby protocol (beyond keepalive)
- Implement match request/join
- Decode game state protobuf
- Simulate player inputs (movement, shooting)
- Handle anti-cheat detection

### Option 2: Emulator + ADB Automation (PRACTICAL - 1-2 weeks)
- Run BGMI in Android emulator (GameLoop/BlueStacks/LDPlayer)
- Use ADB for screen capture + tap injection
- Bot handles: queue, land, move, survive
- Computer vision for game state (minimap, health, zone)
- No protocol RE needed

### Option 3: Capture & Replay Hybrid (MEDIUM - 2-4 weeks)
- Capture ONE full match session (10-15 min)
- Extract the match start sequence
- Replay match join packets
- Bot only needs to join and AFK (for survival time rewards)
- Auto-disconnect after death

## For the BGMI Event Bot specifically:

The event reward claim system likely uses the LOBBY protocol (not match):
- Event commands are lobby-level (you claim from lobby, not in-match)
- The 22-byte keepalive is just keepalive
- Actual commands use LARGER packets with the same or different magic
- We need a capture WITH a reward claim to see the exact packet

## Key Discovery: GCloud Auth Hash

The auth_hash `bc24d88f33ec74868ce891999438af86` sent to the voice config server
is an MD5 but we couldn't crack it with simple combinations. It's likely:
- MD5(openid + gcloud_app_secret)
- Or a HMAC derived from the session
- Computed by the GCloud SDK native library
