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
        phi.append("User-Agent", "LeanCloud-CSharp-SDK/1.0.3".parse().expect("无法解析User-Agent头"));
        phi.append("X-LC-Id", "rAK3FfdieFob2Nn8Am".parse().expect("无法解析X-LC-Id头"));
        phi.append(
            "X-LC-Key",
            "Qr9AEqtuoSVS3zeD6iVbM4ZC0AtkJcQ89tywVyi0".parse().expect("无法解析X-LC-Key头"),
        );
        phi.append("Content-Type", "application/json".parse().expect("无法解析Content-Type头"));
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
        // 使用 serde_json 构建安全的 JSON 请求体
        let body = serde_json::json!({
            "authData": {
                "taptap": {
                    "kid": token.kid,
                    "access_token": token.kid,
                    "token_type": "mac",
                    "mac_key": token.mac_key,
                    "mac_algorithm": "hmac-sha-1",
                    "openid": openid,
                    "unionid": unionid
                }
            }
        }).to_string();
        
        let response = self
            .client
            .post("https://rak3ffdi.cloud.tds1.tapapis.cn/1.1/users")
            .headers(self.phi.clone())  // HeaderMap 的克隆操作相对轻量，且 headers 方法需要所有权
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(response)
    }
}
