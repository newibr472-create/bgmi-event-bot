use anyhow::{Context, Result};
use chrono::Utc;
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::protocol::{MatchConfig, MatchMode, Packet, PacketType, Perspective};
use crate::network::client::GameClient;

/// Minimum match duration to trigger time-based rewards (from event system analysis).
/// BGMI requires at least 5 minutes in a classic match for it to count.
const MIN_MATCH_DURATION_SEC: u64 = 300;

/// Additional random time to add to avoid detection patterns.
const JITTER_MAX_SEC: u64 = 120;

/// Time between position updates to appear "alive" in match.
const POSITION_UPDATE_INTERVAL_SEC: u64 = 10;

#[derive(Debug, Clone)]
pub struct MatchSimConfig {
    pub target_duration_sec: u64,
    pub mode: MatchMode,
    pub perspective: Perspective,
    pub map_id: u32,
    pub auto_exit: bool,
}

impl Default for MatchSimConfig {
    fn default() -> Self {
        Self {
            target_duration_sec: MIN_MATCH_DURATION_SEC + 60,
            mode: MatchMode::Classic,
            perspective: Perspective::TPP,
            map_id: 1, // Erangel
            auto_exit: true,
        }
    }
}

pub struct MatchSimulator {
    config: MatchSimConfig,
}

impl MatchSimulator {
    pub fn new(config: MatchSimConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(MatchSimConfig::default())
    }

    /// Execute a full match lifecycle: queue → join → idle → exit
    /// This accumulates match time for time-based event rewards.
    pub async fn run_match(&self, client: &mut GameClient) -> Result<MatchResult> {
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(0..JITTER_MAX_SEC);
        let total_duration = self.config.target_duration_sec + jitter;

        info!(
            "starting match simulation: {}s ({}s base + {}s jitter)",
            total_duration, self.config.target_duration_sec, jitter
        );

        // 1. send match join request
        let join_payload = self.build_join_payload();
        client
            .send_packet(&Packet::new(PacketType::MatchJoinRequest, join_payload))
            .await
            .context("failed to send join request")?;

        // 2. wait for match join confirmation
        let response = client
            .recv_packet()
            .await
            .context("no join response")?;

        if response.packet_type != PacketType::MatchJoinResponse {
            anyhow::bail!("unexpected response to join: {:?}", response.packet_type);
        }

        let join_data: serde_json::Value = serde_json::from_slice(&response.payload)?;
        let match_id = join_data
            .get("match_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        info!("joined match: {}", match_id);

        // 3. wait for match start signal
        loop {
            let pkt = client.recv_packet().await?;
            if pkt.packet_type == PacketType::MatchStart {
                debug!("match started");
                break;
            }
        }

        // 4. idle in match - send periodic position updates to maintain presence
        let start = tokio::time::Instant::now();
        let target = Duration::from_secs(total_duration);

        let spawn_pos = self.generate_idle_position(&mut rng);
        let mut ticks = 0u32;

        while start.elapsed() < target {
            sleep(Duration::from_secs(POSITION_UPDATE_INTERVAL_SEC)).await;
            ticks += 1;

            // send minimal movement telemetry so server doesn't flag as AFK disconnect
            let pos_update = self.build_position_update(&spawn_pos, ticks);
            if let Err(e) = client.send_packet(&Packet::new(PacketType::TelemetryReport, pos_update)).await {
                warn!("failed to send position update: {}", e);
                break;
            }

            // check for any incoming packets (match end, kick, etc.)
            if let Ok(Some(pkt)) = client.try_recv_packet().await {
                match pkt.packet_type {
                    PacketType::MatchEnd => {
                        info!("match ended by server (zone kill or game over)");
                        break;
                    }
                    PacketType::KickNotice => {
                        warn!("kicked from match");
                        return Ok(MatchResult {
                            match_id,
                            duration_sec: start.elapsed().as_secs(),
                            counted_for_rewards: false,
                            exit_reason: ExitReason::Kicked,
                        });
                    }
                    _ => {}
                }
            }

            if ticks % 6 == 0 {
                debug!(
                    "match progress: {}/{}s",
                    start.elapsed().as_secs(),
                    total_duration
                );
            }
        }

        // 5. graceful exit
        let exit_payload = serde_json::json!({
            "match_id": match_id,
            "reason": "leave",
            "timestamp": Utc::now().timestamp(),
        });
        client
            .send_packet(&Packet::new(
                PacketType::MatchLeave,
                serde_json::to_vec(&exit_payload)?,
            ))
            .await?;

        let duration = start.elapsed().as_secs();
        let counted = duration >= MIN_MATCH_DURATION_SEC;

        info!(
            "match {} completed: {}s, counted={}",
            match_id, duration, counted
        );

        Ok(MatchResult {
            match_id,
            duration_sec: duration,
            counted_for_rewards: counted,
            exit_reason: ExitReason::Normal,
        })
    }

    fn build_join_payload(&self) -> Vec<u8> {
        let config = MatchConfig {
            map_id: self.config.map_id,
            mode: self.config.mode,
            perspective: self.config.perspective,
            max_players: 100,
            bot_fill: true,
        };

        let payload = serde_json::json!({
            "config": config,
            "squad_type": "solo",
            "timestamp": Utc::now().timestamp(),
        });

        serde_json::to_vec(&payload).unwrap_or_default()
    }

    fn generate_idle_position(&self, rng: &mut impl Rng) -> Position {
        // pick a spot far from the flight path - reduces chance of encounters
        Position {
            x: rng.gen_range(1000.0..7000.0),
            y: rng.gen_range(1000.0..7000.0),
            z: 0.0, // ground level
        }
    }

    fn build_position_update(&self, base_pos: &Position, tick: u32) -> Vec<u8> {
        // minimal drift to appear alive but not moving suspiciously
        let drift = (tick as f32 * 0.01).sin() * 2.0;
        let payload = serde_json::json!({
            "type": "pos",
            "x": base_pos.x + drift,
            "y": base_pos.y + drift * 0.5,
            "z": base_pos.z,
            "yaw": 0.0,
            "state": "idle",
            "ts": Utc::now().timestamp_millis(),
        });
        serde_json::to_vec(&payload).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub match_id: String,
    pub duration_sec: u64,
    pub counted_for_rewards: bool,
    pub exit_reason: ExitReason,
}

#[derive(Debug, Clone, Copy)]
pub enum ExitReason {
    Normal,
    Killed,
    Kicked,
    Disconnected,
    ServerEnd,
}
