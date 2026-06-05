use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::crypto::decrypt_token_payload;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub open_id: String,
    pub auth_token: String,
    pub refresh_token: Option<String>,
    pub region: ServerRegion,
    pub level: u32,
    pub last_login: Option<DateTime<Utc>>,
    pub session_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ServerRegion {
    India,
    Korea,
    Global,
}

impl ServerRegion {
    pub fn gateway_host(&self) -> &'static str {
        match self {
            Self::India => "bgmi-gateway.pubg.com",
            Self::Korea => "kr-gateway.pubg.com",
            Self::Global => "global-gateway.pubg.com",
        }
    }

    pub fn gateway_port(&self) -> u16 {
        17500
    }
}

#[derive(Clone)]
pub struct AccountManager {
    accounts: DashMap<String, Account>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            accounts: DashMap::new(),
        }
    }

    /// Import account using auth token extracted from the game client.
    /// Token format: base64(json{ open_id, token, ts, sig })
    pub fn import_from_token(&self, raw_token: &str) -> Result<Account> {
        let payload = decrypt_token_payload(raw_token)?;

        let open_id = payload
            .get("open_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing open_id in token"))?
            .to_string();

        let token = payload
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing token field"))?
            .to_string();

        // check for duplicate
        if self.accounts.iter().any(|e| e.value().open_id == open_id) {
            bail!("account {} already imported", open_id);
        }

        let account = Account {
            id: Uuid::new_v4().to_string(),
            display_name: format!("Player_{}", &open_id[..6]),
            open_id,
            auth_token: token,
            refresh_token: payload.get("refresh").and_then(|v| v.as_str()).map(String::from),
            region: ServerRegion::India,
            level: 0,
            last_login: None,
            session_active: false,
            created_at: Utc::now(),
        };

        self.accounts.insert(account.id.clone(), account.clone());
        Ok(account)
    }

    pub fn remove(&self, account_id: &str) -> Option<Account> {
        self.accounts.remove(account_id).map(|(_, v)| v)
    }

    pub fn get(&self, account_id: &str) -> Option<Account> {
        self.accounts.get(account_id).map(|r| r.value().clone())
    }

    pub fn list(&self) -> Vec<Account> {
        self.accounts.iter().map(|r| r.value().clone()).collect()
    }

    pub fn update_session_status(&self, account_id: &str, active: bool) {
        if let Some(mut entry) = self.accounts.get_mut(account_id) {
            entry.session_active = active;
            if active {
                entry.last_login = Some(Utc::now());
            }
        }
    }

    pub fn count(&self) -> usize {
        self.accounts.len()
    }

    /// Persist accounts to disk (encrypted)
    pub fn save_to_disk(&self, path: &std::path::Path) -> Result<()> {
        let accounts: Vec<Account> = self.list();
        let json = serde_json::to_string_pretty(&accounts)?;
        // TODO: encrypt with local machine key before writing
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load accounts from disk
    pub fn load_from_disk(&self, path: &std::path::Path) -> Result<usize> {
        if !path.exists() {
            return Ok(0);
        }
        let data = std::fs::read_to_string(path)?;
        let accounts: Vec<Account> = serde_json::from_str(&data)?;
        let count = accounts.len();
        for acc in accounts {
            self.accounts.insert(acc.id.clone(), acc);
        }
        Ok(count)
    }
}
