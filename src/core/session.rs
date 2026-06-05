use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, warn};

use crate::core::account::{Account, AccountStatus};
use crate::core::events::{CollectionResult, EventCollector};
use crate::network::client::BgmiClient;

/// Manages the lifecycle of a single account session.
/// Handles periodic collection, cooldowns, and retry logic.
pub struct SessionManager {
    accounts: Arc<RwLock<Vec<Account>>>,
    results: Arc<RwLock<Vec<(String, CollectionResult)>>>,
    running: Arc<RwLock<bool>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(Vec::new())),
            results: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn add_account(&self, account: Account) {
        self.accounts.write().await.push(account);
    }

    pub async fn remove_account(&self, id: &str) -> bool {
        let mut accounts = self.accounts.write().await;
        if let Some(pos) = accounts.iter().position(|a| a.id == id) {
            accounts.remove(pos);
            true
        } else {
            false
        }
    }

    pub async fn get_accounts(&self) -> Vec<Account> {
        self.accounts.read().await.clone()
    }

    pub async fn get_results(&self) -> Vec<(String, CollectionResult)> {
        self.results.read().await.clone()
    }

    /// Run collection for a single account
    pub async fn collect_for_account(&self, account_id: &str) -> Result<Vec<CollectionResult>> {
        let account = {
            let accounts = self.accounts.read().await;
            accounts
                .iter()
                .find(|a| a.id == account_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("account not found"))?
        };

        // update status
        self.update_account_status(&account.id, AccountStatus::Collecting)
            .await;

        let client = BgmiClient::new(
            &account.device.device_id,
            &account.device.model,
            &account.device.brand,
        )?;

        let mut collector = EventCollector::new(client, account.clone());

        match collector.run_full_collection().await {
            Ok(results) => {
                // store results
                let mut all_results = self.results.write().await;
                for r in &results {
                    all_results.push((account.id.clone(), r.clone()));
                }

                self.update_account_status(&account.id, AccountStatus::Cooldown)
                    .await;
                Ok(results)
            }
            Err(e) => {
                error!("collection failed for {}: {}", account.label, e);
                self.update_account_status(&account.id, AccountStatus::Error)
                    .await;
                Err(e)
            }
        }
    }

    /// Run collection for all accounts sequentially with delays
    pub async fn collect_all(&self) -> Vec<(String, Vec<CollectionResult>)> {
        let accounts = self.accounts.read().await.clone();
        let mut all_results = Vec::new();

        for account in &accounts {
            if account.status == AccountStatus::Banned {
                continue;
            }

            match self.collect_for_account(&account.id).await {
                Ok(results) => {
                    all_results.push((account.id.clone(), results));
                }
                Err(e) => {
                    warn!("skipping account {}: {}", account.label, e);
                }
            }

            // delay between accounts to avoid rate limiting
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        all_results
    }

    /// Start periodic collection loop
    pub async fn start_periodic(&self, interval_mins: u64) {
        let mut running = self.running.write().await;
        if *running {
            return;
        }
        *running = true;
        drop(running);

        let accounts = self.accounts.clone();
        let results = self.results.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_mins * 60));

            loop {
                ticker.tick().await;

                if !*running.read().await {
                    break;
                }

                let accs = accounts.read().await.clone();
                for account in &accs {
                    if account.status == AccountStatus::Banned {
                        continue;
                    }

                    let client = match BgmiClient::new(
                        &account.device.device_id,
                        &account.device.model,
                        &account.device.brand,
                    ) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("failed to create client: {}", e);
                            continue;
                        }
                    };

                    let mut collector = EventCollector::new(client, account.clone());
                    match collector.run_full_collection().await {
                        Ok(res) => {
                            let mut all = results.write().await;
                            for r in res {
                                all.push((account.id.clone(), r));
                            }
                        }
                        Err(e) => {
                            warn!("periodic collection failed for {}: {}", account.label, e);
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });
    }

    pub async fn stop_periodic(&self) {
        *self.running.write().await = false;
    }

    async fn update_account_status(&self, id: &str, status: AccountStatus) {
        let mut accounts = self.accounts.write().await;
        if let Some(acc) = accounts.iter_mut().find(|a| a.id == id) {
            acc.status = status;
        }
    }
}
