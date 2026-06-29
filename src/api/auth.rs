const AUTH_PATH: &str = "/emby/Users/AuthenticateByName";

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub access_token: String,
    pub user_id: String,
    pub server_url: String,
    pub username: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LoginResponse {
    access_token: String,
    user: LoginUser,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LoginUser {
    id: String,
    name: String,
}

pub async fn authenticate(
    client: &reqwest::Client,
    server_url: &str,
    username: &str,
    password: &str,
) -> anyhow::Result<AuthInfo> {
    let url = format!(
        "{}{}",
        server_url.trim_end_matches('/'),
        AUTH_PATH
    );

    let body = serde_json::json!({
        "Username": username,
        "Pw": password,
    });

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "X-Emby-Authorization",
            "MediaBrowser Client=\"emby-dl\", Device=\"CLI\", DeviceId=\"emby-dl\", Version=\"0.1.0\"",
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("认证请求失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await
            .unwrap_or_else(|e| format!("(读取错误响应失败: {})", e));
        return Err(anyhow::anyhow!("认证失败 ({}): {}", status, text));
    }

    let login: LoginResponse = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("解析认证响应失败: {}", e))?;

    Ok(AuthInfo {
        access_token: login.access_token,
        user_id: login.user.id,
        server_url: server_url.trim_end_matches('/').to_string(),
        username: login.user.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_login_response() {
        let json = r#"{
            "AccessToken": "abc123token",
            "User": {
                "Id": "user_001",
                "Name": "testuser"
            }
        }"#;
        let resp: LoginResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "abc123token");
        assert_eq!(resp.user.id, "user_001");
        assert_eq!(resp.user.name, "testuser");
    }

    #[test]
    fn trim_server_url() {
        assert_eq!("https://example.com/".trim_end_matches('/'), "https://example.com");
        assert_eq!("https://example.com".trim_end_matches('/'), "https://example.com");
    }
}
