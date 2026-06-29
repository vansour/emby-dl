use std::path::Path;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::utils::progress::{DownloadProgress, DownloadProgressUnknown};

trait ProgressInc {
    fn inc(&self, n: u64);
    fn finish(&self);
}

impl ProgressInc for DownloadProgress {
    fn inc(&self, n: u64) { self.inc(n); }
    fn finish(&self) { self.finish(); }
}

impl ProgressInc for DownloadProgressUnknown {
    fn inc(&self, n: u64) { self.inc(n); }
    fn finish(&self) { self.finish(); }
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

    let existing_size = if path.exists() {
        tokio::fs::metadata(path).await?.len()
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

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|e| anyhow::anyhow!("无法创建文件 {}: {}", path.display(), e))?;

    let mut writer = tokio::io::BufWriter::new(file);

    if existing_size > 0 {
        eprintln!("检测到已有 {}，断点续传中...", filename);
    }

    let offset = if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        existing_size
    } else {
        if existing_size > 0 {
            eprintln!("服务器不支持断点续传，重新下载");
        }
        0
    };

    let mut stream = resp.bytes_stream();

    let progress: Box<dyn ProgressInc> = if let Some(total) = final_total {
        let p = DownloadProgress::new(filename, offset + total)?;
        if offset > 0 {
            p.set_position(offset);
        }
        Box::new(p)
    } else {
        Box::new(DownloadProgressUnknown::new(filename)?)
    };

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!("读取流失败: {}", e))?;
        writer.write_all(&chunk).await?;
        progress.inc(chunk.len() as u64);
    }
    progress.finish();

    writer.flush().await?;
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
