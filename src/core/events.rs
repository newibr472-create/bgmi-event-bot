use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::protocol::{Packet, PacketType};

/// Known event IDs from BGMI decompilation (com.pubg.imobile)
/// These rotate seasonally but the underlying system stays the same.
pub mod event_ids {
    pub const DAILY_LOGIN: u32 = 1001;
    pub const WEEKLY_PLAYTIME: u32 = 1010;
    pub const MATCH_COUNT_REWARD: u32 = 1020;
    pub const POPULARITY_FREE_GIFT: u32 = 2001;
    pub const POPULARITY_MUTUAL: u32 = 2002;
    pub const SEASON_PASS_FREE: u32 = 3001;
    pub const TIME_LIMITED_EVENT: u32 = 4000; // base, actual = 4000 + event_index
    pub const ACHIEVEMENT_UNLOCK: u32 = 5000;
    pub const RECALL_EVENT: u32 = 6001;
    pub const SHARE_REWARD: u32 = 7001;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEvent {
    pub event_id: u32,
    pub name: String,
    pub event_type: EventType,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub rewards: Vec<Reward>,
    pub requirements: Vec<Requirement>,
    pub claimed: bool,
    pub progress: f32, // 0.0 - 1.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    DailyLogin,
    TimeBased,       // spend X minutes in match
    MatchCount,      // play N matches
    Popularity,      // free popularity exchange
    Achievement,     // one-time unlock
    SeasonPass,      // RP rewards
    TimeLimited,     // rotating events
    Social,          // share/recall
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    pub reward_id: u32,
    pub item_type: RewardItemType,
    pub amount: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RewardItemType {
    Silver,          // BP (battle points)
    UC,              // premium currency (rarely free)
    Crate,           // supply crate
    Fragment,        // outfit fragment
    Popularity,      // popularity points
    RoomCard,        // room card
    RP,              // royale pass points
    Outfit,          // cosmetic
    Emote,           // emote
    Coupon,          // discount coupon
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub req_type: RequirementType,
    pub target_value: u32,
    pub current_value: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RequirementType {
    MatchesPlayed,
    MinutesInMatch,
    DaysLoggedIn,
    PopularityGiven,
    KillCount,
    Top10Finishes,
    ShareCount,
}

#[derive(Clone)]
pub struct EventScheduler {
    active_events: Arc<RwLock<Vec<GameEvent>>>,
    claim_queue: Arc<RwLock<Vec<ClaimTask>>>,
}

#[derive(Debug, Clone)]
struct ClaimTask {
    account_id: String,
    event_id: u32,
    scheduled_at: DateTime<Utc>,
}

impl EventScheduler {
    pub fn new() -> Self {
        Self {
            active_events: Arc::new(RwLock::new(Vec::new())),
            claim_queue: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Main scheduler loop - checks for claimable events periodically
    pub async fn run_loop(&self) -> Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            self.process_claim_queue().await;
        }
    }

    /// Query active events from server
    pub fn build_event_list_packet() -> Packet {
        Packet::new(PacketType::EventList, vec![])
    }

    /// Parse event list from server response
    pub fn parse_event_list(payload: &[u8]) -> Result<Vec<GameEvent>> {
        let events: Vec<GameEvent> = serde_json::from_slice(payload)?;
        Ok(events)
    }

    /// Check which events are ready to claim
    pub async fn get_claimable_events(&self) -> Vec<GameEvent> {
        let events = self.active_events.read().await;
        events
            .iter()
            .filter(|e| !e.claimed && e.progress >= 1.0 && Utc::now() < e.end_time)
            .cloned()
            .collect()
    }

    /// Build claim request packet for a specific event
    pub fn build_claim_packet(event_id: u32) -> Packet {
        let payload = serde_json::json!({
            "event_id": event_id,
            "timestamp": Utc::now().timestamp(),
        });
        Packet::new(
            PacketType::EventClaimRequest,
            serde_json::to_vec(&payload).unwrap_or_default(),
        )
    }

    /// Build popularity claim packet (free daily gift)
    pub fn build_popularity_claim(target_open_id: &str) -> Packet {
        let payload = serde_json::json!({
            "event_id": event_ids::POPULARITY_FREE_GIFT,
            "target_open_id": target_open_id,
            "gift_type": 1, // free type
            "timestamp": Utc::now().timestamp(),
        });
        Packet::new(
            PacketType::PopularityClaim,
            serde_json::to_vec(&payload).unwrap_or_default(),
        )
    }

    /// Schedule a claim for the future (e.g., when time requirement will be met)
    pub async fn schedule_claim(&self, account_id: &str, event_id: u32, at: DateTime<Utc>) {
        let mut queue = self.claim_queue.write().await;
        queue.push(ClaimTask {
            account_id: account_id.to_string(),
            event_id,
            scheduled_at: at,
        });
        info!("scheduled claim for event {} at {}", event_id, at);
    }

    /// Process pending claims that are now due
    async fn process_claim_queue(&self) {
        let now = Utc::now();
        let mut queue = self.claim_queue.write().await;

        let (ready, pending): (Vec<_>, Vec<_>) =
            queue.drain(..).partition(|t| t.scheduled_at <= now);

        *queue = pending;

        for task in ready {
            debug!(
                "executing scheduled claim: event {} for account {}",
                task.event_id, task.account_id
            );
            // the actual send happens through the session associated with the account
            // this just signals readiness - the session loop picks it up
        }
    }

    /// Update local event state from server push
    pub async fn update_events(&self, events: Vec<GameEvent>) {
        let mut current = self.active_events.write().await;
        *current = events;
    }

    /// Calculate time remaining until a time-based event can be claimed
    pub fn time_until_claimable(event: &GameEvent) -> Option<Duration> {
        if event.progress >= 1.0 {
            return Some(Duration::zero());
        }
        // estimate based on progress rate
        for req in &event.requirements {
            if req.req_type as u8 == RequirementType::MinutesInMatch as u8 {
                let remaining_minutes = req.target_value.saturating_sub(req.current_value);
                return Some(Duration::minutes(remaining_minutes as i64));
            }
        }
        None
    }
}
