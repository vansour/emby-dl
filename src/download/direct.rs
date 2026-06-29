use std::path::{Path, PathBuf};

use anyhow::Context;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::utils::progress::{DownloadProgress, DownloadProgressUnknown};

enum ProgressType {
    Known(DownloadProgress),
    Unknown(DownloadProgressUnknown),
}

impl ProgressType {
    fn inc(&self, n: u64) {
        match self {
            ProgressType::Known(p) => p.inc(n),
            ProgressType::Unknown(p) => p.inc(n),
        }
    }
    fn finish(&self) {
        match self {
            ProgressType::Known(p) => p.finish(),
            ProgressType::Unknown(p) => p.finish(),
        }
    }
}

fn part_path(path: &Path) -> PathBuf {
    let name = format!("{}.part", path.file_name().unwrap_or_default().to_string_lossy());
    path.parent().unwrap_or(Path::new(".")).join(name)
}

pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    total_size: Option<u64>,
    token: &str,
) -> anyhow::Result<()> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let part = part_path(path);

    let existing_size = if part.exists() {
        tokio::fs::metadata(&part).await?.len()
    } else {
        0
    };

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "X-Emby-Token",
        reqwest::header::HeaderValue::from_str(token)
            .map_err(|e| anyhow::anyhow!("无效的 access token: {}", e))?,
    );
    if existing_size > 0 {
        let range = format!("bytes={}-", existing_size);
        headers.insert(
            "Range",
            reqwest::header::HeaderValue::from_str(&range)
                .map_err(|e| anyhow::anyhow!("无效的 Range header: {}", e))?,
        );
    }

    let resp = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("下载请求失败: {}", e))?;

    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(anyhow::anyhow!("下载失败: HTTP {}", resp.status()));
    }

    let content_type = resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content_type.starts_with("text/html") {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let preview: String = body.chars().take(300).collect();
        anyhow::bail!("服务器返回 HTML (HTTP {}) 而非视频文件: {}", status, preview);
    }

    let content_length = resp.content_length();
    let final_total = content_length.or(total_size);

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let file = if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part)
            .await
            .map_err(|e| anyhow::anyhow!("无法打开文件 {}: {}", part.display(), e))?
    } else {
        tokio::fs::File::create(&part)
            .await
            .map_err(|e| anyhow::anyhow!("无法创建文件 {}: {}", part.display(), e))?
    };

    let mut writer = tokio::io::BufWriter::with_capacity(256 * 1024, file);

    if existing_size > 0 {
        info!("检测到已有 {}，断点续传中...", filename);
    }

    let offset = if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        existing_size
    } else {
        if existing_size > 0 {
            info!("服务器不支持断点续传，重新下载");
        }
        0
    };

    let progress = if let Some(total) = final_total {
        let total = offset.checked_add(total).context("文件大小溢出")?;
        let p = DownloadProgress::new(filename, total)?;
        if offset > 0 {
            p.set_position(offset);
        }
        ProgressType::Known(p)
    } else {
        ProgressType::Unknown(DownloadProgressUnknown::new(filename)?)
    };

    let mut stream = resp.bytes_stream();
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = cancel_tx.send(());
    });

    loop {
        tokio::select! {
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(data)) => {
                        writer.write_all(&data).await?;
                        progress.inc(data.len() as u64);
                    }
                    Some(Err(e)) => return Err(anyhow::anyhow!("读取流失败: {}", e)),
                    None => break,
                }
            }
            _ = &mut cancel_rx => {
                drop(writer);
                let _ = tokio::fs::remove_file(&part).await;
                anyhow::bail!("下载被用户中断");
            }
        }
    }

    progress.finish();
    writer.flush().await?;
    drop(writer);

    tokio::fs::rename(&part, path).await
        .map_err(|e| anyhow::anyhow!("重命名临时文件失败: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_server(content: &[u8]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}/test", addr);

        let content = content.to_vec();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                content.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.write_all(&content).await.unwrap();
        });

        url
    }

    #[tokio::test]
    async fn test_download_success() {
        let content = b"hello world!";
        let url = test_server(content).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-direct");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");

        download_file(&client, &url, &path, Some(content.len() as u64), "")
            .await
            .unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_no_total_size() {
        let content = b"test data without total size";
        let url = test_server(content).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-nototal");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");

        download_file(&client, &url, &path, None, "").await.unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }
}
