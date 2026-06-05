use anyhow::{Context, Result};
use base64::Engine;

/// sValidKey generation - reverse engineered from captured traffic.
/// The SDK sends a different sValidKey per request.
/// It's an MD5 hash of: sorted(params) + secret
///
/// From the APK analysis: libsigner.so handles this but the ITOP SDK
/// Java code also has a fallback implementation.
///
/// Formula: md5(sorted_param_string + sdk_key)
/// The sdk_key for BGMI India is derived from the offer_id and game_id.
const SDK_SIGN_KEY: &str = "2dedb362cb224c6e8f22e4c4b2236630";

/// Generate sValidKey for SDK API calls
/// Takes the query params (excluding sValidKey itself) and produces the signature
pub fn compute_valid_key(params: &[(&str, &str)]) -> String {
    let mut sorted: Vec<(&str, &str)> = params
        .iter()
        .filter(|(k, _)| *k != "sValidKey" && *k != "sRefer")
        .copied()
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let param_str: String = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");

    let sign_input = format!("{}{}", param_str, SDK_SIGN_KEY);
    let hash = md5::compute(sign_input.as_bytes());
    format!("{:x}", hash)
}

/// Decrypt the encrypt_msg field from min-pay responses
/// The key_info returned by get_key is used as the AES key
/// Format: hex-encoded AES-128-ECB encrypted data
pub fn decrypt_pay_message(encrypted_hex: &str, key_info: &str) -> Result<Vec<u8>> {
    use aes::cipher::{BlockDecrypt, KeyInit};
    use aes::Aes128;

    let encrypted = hex::decode(encrypted_hex).context("invalid hex in encrypted msg")?;

    // key_info is also hex - first 32 hex chars = 16 bytes for AES-128
    let key_bytes = hex::decode(&key_info[..32]).context("invalid key_info hex")?;

    let cipher = Aes128::new_from_slice(&key_bytes).context("invalid AES key length")?;

    let mut result = Vec::with_capacity(encrypted.len());
    for chunk in encrypted.chunks(16) {
        let mut block = aes::Block::default();
        block[..chunk.len()].copy_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        result.extend_from_slice(&block);
    }

    // PKCS7 unpad
    if let Some(&pad_len) = result.last() {
        let pad_len = pad_len as usize;
        if pad_len > 0 && pad_len <= 16 {
            result.truncate(result.len() - pad_len);
        }
    }

    Ok(result)
}

/// Encrypt a message for min-pay requests
pub fn encrypt_pay_message(plaintext: &[u8], key_info: &str) -> Result<String> {
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::Aes128;

    let key_bytes = hex::decode(&key_info[..32]).context("invalid key_info hex")?;
    let cipher = Aes128::new_from_slice(&key_bytes).context("invalid AES key len")?;

    // PKCS7 padding
    let pad_len = 16 - (plaintext.len() % 16);
    let mut padded = plaintext.to_vec();
    padded.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let mut result = Vec::with_capacity(padded.len());
    for chunk in padded.chunks(16) {
        let mut block = aes::Block::default();
        block.copy_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        result.extend_from_slice(&block);
    }

    Ok(hex::encode_upper(&result))
}

/// Generate xg_mid / session_token (UUID v4 format)
pub fn gen_session_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Parse the sTicket to extract embedded auth data
/// Format: [random_prefix_bytes][base64_json_payload][suffix_bytes]
/// The JSON contains: sInnerToken, iOpenid, iGameId, iCTime, sEnv
pub fn decode_ticket(ticket: &str) -> Option<TicketPayload> {
    // find the base64 JSON portion - it always starts with encoded "erToken"
    // which is base64 "ZXJUb2tlbi"
    let marker = "ZXJ";
    if let Some(pos) = ticket.find(marker) {
        let b64_part = &ticket[pos..];
        // try decoding with various padding
        for padding in &["", "=", "==", "==="] {
            let attempt = format!("{}{}", b64_part, padding);
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(attempt.as_bytes()) {
                let text = String::from_utf8_lossy(&decoded);
                // prepend the "sInn" that was in the encrypted header
                let full_json = format!("{{\"sInn{}", text.trim_end_matches(|c: char| !c.is_ascii()));
                if let Ok(payload) = serde_json::from_str::<TicketPayload>(&full_json) {
                    return Some(payload);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TicketPayload {
    #[serde(rename = "sInnerToken")]
    pub inner_token: Option<String>,
    #[serde(rename = "iOpenid")]
    pub openid: Option<u64>,
    #[serde(rename = "iGameId")]
    pub game_id: Option<u32>,
    #[serde(rename = "iCTime")]
    pub create_time: Option<u64>,
    #[serde(rename = "sEnv")]
    pub env: Option<String>,
}
