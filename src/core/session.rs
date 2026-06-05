use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};
use tracing::{debug, error, info, warn};

use super::account::{Account, ServerRegion};
use super::protocol::{Packet, PacketType};
use crate::network::client::GameClient;

const HEARTBEAT_INTERVAL_SEC: u64 = 15;
const SESSION_TIMEOUT_SEC: u64 = 300;
const RECONNECT_DELAY_MS: u64 = 3000;
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Authenticating,
    Lobby,
    InMatch,
    Error(String),
}

pub struct GameSession {
    account: Account,
    state: SessionState,
    client: Option<GameClient>,
    session_token: Option<Vec<u8>>,
    last_heartbeat: Instant,
    reconnect_count: u32,
    shutdown_rx: mpsc::Receiver<()>,
}

impl GameSession {
    pub fn new(account: Account, shutdown_rx: mpsc::Receiver<()>) -> Self {
        Self {
            account,
            state: SessionState::Disconnected,
            client: None,
            session_token: None,
            last_heartbeat: Instant::now(),
            reconnect_count: 0,
            shutdown_rx,
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        self.state = SessionState::Connecting;
        info!(
            "connecting account {} to {}",
            self.account.display_name,
            self.account.region.gateway_host()
        );

        let client = GameClient::connect(
            self.account.region.gateway_host(),
            self.account.region.gateway_port(),
        )
        .await
        .context("failed to connect to gateway")?;

        self.client = Some(client);
        self.state = SessionState::Authenticating;

        self.authenticate().await?;
        self.state = SessionState::Lobby;
        self.reconnect_count = 0;

        info!("session established for {}", self.account.display_name);
        Ok(())
    }

    async fn authenticate(&mut self) -> Result<()> {
        let client = self.client.as_mut().unwrap();

        // build login packet
        let login_payload = serde_json::json!({
            "open_id": self.account.open_id,
            "token": self.account.auth_token,
            "client_version": "2.9.0",
            "os": "android",
            "device_id": uuid::Uuid::new_v4().to_string(),
        });

        let login_packet = Packet::new(
            PacketType::LoginRequest,
            serde_json::to_vec(&login_payload)?,
        );

        client.send_packet(&login_packet).await?;

        // wait for auth response
        let response = client
            .recv_packet()
            .await
            .context("no response to login")?;

        match response.packet_type {
            PacketType::LoginResponse => {
                let body: serde_json::Value = serde_json::from_slice(&response.payload)?;
                let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                if code != 0 {
                    let msg = body
                        .get("msg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    anyhow::bail!("login failed ({}): {}", code, msg);
                }
                // extract session token
                if let Some(tok) = body.get("session_token").and_then(|v| v.as_str()) {
                    self.session_token = Some(base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        tok,
                    )?);
                }
                Ok(())
            }
            PacketType::ErrorResponse => {
                anyhow::bail!("server rejected login");
            }
            _ => {
                anyhow::bail!("unexpected response type: {:?}", response.packet_type);
            }
        }
    }

    pub async fn run_loop(&mut self) -> Result<()> {
        self.connect().await?;
        let mut heartbeat_tick = interval(Duration::from_secs(HEARTBEAT_INTERVAL_SEC));

        loop {
            tokio::select! {
                _ = heartbeat_tick.tick() => {
                    if let Err(e) = self.send_heartbeat().await {
                        warn!("heartbeat failed: {}", e);
                        if let Err(re) = self.try_reconnect().await {
                            error!("reconnect failed: {}", re);
                            break;
                        }
                    }
                }
                packet = self.recv_next() => {
                    match packet {
                        Ok(Some(pkt)) => self.handle_packet(pkt).await,
                        Ok(None) => {
                            debug!("connection closed by server");
                            if let Err(e) = self.try_reconnect().await {
                                error!("reconnect failed: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("recv error: {}", e);
                        }
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    info!("session shutdown requested");
                    break;
                }
            }
        }

        self.disconnect().await;
        Ok(())
    }

    async fn send_heartbeat(&mut self) -> Result<()> {
        let client = self.client.as_mut().ok_or_else(|| anyhow::anyhow!("no client"))?;
        let hb = Packet::new(PacketType::Heartbeat, vec![]);
        client.send_packet(&hb).await?;
        self.last_heartbeat = Instant::now();
        Ok(())
    }

    async fn recv_next(&mut self) -> Result<Option<Packet>> {
        let client = self.client.as_mut().ok_or_else(|| anyhow::anyhow!("no client"))?;
        client.recv_packet_timeout(Duration::from_secs(HEARTBEAT_INTERVAL_SEC)).await
    }

    async fn handle_packet(&mut self, packet: Packet) {
        match packet.packet_type {
            PacketType::HeartbeatAck => {
                debug!("heartbeat ack");
            }
            PacketType::EventNotification => {
                if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&packet.payload) {
                    info!("event notification: {:?}", data);
                }
            }
            PacketType::RewardGrant => {
                if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&packet.payload) {
                    info!("reward granted: {:?}", data);
                }
            }
            PacketType::KickNotice => {
                warn!("kicked from server");
                self.state = SessionState::Disconnected;
            }
            _ => {
                debug!("unhandled packet type: {:?}", packet.packet_type);
            }
        }
    }

    async fn try_reconnect(&mut self) -> Result<()> {
        if self.reconnect_count >= MAX_RECONNECT_ATTEMPTS {
            anyhow::bail!("max reconnect attempts reached");
        }
        self.reconnect_count += 1;
        warn!(
            "reconnecting (attempt {}/{})",
            self.reconnect_count, MAX_RECONNECT_ATTEMPTS
        );
        tokio::time::sleep(Duration::from_millis(RECONNECT_DELAY_MS)).await;
        self.connect().await
    }

    async fn disconnect(&mut self) {
        if let Some(ref mut client) = self.client {
            let _ = client
                .send_packet(&Packet::new(PacketType::Disconnect, vec![]))
                .await;
            client.close().await;
        }
        self.client = None;
        self.state = SessionState::Disconnected;
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }
}
