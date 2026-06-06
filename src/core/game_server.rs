//! BGMI Game Server Protocol Implementation
//!
//! Handles the UDP connection to lobby servers (port 9031)
//! and game server protocol (port 9030 gateway).
//!
//! Protocol decoded from live capture analysis:
//! - Magic: 0x74AC (keepalive)
//! - Magic: 0x7575 (GCloud SDK)
//! - Magic: 0x7572 (Voice QOS)

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use tracing::{debug, info, warn, error};

/// Lobby server addresses (Azure India)
const LOBBY_SERVERS: &[&str] = &[
    "20.219.78.62:9031",   // Primary
    "34.0.11.52:9031",     // Secondary
    "20.204.17.153:9031",  // Tertiary
];

/// Gateway server (director/load balancer)
const GATEWAY_SERVER: &str = "20.41.230.28:9030";

/// GCloud voice config server
const GCLOUD_CONFIG_SERVER: &str = "20.193.140.198:8700";

/// Keepalive interval per server (milliseconds)
const KEEPALIVE_INTERVAL_MS: u64 = 2500;

/// Keepalive packet magic
const MAGIC_KEEPALIVE: [u8; 2] = [0x74, 0xAC];

/// GCloud packet magic
const MAGIC_GCLOUD: [u8; 2] = [0x75, 0x75];

/// Session state for lobby connection
#[derive(Debug, Clone)]
pub struct LobbySession {
    /// Session token bytes (from login)
    pub session_token: [u8; 4],  // e.g. [0x33, 0xC3, 0xDE, 0x08]
    /// Current sequence counter
    pub sequence: u8,
    /// Player's openid
    pub openid: String,
    /// App ID for GCloud
    pub app_id: String,
    /// GCloud auth hash (MD5)
    pub gcloud_auth_hash: String,
    /// Lobby server IPs (may be dynamically assigned)
    pub lobby_servers: Vec<SocketAddr>,
}

/// Keepalive packet builder (22 bytes)
pub fn build_keepalive(session: &LobbySession, seq: u8, server_idx: u8) -> [u8; 22] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Timestamp: 5 bytes derived from current time
    let ts_bytes = [
        ((now >> 24) & 0xFF) as u8,
        (((now >> 16) & 0xFF) as u8).wrapping_add(server_idx),  // per-server offset
        ((now >> 8) & 0xFF) as u8,
        (now & 0xFF) as u8,
        ((now >> 32) & 0xFF) as u8,
    ];

    // TTL starts at ~70 (0x46) and decrements
    // For now we use a fixed value since exact algorithm unknown
    let ttl = 0x46u8.wrapping_sub(seq.wrapping_mul(3));

    let mut pkt = [0u8; 22];
    pkt[0..2].copy_from_slice(&MAGIC_KEEPALIVE);
    pkt[2] = 0x00;  // version
    pkt[3] = seq;
    pkt[4..9].copy_from_slice(&ts_bytes);
    pkt[9] = ttl;
    pkt[10..14].copy_from_slice(&session.session_token);
    pkt[14..18].copy_from_slice(&[0xAA, 0xAA, 0xAA, 0xAA]);
    pkt[18..22].copy_from_slice(&[0xBB, 0xBB, 0xBB, 0xBB]);

    pkt
}

/// GCloud config request builder (97 bytes)
pub fn build_gcloud_config_request(session: &LobbySession) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(128);

    // Header
    pkt.extend_from_slice(&MAGIC_GCLOUD);

    // Length placeholder (fill later)
    let len_pos = pkt.len();
    pkt.extend_from_slice(&[0x00, 0x00]);

    // Command: 0x0016 = GET_CONFIG
    pkt.extend_from_slice(&[0x00, 0x16]);

    // Session/flags
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0xDE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // app_id (length-prefixed)
    let app_id = session.app_id.as_bytes();
    pkt.push(app_id.len() as u8);
    pkt.extend_from_slice(app_id);
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    // openid (length-prefixed)
    let openid = session.openid.as_bytes();
    pkt.push(openid.len() as u8);
    pkt.extend_from_slice(openid);

    // Separator
    pkt.push(0x00);

    // Auth hash (some prefix bytes first)
    pkt.extend_from_slice(&[0x6A, 0x23, 0x1A, 0x48, 0x00, 0x00, 0x00]);

    // Hash (length-prefixed)
    let hash = session.gcloud_auth_hash.as_bytes();
    pkt.push(hash.len() as u8);
    pkt.extend_from_slice(hash);

    // Trailing
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x01, 0x00]);

    // Fill length
    let payload_len = (pkt.len() - 4) as u16;  // exclude magic + length field
    pkt[len_pos] = (payload_len >> 8) as u8;
    pkt[len_pos + 1] = (payload_len & 0xFF) as u8;

    pkt
}

