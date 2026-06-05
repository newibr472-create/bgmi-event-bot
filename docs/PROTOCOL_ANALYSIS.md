# BGMI Protocol Analysis - From Real Traffic Captures

## Overview

BGMI (Battlegrounds Mobile India) v4.4.0 uses a dual-protocol architecture:
- **HTTPS REST API** for authentication, events, payments, and configuration
- **UDP (TGCP)** for realtime match gameplay only (port 9030/9031)

Event reward collection operates entirely over HTTPS — no need to implement the UDP game protocol.

## API Infrastructure

| Host | Purpose | Protocol |
|------|---------|----------|
| `in-sdkapi.globh.com` | Authentication (ITOP SDK) | HTTPS GET |
| `in-notice.globh.com` | Push notifications/events | HTTPS GET |
| `min-pay.globh.com` | Payment, rewards, telemetry | HTTPS POST |
| `in-cloudctrl.globh.com` | Dynamic game config | HTTPS GET |
| `in-voiceconfig.globh.com` | Voice chat config (Thrift) | HTTPS POST |
| `in-f.globh.com` | Game asset CDN | HTTPS GET |
| `media.battlegroundsmobileindia.com` | Event banners/images | HTTPS GET |
| `h5.battlegroundsmobileindia.com` | H5 event page assets | HTTPS GET |

## Authentication Flow

### Step 1: Login (`/v1.0/user/login`)

```
GET https://in-sdkapi.globh.com/v1.0/user/login?
  did=<device_uuid>&
  dinfo=1|40455|<model>|en|4.4.0|<timestamp_ms>|2.625|2400*1080|<brand>&
  gameversion=4.4.0&
  iChannel=35&        # 35=Twitter, 28=Facebook, 4=Google
  iGameId=1450&
  iPlatform=2&        # 2=Android
  oauthToken=<twitter_oauth_token>&
  oauthTokenSecret=<twitter_oauth_secret>&
  package_name=com.pubg.imobile&
  sGuestId=<md5_device_fingerprint>&
  sOriginalId=<same_as_guest_id>&
  sValidKey=<md5_signature>&
  sdkversion=2.10.3
```

Response:
```json
{
  "code": 1,
  "desc": "SUCCESS",
  "iOpenid": "19112301001311658",
  "sInnerToken": "351cf6d5d921b0dcf25867ca04546e28",
  "iGuid": "26406653272674426",
  "iChannel": 35,
  "sUserName": "jone sins",
  "iExpireTime": 1783277379
}
```

### Step 2: Get Ticket (`/v1.0/user/getTicket`)

Uses `iOpenid` and `sInnerToken` from login response.

Response:
```json
{
  "code": 1,
  "desc": "SUCCESS",
  "sTicket": "edywJpz_SrWe5suuZXJ..."
}
```

The ticket is a signed blob containing: `{"sInnerToken":"...","iOpenid":...,"iGameId":1450,"iCTime":...,"sEnv":"release_id_igame"}`

### Step 3: Init Payment Session (`/v1/r/1450025957/mobile_overseas_common`)

```
POST https://min-pay.globh.com/v1/r/1450025957/mobile_overseas_common
Content-Type: application/x-www-form-urlencoded

encrypt_msg=<hex_encrypted>&
openid=19112301001311658&
format=json&
offer_id=1450025957&
session_token=<uuid>&
pf=IEG_iTOP-2001-android-2011-TW-1450-<openid>-igame&
overseas_cmd=get_key|get_ip&
get_key_type=secret&
key_len=newkey
```

Response returns encryption `key_info` for subsequent encrypted communications.

## sValidKey Signature

Each SDK API call requires `sValidKey` = MD5 of sorted parameters + SDK secret key.

Formula: `md5(sorted_params_string + sdk_key)`
- Sort all params alphabetically (excluding `sValidKey` and `sRefer`)
- Concatenate as `key1=value1&key2=value2&...`
- Append the SDK signing key
- MD5 hash the result

## Payment Encryption

Messages to `min-pay.globh.com` use:
- `encrypt_msg`: AES-128-ECB encrypted payload (hex encoded)
- Key: first 16 bytes of the `key_info` returned by `get_key` command
- Padding: PKCS7

## UDP Game Protocol (Not Needed for Events)

UDP packets on port 9030/9031 are all 22-byte TGCP keepalive pings:
```
[0x2A][seq][0x00][timestamp:4][hash:6][flags:1][0xAAAAAAAA][0xBBBBBBBB]
```

Server echoes them back unchanged. Real game data within UDP requires full UE4 protocol reimplementation.

## Device Fingerprint

```
dinfo format: "1|40455|<model>|<language>|<game_version>|<timestamp_ms>|<screen_density>|<resolution>|<brand>"
Example: "1|40455|I2405|en|4.4.0|1780685377880|2.625|2400*1080|iQOO"
```

## Telemetry

The app sends telemetry to `min-pay.globh.com/cgi-bin/log_data.fcg` with gzipped event records.
Important to replicate this for anti-detection.

## Captured Endpoints Summary

| # | Endpoint | Captured Data |
|---|----------|---------------|
| 138 | `/v1.0/user/login` | Full auth with Twitter OAuth |
| 135 | `/v1.0/user/getTicket` | Session ticket generation |
| 137 | `/v1.0/bind/bindRelationInfo` | Account binding info |
| 134 | `/v1.0/notice/getNotice` | Event notifications |
| 113 | `/v1/r/1450025957/mobile_overseas_common` | Payment session + key exchange |
| 82-83 | `/cgi-bin/log_data.fcg` | Telemetry/analytics |
| 145 | `/cfgpush/getConfig` | Dynamic config |
| 109 | `/` (voiceconfig) | Thrift-based voice config |
