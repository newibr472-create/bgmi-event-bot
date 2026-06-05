use serde::{Deserialize, Serialize};

use crate::core::protocol::AuthCredential;

/// Represents a BGMI account with all required auth data.
/// Based on real captured parameters from HttpCanary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub label: String,
    pub credential: StoredCredential,
    pub device: DeviceProfile,
    // populated after login
    #[serde(default)]
    pub openid: Option<String>,
    #[serde(default)]
    pub inner_token: Option<String>,
    #[serde(default)]
    pub guid: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub guest_id: Option<String>,
    #[serde(default)]
    pub last_login: Option<u64>,
    #[serde(default)]
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoredCredential {
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
    Guest,
}

impl StoredCredential {
    pub fn to_auth_credential(&self) -> AuthCredential {
        match self {
            Self::Twitter {
                oauth_token,
                oauth_token_secret,
            } => AuthCredential::Twitter {
                oauth_token: oauth_token.clone(),
                oauth_token_secret: oauth_token_secret.clone(),
            },
            Self::Facebook { access_token } => AuthCredential::Facebook {
                access_token: access_token.clone(),
            },
            Self::Google { id_token } => AuthCredential::Google {
                id_token: id_token.clone(),
            },
            Self::Guest => AuthCredential::Guest {
                guest_id: String::new(),
            },
        }
    }
}

/// Device fingerprint - must match a real device to avoid detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub device_id: String,     // UUID format
    pub model: String,         // e.g. "I2405"
    pub brand: String,         // e.g. "iQOO"
    pub android_version: u32,  // e.g. 16
    pub screen_density: f32,   // e.g. 2.625
    pub screen_resolution: String, // e.g. "2400*1080"
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self {
            device_id: uuid::Uuid::new_v4().to_string(),
            model: "I2405".to_string(),
            brand: "iQOO".to_string(),
            android_version: 16,
            screen_density: 2.625,
            screen_resolution: "2400*1080".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    Ready,
    LoggingIn,
    Active,
    Collecting,
    Cooldown,
    Error,
    Banned,
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self::Ready
    }
}

impl std::fmt::Display for AccountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "ready"),
            Self::LoggingIn => write!(f, "logging_in"),
            Self::Active => write!(f, "active"),
            Self::Collecting => write!(f, "collecting"),
            Self::Cooldown => write!(f, "cooldown"),
            Self::Error => write!(f, "error"),
            Self::Banned => write!(f, "banned"),
        }
    }
}

impl Account {
    pub fn new(label: &str, credential: StoredCredential, device: DeviceProfile) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let guest_id = md5_hex(&id);

        Self {
            id,
            label: label.to_string(),
            credential,
            device,
            openid: None,
            inner_token: None,
            guid: None,
            username: None,
            guest_id: Some(guest_id),
            last_login: None,
            status: AccountStatus::Ready,
        }
    }

    pub fn guest_id(&self) -> &str {
        self.guest_id.as_deref().unwrap_or("0000000000000000")
    }
}

fn md5_hex(input: &str) -> String {
    let hash = md5::compute(input.as_bytes());
    format!("{:x}", hash)
}
