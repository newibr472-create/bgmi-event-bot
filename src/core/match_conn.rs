//! BGMI Match Connection & Auto-Play (AFK) Module
//!
//! After matchmaking assigns a server, this module:
//! 1. Connects to the match server with the session ticket
//! 2. Sends periodic keepalive to stay in the match
//! 3. Handles basic game state (zone, alive status)
//! 4. Auto-disconnects after death or match end
//!
//! The match server uses UDP with AES-128-GCM encryption for game state.
//! However, the initial handshake and keepalive are simpler.
//!
//! BGMI kicks AFK players after ~3 minutes without input packets.
//! We send minimal "idle" input packets to stay connected.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::core::lobby::MatchAssignment;

// ─── Constants ──────────────────────────────────────────────────────────────

/// Match keepalive interval (must be < 3 min to avoid AFK kick)
const MATCH_KEEPALIVE_MS: u64 = 5000; // 5 seconds

/// Input simulation interval (send idle movement to avoid AFK detection)
const INPUT_INTERVAL_MS: u64 = 15000; // 15 seconds

/// Maximum match duration before auto-disconnect (30 min)
const MAX_MATCH_DURATION_SECS: u64 = 1800;

/// Match protocol magic
const MAGIC_MATCH_HANDSHAKE: [u8; 2] = [0x74, 0xAC];
const MAGIC_MATCH_DATA: [u8; 2] = [0x75, 0x72]; // "ur" - game data

/// Match packet types
const PKT_JOIN: u8 = 0x01;
const PKT_KEEPALIVE: u8 = 0x02;
const PKT_INPUT: u8 = 0x03;
const PKT_STATE: u8 = 0x04;
const PKT_DEATH: u8 = 0x05;
const PKT_DISCONNECT: u8 = 0x06;
const PKT_ZONE_UPDATE: u8 = 0x10;
const PKT_PLAYER_COUNT: u8 = 0x11;

// ─── Data Types ─────────────────────────────────────────────────────────────

/// Match connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchState {
    Connecting,
    WaitingForStart,  // In plane/lobby
    InGame,           // Playing (AFK)
    Dead,             // Eliminated
    Finished,         // Match ended
    Disconnected,
    Error,
}

/// Match statistics
#[derive(Debug, Clone)]
pub struct MatchStats {
    pub match_id: String,
    pub duration_secs: u64,
    pub alive_time_secs: u64,
    pub players_remaining: u8,
    pub placement: u8,
    pub state: MatchState,
}

/// Simple player position (for idle movement simulation)
#[derive(Debug, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

impl Position {
    fn default_spawn() -> Self {
        // Random position on Erangel (center-ish)
        Self {
            x: 400000.0,
            y: 400000.0,
            z: 1000.0, // ground level
        }
    }

    /// Tiny movement to simulate idle (sway in place)
    fn idle_move(&mut self, tick: u32) {
        let angle = (tick as f32) * 0.1;
        self.x += angle.sin() * 10.0;
        self.y += angle.cos() * 10.0;
    }
}

// ─── Match Connection ───────────────────────────────────────────────────────

pub struct MatchConnection {
    socket: UdpSocket,
    server_addr: SocketAddr,
    assignment: MatchAssignment,
    state: Arc<RwLock<MatchState>>,
    running: Arc<AtomicBool>,
    stats: Arc<RwLock<MatchStats>>,
    position: RwLock<Position>,
    tick_counter: std::sync::atomic::AtomicU32,
}

impl MatchConnection {
    /// Create connection from match assignment
    pub fn new(assignment: MatchAssignment) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("failed to bind match UDP socket")?;
        socket.set_nonblocking(true)?;

        let stats = MatchStats {
            match_id: assignment.match_id.clone(),
            duration_secs: 0,
            alive_time_secs: 0,
            players_remaining: 100,
            placement: 0,
            state: MatchState::Connecting,
        };

        info!("match connection created: server={}, match_id={}",
            assignment.server_addr, assignment.match_id);

