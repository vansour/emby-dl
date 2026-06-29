pub mod direct;

use std::path::PathBuf;

use crate::api::client::EmbyClient;
use crate::api::items::{EmbyItem, MediaSourceInfo};
use crate::utils::filename;

pub struct DownloadOptions {
    pub output_dir: PathBuf,
    pub overwrite: bool,
}

/// 从 MediaSource 中提取实际容器格式，默认 mkv
pub fn extract_container(source: &MediaSourceInfo) -> String {
    source
        .container
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.split(',').next().filter(|s| !s.is_empty()))
        .unwrap_or("mkv")
        .to_string()
}

/// 生成下载直链
pub fn build_download_url(client: &EmbyClient, item_id: &str, source: &MediaSourceInfo) -> String {
    client.build_stream_url(item_id, &source.id)
}

pub async fn download_item(
    client: &EmbyClient,
    item: &EmbyItem,
    opts: &DownloadOptions,
) -> anyhow::Result<()> {
    match item.item_type.as_deref() {
        Some("Series") => {
            let seasons = client.get_series_seasons(&item.id).await?;
            if seasons.is_empty() {
                anyhow::bail!("系列「{}」没有季", item.name);
            }
            for season in &seasons {
                let series_name = season.series_name.as_deref().unwrap_or(&item.name);
                let episodes = match client.get_child_items(&season.id, "Episode").await {
                    Ok(v) => v,
                    Err(e) => { eprintln!("获取季剧集失败: {}", e); continue; }
                };
                for ep in &episodes {
                    if let Err(e) = download_single(client, ep, Some(series_name), opts).await {
                        eprintln!("下载失败 [{}]: {}", ep.name, e);
                    }
                }
            }
            Ok(())
        }
        Some("Season") => {
            let series_name = item.series_name.as_deref();
            let episodes = client.get_child_items(&item.id, "Episode").await?;
            for ep in &episodes {
                if let Err(e) = download_single(client, ep, series_name, opts).await {
                    eprintln!("下载失败 [{}]: {}", ep.name, e);
                }
            }
            Ok(())
        }
        _ => download_single(client, item, None, opts).await,
    }
}

async fn download_single(
    client: &EmbyClient,
    item: &EmbyItem,
    series_name: Option<&str>,
    opts: &DownloadOptions,
) -> anyhow::Result<()> {
    let sources = client.get_playback_info(&item.id).await?;
    let source = sources
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到可用的媒体源"))?;

    let container = extract_container(&source);
    let stream_url = build_download_url(client, &item.id, &source);
    let filename = filename::build_item_filename(item, &container);
    let season_dir = format!("Season {:02}", item.parent_index_number.unwrap_or(1));
    let sn = item.series_name.as_deref().or(series_name);
    let dest = match sn {
        Some(name) => opts.output_dir
            .join(filename::sanitize(name))
            .join(&season_dir)
            .join(&filename),
        None => opts.output_dir.join(&filename),
    };

    if dest.exists() && !opts.overwrite {
        eprintln!("跳过已存在的文件: {}", filename);
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    eprintln!("下载: {}", filename);
    direct::download_file(
        &client.http,
        &stream_url,
        &dest,
        source.size.map(|s| s as u64),
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source(container: Option<&str>) -> MediaSourceInfo {
        MediaSourceInfo {
            id: "src1".into(),
            name: None,
            container: container.map(|s| s.into()),
            path: None,
            size: None,
            media_streams: vec![],
        }
    }

    #[test]
    fn extract_container_mkv_default() {
        let source = make_source(None);
        assert_eq!(extract_container(&source), "mkv");
    }

    #[test]
    fn extract_container_single() {
        let source = make_source(Some("mp4"));
        assert_eq!(extract_container(&source), "mp4");
    }

    #[test]
    fn extract_container_first_of_many() {
        let source = make_source(Some("mkv,mp4,avi"));
        assert_eq!(extract_container(&source), "mkv");
    }

    #[test]
    fn extract_container_handles_empty_string() {
        let source = make_source(Some(""));
        assert_eq!(extract_container(&source), "mkv");
    }

    #[test]
    fn extract_container_webm() {
        let source = make_source(Some("webm"));
        assert_eq!(extract_container(&source), "webm");
    }
}
