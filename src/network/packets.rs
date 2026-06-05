use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};

use crate::core::protocol::{PacketType, PACKET_HEADER_SIZE};

/// Higher-level packet builder with typed payloads.
/// This wraps the raw binary protocol with structured request/response types.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequestPayload {
    pub open_id: String,
    pub token: String,
    pub client_version: String,
    pub os: String,
    pub device_id: String,
    pub device_model: String,
    pub os_version: String,
    pub language: String,
    pub region: String,
}

impl LoginRequestPayload {
    pub fn new(open_id: String, token: String) -> Self {
        Self {
            open_id,
            token,
            client_version: "2.9.0".to_string(),
            os: "android".to_string(),
            device_id: uuid::Uuid::new_v4().to_string(),
            device_model: "SM-G998B".to_string(),
            os_version: "13".to_string(),
            language: "en".to_string(),
            region: "IN".to_string(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponsePayload {
    pub code: i32,
    pub msg: String,
    pub session_token: Option<String>,
    pub player_id: Option<String>,
    pub server_time: Option<i64>,
    pub lobby_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventClaimPayload {
    pub event_id: u32,
    pub sub_id: Option<u32>,
    pub timestamp: i64,
    pub nonce: String,
}

impl EventClaimPayload {
    pub fn new(event_id: u32) -> Self {
        Self {
            event_id,
            sub_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            nonce: uuid::Uuid::new_v4().to_string(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopularityClaimPayload {
    pub event_id: u32,
    pub target_open_id: String,
    pub gift_type: u32,
    pub count: u32,
    pub timestamp: i64,
}

impl PopularityClaimPayload {
    pub fn free_gift(target_open_id: String) -> Self {
        Self {
            event_id: 2001,
            target_open_id,
            gift_type: 1, // 1 = free, 2 = paid
            count: 1,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchJoinPayload {
    pub map_id: u32,
    pub mode: String,
    pub perspective: String,
    pub squad_type: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryPayload {
    pub event_type: String,
    pub data: serde_json::Value,
    pub client_ts: i64,
    pub seq: u64,
}

impl TelemetryPayload {
    pub fn position_update(x: f32, y: f32, z: f32, seq: u64) -> Self {
        Self {
            event_type: "pos".to_string(),
            data: serde_json::json!({
                "x": x,
                "y": y,
                "z": z,
                "yaw": 0.0,
                "state": "idle",
            }),
            client_ts: chrono::Utc::now().timestamp_millis(),
            seq,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

/// Packet frame codec for use with tokio's framed reads
pub struct PacketCodec;

impl PacketCodec {
    /// Try to decode a complete frame from the buffer
    pub fn decode_frame(buf: &mut BytesMut) -> Option<(PacketType, Vec<u8>)> {
        if buf.len() < PACKET_HEADER_SIZE {
            return None;
        }

        let ptype = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let length = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;

        if buf.len() < PACKET_HEADER_SIZE + length {
            return None;
        }

        buf.advance(PACKET_HEADER_SIZE);
        let payload = buf.split_to(length).to_vec();
        Some((PacketType::from(ptype), payload))
    }

    /// Encode a typed packet into wire format
    pub fn encode_frame(ptype: PacketType, payload: &[u8]) -> BytesMut {
        let mut buf = BytesMut::with_capacity(PACKET_HEADER_SIZE + payload.len());
        buf.put_u32(ptype as u32);
        buf.put_u32(payload.len() as u32);
        buf.put_slice(payload);
        buf
    }
}
