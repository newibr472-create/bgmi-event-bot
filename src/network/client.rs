use anyhow::{Context, Result};
use bytes::BytesMut;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, trace};

use crate::core::crypto::{decrypt_payload, derive_session_key, encrypt_payload};
use crate::core::protocol::{Packet, PacketType, PACKET_HEADER_SIZE};

const READ_BUF_SIZE: usize = 8192;
const CONNECT_TIMEOUT_SEC: u64 = 10;
const READ_TIMEOUT_SEC: u64 = 30;

pub struct GameClient {
    stream: TcpStream,
    read_buf: BytesMut,
    session_key: Option<[u8; 32]>,
    packets_sent: u64,
    packets_recv: u64,
}

impl GameClient {
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        let addr = format!("{}:{}", host, port);
        debug!("connecting to {}", addr);

        let stream = timeout(
            Duration::from_secs(CONNECT_TIMEOUT_SEC),
            TcpStream::connect(&addr),
        )
        .await
        .context("connection timeout")?
        .context("tcp connect failed")?;

        // set tcp options for low-latency
        stream.set_nodelay(true)?;

        Ok(Self {
            stream,
            read_buf: BytesMut::with_capacity(READ_BUF_SIZE),
            session_key: None,
            packets_sent: 0,
            packets_recv: 0,
        })
    }

    /// Set the session encryption key (called after successful auth)
    pub fn set_session_key(&mut self, session_token: &[u8]) {
        let ts = chrono::Utc::now().timestamp();
        self.session_key = Some(derive_session_key(session_token, ts));
    }

    pub async fn send_packet(&mut self, packet: &Packet) -> Result<()> {
        let mut data = packet.encode();

        // encrypt payload portion if we have a session key and packet requires it
        if packet.is_encrypted() {
            if let Some(ref key) = self.session_key {
                let encrypted = encrypt_payload(&packet.payload, key)?;
                // rebuild packet with encrypted payload
                let enc_packet = Packet::new(packet.packet_type, encrypted);
                data = enc_packet.encode();
            }
        }

        self.stream.write_all(&data).await?;
        self.stream.flush().await?;
        self.packets_sent += 1;
        trace!("sent packet {:?} ({} bytes)", packet.packet_type, data.len());
        Ok(())
    }

    pub async fn recv_packet(&mut self) -> Result<Packet> {
        loop {
            // try to parse from existing buffer first
            if let Some(mut packet) = Packet::decode(&mut self.read_buf)? {
                // decrypt if needed
                if packet.is_encrypted() {
                    if let Some(ref key) = self.session_key {
                        packet.payload = decrypt_payload(&packet.payload, key)?;
                    }
                }
                self.packets_recv += 1;
                return Ok(packet);
            }

            // need more data
            let n = self.stream.read_buf(&mut self.read_buf).await?;
            if n == 0 {
                anyhow::bail!("connection closed");
            }
        }
    }

    pub async fn recv_packet_timeout(&mut self, dur: Duration) -> Result<Option<Packet>> {
        match timeout(dur, self.recv_packet()).await {
            Ok(Ok(pkt)) => Ok(Some(pkt)),
            Ok(Err(e)) => Err(e),
            Err(_) => Ok(None), // timeout, no data
        }
    }

    /// Non-blocking attempt to read a packet (returns None immediately if nothing available)
    pub async fn try_recv_packet(&mut self) -> Result<Option<Packet>> {
        // check existing buffer
        if let Some(mut packet) = Packet::decode(&mut self.read_buf)? {
            if packet.is_encrypted() {
                if let Some(ref key) = self.session_key {
                    packet.payload = decrypt_payload(&packet.payload, key)?;
                }
            }
            self.packets_recv += 1;
            return Ok(Some(packet));
        }

        // try a non-blocking read
        let mut tmp = [0u8; READ_BUF_SIZE];
        match self.stream.try_read(&mut tmp) {
            Ok(0) => Ok(None),
            Ok(n) => {
                self.read_buf.extend_from_slice(&tmp[..n]);
                if let Some(mut packet) = Packet::decode(&mut self.read_buf)? {
                    if packet.is_encrypted() {
                        if let Some(ref key) = self.session_key {
                            packet.payload = decrypt_payload(&packet.payload, key)?;
                        }
                    }
                    self.packets_recv += 1;
                    Ok(Some(packet))
                } else {
                    Ok(None)
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn close(&mut self) {
        let _ = self.stream.shutdown().await;
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.packets_sent, self.packets_recv)
    }
}
