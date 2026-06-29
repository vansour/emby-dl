use crate::api::items::EmbyItem;

const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];

pub fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if INVALID_CHARS.contains(&c) { ' ' } else { c })
        .collect();
    let trimmed: String = s.split_whitespace().fold(String::new(), |mut acc, w| {
        if !acc.is_empty() {
            acc.push(' ');
        }
        acc.push_str(w);
        acc
    });
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed
    }
}

pub fn build_item_filename(item: &EmbyItem, ext: &str) -> String {
    let name = sanitize(&item.name);
    if let Some(series) = &item.series_name {
        let season = item.parent_index_number.unwrap_or(1);
        let episode = item.index_number.unwrap_or(1);
        format!(
            "{} - S{:02}E{:02} - {}.{}",
            sanitize(series),
            season,
            episode,
            name,
            ext
        )
    } else {
        let year = item
            .production_year
            .map(|y| format!(" ({})", y))
            .unwrap_or_default();
        format!("{}{}.{}", name, year, ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::items::EmbyItem;

    fn make_item(name: &str, series: Option<&str>, season: Option<i32>, episode: Option<i32>, year: Option<i32>) -> EmbyItem {
        EmbyItem {
            id: "1".into(),
            name: name.into(),
            item_type: None,
            series_name: series.map(|s| s.into()),
            season_name: None,
            index_number: episode,
            parent_index_number: season,
            production_year: year,
            overview: None,
            container: None,
            media_sources: None,
        }
    }

    #[test]
    fn sanitize_removes_invalid_chars() {
        assert_eq!(sanitize("a/b:c*d?e\"f<g>h|i\0j"), "a b c d e f g h i j");
    }

    #[test]
    fn sanitize_handles_multiple_spaces() {
        assert_eq!(sanitize("a   b"), "a b");
    }

    #[test]
    fn sanitize_handles_leading_trailing_whitespace() {
        assert_eq!(sanitize("  hello  "), "hello");
    }

    #[test]
    fn sanitize_returns_untitled_for_empty() {
        assert_eq!(sanitize(""), "untitled");
    }

    #[test]
    fn sanitize_returns_untitled_for_only_invalid() {
        assert_eq!(sanitize(":*?"), "untitled");
    }

    #[test]
    fn sanitize_allows_normal_text() {
        assert_eq!(sanitize("The Matrix (1999)"), "The Matrix (1999)");
    }

    #[test]
    fn sanitize_allows_chinese() {
        assert_eq!(sanitize("让子弹飞"), "让子弹飞");
    }

    #[test]
    fn build_filename_movie_with_year() {
        let item = make_item("The Matrix", None, None, None, Some(1999));
        assert_eq!(build_item_filename(&item, "mkv"), "The Matrix (1999).mkv");
    }

    #[test]
    fn build_filename_movie_without_year() {
        let item = make_item("Test Movie", None, None, None, None);
        assert_eq!(build_item_filename(&item, "mp4"), "Test Movie.mp4");
    }

    #[test]
    fn build_filename_tv_episode() {
        let item = make_item("The One", Some("The Matrix"), Some(1), Some(4), None);
        assert_eq!(build_item_filename(&item, "mkv"), "The Matrix - S01E04 - The One.mkv");
    }

    #[test]
    fn build_filename_tv_defaults_to_1() {
        let item = make_item("Pilot", Some("Stranger Things"), None, None, None);
        assert_eq!(build_item_filename(&item, "mkv"), "Stranger Things - S01E01 - Pilot.mkv");
    }

    #[test]
    fn build_filename_sanitizes_series_name() {
        let item = make_item("Episode 1", Some("Bad:Series/Name"), Some(2), Some(3), None);
        assert_eq!(build_item_filename(&item, "avi"), "Bad Series Name - S02E03 - Episode 1.avi");
    }

    #[test]
    fn build_filename_uses_custom_extension() {
        let item = make_item("Movie", None, None, None, Some(2024));
        assert_eq!(build_item_filename(&item, "mp4"), "Movie (2024).mp4");
        assert_eq!(build_item_filename(&item, "webm"), "Movie (2024).webm");
    }
}
