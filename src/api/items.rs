use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EmbyItems {
    pub items: Vec<EmbyItem>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
pub struct EmbyItem {
    pub id: String,
    pub name: String,
    #[serde(rename = "Type")]
    pub item_type: Option<String>,
    pub series_name: Option<String>,
    pub season_name: Option<String>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub production_year: Option<i32>,
    pub overview: Option<String>,
    pub container: Option<String>,
    pub media_sources: Option<Vec<MediaSourceInfo>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
pub struct MediaSourceInfo {
    pub id: String,
    pub name: Option<String>,
    pub container: Option<String>,
    pub path: Option<String>,
    pub size: Option<i64>,
    pub media_streams: Vec<MediaStream>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
pub struct MediaStream {
    #[serde(rename = "Type")]
    pub stream_type: String,
    pub index: i32,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub display_title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct View {
    pub id: String,
    pub name: String,
    pub collection_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_movie() {
        let json = r#"{
            "Id": "123",
            "Name": "The Matrix",
            "Type": "Movie",
            "ProductionYear": 1999,
            "Container": "mkv",
            "MediaSources": [
                {
                    "Id": "src1",
                    "Container": "mkv",
                    "Size": 1234567890,
                    "MediaStreams": [
                        { "Type": "Video", "Index": 0, "Codec": "h264", "Language": null, "DisplayTitle": null }
                    ]
                }
            ]
        }"#;
        let item: EmbyItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, "123");
        assert_eq!(item.name, "The Matrix");
        assert_eq!(item.item_type.as_deref(), Some("Movie"));
        assert_eq!(item.production_year, Some(1999));
        assert_eq!(item.container.as_deref(), Some("mkv"));
        assert!(item.series_name.is_none());

        let sources = item.media_sources.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "src1");
        assert_eq!(sources[0].container.as_deref(), Some("mkv"));
        assert_eq!(sources[0].size, Some(1234567890));
        assert_eq!(sources[0].media_streams.len(), 1);
        assert_eq!(sources[0].media_streams[0].stream_type, "Video");
        assert_eq!(sources[0].media_streams[0].codec.as_deref(), Some("h264"));
    }

    #[test]
    fn deserialize_episode() {
        let json = r#"{
            "Id": "456",
            "Name": "Winter Is Coming",
            "Type": "Episode",
            "SeriesName": "Game of Thrones",
            "SeasonName": "Season 1",
            "IndexNumber": 1,
            "ParentIndexNumber": 1,
            "ProductionYear": 2011
        }"#;
        let item: EmbyItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, "456");
        assert_eq!(item.name, "Winter Is Coming");
        assert_eq!(item.series_name.as_deref(), Some("Game of Thrones"));
        assert_eq!(item.season_name.as_deref(), Some("Season 1"));
        assert_eq!(item.index_number, Some(1));
        assert_eq!(item.parent_index_number, Some(1));
        assert_eq!(item.production_year, Some(2011));
    }

    #[test]
    fn deserialize_emby_items_wrapper() {
        let json = r#"{
            "Items": [
                { "Id": "1", "Name": "Movie A" },
                { "Id": "2", "Name": "Movie B" }
            ]
        }"#;
        let resp: EmbyItems = serde_json::from_str(json).unwrap();
        assert_eq!(resp.items.len(), 2);
        assert_eq!(resp.items[0].name, "Movie A");
        assert_eq!(resp.items[1].id, "2");
    }

    #[test]
    fn deserialize_view() {
        let json = r#"{
            "Id": "lib1",
            "Name": "Movies",
            "CollectionType": "movies"
        }"#;
        let view: View = serde_json::from_str(json).unwrap();
        assert_eq!(view.id, "lib1");
        assert_eq!(view.name, "Movies");
        assert_eq!(view.collection_type.as_deref(), Some("movies"));
    }

    #[test]
    fn deserialize_view_no_collection_type() {
        let json = r#"{
            "Id": "lib2",
            "Name": "My Library"
        }"#;
        let view: View = serde_json::from_str(json).unwrap();
        assert_eq!(view.id, "lib2");
        assert_eq!(view.name, "My Library");
        assert!(view.collection_type.is_none());
    }

    #[test]
    fn deserialize_media_stream_with_language() {
        let json = r#"{
            "Type": "Audio",
            "Index": 1,
            "Codec": "aac",
            "Language": "eng",
            "DisplayTitle": "English"
        }"#;
        let stream: MediaStream = serde_json::from_str(json).unwrap();
        assert_eq!(stream.stream_type, "Audio");
        assert_eq!(stream.index, 1);
        assert_eq!(stream.codec.as_deref(), Some("aac"));
        assert_eq!(stream.language.as_deref(), Some("eng"));
        assert_eq!(stream.display_title.as_deref(), Some("English"));
    }

    #[test]
    fn deserialize_item_minimal() {
        let json = r#"{"Id": "789", "Name": "Minimal"}"#;
        let item: EmbyItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.id, "789");
        assert_eq!(item.name, "Minimal");
        assert!(item.item_type.is_none());
        assert!(item.production_year.is_none());
        assert!(item.media_sources.is_none());
    }

    #[test]
    fn deserialize_media_source_multiple_streams() {
        let json = r#"{
            "Id": "src2",
            "Container": "mkv,mp4",
            "MediaStreams": [
                { "Type": "Video", "Index": 0, "Codec": "h265" },
                { "Type": "Audio", "Index": 1, "Codec": "aac", "Language": "jpn" },
                { "Type": "Subtitle", "Index": 2, "Language": "chi" }
            ]
        }"#;
        let source: MediaSourceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(source.container.as_deref(), Some("mkv,mp4"));
        assert_eq!(source.media_streams.len(), 3);
        assert_eq!(source.media_streams[1].stream_type, "Audio");
        assert_eq!(source.media_streams[1].language.as_deref(), Some("jpn"));
        assert_eq!(source.media_streams[2].stream_type, "Subtitle");
    }
}
