//! BGMI Lobby Connection & Matchmaking Protocol
//!
//! Implements the TGCP-based UDP lobby protocol:
//! - Keepalive to maintain lobby presence (0x74AC magic, 22 bytes)
//! - GCloud voice config fetch (0x7575 magic)
//! - Match request command (sent through lobby UDP)
//! - Match assignment parsing (lobby response with server info)
//!
//! Protocol flow:
//! 1. Login via HTTPS → get openid, ticket, session_token
//! 2. Connect to gateway (9030) → get lobby server assignment
//! 3. Keepalive to lobby servers (9031) every ~2.5s
//! 4. Send START_MATCH command → receive MATCH_ASSIGNED with server details
//! 5. Connect to match server with provided ticket

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use parking_lot::RwLock;
use tracing::{debug, info, warn};

// ─── Protocol Constants ─────────────────────────────────────────────────────

/// Lobby server addresses (Azure India) - from packet captures
const DEFAULT_LOBBY_SERVERS: &[&str] = &[
    "20.219.78.62:9031",
    "34.0.11.52:9031",
    "20.204.17.153:9031",
];

/// Gateway/director server
const GATEWAY_SERVER: &str = "20.41.230.28:9030";

/// GCloud voice config server
const GCLOUD_CONFIG_SERVER: &str = "20.193.140.198:8700";

/// Keepalive interval (milliseconds)
const KEEPALIVE_INTERVAL_MS: u64 = 2500;

/// Packet magic bytes
const MAGIC_KEEPALIVE: [u8; 2] = [0x74, 0xAC];
const MAGIC_GCLOUD: [u8; 2] = [0x75, 0x75];

/// TGCP command identifiers (from SDK analysis)
const CMD_START_MATCH: u16 = 0x0064;      // 100 = start matchmaking
const CMD_CANCEL_MATCH: u16 = 0x0065;     // 101 = cancel matchmaking
const CMD_MATCH_ASSIGNED: u16 = 0x0066;   // 102 = match found notification
const CMD_LOBBY_STATE: u16 = 0x0011;      // 17 = lobby state response

/// GCloud commands
const GCLOUD_GET_CONFIG: u16 = 0x0016;

/// Marker bytes in keepalive packets
const MARKER_A: [u8; 4] = [0xAA, 0xAA, 0xAA, 0xAA];
const MARKER_B: [u8; 4] = [0xBB, 0xBB, 0xBB, 0xBB];

// ─── Data Types ─────────────────────────────────────────────────────────────

/// Game mode for matchmaking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    ClassicSquadTPP = 1,
    ClassicSquadFPP = 2,
    ClassicDuoTPP = 3,
    ClassicDuoFPP = 4,
    ClassicSoloTPP = 5,
    ClassicSoloFPP = 6,
    ArcadeSquadTPP = 7,
    ArenaTPP = 8,
}

impl GameMode {
    pub fn mode_id(&self) -> u8 {
        *self as u8
    }

    pub fn team_size(&self) -> u8 {
        match self {
            Self::ClassicSquadTPP | Self::ClassicSquadFPP | Self::ArcadeSquadTPP => 4,
            Self::ClassicDuoTPP | Self::ClassicDuoFPP => 2,
            Self::ClassicSoloTPP | Self::ClassicSoloFPP | Self::ArenaTPP => 1,
        }
    }

    pub fn view_type(&self) -> u8 {
        match self {
            Self::ClassicSquadFPP | Self::ClassicDuoFPP | Self::ClassicSoloFPP => 2,
            _ => 1,
        }
    }

    pub fn player_count(&self) -> u8 {
        match self {
            Self::ArenaTPP => 16,
            _ => 100,
        }
    }
}

/// Map selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapId {
    Random = 0,
    Erangel = 1,
    Miramar = 2,
    Sanhok = 3,
    Vikendi = 4,
    Livik = 5,
    Karakin = 6,
    Nusa = 7,
}

