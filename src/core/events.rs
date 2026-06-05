use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::core::account::Account;
use crate::network::client::BgmiClient;

/// Event types that can be collected - based on BGMI India event system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    DailyLogin,
    PopularityReward,
    ExtraReward,
    SeasonPass,
    AchievementUnlock,
    InviteReward,
    Custom(String),
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::DailyLogin => write!(f, "daily_login"),
            Self::PopularityReward => write!(f, "popularity"),
            Self::ExtraReward => write!(f, "extra_reward"),
            Self::SeasonPass => write!(f, "season_pass"),
            Self::AchievementUnlock => write!(f, "achievement"),
            Self::InviteReward => write!(f, "invite"),
            Self::Custom(name) => write!(f, "custom:{}", name),
        }
    }
}

/// Result of attempting to collect an event reward
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionResult {
    pub event_type: String,
    pub success: bool,
    pub message: String,
    pub reward_desc: Option<String>,
    pub timestamp: u64,
}

/// Main event collection orchestrator
pub struct EventCollector {
    client: BgmiClient,
    account: Account,
    results: Vec<CollectionResult>,
}

impl EventCollector {
    pub fn new(client: BgmiClient, account: Account) -> Self {
        Self {
            client,
            account,
            results: Vec::new(),
        }
    }

    /// Full collection flow: login -> init session -> collect all events
    pub async fn run_full_collection(&mut self) -> Result<Vec<CollectionResult>> {
        info!("starting collection for account: {}", self.account.label);

        // Step 1: Login
        let cred = self.account.credential.to_auth_credential();
        let guest_id = self.account.guest_id().to_string();

        self.client
            .login(&cred, &guest_id)
            .await
            .context("login failed")?;

        // small delay to mimic human behavior
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        // Step 2: Get ticket
        self.client
            .get_ticket(&guest_id)
            .await
            .context("get ticket failed")?;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Step 3: Init pay session (needed for reward claims)
        self.client
            .init_pay_session()
            .await
            .context("pay session init failed")?;

        // Step 4: Send initial telemetry (anti-detection)
        let _ = self.client.send_telemetry("sdk_login_success").await;

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Step 5: Collect events
        self.collect_daily_login().await;
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

        self.collect_popularity_reward().await;
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        self.collect_extra_rewards().await;

        info!(
            "collection complete: {} results, {} successful",
            self.results.len(),
            self.results.iter().filter(|r| r.success).count()
        );

        Ok(self.results.clone())
    }

    async fn collect_daily_login(&mut self) {
        debug!("attempting daily login reward");

        let payload = serde_json::json!({
            "cmd": "claim_daily_login",
            "openid": self.client.openid.as_deref().unwrap_or(""),
            "zoneid": 1,
            "plat": 2,
        });

        match self
            .client
            .send_pay_command("claim_reward", &payload.to_string())
            .await
        {
            Ok(resp) => {
                let ret = resp.get("ret").and_then(|v| v.as_i64()).unwrap_or(-1);
                let msg = resp
                    .get("msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                self.results.push(CollectionResult {
                    event_type: "daily_login".to_string(),
                    success: ret == 0,
                    message: msg.to_string(),
                    reward_desc: resp.get("reward").and_then(|v| v.as_str()).map(String::from),
                    timestamp: crate::core::protocol::current_ts_ms(),
                });
            }
            Err(e) => {
                warn!("daily login collection failed: {}", e);
                self.results.push(CollectionResult {
                    event_type: "daily_login".to_string(),
                    success: false,
                    message: e.to_string(),
                    reward_desc: None,
                    timestamp: crate::core::protocol::current_ts_ms(),
                });
            }
        }
    }

    async fn collect_popularity_reward(&mut self) {
        debug!("attempting popularity reward");

        let payload = serde_json::json!({
            "cmd": "claim_popularity",
            "openid": self.client.openid.as_deref().unwrap_or(""),
            "zoneid": 1,
            "plat": 2,
            "type": "popularity",
        });

        match self
            .client
            .send_pay_command("claim_reward", &payload.to_string())
            .await
        {
            Ok(resp) => {
                let ret = resp.get("ret").and_then(|v| v.as_i64()).unwrap_or(-1);
                let msg = resp
                    .get("msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                self.results.push(CollectionResult {
                    event_type: "popularity".to_string(),
                    success: ret == 0,
                    message: msg.to_string(),
                    reward_desc: resp.get("reward").and_then(|v| v.as_str()).map(String::from),
                    timestamp: crate::core::protocol::current_ts_ms(),
                });
            }
            Err(e) => {
                warn!("popularity reward failed: {}", e);
                self.results.push(CollectionResult {
                    event_type: "popularity".to_string(),
                    success: false,
                    message: e.to_string(),
                    reward_desc: None,
                    timestamp: crate::core::protocol::current_ts_ms(),
                });
            }
        }
    }

    async fn collect_extra_rewards(&mut self) {
        debug!("attempting extra rewards");

        let payload = serde_json::json!({
            "cmd": "claim_extra",
            "openid": self.client.openid.as_deref().unwrap_or(""),
            "zoneid": 1,
            "plat": 2,
            "type": "extra",
        });

        match self
            .client
            .send_pay_command("claim_reward", &payload.to_string())
            .await
        {
            Ok(resp) => {
                let ret = resp.get("ret").and_then(|v| v.as_i64()).unwrap_or(-1);
                let msg = resp
                    .get("msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                self.results.push(CollectionResult {
                    event_type: "extra_reward".to_string(),
                    success: ret == 0,
                    message: msg.to_string(),
                    reward_desc: resp.get("reward").and_then(|v| v.as_str()).map(String::from),
                    timestamp: crate::core::protocol::current_ts_ms(),
                });
            }
            Err(e) => {
                warn!("extra reward failed: {}", e);
                self.results.push(CollectionResult {
                    event_type: "extra_reward".to_string(),
                    success: false,
                    message: e.to_string(),
                    reward_desc: None,
                    timestamp: crate::core::protocol::current_ts_ms(),
                });
            }
        }
    }
}
