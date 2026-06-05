/// UDP keepalive packet format - from captured TGCP traffic.
/// All UDP packets on port 9030/9031 are exactly 22 bytes:
/// [header: 14 bytes][marker_a: 4 bytes = 0xAAAAAAAA][marker_b: 4 bytes = 0xBBBBBBBB]
///
/// The header contains:
/// [0x2A][sequence: 1 byte][0x00][timestamp: 4 bytes][hash: 6 bytes][flags: 1 byte]
///
/// These are TGCP keepalive pings sent every ~200ms.
/// The game server echoes them back unchanged (request == response).
///
/// The actual game data (events, rewards) goes through HTTPS, not UDP.
/// UDP is only used for realtime match gameplay.

pub const KEEPALIVE_SIZE: usize = 22;
pub const MARKER_A: u32 = 0xAAAAAAAA;
pub const MARKER_B: u32 = 0xBBBBBBBB;

/// Game server endpoints from captures
pub mod servers {
    /// UDP keepalive/ping servers (port 9030 = auth, 9031 = game)
    pub const AUTH_SERVERS: &[(&str, u16)] = &[
        ("20.41.230.56", 9030),
    ];

    pub const GAME_SERVERS: &[(&str, u16)] = &[
        ("104.211.240.59", 9031),
        ("20.204.189.60", 9031),
        ("34.0.4.63", 9031),
    ];
}

/// Build a TGCP keepalive packet (for future match simulation)
pub fn build_keepalive(sequence: u8, timestamp: u32) -> [u8; KEEPALIVE_SIZE] {
    let mut pkt = [0u8; KEEPALIVE_SIZE];
    pkt[0] = 0x2A;
    pkt[1] = sequence;
    pkt[2] = 0x00;
    // timestamp LE
    pkt[3..7].copy_from_slice(&timestamp.to_le_bytes());
    // hash placeholder (from real captures this varies)
    pkt[7] = 0xC7;
    pkt[8] = 0x89;
    pkt[9] = 0xB2;
    pkt[10] = 0xD0;
    pkt[11] = 0x5B;
    pkt[12] = 0xA2;
    pkt[13] = 0xC8;
    // markers
    pkt[14..18].copy_from_slice(&MARKER_A.to_be_bytes());
    pkt[18..22].copy_from_slice(&MARKER_B.to_be_bytes());
    pkt
}
