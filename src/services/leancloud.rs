use crate::services::taptap::TapTapToken;
use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

pub struct LeanCloudService {
    client: Client,
    phi: reqwest::header::HeaderMap,
}

impl LeanCloudService {
    pub fn new() -> Self {
        let mut phi = reqwest::header::HeaderMap::new();
        phi.append("User-Agent", "LeanCloud-CSharp-SDK/1.0.3".parse().unwrap());
        phi.append("X-LC-Id", "rAK3FfdieFob2Nn8Am".parse().unwrap());
        phi.append(
            "X-LC-Key",
            "Qr9AEqtuoSVS3zeD6iVbM4ZC0AtkJcQ89tywVyi0".parse().unwrap(),
        );
        phi.append("Content-Type", "application/json".parse().unwrap());
        LeanCloudService {
            client: Client::new(),
            phi,
        }
    }

    pub async fn login_with_taptap(
        &self,
        token: &TapTapToken,
        openid: &str,
        unionid: &str,
    ) -> Result<Value> {
        let body = format!(
            r#"{{"authData":{{"taptap":{{"kid":"{}","access_token":"{}","token_type":"mac","mac_key":"{}","mac_algorithm":"hmac-sha-1","openid":"{}","unionid":"{}"}}}}}}"#,
            token.kid, token.kid, token.mac_key, openid, unionid
        );
        let response = self
            .client
            .post("https://rak3ffdi.cloud.tds1.tapapis.cn/1.1/users")
            .headers(self.phi.clone())
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(response)
    }
}