        Ok(Self {
            socket,
            server_addr: assignment.server_addr,
            assignment,
            state: Arc::new(RwLock::new(MatchState::Connecting)),
            running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(RwLock::new(stats)),
            position: RwLock::new(Position::default_spawn()),
            tick_counter: std::sync::atomic::AtomicU32::new(0),
        })
    }

    pub fn state(&self) -> MatchState {
        *self.state.read()
    }

    pub fn stats(&self) -> MatchStats {
        self.stats.read().clone()
    }

    /// Join the match (send handshake with ticket)
    pub fn join(&self) -> Result<()> {
        info!("joining match at {}", self.server_addr);

        let pkt = self.build_join_packet();
        self.socket.send_to(&pkt, self.server_addr)?;

        *self.state.write() = MatchState::WaitingForStart;
        info!("join packet sent ({} bytes)", pkt.len());
        Ok(())
    }

    /// Run the match loop (blocking - call from thread)
    /// Sends keepalive + idle input, handles game state
    pub fn run_match_loop(&self) -> Result<MatchStats> {
        self.running.store(true, Ordering::SeqCst);

        let start_time = Instant::now();
        let mut last_keepalive = Instant::now();
        let mut last_input = Instant::now();
        let alive_start = Instant::now();

        info!("match loop started");

        while self.running.load(Ordering::Relaxed) {
            let now = Instant::now();
            let elapsed = now.duration_since(start_time);

            // Timeout check
            if elapsed.as_secs() > MAX_MATCH_DURATION_SECS {
                info!("max match duration reached, disconnecting");
                break;
            }

            // Send keepalive
            if now.duration_since(last_keepalive) >= Duration::from_millis(MATCH_KEEPALIVE_MS) {
                self.send_keepalive()?;
                last_keepalive = now;
            }

            // Send idle input (anti-AFK)
            if now.duration_since(last_input) >= Duration::from_millis(INPUT_INTERVAL_MS) {
                if *self.state.read() == MatchState::InGame {
                    self.send_idle_input()?;
                }
                last_input = now;
            }

            // Receive and handle packets
            let mut buf = [0u8; 4096];
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    self.handle_match_packet(&buf[..n], addr);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    warn!("match recv error: {}", e);
                }
            }

            // Check if dead or finished
            let current_state = *self.state.read();
            match current_state {
                MatchState::Dead | MatchState::Finished | MatchState::Error => {
                    info!("match ended: {:?}", current_state);
                    break;
                }
                MatchState::InGame => {
                    // Update alive time
                    let alive_secs = now.duration_since(alive_start).as_secs();
                    self.stats.write().alive_time_secs = alive_secs;
                }
                _ => {}
            }

            // Update duration
            self.stats.write().duration_secs = elapsed.as_secs();

            std::thread::sleep(Duration::from_millis(100));
        }

        // Send disconnect
        let _ = self.send_disconnect();

        let final_stats = self.stats.read().clone();
        info!(
            "match complete: duration={}s, alive={}s, placement=#{}",
            final_stats.duration_secs, final_stats.alive_time_secs, final_stats.placement
        );

        Ok(final_stats)
    }

    /// Stop the match loop
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    // ─── Packet Building ────────────────────────────────────────────────────

    fn build_join_packet(&self) -> Vec<u8> {
        let ticket = &self.assignment.session_ticket;

        let mut pkt = Vec::with_capacity(64 + ticket.len());

        // Header
        pkt.extend_from_slice(&MAGIC_MATCH_HANDSHAKE);
        pkt.push(PKT_JOIN);
        pkt.push(0x00); // flags

        // Ticket length + data
        let ticket_len = ticket.len() as u16;
        pkt.extend_from_slice(&ticket_len.to_be_bytes());
        pkt.extend_from_slice(ticket);

        // Session token
        let session_token = &self.assignment.session_ticket[..4.min(ticket.len())];
        pkt.extend_from_slice(session_token);

        // Timestamp
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
        pkt.extend_from_slice(&ts.to_le_bytes());

        // Padding
        while pkt.len() % 4 != 0 {
            pkt.push(0x00);
        }

        pkt
    }

    fn send_keepalive(&self) -> Result<()> {
        let tick = self.tick_counter.fetch_add(1, Ordering::Relaxed);

        let mut pkt = [0u8; 22];
        pkt[0..2].copy_from_slice(&MAGIC_MATCH_HANDSHAKE);
        pkt[2] = PKT_KEEPALIVE;
        pkt[3] = (tick & 0xFF) as u8;

        // Timestamp
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;
        pkt[4..8].copy_from_slice(&ts.to_le_bytes());

        // Session bytes
        let ticket_slice = &self.assignment.session_ticket;
        let copy_len = ticket_slice.len().min(14);
        pkt[8..8 + copy_len].copy_from_slice(&ticket_slice[..copy_len]);

        self.socket.send_to(&pkt, self.server_addr)?;
        debug!("match keepalive #{}", tick);
        Ok(())
    }

    fn send_idle_input(&self) -> Result<()> {
        let tick = self.tick_counter.load(Ordering::Relaxed);

        // Update position with tiny idle movement
        self.position.write().idle_move(tick);
        let pos = *self.position.read();

        let mut pkt = Vec::with_capacity(32);
        pkt.extend_from_slice(&MAGIC_MATCH_DATA);
        pkt.push(PKT_INPUT);
        pkt.push((tick & 0xFF) as u8);

        // Position (x, y, z as f32 LE)
        pkt.extend_from_slice(&pos.x.to_le_bytes());
        pkt.extend_from_slice(&pos.y.to_le_bytes());
        pkt.extend_from_slice(&pos.z.to_le_bytes());

        // Rotation (yaw only - look straight)
        let yaw: f32 = 0.0;
        pkt.extend_from_slice(&yaw.to_le_bytes());

        // Input flags: 0 = no buttons pressed (idle)
        pkt.push(0x00);

        // Timestamp
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;
        pkt.extend_from_slice(&ts.to_le_bytes());

        self.socket.send_to(&pkt, self.server_addr)?;
        debug!("idle input sent (tick={})", tick);
        Ok(())
    }

    fn send_disconnect(&self) -> Result<()> {
        let mut pkt = [0u8; 8];
        pkt[0..2].copy_from_slice(&MAGIC_MATCH_HANDSHAKE);
        pkt[2] = PKT_DISCONNECT;
        pkt[3] = 0x00;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
        pkt[4..8].copy_from_slice(&ts.to_le_bytes());

        self.socket.send_to(&pkt, self.server_addr)?;
        info!("disconnect sent");
        Ok(())
    }

    // ─── Packet Handling ────────────────────────────────────────────────────

    fn handle_match_packet(&self, data: &[u8], addr: SocketAddr) {
        if data.len() < 3 {
            return;
        }

        let pkt_type = data[2];

        match pkt_type {
            PKT_KEEPALIVE => {
                debug!("match keepalive echo from {}", addr);
            }
            PKT_STATE => {
                self.handle_state_update(data);
            }
            PKT_DEATH => {
                self.handle_death(data);
            }
            PKT_ZONE_UPDATE => {
                self.handle_zone_update(data);
            }
            PKT_PLAYER_COUNT => {
                self.handle_player_count(data);
            }
            _ => {
                // Any response from server means we're connected
                if *self.state.read() == MatchState::WaitingForStart {
                    info!("received first game packet, match is live!");
                    *self.state.write() = MatchState::InGame;
                }
                debug!("match pkt type=0x{:02X} ({} bytes)", pkt_type, data.len());
            }
        }
    }

    fn handle_state_update(&self, _data: &[u8]) {
        // Game state packet - means match is active
        if *self.state.read() == MatchState::WaitingForStart {
            *self.state.write() = MatchState::InGame;
            info!("match started (received state update)");
        }
    }

    fn handle_death(&self, data: &[u8]) {
        info!("player died!");
        *self.state.write() = MatchState::Dead;

        // Try to parse placement
        if data.len() >= 4 {
            let placement = data[3];
            self.stats.write().placement = placement;
            info!("placement: #{}", placement);
        }
    }

    fn handle_zone_update(&self, data: &[u8]) {
        debug!("zone update ({} bytes)", data.len());
        // Zone shrinking - we don't move, so we'll die to zone eventually
        // This is expected for AFK bot
    }

    fn handle_player_count(&self, data: &[u8]) {
        if data.len() >= 4 {
            let remaining = data[3];
            self.stats.write().players_remaining = remaining;
            debug!("players remaining: {}", remaining);

            // If we're last few, match is ending
            if remaining <= 1 {
                *self.state.write() = MatchState::Finished;
                self.stats.write().placement = 1;
                info!("winner winner chicken dinner! (or last standing)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_assignment() -> MatchAssignment {
        MatchAssignment {
            match_id: "test_match_001".to_string(),
            server_addr: "127.0.0.1:9031".parse().unwrap(),
            session_ticket: vec![0x33, 0xC3, 0xDE, 0x08, 0x11, 0x22, 0x33, 0x44],
            encryption_key: vec![0u8; 16],
            map_id: 1,
        }
    }

    #[test]
    fn test_match_connection_create() {
        let conn = MatchConnection::new(test_assignment()).unwrap();
        assert_eq!(conn.state(), MatchState::Connecting);
    }

    #[test]
    fn test_join_packet_format() {
        let conn = MatchConnection::new(test_assignment()).unwrap();
        let pkt = conn.build_join_packet();

        assert_eq!(pkt[0..2], MAGIC_MATCH_HANDSHAKE);
        assert_eq!(pkt[2], PKT_JOIN);
        assert!(pkt.len() >= 12);
    }

    #[test]
    fn test_idle_position() {
        let mut pos = Position::default_spawn();
        let original_x = pos.x;
        pos.idle_move(10);
        // Position should change slightly
        assert_ne!(pos.x, original_x);
    }
}
