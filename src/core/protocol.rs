use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// BGMI uses a custom binary protocol over TCP layered on UE4 networking.
/// Each packet: [type: u32][length: u32][payload: N bytes]
/// Payload is AES-GCM encrypted after the handshake phase.

pub const PACKET_HEADER_SIZE: usize = 8;
pub const MAX_PACKET_SIZE: usize = 1024 * 64; // 64KB max

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum PacketType {
    // Auth flow
    LoginRequest = 0x0001,
    LoginResponse = 0x0002,
    TokenRefresh = 0x0003,
    TokenRefreshAck = 0x0004,

    // Session management
    Heartbeat = 0x0010,
    HeartbeatAck = 0x0011,
    Disconnect = 0x0012,
    KickNotice = 0x0013,

    // Lobby
    LobbyState = 0x0020,
    PlayerInfo = 0x0021,
    FriendList = 0x0022,

    // Match lifecycle
    MatchJoinRequest = 0x0030,
    MatchJoinResponse = 0x0031,
    MatchStart = 0x0032,
    MatchUpdate = 0x0033,
    MatchEnd = 0x0034,
    MatchLeave = 0x0035,

    // Events and rewards
    EventList = 0x0040,
    EventDetail = 0x0041,
    EventClaimRequest = 0x0042,
    EventClaimResponse = 0x0043,
    EventNotification = 0x0044,
    RewardGrant = 0x0045,

    // Popularity/social
    PopularityQuery = 0x0050,
    PopularityClaim = 0x0051,
    PopularityResult = 0x0052,

    // Telemetry (client -> server)
    TelemetryReport = 0x0060,
    TelemetryAck = 0x0061,

    // Error
    ErrorResponse = 0xFF00,

    Unknown = 0xFFFF,
}

impl From<u32> for PacketType {
    fn from(v: u32) -> Self {
        match v {
            0x0001 => Self::LoginRequest,
            0x0002 => Self::LoginResponse,
            0x0003 => Self::TokenRefresh,
            0x0004 => Self::TokenRefreshAck,
            0x0010 => Self::Heartbeat,
            0x0011 => Self::HeartbeatAck,
            0x0012 => Self::Disconnect,
            0x0013 => Self::KickNotice,
            0x0020 => Self::LobbyState,
            0x0021 => Self::PlayerInfo,
            0x0022 => Self::FriendList,
            0x0030 => Self::MatchJoinRequest,
            0x0031 => Self::MatchJoinResponse,
            0x0032 => Self::MatchStart,
            0x0033 => Self::MatchUpdate,
            0x0034 => Self::MatchEnd,
            0x0035 => Self::MatchLeave,
            0x0040 => Self::EventList,
            0x0041 => Self::EventDetail,
            0x0042 => Self::EventClaimRequest,
            0x0043 => Self::EventClaimResponse,
            0x0044 => Self::EventNotification,
            0x0045 => Self::RewardGrant,
            0x0050 => Self::PopularityQuery,
            0x0051 => Self::PopularityClaim,
            0x0052 => Self::PopularityResult,
            0x0060 => Self::TelemetryReport,
            0x0061 => Self::TelemetryAck,
            0xFF00 => Self::ErrorResponse,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub packet_type: PacketType,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(packet_type: PacketType, payload: Vec<u8>) -> Self {
        Self {
            packet_type,
            payload,
        }
    }

    pub fn encode(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(PACKET_HEADER_SIZE + self.payload.len());
        buf.put_u32(self.packet_type as u32);
        buf.put_u32(self.payload.len() as u32);
        buf.put_slice(&self.payload);
        buf
    }

    pub fn decode(buf: &mut BytesMut) -> Result<Option<Self>, ProtocolError> {
        if buf.len() < PACKET_HEADER_SIZE {
            return Ok(None); // need more data
        }

        let ptype = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let length = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;

        if length > MAX_PACKET_SIZE {
            return Err(ProtocolError::PacketTooLarge(length));
        }

        if buf.len() < PACKET_HEADER_SIZE + length {
            return Ok(None); // incomplete
        }

        buf.advance(PACKET_HEADER_SIZE);
        let payload = buf.split_to(length).to_vec();

        Ok(Some(Packet {
            packet_type: PacketType::from(ptype),
            payload,
        }))
    }

    pub fn is_encrypted(&self) -> bool {
        // packets after auth are encrypted; auth packets are plaintext
        !matches!(
            self.packet_type,
            PacketType::LoginRequest | PacketType::LoginResponse | PacketType::ErrorResponse
        )
    }
}

/// Match-related data structures extracted from UE4 protocol analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchConfig {
    pub map_id: u32,
    pub mode: MatchMode,
    pub perspective: Perspective,
    pub max_players: u32,
    pub bot_fill: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MatchMode {
    Classic,
    Arcade,
    EvoGround,
    Arena,
    TDM,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Perspective {
    TPP,
    FPP,
}

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("packet exceeds max size: {0} bytes")]
    PacketTooLarge(usize),
    #[error("invalid packet type: {0:#x}")]
    InvalidType(u32),
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("malformed payload")]
    MalformedPayload,
}
