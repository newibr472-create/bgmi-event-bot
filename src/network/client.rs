use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, info};

use crate::core::crypto::{compute_valid_key, gen_session_token};
use crate::core::protocol::*;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str =
    "Dalvik/2.1.0 (Linux; U; Android 16; I2405 Build/BP2A.250605.031.A3_V000L1)";

/// Real BGMI network client using HTTPS to globh.com infrastructure.
/// Based on actual HttpCanary captures of BGMI 4.4.0 traffic.
pub struct BgmiClient {
    http: Client,
    device_id: String,
    model: String,
    brand: String,
    // auth state
    pub openid: Option<String>,
    pub inner_token: Option<String>,
    pub ticket: Option<String>,
    pub guid: Option<String>,
    pub session_token: Option<String>,
    pub pay_key: Option<String>,
    pub username: Option<String>,
    // stats
    requests_made: u64,
}

impl BgmiClient {
    pub fn new(device_id: &str, model: &str, brand: &str) -> Result<Self> {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .danger_accept_invalid_certs(false)
            .build()
            .context("failed to build http client")?;

        Ok(Self {
            http,
            device_id: device_id.to_string(),
            model: model.to_string(),
            brand: brand.to_string(),
            openid: None,
            inner_token: None,
            ticket: None,
            guid: None,
            session_token: None,
            pay_key: None,
            username: None,
            requests_made: 0,
        })
    }

    fn dinfo(&self) -> String {
        build_dinfo(&self.model, &self.brand, current_ts_ms())
    }

    /// Step 1: Login via ITOP SDK (in-sdkapi.globh.com/v1.0/user/login)
    /// Returns the login response with openid and innerToken
    pub async fn login(&mut self, cred: &AuthCredential, guest_id: &str) -> Result<LoginResponse> {
        let channel = cred.channel().to_string();
        let dinfo = self.dinfo();
        let game_id = GAME_ID.to_string();
        let platform = PLATFORM.to_string();

        let mut params: Vec<(&str, &str)> = vec![
            ("did", &self.device_id),
            ("dinfo", &dinfo),
            ("gameversion", GAME_VERSION),
            ("iChannel", &channel),
            ("iGameId", &game_id),
            ("iPlatform", &platform),
            ("package_name", "com.pubg.imobile"),
            ("sGuestId", guest_id),
            ("sOriginalId", guest_id),
            ("sdkversion", SDK_VERSION),
        ];

        // add credential-specific params
        let (tok, sec);
        match cred {
            AuthCredential::Twitter {
                oauth_token,
                oauth_token_secret,
            } => {
                tok = oauth_token.clone();
                sec = oauth_token_secret.clone();
                params.push(("oauthToken", &tok));
                params.push(("oauthTokenSecret", &sec));
            }
            AuthCredential::Facebook { access_token } => {
                tok = access_token.clone();
                params.push(("accessToken", &tok));
                sec = String::new();
            }
            AuthCredential::Google { id_token } => {
                tok = id_token.clone();
                params.push(("token", &tok));
                sec = String::new();
            }
            AuthCredential::Guest { guest_id: gid } => {
                tok = gid.clone();
                params.push(("sGuestId", &tok));
                sec = String::new();
            }
        }

        // compute signature
        let valid_key = compute_valid_key(&params);
        params.push(("sValidKey", &valid_key));
        params.push(("sRefer", ""));

        let url = format!("https://{}/v1.0/user/login", HOST_SDK_API);
        debug!("login request to {}", url);

        let resp = self
            .http
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("login request failed")?;

        self.requests_made += 1;

        let login_resp: LoginResponse = resp.json().await.context("failed to parse login response")?;

        if login_resp.code != 1 {
            anyhow::bail!("login failed: {} (code {})", login_resp.desc, login_resp.code);
        }

        info!("logged in as {} (openid={})", login_resp.username, login_resp.openid);
        self.openid = Some(login_resp.openid.clone());
        self.inner_token = Some(login_resp.inner_token.clone());
        self.guid = Some(login_resp.guid.clone());
        self.username = Some(login_resp.username.clone());

        Ok(login_resp)
    }

