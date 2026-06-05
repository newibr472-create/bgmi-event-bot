# BGMI APK Deep Analysis Report
## From: bg.apk (com.pubg.imobile) - BGMI India

Generated from jadx decompilation + native library string analysis.

---

## 1. Architecture Overview

BGMI is built on:
- **Unreal Engine 4** (libUE4.so - main game binary, ~500MB)
- **ITOP SDK** (com.itop.imsdk) - Tencent's gaming platform SDK for auth/events/services
- **TGCP** (Tencent Game Communication Protocol) - Custom TCP networking layer
- **Apollo** - Tencent's game messaging framework on top of TGCP
- **Pebble** - Service discovery/address resolution
- **MMKV** (libmeemo_mmkv.so) - Local key-value storage
- **HDmpve** (libhdmpve.so/libhdmpvecore.so) - Telemetry/upload system
- **TBlueData** (libTBlueData.so) - TDM telemetry
- **Anogs/ACE** (libanogs.so/libanort.so) - Anti-cheat
- **libsigner.so** - Request signing library

## 2. Network Protocol

### Transport Layer
- **TCP ONLY** (confirmed: "Tcp does not support recv udp" / "Tcp does not support send udp")
- Uses **TLS** (Client Hello → Key Exchange → Finished → Encrypted)
- Cipher: **AES256-GCM-SHA384** (TLS 1.3)
- HTTPDNS for DNS resolution (anti-intercept)

### TGCP Protocol
```
tgcpapi_init unsupport V1 AuthType:%d, iVersion:%d
tgcpapi_check_connect
tgcpapi_start success:%s
tgcpapi_get_account unsupported account type:%d
tgcpapi_get_openid unsupported account type:%d
CTGcp::OnThreadProc checkTimeOut. url:%s
```

### Message Structure (from libUE4.so strings)
Fields found in protocol messages:
- `msgType` - Message type identifier
- `channelId` / `channelID` - Channel identifier
- `openId` / `openID` / `openid` - Player identifier (3 casing variants used)
- `sessionId` - Session identifier
- `serviceName` - Service name for RPC

### Packet System (UE4 Channels)
- `ChannelUsage`, `ChannelWeights`, `Channels`
- `ChannelsToCast`, `ChannelsToTick`, `ChannelsUsed`
- `SendPacketCount`, `SendPacketPool`
- `ServerAddress`

## 3. Authentication (ITOP SDK)

### SDK Structure
```
com/itop/imsdk/android/base/
├── auth/IMSDKAuthManager.java       ← Auth manager
├── login/                            ← Login flow (JSON-based)
├── config/                           ← SDK configuration
├── gameservice/IMSDKGameServiceManager.java ← Events/Rewards!
├── notice/imsdk/IMSDKNotice.java     ← Notifications
├── push/                             ← Push notifications
├── relation/IMSDKFriendBase.java     ← Social/Friends
└── stat/                             ← Analytics
```

### Login Flow
1. ITOP SDK handles initial auth (Facebook/Google/Guest login)
2. Returns `openId` + auth token
3. Token passed to native layer via JNI
4. Native layer establishes TGCP connection with `openId`
5. Session ID established

### Key Methods
- `dealUnifiedAccount` - Unified account processing
- `IMSDKLoginResult(retCode, thirdRetCode, message)` - Login result callback
- Error code 9999 = JSON parse exception

## 4. Game Service API (Events/Rewards)

### IMSDKGameServiceManager
```java
public abstract void showAchievement(IMSDKResultListener, Object...);
public abstract void showLeaderBoard(String, IMSDKResultListener, Object...);
public abstract void unlockAchieve(String, IMSDKResultListener, Object...);
```

### Event IDs (from libUE4.so)
```
EVENTID_DOGTAGID_ITEM_ADD          - Dogtag item reward
EVENTID_ENERGY_REGENERATION_EVENT  - Energy regeneration
EVENTID_FETCH_CLIENT_VERSION       - Version check
EVENTID_FIND_ENEMY_WARNING         - Enemy spotted
EVENTID_FINISH_SOCIAL_ISLAND_INTERACT - Social island completion
EVENTID_GAMETYPE_CHANGED           - Game mode change
EVENTID_GAME_MODE_INIT             - Mode initialization
EVENTID_GAME_MODE_STATE_CHANGE     - Mode state change
EVENTID_GAME_STATE_WEATHER_CHANGE  - Weather change
```

## 5. Server Infrastructure

### Confirmed Endpoints
- `https://share.globh.com/` - Sharing service
- Port `:443` - HTTPS/TLS
- BGMI data path: `/sdcard/Android/data/com.pubg.imobile/files/`

### Server Types (inferred from strings)
- `VoiceServerURLMap` - Voice chat servers
- `battle_voice_server_url` - In-battle voice
- `ServerAddress` - Game servers
- `bStationaryEndpoints` - Static/lobby endpoints

### Build Origin
```
/Users/intl/devops/PUBGM/TDMWorkspace/tdm/Project/TDM/
Source/Task/Timer/TDMThreadTimer.cpp
Source/System/TSystem.h
Source/TBlueDataCommon.h
```
Built by Tencent "intl" (international) devops team.

## 6. Native Libraries

| Library | Purpose | Size |
|---------|---------|------|
| libUE4.so | Main game engine + all game logic | ~500MB |
| libanogs.so | Anti-cheat (ACE/Anogs) | Medium |
| libanort.so | Anti-cheat component | Small |
| libhdmpve.so | Telemetry/upload | Small |
| libhdmpvecore.so | Telemetry core | Small |
| libTBlueData.so | TDM telemetry | Small |
| libsigner.so | Request signing | Small |
| libmeemo_mmkv.so | Key-value storage | Small |
| libRoosterPlugin.so | Anti-cheat scheduling | Small |
| libkk-image.so | Image processing | Small |
| libswappy.so | Frame pacing | Small |

## 7. What We Need from HTTP Canary Capture

To build the bot, we need to capture these specific requests:

1. **Login/Auth Request** → To `gp-sea-game.battlegroundsmobileindia.com` or similar
   - Headers: Authorization token, device info
   - Response: openId, sessionId, token

2. **Event List Request** → HTTP GET/POST to event service
   - Active events, rewards available, progress

3. **Event Claim Request** → HTTP POST to claim rewards
   - Event ID, reward ID, player verification

4. **Heartbeat/Keep-alive** → Periodic ping to maintain session

5. **Match Entry/Exit** → For time-based reward triggers

## 8. Bot Strategy (Based on Analysis)

### Approach 1: HTTP API Replay (Easier, Recommended)
- Capture HTTP REST API calls from HttpCanary
- Replay them with captured auth tokens
- Events/rewards are likely REST API (not game protocol)
- No need to implement full TGCP

### Approach 2: Full Protocol (Harder)
- Implement TGCP handshake
- Handle UE4 channel replication
- Would allow match simulation
- Much more complex, more detectable

### Recommended: Start with Approach 1
1. Get auth token from capture
2. Find event API endpoints
3. Replay claim requests
4. Only implement TGCP if needed for match-based rewards

---

## Status: Waiting for HTTP Canary capture data