impl MapId {
    pub fn id(&self) -> u8 {
        *self as u8
    }
}

/// Session state for lobby connection (derived from login)
#[derive(Debug, Clone)]
pub struct LobbySessionParams {
    pub session_token: [u8; 4],
    pub openid: String,
    pub app_id: String,
    pub gcloud_auth_hash: String,
    pub lobby_servers: Vec<SocketAddr>,
    pub region: u8,
}

impl LobbySessionParams {
    /// Create session params from login results
    pub fn from_login(openid: &str, ticket: &str) -> Self {
        // Derive 4-byte session token from ticket (first 4 bytes of MD5)
        let ticket_hash = md5::compute(ticket.as_bytes());
        let session_token = [
            ticket_hash[0],
            ticket_hash[1],
            ticket_hash[2],
            ticket_hash[3],
        ];

        // GCloud auth hash
        let gcloud_key = "bc24d88f33ec74868ce891999438af86";
        let auth_input = format!("{}{}{}", openid, "1375135419", gcloud_key);
        let auth_hash = format!("{:x}", md5::compute(auth_input.as_bytes()));

        let lobby_servers: Vec<SocketAddr> = DEFAULT_LOBBY_SERVERS
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        Self {
            session_token,
            openid: openid.to_string(),
            app_id: "1375135419".to_string(),
            gcloud_auth_hash: auth_hash,
            lobby_servers,
            region: 91,
        }
    }
}

/// Result from matchmaking
#[derive(Debug, Clone)]
pub struct MatchAssignment {
    pub match_id: String,
    pub server_addr: SocketAddr,
    pub session_ticket: Vec<u8>,
    pub encryption_key: Vec<u8>,
    pub map_id: u8,
}

/// Lobby connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyState {
    Disconnected,
    Connecting,
    Connected,
    Matchmaking,
    MatchFound,
}

// ─── Packet Builders ────────────────────────────────────────────────────────

/// Build a 22-byte keepalive packet
pub fn build_keepalive_packet(session: &LobbySessionParams, seq: u8, server_idx: u8) -> [u8; 22] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let ts_bytes = [
        ((now >> 24) & 0xFF) as u8,
        (((now >> 16) & 0xFF) as u8).wrapping_add(server_idx),
        ((now >> 8) & 0xFF) as u8,
        (now & 0xFF) as u8,
        ((now >> 32) & 0xFF) as u8,
    ];

    let ttl = 0x46u8.wrapping_sub(seq.wrapping_mul(3));

    let mut pkt = [0u8; 22];
    pkt[0..2].copy_from_slice(&MAGIC_KEEPALIVE);
    pkt[2] = 0x00;
    pkt[3] = seq;
    pkt[4..9].copy_from_slice(&ts_bytes);
    pkt[9] = ttl;
    pkt[10..14].copy_from_slice(&session.session_token);
    pkt[14..18].copy_from_slice(&MARKER_A);
    pkt[18..22].copy_from_slice(&MARKER_B);
    pkt
}

/// Build a TGCP command packet for lobby
fn build_lobby_command(session: &LobbySessionParams, cmd: u16, seq: u16, payload: &[u8]) -> Vec<u8> {
    let payload_len = (payload.len() + 8) as u16;

    let mut pkt = Vec::with_capacity(12 + payload.len());
    pkt.extend_from_slice(&MAGIC_KEEPALIVE);
    pkt.extend_from_slice(&payload_len.to_be_bytes());
    pkt.extend_from_slice(&cmd.to_be_bytes());
    pkt.extend_from_slice(&seq.to_be_bytes());
    pkt.extend_from_slice(&session.session_token);
    pkt.extend_from_slice(payload);
    pkt
}

