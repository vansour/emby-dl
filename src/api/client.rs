const VIEWS_PATH: &str = "/emby/Users/{}/Views";
const ITEMS_PATH: &str = "/emby/Users/{}/Items";
const ITEM_PATH: &str = "/emby/Users/{}/Items/{}";
const SERIES_SEASONS_PATH: &str = "/emby/Shows/{}/Seasons";
const STREAM_PATH: &str = "/emby/Videos/{}/stream?Static=true&MediaSourceId={}";

use crate::api::auth::AuthInfo;
use crate::api::items::*;
use serde::de::DeserializeOwned;

pub struct EmbyClient {
    pub http: reqwest::Client,
    pub auth: AuthInfo,
}

impl EmbyClient {
    pub fn new(http: reqwest::Client, auth: AuthInfo) -> Self {
        Self { http, auth }
    }

    fn headers(&self) -> anyhow::Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "X-Emby-Token",
            reqwest::header::HeaderValue::from_str(&self.auth.access_token)
                .map_err(|e| anyhow::anyhow!("无效的 access token: {}", e))?,
        );
        Ok(headers)
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let base = format!("{}{}", self.auth.server_url, path);
        let mut url = reqwest::Url::parse(&base)
            .map_err(|e| anyhow::anyhow!("无效 URL {}: {}", base, e))?;
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        let resp = self
            .http
            .get(url)
            .headers(self.headers()?)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("请求失败: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await
                .unwrap_or_else(|e| format!("(读取错误响应失败: {})", e));
            return Err(anyhow::anyhow!("请求失败 ({}): {}", status, text));
        }

        resp.json()
            .await
            .map_err(|e| anyhow::anyhow!("解析响应失败: {}", e))
    }

    pub async fn list_views(&self) -> anyhow::Result<Vec<View>> {
        let path = VIEWS_PATH.replace("{}", &self.auth.user_id);
        let resp: EmbyViewResponse = self.get_json(&path, &[]).await?;
        Ok(resp.items)
    }

    pub async fn search_items(
        &self,
        query: &str,
        parent_id: Option<&str>,
        limit: i32,
    ) -> anyhow::Result<Vec<EmbyItem>> {
        let limit_str = limit.to_string();
        let mut all_params: Vec<(&str, &str)> = vec![
            ("SearchTerm", query),
            ("Recursive", "true"),
            ("Limit", &limit_str),
        ];
        if let Some(pid) = parent_id {
            all_params.push(("ParentId", pid));
        }

        let path = ITEMS_PATH.replace("{}", &self.auth.user_id);
        let resp: EmbyItems = self.get_json(&path, &all_params).await?;
        Ok(resp.items)
    }

    pub async fn get_series_seasons(&self, series_id: &str) -> anyhow::Result<Vec<EmbyItem>> {
        let path = SERIES_SEASONS_PATH.replace("{}", series_id);
        let resp: EmbyItems = self.get_json(&path, &[
            ("UserId", &self.auth.user_id),
            ("Fields", "SeriesName,IndexNumber"),
        ]).await?;
        Ok(resp.items)
    }

    pub async fn get_child_items(&self, parent_id: &str, include_types: &str) -> anyhow::Result<Vec<EmbyItem>> {
        let path = ITEMS_PATH.replace("{}", &self.auth.user_id);
        let resp: EmbyItems = self.get_json(&path, &[
            ("ParentId", parent_id),
            ("IncludeItemTypes", include_types),
            ("Fields", "SeriesName,ParentIndexNumber,IndexNumber"),
        ]).await?;
        Ok(resp.items)
    }

    pub async fn get_item(&self, item_id: &str) -> anyhow::Result<EmbyItem> {
        let path = ITEM_PATH
            .replacen("{}", &self.auth.user_id, 1)
            .replacen("{}", item_id, 1);
        self.get_json(&path, &[]).await
    }

    pub fn build_stream_url(&self, item_id: &str, source_id: &str) -> String {
        format!(
            "{}{}",
            self.auth.server_url,
            STREAM_PATH
                .replacen("{}", item_id, 1)
                .replacen("{}", source_id, 1)
        )
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyViewResponse {
    items: Vec<View>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::AuthInfo;

    fn test_client() -> EmbyClient {
        EmbyClient::new(
            reqwest::Client::new(),
            AuthInfo {
                access_token: "test_token".into(),
                user_id: "user_1".into(),
                server_url: "https://example.com".into(),
                username: "test".into(),
            },
        )
    }

    #[test]
    fn build_stream_url_uses_static() {
        let client = test_client();
        let url = client.build_stream_url("item_42", "src_abc");
        assert_eq!(url, "https://example.com/emby/Videos/item_42/stream?Static=true&MediaSourceId=src_abc");
    }

    #[test]
    fn headers_contains_token() {
        let client = test_client();
        let headers = client.headers().unwrap();
        assert_eq!(
            headers.get("X-Emby-Token").unwrap().to_str().unwrap(),
            "test_token"
        );
    }

    #[test]
    fn new_sets_auth() {
        let info = AuthInfo {
            access_token: "tok".into(),
            user_id: "uid".into(),
            server_url: "https://srv".into(),
            username: "usr".into(),
        };
        let client = EmbyClient::new(reqwest::Client::new(), info);
        assert_eq!(client.auth.access_token, "tok");
        assert_eq!(client.auth.server_url, "https://srv");
    }
}
