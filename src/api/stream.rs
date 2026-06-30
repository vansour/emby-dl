const PLAYBACK_INFO_PATH: &str = "/emby/Items/{}/PlaybackInfo";

use crate::api::client::EmbyClient;
use crate::api::items::MediaSourceInfo;

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackInfoResponse {
    media_sources: Vec<MediaSourceInfo>,
}

impl EmbyClient {
    pub async fn get_playback_info(&self, item_id: &str) -> anyhow::Result<Vec<MediaSourceInfo>> {
        let path = PLAYBACK_INFO_PATH.replace("{}", item_id);
        let query = &[("UserId", &self.auth.user_id as &str)];
        let resp: PlaybackInfoResponse = self.get_json(&path, query).await?;
        Ok(resp.media_sources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_playback_info_response() {
        let json = r#"{
            "MediaSources": [
                {
                    "Id": "src1",
                    "Container": "mkv",
                    "MediaStreams": [
                        { "Type": "Video", "Index": 0, "Codec": "h264" }
                    ]
                }
            ]
        }"#;
        let resp: PlaybackInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.media_sources.len(), 1);
        assert_eq!(resp.media_sources[0].id, "src1");
        assert_eq!(resp.media_sources[0].container.as_deref(), Some("mkv"));
    }

    #[test]
    fn deserialize_playback_info_empty_sources() {
        let json = r#"{"MediaSources": []}"#;
        let resp: PlaybackInfoResponse = serde_json::from_str(json).unwrap();
        assert!(resp.media_sources.is_empty());
    }

    #[test]
    fn deserialize_playback_info_multiple_sources() {
        let json = r#"{
            "MediaSources": [
                { "Id": "src1", "MediaStreams": [] },
                { "Id": "src2", "MediaStreams": [] }
            ]
        }"#;
        let resp: PlaybackInfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.media_sources.len(), 2);
        assert_eq!(resp.media_sources[0].id, "src1");
        assert_eq!(resp.media_sources[1].id, "src2");
    }
}
