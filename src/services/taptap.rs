use anyhow::Result;
use base64::prelude::{BASE64_STANDARD, Engine as _};
use hmac::Mac;
use percent_encoding::{NON_ALPHANUMERIC, percent_encode};
use rand::{RngCore, SeedableRng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::services::leancloud::LeanCloudService;

#[derive(Debug, Serialize, Deserialize)]
pub struct TapTapQrCodeResponse {
    pub qrcode_url: String,
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(serde::Deserialize)]
struct Wrap<T> {
    success: bool,
    data: T,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct TapTapToken {
    pub kid: String,
    pub mac_key: String,
    pub access_token: Option<String>,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(serde::Deserialize)]
struct Account {
    openid: String,
    unionid: String,
}

fn mac(token: &TapTapToken) -> String {
    let ts: u64 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let nonce: u32 = rand::rngs::SmallRng::seed_from_u64(ts).next_u32();
    let input: String = format!("{}\n{}\nGET\n/account/basic-info/v1?client_id=rAK3FfdieFob2Nn8Am\nopen.tapapis.cn\n443\n\n", ts, nonce);
    let mut mac = hmac::Hmac::<sha1::Sha1>::new_from_slice(token.mac_key.as_bytes()).unwrap();
    mac.update(input.as_bytes());
    let mac_string: String = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    format!("MAC id=\"{}\",ts=\"{}\",nonce=\"{}\",mac=\"{}\"", token.kid, ts, nonce, mac_string)
}

pub struct TapTapService {
    client: Client,
    leancloud_service: LeanCloudService,
}

impl TapTapService {
    pub fn new() -> Self {
        TapTapService {
            client: Client::builder()
                .http1_title_case_headers()
                .user_agent("TapTapUnitySDK/1.0 UnityPlayer/2021.3.40f1c1")
                .build()
                .unwrap(),
            leancloud_service: LeanCloudService::new(),
        }
    }

    pub async fn request_login_qr_code(&self, device_id: &str) -> Result<Value> {
        let response: Wrap<Value> = self.client.post("https://www.taptap.com/oauth2/v1/device/code")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", "TapTapAndroidSDK/3.16.5")
            .body(format!("client_id=rAK3FfdieFob2Nn8Am&response_type=device_code&scope=basic_info&version=1.2.0&platform=unity&info=%7b%22device_id%22%3a%22{}%22%7d", percent_encoding::percent_encode(device_id.as_bytes(), percent_encoding::NON_ALPHANUMERIC)))
            .send().await?.json().await?;
        Ok(response.data)
    }

    pub async fn check_qr_code_result(&self, device_code: &str, device_id: &str) -> Result<Value> {
        let response = self.client.post("https://www.taptap.cn/oauth2/v1/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", "TapTapAndroidSDK/3.16.5")
            .body(format!("grant_type=device_token&client_id=rAK3FfdieFob2Nn8Am&secret_type=hmac-sha-1&code={}&version=1.0&platform=unity&info=%7b%22device_id%22%3a%22{}%22%7d", device_code, percent_encoding::percent_encode(device_id.as_bytes(), percent_encoding::NON_ALPHANUMERIC)))
            .send().await?.json::<Wrap<Value>>().await?;

        if !response.success {
            return Ok(response.data);
        }

        let token: TapTapToken = serde_json::from_value(response.data)?;
        let account: Account = self.client.get("https://open.tapapis.cn/account/basic-info/v1?client_id=rAK3FfdieFob2Nn8Am")
            .header("User-Agent", "TapTapAndroidSDK/3.16.5")
            .header("Authorization", mac(&token))
            .send().await?.json::<Wrap<Account>>().await?.data;

        self.leancloud_service.login_with_taptap(&token, &account.openid, &account.unionid).await
    }

    pub async fn get_profile(&self, authorization: &str) -> Result<String> {
        let response = self.client.get("https://open.tapapis.cn/account/basic-info/v1?client_id=rAK3FfdieFob2Nn8Am")
            .header("User-Agent", "TapTapAndroidSDK/3.16.5")
            .header("Authorization", authorization)
            .send().await?.text().await?;
        Ok(response)
    }
}
