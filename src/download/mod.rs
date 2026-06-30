pub mod direct;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::api::client::EmbyClient;
use tracing::{error, info};
use crate::api::items::{EmbyItem, MediaSourceInfo};
use crate::utils::filename;

#[derive(Clone)]
pub struct DownloadOptions {
    pub output_dir: PathBuf,
    pub overwrite: bool,
    pub dry_run: bool,
    pub no_resume: bool,
}

static USED_NAMES: Mutex<Option<HashSet<String>>> = Mutex::new(None);

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

fn deduplicate_filename(filename: &str) -> String {
    let mut map = USED_NAMES.lock().unwrap();
    let set = map.get_or_insert_with(HashSet::new);
    if set.insert(filename.to_string()) {
        filename.to_string()
    } else {
        let stem = std::path::Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let mut suffix = 2;
        loop {
            let candidate = if ext.is_empty() {
                format!("{} ({})", stem, suffix)
            } else {
                format!("{} ({}).{}", stem, suffix, ext)
            };
            if set.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }
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
                let season_number = season.index_number;
                let episodes = match client.get_child_items(&season.id, "Episode").await {
                    Ok(v) => v,
                    Err(e) => { error!("获取季剧集失败: {}", e); continue; }
                };
                for mut ep in episodes {
                    if ep.parent_index_number.is_none() {
                        ep.parent_index_number = season_number;
                    }
                    if let Err(e) = download_single(client, &ep, Some(series_name), opts).await {
                        error!("下载失败 [{}]: {}", ep.name, e);
                    }
                }
            }
            Ok(())
        }
        Some("Season") => {
            let series_name = item.series_name.as_deref();
            let season_number = item.index_number;
            let episodes = client.get_child_items(&item.id, "Episode").await?;
            for mut ep in episodes {
                if ep.parent_index_number.is_none() {
                    ep.parent_index_number = season_number;
                }
                if let Err(e) = download_single(client, &ep, series_name, opts).await {
                    error!("下载失败 [{}]: {}", ep.name, e);
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
    let filename = deduplicate_filename(&filename::build_item_filename(item, &container, series_name));
    let season_dir = format!("Season {:02}", item.parent_index_number.unwrap_or(1));
    let sn = item.series_name.as_deref().or(series_name);
    let dest = match sn {
        Some(name) => opts.output_dir
            .join(filename::sanitize(name))
            .join(&season_dir)
            .join(&filename),
        None => {
            let folder = std::path::Path::new(&filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("movie");
            opts.output_dir.join(folder).join(&filename)
        }
    };

    if dest.exists() {
        if opts.overwrite {
            tokio::fs::remove_file(&dest).await?;
        } else if let Some(expected) = source.size {
            let actual = tokio::fs::metadata(&dest).await?.len();
            if actual == expected as u64 {
                info!("跳过已存在的文件: {}", filename);
                return Ok(());
            }
            anyhow::bail!("文件已存在且大小不匹配: {}", filename);
        } else {
            anyhow::bail!("文件已存在: {}", filename);
        }
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // --no-resume: 删除 .part 临时文件，强制重新下载
    let part_path = direct::part_path(&dest);
    if opts.no_resume || opts.overwrite {
        if part_path.exists() {
            tokio::fs::remove_file(&part_path).await?;
        }
    }

    info!("下载: {}", filename);
    direct::download_file(
        &client.http,
        &stream_url,
        &dest,
        source.size.map(|s| s as u64),
        &client.auth.access_token,
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