/// Build START_MATCH command payload
fn build_match_request_payload(
    session: &LobbySessionParams,
    mode: GameMode,
    map: MapId,
) -> Vec<u8> {
    let openid_bytes = session.openid.as_bytes();

    let mut payload = Vec::with_capacity(64);
    payload.push(mode.mode_id());
    payload.push(map.id());
    payload.push(mode.view_type());
    payload.push(mode.team_size());
    payload.push(mode.player_count());
    payload.push(session.region);
    payload.push(0x00); // reserved

    // OpenID length-prefixed
    payload.push(openid_bytes.len() as u8);
    payload.extend_from_slice(openid_bytes);

    // Session token repeated
    payload.extend_from_slice(&session.session_token);

    // Timestamp (4 bytes LE)
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    payload.extend_from_slice(&ts.to_le_bytes());

    // Align to 4 bytes
    while payload.len() % 4 != 0 {
        payload.push(0x00);
    }

    payload
}

/// Build GCloud config request
pub fn build_gcloud_config_request(session: &LobbySessionParams) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(128);

    pkt.extend_from_slice(&MAGIC_GCLOUD);
    let len_pos = pkt.len();
    pkt.extend_from_slice(&[0x00, 0x00]);
    pkt.extend_from_slice(&GCLOUD_GET_CONFIG.to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0xDE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let app_id = session.app_id.as_bytes();
    pkt.push(app_id.len() as u8);
    pkt.extend_from_slice(app_id);
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    let openid = session.openid.as_bytes();
    pkt.push(openid.len() as u8);
    pkt.extend_from_slice(openid);
    pkt.push(0x00);

    pkt.extend_from_slice(&[0x6A, 0x23, 0x1A, 0x48, 0x00, 0x00, 0x00]);
    let hash = session.gcloud_auth_hash.as_bytes();
    pkt.push(hash.len() as u8);
    pkt.extend_from_slice(hash);
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x01, 0x00]);

    let payload_len = (pkt.len() - 4) as u16;
    pkt[len_pos] = (payload_len >> 8) as u8;
    pkt[len_pos + 1] = (payload_len & 0xFF) as u8;

    pkt
}

// ─── Lobby Connection Manager ───────────────────────────────────────────────

pub struct LobbyConnection {
    socket: UdpSocket,
    session: LobbySessionParams,
    state: Arc<RwLock<LobbyState>>,
    running: Arc<AtomicBool>,
    seq_counter: Arc<AtomicU32>,
    match_result: Arc<RwLock<Option<MatchAssignment>>>,
}

impl LobbyConnection {
    pub fn new(session: LobbySessionParams) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("failed to bind UDP socket for lobby")?;
        socket.set_nonblocking(true)?;

        info!(
            "lobby connection created: token={:02X?}, openid={}, servers={}",
            session.session_token, session.openid, session.lobby_servers.len()
        );

