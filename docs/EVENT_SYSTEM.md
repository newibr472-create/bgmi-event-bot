# BGMI Event System Analysis

Documentation of how the event and reward system works in BGMI, based on client analysis and network observation.

## Event Categories

### 1. Daily Login Rewards (ID: 1001)
- **Trigger**: First connection of the day (server timezone: UTC+5:30)
- **Reward**: Escalating daily rewards (silver, crates, fragments)
- **Claim**: Automatic on login, but requires explicit claim packet for bonus milestones (7-day, 14-day, 28-day)
- **Reset**: Daily at 00:00 IST

### 2. Weekly Playtime (ID: 1010)
- **Trigger**: Accumulated match time reaches thresholds
- **Thresholds**: 30min, 60min, 120min, 300min per week
- **Reward**: Silver (BP), crate coupons, outfit fragments
- **Key insight**: Only time in actual matches counts. Lobby time doesn't count.
- **Minimum match duration**: 5 minutes for classic, 3 minutes for TDM/arena

### 3. Match Count Rewards (ID: 1020)
- **Trigger**: Complete N matches in a period
- **Thresholds**: 1, 3, 5, 10, 20 matches
- **Reward**: Silver, supply crate
- **Note**: Match must end normally (leave after 5+ min counts, disconnect doesn't)

### 4. Free Popularity Gifts (ID: 2001)
- **Trigger**: Manual claim, once per day per recipient
- **Mechanism**: Send free popularity to another player (1 point)
- **Limit**: 10 free gifts per day per sender
- **Strategy**: Bot accounts send popularity to each other (mutual)
- **Packet type**: 0x0051 (PopularityClaim) with `gift_type: 1`

### 5. Mutual Popularity (ID: 2002)
- **Trigger**: Both players exchange popularity within 24h
- **Reward**: Bonus popularity points for both
- **Strategy**: Coordinate between bot accounts

### 6. Season Pass Free Track (ID: 3001)
- **Trigger**: RP points from missions and match XP
- **Reward**: Free track items at specific RP levels
- **Claim**: Requires explicit claim at each level
- **Note**: Only free track; paid track requires UC purchase

### 7. Time-Limited Events (ID: 4000+)
- **Trigger**: Varies per event (login, match, social actions)
- **Duration**: Usually 7-14 days
- **Reward**: Limited cosmetics, crates, silver
- **Discovery**: Query EventList (0x0040) to find active events
- **Rotation**: New events every 1-2 weeks, tied to game updates

### 8. Achievement Rewards (ID: 5000+)
- **Trigger**: One-time achievements (first win, first 10 kills, etc.)
- **Reward**: Title, silver, frame
- **Note**: Most already claimed on established accounts

### 9. Recall Events (ID: 6001)
- **Trigger**: Return after inactivity period (7+ days)
- **Reward**: Returning player bonuses
- **Strategy**: Rotate accounts with forced inactivity windows

### 10. Share Rewards (ID: 7001)
- **Trigger**: Share result screen to social media
- **Reward**: Small silver bonus
- **Implementation**: Server just needs the share_complete packet; actual sharing not verified

## Collection Flow

```
┌─────────────────┐
│  Query Events   │ ──── PacketType::EventList (0x0040)
│  (on login)     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Parse Response │ ──── PacketType::EventDetail (0x0041)
│  (event list)   │      Contains: event_id, requirements, progress, rewards
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌──────────────────┐
│ Check Progress  │────▶│  progress >= 1.0  │──── YES ───┐
│                 │     │  and !claimed     │             │
└─────────────────┘     └──────────────────┘             │
                                │                         │
                              NO                          ▼
                                │              ┌─────────────────────┐
                                ▼              │  Send Claim Packet  │
                        ┌───────────────┐      │  (0x0042)           │
                        │ Farm Progress │      └──────────┬──────────┘
                        │ (match sim,   │                 │
                        │  wait, etc)   │                 ▼
                        └───────────────┘      ┌─────────────────────┐
                                               │  Verify Response    │
                                               │  (0x0043)           │
                                               └─────────────────────┘
```

## Claim Request Format

```json
{
  "event_id": 1010,
  "sub_id": 2,           // optional: sub-tier within event
  "timestamp": 1703275200,
  "nonce": "uuid-v4"     // replay protection
}
```

## Claim Response Format

```json
{
  "code": 0,              // 0 = success
  "event_id": 1010,
  "rewards": [
    {
      "reward_id": 50023,
      "item_type": "silver",
      "amount": 500,
      "name": "500 BP"
    }
  ],
  "next_claim_at": null   // null if fully claimed, timestamp if repeating
}
```

## Error Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1001 | Event not found / expired |
| 1002 | Requirements not met |
| 1003 | Already claimed |
| 1004 | Rate limited (try again later) |
| 1005 | Invalid nonce |
| 2001 | Account restricted |
| 9999 | Server error |

## Popularity System Details

### Gift Types
- Type 1: Free daily gift (1 popularity point)
- Type 2: Paid gift - Like (5 points, costs 10 UC)
- Type 3: Paid gift - Love (20 points, costs 50 UC)
- Type 4: Paid gift - Superstar (100 points, costs 200 UC)

### Free Gift Strategy
Each account can send 10 free gifts per day. With N accounts:
- Total free popularity per account per day: `(N-1) * 1` points received
- Total sends per account per day: min(10, N-1)
- Optimal: 11 accounts = 10 popularity/day each

### Popularity Claim Packet
```json
{
  "event_id": 2001,
  "target_open_id": "receiver_open_id",
  "gift_type": 1,
  "count": 1,
  "timestamp": 1703275200
}
```

## Timing Considerations

### Server Reset Times
- Daily reset: 00:00 IST (18:30 UTC previous day)
- Weekly reset: Monday 00:00 IST
- Season reset: ~every 8 weeks (announced in-game)

### Optimal Bot Schedule
```
00:01 IST  - Daily login claim (all accounts)
00:05 IST  - Popularity exchange between accounts
00:10 IST  - Start match simulation (for time-based)
06:00 IST  - Check for new time-limited events
12:00 IST  - Mid-day event check, claim anything newly available
18:00 IST  - Evening match sim if weekly time not met
23:00 IST  - Final claim sweep, prepare for reset
```

### Match Duration Requirements
| Mode | Minimum for credit | Recommended idle |
|------|-------------------|------------------|
| Classic (Erangel/Miramar) | 5 min | 7-10 min |
| Classic (Livik/Sanhok) | 5 min | 6-8 min |
| TDM | 3 min | 4-5 min |
| Arena | 3 min | 4-5 min |

## Anti-Detection Notes

### Behavioral Red Flags
- Claiming all events within seconds of login
- Always exact same match duration
- Multiple accounts from same IP claiming simultaneously
- Zero kills + zero damage in every match
- Login-claim-logout pattern with no actual gameplay

### Mitigations
- Add random delays (30s-300s) between claim operations
- Vary match duration with jitter (±2 min)
- Stagger account logins (30s between each)
- Occasionally vary match modes
- Send minimal telemetry to appear as a real (bad) player
- Use different IPs/proxies per account if possible

## Season Pass (RP) Free Track

The free track of the Royale Pass provides rewards at specific RP levels. RP is earned through:
- Daily missions (auto-complete with match play)
- Weekly missions (specific objectives)
- Match XP (based on duration and performance)

For bot purposes, daily match play naturally accumulates RP. The bot should:
1. Check current RP level
2. Claim any available free track rewards
3. Continue accumulating through idle play

Free track notable rewards typically include:
- RP 5: Silver crate
- RP 10: Outfit piece
- RP 15: Silver
- RP 20: Emote or spray
- RP 30: Outfit set
- RP 40+: Frames, titles