/// Lobby connection manager
pub struct LobbyConnection {
    socket: UdpSocket,
    session: LobbySession,
    running: Arc<AtomicBool>,
    seq_counter: AtomicU8,
}

impl LobbyConnection {
    /// Create a new lobby connection
    pub fn new(session: LobbySession) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("failed to bind UDP socket")?;
        socket.set_nonblocking(true)?;

        let lobby_servers: Vec<SocketAddr> = if session.lobby_servers.is_empty() {
            LOBBY_SERVERS.iter()
                .map(|s| s.parse().unwrap())
                .collect()
        } else {
            session.lobby_servers.clone()
        };

        Ok(Self {
            socket,
            session: LobbySession {
                lobby_servers,
                ..session
            },
            running: Arc::new(AtomicBool::new(false)),
            seq_counter: AtomicU8::new(0),
        })
    }

    /// Connect to gateway and get lobby server assignment
    pub fn connect_gateway(&self) -> Result<()> {
        let gateway: SocketAddr = GATEWAY_SERVER.parse()?;

        // Send initial keepalive to gateway (seq=0)
        let pkt = build_keepalive(&self.session, 0, 0);
        self.socket.send_to(&pkt, gateway)?;
        info!("sent gateway handshake to {}", GATEWAY_SERVER);

        // Wait for response (timeout 5s)
        self.socket.set_nonblocking(false)?;
        self.socket.set_read_timeout(Some(Duration::from_secs(5)))?;

        let mut buf = [0u8; 1024];
        match self.socket.recv_from(&mut buf) {
            Ok((n, addr)) => {
                info!("gateway response: {} bytes from {}", n, addr);
                debug!("gateway data: {:02x?}", &buf[..n]);
                // Parse lobby server list from response
                // (format unknown - needs capture with response)
            }
            Err(e) => {
                warn!("gateway timeout (expected - we'll use default lobbies): {}", e);
            }
        }

        self.socket.set_nonblocking(true)?;
        Ok(())
    }

    /// Start sending keepalive packets to lobby servers
    pub fn start_keepalive(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);

        info!(
            "starting keepalive to {} lobby servers",
            self.session.lobby_servers.len()
        );

        let start = Instant::now();
        let mut last_send = Instant::now() - Duration::from_secs(10);

        while self.running.load(Ordering::Relaxed) {
            let now = Instant::now();

            if now.duration_since(last_send) >= Duration::from_millis(KEEPALIVE_INTERVAL_MS) {
                let seq = self.seq_counter.fetch_add(
                    self.session.lobby_servers.len() as u8,
                    Ordering::Relaxed,
                );

                for (i, server) in self.session.lobby_servers.iter().enumerate() {
                    let pkt = build_keepalive(
                        &self.session,
                        seq + i as u8,
                        i as u8,
                    );
                    if let Err(e) = self.socket.send_to(&pkt, server) {
                        warn!("keepalive send error to {}: {}", server, e);
                    }
                }

                last_send = now;
                debug!("keepalive #{} sent to all servers", seq);
            }

            // Check for incoming data
            let mut buf = [0u8; 2048];
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    self.handle_incoming(&buf[..n], addr);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    warn!("recv error: {}", e);
                }
            }

            std::thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }

    /// Stop keepalive
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Handle incoming packet from lobby server
    fn handle_incoming(&self, data: &[u8], addr: SocketAddr) {
        if data.len() < 2 {
            return;
        }

        let magic = [data[0], data[1]];

        match magic {
            MAGIC_KEEPALIVE => {
                debug!("keepalive response from {} ({} bytes)", addr, data.len());
                // Keepalive echo - server is alive
            }
            MAGIC_GCLOUD => {
                info!("GCloud response from {} ({} bytes)", addr, data.len());
                self.handle_gcloud_response(data);
            }
            _ => {
                info!(
                    "UNKNOWN packet from {} ({} bytes): {:02x?}",
                    addr,
                    data.len(),
                    &data[..data.len().min(32)]
                );
                // This could be a game command response!
                // Log it for analysis
            }
        }
    }

    /// Parse GCloud config response
    fn handle_gcloud_response(&self, data: &[u8]) {
        // Find JSON in response
        if let Some(json_start) = data.windows(1).position(|w| w[0] == b'{') {
            if let Some(json_end) = data.iter().rposition(|&b| b == b'}') {
                let json_slice = &data[json_start..=json_end];
                match serde_json::from_slice::<serde_json::Value>(json_slice) {
                    Ok(config) => {
                        info!("GCloud config: {}", config);
                    }
                    Err(e) => {
                        warn!("GCloud JSON parse error: {}", e);
                    }
                }
            }
        }
    }

    /// Request match (THEORETICAL - needs capture to verify)
    /// This is the match start request that goes through the lobby connection.
    /// Format is UNKNOWN without a capture of actual match start.
    pub fn request_match(&self, _mode: GameMode) -> Result<()> {
        warn!("MATCH REQUEST NOT YET IMPLEMENTED - needs packet capture");
        // When we have the capture, the implementation will be:
        // 1. Build match request packet (larger than 22 bytes)
        // 2. Send to primary lobby server
        // 3. Wait for MATCH_QUEUED response
        // 4. Wait for MATCH_FOUND response
        // 5. Extract match_server_ip, port, ticket
        // 6. Connect to match server
        Ok(())
    }

    /// Fetch GCloud voice config
    pub fn fetch_voice_config(&self) -> Result<()> {
        let gcloud_addr: SocketAddr = GCLOUD_CONFIG_SERVER.parse()?;
        let pkt = build_gcloud_config_request(&self.session);

        self.socket.send_to(&pkt, gcloud_addr)?;
        info!("sent GCloud config request to {}", GCLOUD_CONFIG_SERVER);
        Ok(())
    }
}

