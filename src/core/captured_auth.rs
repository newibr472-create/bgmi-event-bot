/// Pre-captured authenticated request parameters.
/// The sValidKey signatures are computed by libsigner.so (native library).
/// Since we can't replicate the signing algorithm without the native key,
/// we use pre-captured parameter sets that include valid signatures.
///
/// These work because:
/// 1. The sValidKey is computed over the params (including dinfo timestamp)
/// 2. As long as we don't change the params, the signature remains valid
/// 3. The server doesn't check if the dinfo timestamp is "current"
///
/// To support multiple accounts, each account needs its own captured request params.

pub struct CapturedAuth {
    pub login_params: &'static [(&'static str, &'static str)],
    pub ticket_params: &'static [(&'static str, &'static str)],
    pub init_encrypt_msg: &'static str,
}

/// Default captured auth for the test account (Twitter: "jone sins")
pub static DEFAULT_AUTH: CapturedAuth = CapturedAuth {
    login_params: LOGIN_QUERY,
    ticket_params: TICKET_QUERY,
    init_encrypt_msg: INIT_ENCRYPT_MSG,
};

/// Login parameters with pre-computed sValidKey
pub const LOGIN_QUERY: &[(&str, &str)] = &[
    ("did", "fb3a2c45-9bf3-484e-9842-4f76647ef40a"),
    ("dinfo", "1|40455|I2405|en|4.4.0|1780685377880|2.625|2400*1080|iQOO"),
    ("gameversion", "4.4.0"),
    ("iChannel", "35"),
    ("iGameId", "1450"),
    ("iPlatform", "2"),
    ("oauthToken", "2059972254298206209-qcUz8RcfqJVWAP7gPMcByu007GpSDC"),
    ("oauthTokenSecret", "5Lpa3xOvxLxgSISgjJNudb2NGn9IXYjAbSrFjDD0LOa4o"),
    ("package_name", "com.pubg.imobile"),
    ("sGuestId", "54eeb06c8dbc49fd6ce56879d5102dae"),
    ("sOriginalId", "54eeb06c8dbc49fd6ce56879d5102dae"),
    ("sValidKey", "6852acb206097241beef701fbac9ad6e"),
    ("sdkversion", "2.10.3"),
    ("sRefer", ""),
];

/// GetTicket parameters with pre-computed sValidKey
/// Note: iOpenid is dynamically inserted, but must be "19112301001311658" for this signature
pub const TICKET_QUERY: &[(&str, &str)] = &[
    ("did", "fb3a2c45-9bf3-484e-9842-4f76647ef40a"),
    ("dinfo", "1|40455|I2405|en|4.4.0|1780685378788|2.625|2400*1080|iQOO"),
    ("gameversion", "4.4.0"),
    ("iChannel", "35"),
    ("iGameId", "1450"),
    ("iOpenid", "19112301001311658"),
    ("iPlatform", "2"),
    ("package_name", "com.pubg.imobile"),
    ("sGuestId", "54eeb06c8dbc49fd6ce56879d5102dae"),
    ("sInnerToken", "351cf6d5d921b0dcf25867ca04546e28"),
    ("sOriginalId", "54eeb06c8dbc49fd6ce56879d5102dae"),
    ("sValidKey", "c3679668c1d38abcc3f46e309cc17cdc"),
    ("sdkversion", "2.10.3"),
    ("sRefer", ""),
];

/// Captured encrypt_msg for pay session initialization (get_key|get_ip)
pub const INIT_ENCRYPT_MSG: &str = "B67F43FA5BDE92084276A3701DDA1FA02439913E272C6598D6B1E4BDB089E3C39FDDBEADD001680BD104E549B952E33351185C99457D4B0C0BAB317BBA469148E57F3F74F5DE21A8C3FBD39005A419F99790208C071235A1D9C1F44656BF0F19783579CB9FBF1697017B3F8BF460C3FE21CB3FEAD73D62354BAE5FE084785A8B964CAE0D1F04ECBB029BE72990EA626FB91BCD79601A5898D662C28DFDA715AD3C2B591B9C2090EED2EF9B9E799F2FAF21D818E0F4A90E54FAE9F1CBD25996A00987EB11BA9C31DADC0AB5FCEE8814B5124F12C70F63D9210BA3CA5B00508260F83E308627768F48727AB7809C6A677B323D781AE6F24C4FA96CB7D2D6C19761900B0BEC4FC0E0EDE342EEA6D2F6CC1FF3F49F66BC74A3EA9E16FED0BF5B363FA40A8F32D8F21F1A2B56C107E571FACF6E56C64D5357452F9AE2471358D8F77569406C4BB21FDEF7D080E01D9E34817AA6E297E56617F26AF67C54591AFBC7FC5B356CE1B11DB82DA130353CED9BBD26B02C2F3AE1103274DB86720CDC1F5FAC48EECDD5F5013725FC10E2AE4A1234A961D15237FACEF85ABCF891B1D79A0670E61911A65859302BC8F790A8489194C3152E5A0965370F9311BEFA917F1AD7F423FDF1D108CA3470DDA7A8621CB1FF0F";