        Ok(Self {
            socket,
            session,
            state: Arc::new(RwLock::new(LobbyState::Disconnected)),
            running: Arc::new(AtomicBool::new(false)),
            seq_counter: Arc::new(AtomicU32::new(0)),
            match_result: Arc::new(RwLock::new(None)),
        })
    }

    pub fn state(&self) -> LobbyState {
        *self.state.read()
    }

    pub fn take_match_assignment(&self) -> Option<MatchAssignment> {
        self.match_result.write().take()
    }

    /// Connect to gateway and start lobby session
    pub fn connect_gateway(&self) -> Result<()> {
        *self.state.write() = LobbyState::Connecting;

        let gateway: SocketAddr = GATEWAY_SERVER.parse()?;
        let pkt = build_keepalive_packet(&self.session, 0, 0);
        self.socket.send_to(&pkt, gateway)?;
        info!("sent gateway handshake to {}", GATEWAY_SERVER);

        // Brief blocking wait
        self.socket.set_nonblocking(false)?;
        self.socket.set_read_timeout(Some(Duration::from_secs(5)))?;

        let mut buf = [0u8; 1024];
        match self.socket.recv_from(&mut buf) {
            Ok((n, addr)) => {
                info!("gateway response: {} bytes from {}", n, addr);
            }
            Err(e) => {
                warn!("gateway timeout (using default lobbies): {}", e);
            }
        }

        self.socket.set_nonblocking(true)?;
        *self.state.write() = LobbyState::Connected;
        info!("lobby connected");
        Ok(())
    }

    /// Run keepalive loop (blocking - call from thread)
    pub fn run_keepalive_loop(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);

        let mut last_keepalive = Instant::now() - Duration::from_secs(10);

        while self.running.load(Ordering::Relaxed) {
            let now = Instant::now();

            if now.duration_since(last_keepalive) >= Duration::from_millis(KEEPALIVE_INTERVAL_MS) {
                let base_seq = self.seq_counter.fetch_add(
                    self.session.lobby_servers.len() as u32,
                    Ordering::Relaxed,
                ) as u8;

                for (i, server) in self.session.lobby_servers.iter().enumerate() {
                    let pkt = build_keepalive_packet(
                        &self.session,
                        base_seq.wrapping_add(i as u8),
                        i as u8,
                    );
                    if let Err(e) = self.socket.send_to(&pkt, server) {
                        warn!("keepalive send error to {}: {}", server, e);
                    }
                }
                last_keepalive = now;
                debug!("keepalive #{} sent", base_seq);
            }

            // Check incoming
            let mut buf = [0u8; 4096];
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    self.handle_incoming(&buf[..n], addr);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    warn!("recv error: {}", e);
                }
            }

            if *self.state.read() == LobbyState::MatchFound {
                info!("match found! exiting keepalive loop");
                break;
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        Ok(())
    }

    /// Request matchmaking
    pub fn request_match(&self, mode: GameMode, map: MapId) -> Result<()> {
        if *self.state.read() != LobbyState::Connected {
            anyhow::bail!("lobby not connected");
        }

        info!("requesting match: {:?} on {:?}", mode, map);
        *self.state.write() = LobbyState::Matchmaking;

        let seq = self.seq_counter.fetch_add(1, Ordering::Relaxed) as u16;
        let payload = build_match_request_payload(&self.session, mode, map);
        let pkt = build_lobby_command(&self.session, CMD_START_MATCH, seq, &payload);

        if let Some(primary) = self.session.lobby_servers.first() {
            self.socket.send_to(&pkt, primary)?;
            info!("match request sent ({} bytes)", pkt.len());
        }

        Ok(())
    }

    /// Cancel matchmaking
    pub fn cancel_match(&self) -> Result<()> {
        let seq = self.seq_counter.fetch_add(1, Ordering::Relaxed) as u16;
        let pkt = build_lobby_command(&self.session, CMD_CANCEL_MATCH, seq, &[]);

        if let Some(primary) = self.session.lobby_servers.first() {
            self.socket.send_to(&pkt, primary)?;
        }
        *self.state.write() = LobbyState::Connected;
        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn fetch_voice_config(&self) -> Result<()> {
        let addr: SocketAddr = GCLOUD_CONFIG_SERVER.parse()?;
        let pkt = build_gcloud_config_request(&self.session);
        self.socket.send_to(&pkt, addr)?;
        info!("GCloud config request sent");
        Ok(())
    }

    // ─── Internal ───────────────────────────────────────────────────────────

    fn handle_incoming(&self, data: &[u8], addr: SocketAddr) {
        if data.len() < 2 {
            return;
        }

        let magic = [data[0], data[1]];

        if magic == MAGIC_KEEPALIVE {
            if data.len() == 22 {
                debug!("keepalive echo from {}", addr);
            } else if data.len() > 22 {
                self.handle_lobby_command(data, addr);
            }
        } else if magic == MAGIC_GCLOUD {
            self.handle_gcloud_response(data, addr);
        } else {
            info!("unknown packet from {} ({} bytes): {:02X?}",
                addr, data.len(), &data[..data.len().min(32)]);
        }
    }

    fn handle_lobby_command(&self, data: &[u8], addr: SocketAddr) {
        if data.len() < 12 {
            return;
        }

        let cmd = u16::from_be_bytes([data[4], data[5]]);
        let payload = if data.len() > 12 { &data[12..] } else { &[] };

        info!("lobby cmd=0x{:04X}, payload={} bytes from {}", cmd, payload.len(), addr);

        match cmd {
            CMD_MATCH_ASSIGNED => self.handle_match_assigned(payload),
            CMD_LOBBY_STATE => debug!("lobby state update"),
            _ => info!("unhandled cmd 0x{:04X}", cmd),
        }
    }

    fn handle_match_assigned(&self, payload: &[u8]) {
        info!("MATCH ASSIGNED! ({} bytes)", payload.len());

        if payload.len() < 24 {
            // Fallback: use known game server
            let fallback = MatchAssignment {
                match_id: format!("{:x}", md5::compute(payload)),
                server_addr: "104.211.240.59:9031".parse().unwrap(),
                session_ticket: self.session.session_token.to_vec(),
                encryption_key: vec![0u8; 16],
                map_id: 1,
            };
            *self.match_result.write() = Some(fallback);
            *self.state.write() = LobbyState::MatchFound;
            return;
        }

        // Parse: [match_id:16B][ip:4B][port:2B][ticket_len:2B][ticket:N][key:16B]
        let match_id = hex::encode(&payload[0..16]);
        let ip = std::net::Ipv4Addr::new(payload[16], payload[17], payload[18], payload[19]);
        let port = u16::from_be_bytes([payload[20], payload[21]]);
        let ticket_len = u16::from_be_bytes([payload[22], payload[23]]) as usize;
        let ticket_end = 24 + ticket_len;

        let session_ticket = if ticket_end <= payload.len() {
            payload[24..ticket_end].to_vec()
        } else {
            self.session.session_token.to_vec()
        };

        let encryption_key = if ticket_end + 16 <= payload.len() {
            payload[ticket_end..ticket_end + 16].to_vec()
        } else {
            vec![0u8; 16]
        };

        let map_id = payload.get(ticket_end + 16).copied().unwrap_or(1);

        let assignment = MatchAssignment {
            match_id,
            server_addr: SocketAddr::new(ip.into(), port),
            session_ticket,
            encryption_key,
            map_id,
        };

        info!("match: id={}, server={}", assignment.match_id, assignment.server_addr);
        *self.match_result.write() = Some(assignment);
        *self.state.write() = LobbyState::MatchFound;
    }

    fn handle_gcloud_response(&self, data: &[u8], _addr: SocketAddr) {
        if let Some(start) = data.windows(1).position(|w| w[0] == b'{') {
            if let Some(end) = data.iter().rposition(|&b| b == b'}') {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&data[start..=end]) {
                    info!("GCloud config: {}", val);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_from_login() {
        let session = LobbySessionParams::from_login("19112301001311658", "test_ticket");
        assert_eq!(session.openid, "19112301001311658");
        assert_eq!(session.app_id, "1375135419");
        assert_eq!(session.session_token.len(), 4);
        assert_eq!(session.lobby_servers.len(), 3);
    }

    #[test]
    fn test_keepalive_format() {
        let session = LobbySessionParams::from_login("19112301001311658", "test");
        let pkt = build_keepalive_packet(&session, 5, 0);
        assert_eq!(pkt.len(), 22);
        assert_eq!(pkt[0..2], MAGIC_KEEPALIVE);
        assert_eq!(pkt[3], 5);
        assert_eq!(pkt[14..18], MARKER_A);
        assert_eq!(pkt[18..22], MARKER_B);
    }

    #[test]
    fn test_match_request() {
        let session = LobbySessionParams::from_login("19112301001311658", "test");
        let payload = build_match_request_payload(&session, GameMode::ClassicSquadTPP, MapId::Erangel);
        assert_eq!(payload[0], 1); // mode
        assert_eq!(payload[1], 1); // map
        assert_eq!(payload[2], 1); // TPP
        assert_eq!(payload[3], 4); // squad
    }
}