/// Game mode for match request
#[derive(Debug, Clone, Copy)]
pub enum GameMode {
    ClassicTPP,
    ClassicFPP,
    ArcadeTPP,
    ArenaTPP,
}

/// Match server connection (separate from lobby)
pub struct MatchConnection {
    socket: UdpSocket,
    match_server: SocketAddr,
    session_ticket: Vec<u8>,
    running: Arc<AtomicBool>,
}

impl MatchConnection {
    /// Connect to a match server with the provided ticket
    pub fn new(server_addr: SocketAddr, ticket: Vec<u8>) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            match_server: server_addr,
            session_ticket: ticket,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Join the match (send ticket to match server)
    pub fn join(&self) -> Result<()> {
        // The exact format is unknown - needs capture
        // Likely: [header] [ticket_data] [player_info]
        warn!("MATCH JOIN NOT YET IMPLEMENTED - needs packet capture");
        Ok(())
    }

    /// Send AFK keepalive (stay in match without playing)
    pub fn afk_keepalive(&self) -> Result<()> {
        // In BGMI, just staying connected counts as "playing"
        // The match server expects periodic input packets
        // Without them, you get kicked after ~3 minutes
        warn!("AFK KEEPALIVE NOT YET IMPLEMENTED");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_keepalive() {
        let session = LobbySession {
            session_token: [0x33, 0xC3, 0xDE, 0x08],
            sequence: 0,
            openid: "19112301001311658".to_string(),
            app_id: "1375135419".to_string(),
            gcloud_auth_hash: "bc24d88f33ec74868ce891999438af86".to_string(),
            lobby_servers: vec![],
        };

        let pkt = build_keepalive(&session, 5, 0);
        assert_eq!(pkt[0..2], MAGIC_KEEPALIVE);
        assert_eq!(pkt[2], 0x00);
        assert_eq!(pkt[3], 5);
        assert_eq!(pkt[10..14], [0x33, 0xC3, 0xDE, 0x08]);
        assert_eq!(pkt[14..18], [0xAA, 0xAA, 0xAA, 0xAA]);
        assert_eq!(pkt[18..22], [0xBB, 0xBB, 0xBB, 0xBB]);
    }

    #[test]
    fn test_build_gcloud_request() {
        let session = LobbySession {
            session_token: [0x33, 0xC3, 0xDE, 0x08],
            sequence: 0,
            openid: "19112301001311658".to_string(),
            app_id: "1375135419".to_string(),
            gcloud_auth_hash: "bc24d88f33ec74868ce891999438af86".to_string(),
            lobby_servers: vec![],
        };

        let pkt = build_gcloud_config_request(&session);
        assert_eq!(pkt[0..2], MAGIC_GCLOUD);
        assert!(pkt.len() > 50);
        // Check app_id is present
        assert!(pkt.windows(10).any(|w| w == b"1375135419"));
    }
}
