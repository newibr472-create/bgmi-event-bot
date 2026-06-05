use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{bail, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Derived from libhdmpvecore.so analysis - this is the static portion of the key
/// used to derive per-session encryption keys. The full key is:
///   SHA256(STATIC_SEED || session_token || timestamp)
const STATIC_SEED: &[u8] = b"bgmi_packet_k3y_s33d_v2";

/// Token payload structure after base64 decode:
/// { "open_id": "...", "token": "...", "ts": unix_ts, "sig": "hmac_hex" }
/// The sig field is HMAC-SHA256(open_id + token + ts, APP_SECRET)
const TOKEN_HMAC_KEY: &[u8] = b"com.pubg.imobile.auth.v1";

pub fn decrypt_token_payload(raw_token: &str) -> Result<serde_json::Value> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw_token.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(raw_token.trim()))?;

    let payload: serde_json::Value = serde_json::from_slice(&decoded)?;

    // verify signature if present
    if let Some(sig) = payload.get("sig").and_then(|v| v.as_str()) {
        let open_id = payload.get("open_id").and_then(|v| v.as_str()).unwrap_or("");
        let token = payload.get("token").and_then(|v| v.as_str()).unwrap_or("");
        let ts = payload.get("ts").and_then(|v| v.as_i64()).unwrap_or(0);

        let msg = format!("{}{}{}", open_id, token, ts);
        if !verify_hmac(msg.as_bytes(), TOKEN_HMAC_KEY, sig) {
            bail!("token signature verification failed");
        }
    }

    Ok(payload)
}

/// Derive session encryption key from the session token + current timestamp
pub fn derive_session_key(session_token: &[u8], timestamp: i64) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(STATIC_SEED);
    hasher.update(session_token);
    hasher.update(timestamp.to_le_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt packet payload using AES-256-GCM
/// Format: [nonce: 12 bytes][ciphertext + tag]
pub fn encrypt_payload(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;

    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt packet payload (inverse of encrypt_payload)
pub fn decrypt_payload(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        bail!("ciphertext too short for nonce");
    }

    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);

    let nonce = Nonce::from_slice(&data[..12]);
    let ciphertext = &data[12..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))?;

    Ok(plaintext)
}

/// Compute HMAC-SHA256 for request signing
pub fn compute_hmac(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Verify an HMAC signature (hex-encoded)
fn verify_hmac(data: &[u8], key: &[u8], expected_hex: &str) -> bool {
    let computed = compute_hmac(data, key);
    let computed_hex = hex_encode(&computed);
    // constant-time comparison
    constant_time_eq(computed_hex.as_bytes(), expected_hex.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Generate device fingerprint to mimic real device auth
pub fn generate_device_fingerprint() -> String {
    use sha2::Digest;
    let mut rng = rand::thread_rng();
    let mut random_bytes = [0u8; 32];
    rng.fill_bytes(&mut random_bytes);

    let mut hasher = sha2::Sha256::new();
    hasher.update(b"android_");
    hasher.update(&random_bytes);
    let hash = hasher.finalize();
    hex_encode(&hash[..16])
}
