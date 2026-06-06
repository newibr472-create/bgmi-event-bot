# BGMI Protocol - Final Analysis Results

## Confirmed Working (LIVE tested against real servers)

| Step | Endpoint | Status |
|------|----------|--------|
| Login | `in-sdkapi.globh.com/v1.0/user/login` | ✅ Working |
| Get Ticket | `in-sdkapi.globh.com/v1.0/user/getTicket` | ✅ Working |
| Payment Session | `min-pay.globh.com/v1/r/1450025957/mobile_overseas_common` | ✅ Working |
| Telemetry | `min-pay.globh.com/cgi-bin/log_data.fcg` | ✅ Working |

## Confirmed NOT Working for Event Rewards

| What | Why |
|------|-----|
| `in-sdkapi.globh.com/v1.0/event/*` | Returns "mod not exists!" — module doesn't exist |
| `in-sdkapi.globh.com/v1.0/reward/*` | Returns "mod not exists!" |
| `min-pay.globh.com` `overseas_cmd` variations | All return empty {} — only `get_key\|get_ip` has data |
| `h5.battlegroundsmobileindia.com` | Static S3 assets only (images/banners) |
| Other game hosts (in-gameapi, in-lobby, etc.) | Don't exist (connection refused) |

## Where Event Rewards Actually Live

**Event reward claims go through the TGCP binary game protocol on UDP port 9031.**

Evidence:
1. No HTTP API endpoint handles event claims (all probed, none work)
2. The game has a `7575` magic header binary protocol (found in packet 111 - GCloud SDK)
3. The APK contains `IMSDKGameServiceManager.unlockAchieve()` which routes through TGCP
4. The UDP capture shows game servers on port 9031 with binary communication

## What We Need to Complete the Bot

1. **Capture the actual claim action** — HttpCanary must be running when you tap "Claim" on a reward
2. The claim will likely appear as a larger UDP packet (>22 bytes) on port 9031
3. OR it may go through an internal TCP connection that HttpCanary labels differently

## sValidKey Analysis

- Algorithm: Unknown (embedded in `libsigner.so` native library)
- Workaround: Replay captured signed params — server doesn't validate timestamp freshness
- Impact: Works for the captured account, new accounts need their own capture

## UDP Protocol Format

### Keepalive (22 bytes):
```
[74 AC] [00] [seq:1B] [timestamp:3B] [d0] [session:5B] [C3 DE 08] [AAAAAAAA] [BBBBBBBB]
```

### GCloud SDK (variable length, found on port 8700):
```
[75 75] [length:2B big-endian] [cmd:2B] [flags:8B] [length-prefixed strings...]
```
- cmd 0x0016 = config request
- cmd 0x0017 = config response
- Strings: [length:1B] [utf8 data]
- Contains: app_id ("1375135419"), openid, auth hash

## Architecture Decision

For a working event reward bot, two approaches:

### Approach A: Full TGCP Protocol (Hard but Complete)
- Reverse engineer the binary protocol on port 9031
- Implement packet framing, encryption, session management
- Can claim any reward programmatically

### Approach B: Capture-and-Replay (Easier but Limited)  
- User captures ONE reward claim session manually
- Bot replays the exact binary sequence
- Limited to rewards that use the same packet structure

### Approach C: Automation (Most Practical)
- Use Android emulator + adb to automate UI clicks
- Bot handles login via HTTP, then controls emulator for claims
- Most reliable, doesn't need protocol RE