    /// Step 2: Get session ticket (in-sdkapi.globh.com/v1.0/user/getTicket)
    /// Must be called after login()
    pub async fn get_ticket(&mut self, guest_id: &str) -> Result<TicketResponse> {
        let openid = self.openid.clone().context("not logged in")?;
        let token = self.inner_token.clone().context("no inner token")?;
        let channel = channels::TWITTER.to_string(); // TODO: dynamic
        let dinfo = self.dinfo();
        let game_id = GAME_ID.to_string();
        let platform = PLATFORM.to_string();

        let mut params: Vec<(&str, &str)> = vec![
            ("did", &self.device_id),
            ("dinfo", &dinfo),
            ("gameversion", GAME_VERSION),
            ("iChannel", &channel),
            ("iGameId", &game_id),
            ("iOpenid", &openid),
            ("iPlatform", &platform),
            ("package_name", "com.pubg.imobile"),
            ("sGuestId", guest_id),
            ("sInnerToken", &token),
            ("sOriginalId", guest_id),
            ("sdkversion", SDK_VERSION),
        ];

        let valid_key = compute_valid_key(&params);
        params.push(("sValidKey", &valid_key));
        params.push(("sRefer", ""));

        let url = format!("https://{}/v1.0/user/getTicket", HOST_SDK_API);
        let resp = self
            .http
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("getTicket request failed")?;

        self.requests_made += 1;

        let ticket_resp: TicketResponse =
            resp.json().await.context("failed to parse ticket response")?;

        if ticket_resp.code != 1 {
            anyhow::bail!(
                "getTicket failed: {} (code {})",
                ticket_resp.desc,
                ticket_resp.code
            );
        }

        info!("got session ticket (len={})", ticket_resp.ticket.len());
        self.ticket = Some(ticket_resp.ticket.clone());
        Ok(ticket_resp)
    }

    /// Step 3: Initialize payment session (min-pay.globh.com)
    /// Gets encryption key for reward operations
    pub async fn init_pay_session(&mut self) -> Result<PaySessionResponse> {
        let openid = self.openid.clone().context("not logged in")?;
        let session_tok = gen_session_token();

        let pf = build_pf(&openid);

        // The encrypt_msg contains the initial handshake data
        // For get_key|get_ip command, the encrypted portion is minimal
        let body = [
            ("encrypt_msg", ""),
            ("xg_mid", &session_tok),
            ("openid", &openid),
            ("format", "json"),
            ("msg_len", "0"),
            ("amode", "1"),
            ("offer_id", OFFER_ID),
            ("session_token", &session_tok),
            ("extend", "wwzwz_goods_zoneid=1"),
            ("vid", "cpay_4.1.1"),
            ("pfkey", "pfKey"),
            ("key_time", ""),
            ("pf", &pf),
            ("zoneid", "1"),
            ("overseas_cmd", "get_key|get_ip"),
            ("goods_zoneid", "1"),
            ("get_key_type", "secret"),
            ("key_len", "newkey"),
        ];

        let url = format!(
            "https://{}/v1/r/{}/mobile_overseas_common",
            HOST_PAY, OFFER_ID
        );

        let resp = self
            .http
            .post(&url)
            .header("Accept-Charset", "UTF-8")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&body)
            .send()
            .await
            .context("pay session init failed")?;

        self.requests_made += 1;

        let pay_resp: PaySessionResponse =
            resp.json().await.context("failed to parse pay response")?;

        if pay_resp.ret != 0 {
            anyhow::bail!("pay session init failed: ret={}", pay_resp.ret);
        }

        if let Some(ref key_result) = pay_resp.get_key {
            info!(
                "got pay session key (len={}, uin_type={})",
                key_result.key_info_len, key_result.user_info.uin_type
            );
            self.pay_key = Some(key_result.key_info.clone());
        }

