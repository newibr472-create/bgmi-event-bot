use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Real BGMI ITOP SDK API protocol - extracted from HttpCanary captures.
/// Auth uses HTTPS REST calls to globh.com infrastructure, NOT raw TCP/UDP.
/// The game uses UDP (port 9030/9031) only for the realtime match protocol.
/// Events/rewards go through min-pay.globh.com HTTPS + encrypted messages.

pub const SDK_VERSION: &str = "2.10.3";
pub const GAME_VERSION: &str = "4.4.0";
pub const GAME_ID: u32 = 1450;
pub const OFFER_ID: &str = "1450025957";
pub const PLATFORM: u32 = 2; // android

// API hosts from real captures
pub const HOST_SDK_API: &str = "in-sdkapi.globh.com";
pub const HOST_NOTICE: &str = "in-notice.globh.com";
pub const HOST_PAY: &str = "min-pay.globh.com";
pub const HOST_CLOUD_CTRL: &str = "in-cloudctrl.globh.com";
pub const HOST_VOICE_CFG: &str = "in-voiceconfig.globh.com";

/// dinfo format from captures: "1|40455|<model>|<lang>|<version>|<timestamp>|<density>|<resolution>|<brand>"
pub fn build_dinfo(model: &str, brand: &str, timestamp: u64) -> String {
    format!(
        "1|40455|{}|en|{}|{}|2.625|2400*1080|{}",
        model, GAME_VERSION, timestamp, brand
    )
}

/// pf (platform fingerprint) for payment calls
pub fn build_pf(openid: &str) -> String {
    format!(
        "IEG_iTOP-2001-android-2011-TW-{}-{}-igame",
        GAME_ID, openid
    )
}

pub fn current_ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Login response from in-sdkapi.globh.com/v1.0/user/login
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub code: i32,
    pub desc: String,
    #[serde(rename = "iOpenid")]
    pub openid: String,
    #[serde(rename = "sInnerToken")]
    pub inner_token: String,
    #[serde(rename = "iGuid")]
    pub guid: String,
    #[serde(rename = "iChannel")]
    pub channel: u32,
    #[serde(rename = "iGameId")]
    pub game_id: u32,
    #[serde(rename = "sChannelId")]
    pub channel_id: String,
    #[serde(rename = "iExpireTime")]
    pub expire_time: u64,
    #[serde(rename = "sUserName")]
    pub username: String,
    #[serde(rename = "sBirthdate")]
    pub birthdate: String,
    #[serde(rename = "iGender")]
    pub gender: u32,
    #[serde(rename = "sPictureUrl")]
    pub picture_url: String,
    #[serde(rename = "firstLoginTag")]
    pub first_login_tag: u32,
    #[serde(rename = "retExtraJson")]
    pub extra_json: String,
}

/// Ticket response from in-sdkapi.globh.com/v1.0/user/getTicket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketResponse {
    pub code: i32,
    pub desc: String,
    #[serde(rename = "sTicket")]
    pub ticket: String,
}

/// Bind relation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindRelationResponse {
    pub code: i32,
    pub desc: String,
    #[serde(rename = "iOpenid")]
    pub openid: Option<String>,
    #[serde(rename = "sInnerToken")]
    pub inner_token: Option<String>,
    #[serde(rename = "iGuid")]
    pub guid: Option<String>,
    #[serde(rename = "ARelationInfo")]
    pub relations: Option<Vec<RelationInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationInfo {
    #[serde(rename = "iChannel")]
    pub channel: u32,
    #[serde(rename = "sUserName")]
    pub username: String,
    #[serde(rename = "sPictureUrl")]
    pub picture_url: String,
    #[serde(rename = "iGender")]
    pub gender: u32,
    #[serde(rename = "iBindTime")]
    pub bind_time: String,
    #[serde(rename = "sChannelId")]
    pub channel_id: String,
}

/// Notice response from in-notice.globh.com
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoticeResponse {
    pub code: i32,
    pub desc: String,
    #[serde(rename = "noticeNum")]
    pub notice_num: u32,
    #[serde(rename = "noticelist")]
    pub notice_list: Vec<serde_json::Value>,
}

/// Payment/reward session init response from min-pay.globh.com
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaySessionResponse {
    pub ret: i32,
    pub get_ip: Option<GetIpResult>,
    pub get_key: Option<GetKeyResult>,
    pub info: Option<serde_json::Value>,
    pub order: Option<serde_json::Value>,
    pub provide: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetIpResult {
    pub ret: i32,
    pub info: Vec<IpInfo>,
    pub unipay_host: Option<String>,
    pub h5_host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpInfo {
    pub ip: String,
    pub province: String,
    pub cat: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetKeyResult {
    pub ret: i32,
    pub key_info: String,
    pub key_info_len: String,
    pub user_info: UserKeyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserKeyInfo {
    pub uin: String,
    pub uin_type: String,
    pub uin_len: u32,
    pub codeindex: u32,
}

/// CloudCtrl config response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudCtrlResponse {
    pub ret: i32,
    pub next_gap: u32,
    pub biz_data: Option<serde_json::Value>,
}

/// Channel constants (from captured iChannel values)
pub mod channels {
    pub const TWITTER: u32 = 35;
    pub const FACEBOOK: u32 = 28;
    pub const GOOGLE: u32 = 4;
    pub const GUEST: u32 = 99;
}

/// Authentication credential types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthCredential {
    Twitter {
        oauth_token: String,
        oauth_token_secret: String,
    },
    Facebook {
        access_token: String,
    },
    Google {
        id_token: String,
    },
    Guest {
        guest_id: String,
    },
}

impl AuthCredential {
    pub fn channel(&self) -> u32 {
        match self {
            Self::Twitter { .. } => channels::TWITTER,
            Self::Facebook { .. } => channels::FACEBOOK,
            Self::Google { .. } => channels::GOOGLE,
            Self::Guest { .. } => channels::GUEST,
        }
    }
}