        self.session_token = Some(session_tok);
        Ok(pay_resp)
    }

    /// Get game notices/events
    pub async fn get_notices(&self, guest_id: &str) -> Result<NoticeResponse> {
        let openid = self.openid.as_ref().context("not logged in")?;
        let dinfo = self.dinfo();
        let channel = channels::TWITTER.to_string();
        let game_id = GAME_ID.to_string();
        let platform = PLATFORM.to_string();

        let mut params: Vec<(&str, &str)> = vec![
            ("did", &self.device_id),
            ("dinfo", &dinfo),
            ("gameversion", GAME_VERSION),
            ("iChannel", &channel),
            ("iGameId", &game_id),
            ("iOpenid", openid),
            ("iPartition", "91"),
            ("iPlatform", &platform),
            ("iRegion", "0"),
            ("package_name", "com.pubg.imobile"),
            ("sExtra", ""),
            ("sGuestId", guest_id),
            ("sLang", "en-US"),
            ("sOriginalId", guest_id),
            ("sVersion", "4.4.0.21175"),
            ("sdkversion", SDK_VERSION),
        ];

        let valid_key = compute_valid_key(&params);
        params.push(("sValidKey", &valid_key));
        params.push(("sRefer", ""));

        let url = format!("https://{}/v1.0/notice/getNotice", HOST_NOTICE);
        let resp = self
            .http
            .get(&url)
            .query(&params)
            .send()
            .await
            .context("getNotice failed")?;

        resp.json().await.context("failed to parse notice response")
    }

    /// Get cloud config
    pub async fn get_cloud_config(&self) -> Result<CloudCtrlResponse> {
        let com_params = serde_json::json!({
            "app_ver": format!("{}.21125", GAME_VERSION),
            "bid": "com.pubg.imobile",
            "cid": "",
            "date": chrono::Utc::now().format("%Y-%m-%d").to_string(),
            "did": "26B11D2FC11082E9D2BD192B7CA6F61",
            "gid": self.openid.as_deref().unwrap_or(""),
            "os": "Android",
            "os_ver": "16",
            "plat": "android",
            "sdk_ver": SDK_VERSION,
        });

        let url = format!("https://{}/cfgpush/getConfig", HOST_CLOUD_CTRL);
        let resp = self
            .http
            .get(&url)
            .query(&[("com_params", serde_json::to_string(&com_params)?)])
            .send()
            .await
            .context("cloud config request failed")?;

        resp.json().await.context("failed to parse cloud config")
    }

    /// Send a command to the payment/reward system with encrypted message
    pub async fn send_pay_command(
        &self,
        cmd: &str,
        payload_json: &str,
    ) -> Result<serde_json::Value> {
        let openid = self.openid.as_ref().context("not logged in")?;
        let session_tok = self.session_token.as_ref().context("no session token")?;
        let key = self.pay_key.as_ref().context("no pay key")?;

        // encrypt the payload
        let encrypted = crate::core::crypto::encrypt_pay_message(payload_json.as_bytes(), key)?;
        let msg_len = payload_json.len().to_string();
        let pf = build_pf(openid);

        let body = [
            ("encrypt_msg", encrypted.as_str()),
            ("xg_mid", session_tok),
            ("openid", openid),
            ("format", "json"),
            ("msg_len", &msg_len),
            ("amode", "1"),
            ("offer_id", OFFER_ID),
            ("session_token", session_tok),
            ("vid", "cpay_4.1.1"),
            ("pfkey", "pfKey"),
            ("pf", &pf),
            ("zoneid", "1"),
            ("overseas_cmd", cmd),
            ("goods_zoneid", "1"),
        ];

        let url = format!(
            "https://{}/v1/r/{}/mobile_overseas_common",
            HOST_PAY, OFFER_ID
        );

        let resp = self
            .http
            .post(&url)
            .header("Accept-Charset", "UTF-8")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&body)
            .send()
            .await
            .context("pay command failed")?;

        resp.json().await.context("failed to parse pay command response")
    }

    /// Send telemetry log (min-pay.globh.com/cgi-bin/log_data.fcg)
    /// This mimics normal app behavior to avoid detection
    pub async fn send_telemetry(&self, event_name: &str) -> Result<()> {
        let openid = self.openid.as_ref().context("not logged in")?;
        let session_tok = self.session_token.as_deref().unwrap_or("unknown");
        let pf = build_pf(openid);

        let record = format!(
            "|8=name%3D{}%26result%3D0|21=sdk.centauri.api.resp|38={}|13=12|3={}|7=0|24={}|26={}|29={}|31=androidoversea_v4.06.151|37=hy_gameid|43=st_dummy",
            event_name,
            current_ts_ms(),
            openid,
            OFFER_ID,
            pf,
            session_tok
        );

        let body = format!("num=1&record0={}", record);

        let url = format!(
            "https://{}/cgi-bin/log_data.fcg?offer_id={}",
            HOST_PAY, OFFER_ID
        );

        self.http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .context("telemetry failed")?;

        Ok(())
    }

    pub fn stats(&self) -> u64 {
        self.requests_made
    }

    pub fn is_authenticated(&self) -> bool {
        self.openid.is_some() && self.inner_token.is_some()
    }

    pub fn has_pay_session(&self) -> bool {
        self.pay_key.is_some()
    }
}
