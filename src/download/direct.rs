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
    fn abort(&self) {
        match self {
            ProgressType::Known(p) => p.abort(),
            ProgressType::Unknown(p) => p.abort(),
        }
    }
}

pub fn part_path(path: &Path) -> PathBuf {
    let name = format!(
        "{}.part",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    path.parent().unwrap_or(Path::new(".")).join(name)
}

/// 在缓存目录中创建一个与目标文件对应的 .part 路径
pub fn part_path_in(cache_dir: &Path, path: &Path) -> PathBuf {
    let name = format!(
        "{}.part",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    cache_dir.join(name)
}

pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    total_size: Option<u64>,
    token: &str,
    display_name: &str,
    part: &Path,
) -> anyhow::Result<()> {
    let filename = display_name;

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

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content_type.starts_with("text/html") {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let preview: String = body.chars().take(300).collect();
        anyhow::bail!(
            "服务器返回 HTML (HTTP {}) 而非视频文件: {}",
            status,
            preview
        );
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

    if let Some(total) = final_total
        && offset > total
    {
        anyhow::bail!(
            "已下载的字节数 ({}) 超过预期文件大小 ({}), 请删除 .part 文件后重试",
            offset,
            total
        );
    }

    let progress = if let Some(total) = final_total {
        // 206 without Content-Length: total_size is the full file size, not remaining bytes
        let total =
            if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT && content_length.is_none() {
                total
            } else {
                offset.checked_add(total).context("文件大小溢出")?
            };
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
                        if let Err(e) = writer.write_all(&data).await {
                            let _ = writer.flush().await;
                            progress.abort();
                            return Err(anyhow::anyhow!("写入文件失败: {}", e));
                        }
                        progress.inc(data.len() as u64);
                    }
                    Some(Err(e)) => {
                        let _ = writer.flush().await;
                        progress.abort();
                        return Err(anyhow::anyhow!("读取流失败: {}", e));
                    }
                    None => break,
                }
            }
            _ = &mut cancel_rx => {
                let _ = writer.flush().await;
                drop(writer);
                progress.abort();
                anyhow::bail!("下载被用户中断，.part 文件已保留，下次自动续传");
            }
        }
    }

    progress.finish();
    writer.flush().await?;
    drop(writer);

    // 尝试 rename（同文件系统即原子操作），失败则 copy+delete（跨文件系统）
    if let Err(e) = tokio::fs::rename(&part, path).await {
        info!("跨文件系统移动，先复制再删除临时文件...");
        tokio::fs::copy(&part, path).await
            .map_err(|ce| anyhow::anyhow!("复制失败 (rename 错误: {}): {}", e, ce))?;
        tokio::fs::remove_file(&part).await
            .map_err(|re| anyhow::anyhow!("删除临时文件失败: {}", re))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// 基础测试服务器：发送 200 OK + 完整内容
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

    /// 支持 Range/206 的测试服务器（单连接）
    async fn test_server_resumable(content: &[u8]) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}/test", addr);

        let content = content.to_vec();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();

            let mut buf = vec![0u8; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);

            let has_range = request.contains("Range: bytes=");

            if has_range {
                let range_val = request
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("range:"))
                    .unwrap();
                let start: u64 = range_val
                    .trim_start_matches("Range: bytes=")
                    .trim()
                    .trim_end_matches('-')
                    .parse()
                    .unwrap();
                let remaining = &content[start as usize..];

                let response = format!(
                    "HTTP/1.1 206 Partial Content\r\n\
                     Content-Range: bytes {}-{}/{}\r\n\
                     Content-Length: {}\r\n\r\n",
                    start,
                    content.len() - 1,
                    content.len(),
                    remaining.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.write_all(remaining).await.unwrap();
            } else {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                    content.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.write_all(&content).await.unwrap();
            }
        });

        url
    }

    /// 模拟真实中断的服务器：
    ///   - 连接 1: 发 200 OK + Content-Length（全量），flush + shutdown 后只发一半数据就断连
    ///   - 连接 2: 发 206 Partial Content（续传）
    async fn test_server_interruptible(content: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}/test", addr);

        tokio::spawn(async move {
            // --- 连接 1：服务器故意中断 ---
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ = socket.read(&mut buf).await;

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    content.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
                // 只发一半数据
                let half = content.len() / 2;
                let _ = socket.write_all(&content[..half]).await;
                let _ = socket.flush().await;
                // 等待 100ms 让客户端收到已发送的数据，然后 shutdown 发送 FIN
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let _ = socket.shutdown().await;
            }

            // --- 连接 2：正常续传 ---
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);

                let has_range = request.contains("Range: bytes=");
                if !has_range {
                    // 无 Range header：以 200 完整发送（备选路径）
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        content.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(&content).await;
                    let _ = socket.flush().await;
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let _ = socket.shutdown().await;
                    return;
                }
                let range_val = request
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("range:"))
                    .unwrap();
                let start: u64 = range_val
                    .trim_start_matches("Range: bytes=")
                    .trim()
                    .trim_end_matches('-')
                    .parse()
                    .unwrap();
                let remaining = &content[start as usize..];

                let response = format!(
                    "HTTP/1.1 206 Partial Content\r\n\
                     Content-Range: bytes {}-{}/{}\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    start,
                    content.len() - 1,
                    content.len(),
                    remaining.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(remaining).await;
                let _ = socket.flush().await;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let _ = socket.shutdown().await;
            }
        });

        url
    }

    /// 模拟服务器不支持续传（无视 Range，始终返回 200 OK + 全量数据）
    async fn test_server_no_resume_support(content: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}/test", addr);

        tokio::spawn(async move {
            // --- 连接 1：中断 ---
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    content.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
                let half = content.len() / 2;
                let _ = socket.write_all(&content[..half]).await;
                let _ = socket.flush().await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let _ = socket.shutdown().await;
            }

            // --- 连接 2：无视 Range，返回 200 OK + 全部数据 ---
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    content.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(&content).await;
                let _ = socket.flush().await;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let _ = socket.shutdown().await;
            }
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

        download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
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

        download_file(&client, &url, &path, None, "", "output.bin", &part_path(&path))
            .await
            .unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    // ============ 断点续传真实场景测试 ============

    #[tokio::test]
    async fn test_download_resume_after_interruption() {
        let content = b"This file download will be interrupted in the middle, then resumed. The complete content must match exactly.";
        let url = test_server_interruptible(content.to_vec()).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-interrupt");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");
        let part = part_path(&path);

        // ---- 第一次下载：服务器中途断连，应失败 ----
        let result = download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await;
        assert!(result.is_err(), "服务器断连后 download_file 应返回错误");

        // ---- 验证 .part 文件已保留部分数据 ----
        assert!(part.exists(), "中断后 .part 文件应保留在磁盘上");
        let partial_size = tokio::fs::metadata(&part).await.unwrap().len();
        assert!(
            partial_size > 0,
            "中断后 .part 应有数据, 实际: {}",
            partial_size
        );
        assert!(
            partial_size < content.len() as u64,
            "中断后 .part 应小于完整大小 {} vs {}",
            partial_size,
            content.len()
        );
        // 验证 .part 内容与源文件前缀一致
        let partial_content = tokio::fs::read(&part).await.unwrap();
        assert_eq!(
            &partial_content[..],
            &content[..partial_size as usize],
            "中断时已写入的内容必须与源文件一致"
        );

        // ---- 第二次下载：续传 ----
        download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await
        .unwrap();

        // ---- 验证最终文件完整 ----
        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content, "续传后文件内容必须与源文件完全一致");

        // .part 已被重命名
        assert!(!part.exists(), "完成后 .part 应已被重命名为最终文件");

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_resume_server_no_range_support() {
        // 服务器不支持 Range（始终返回 200 OK）
        let content = b"Server without range support test data for resume fallback.";
        let url = test_server_no_resume_support(content.to_vec()).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-no-range");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");
        let part = part_path(&path);

        // 第一次：中断
        let result = download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await;
        assert!(result.is_err());
        assert!(part.exists());
        assert!(tokio::fs::metadata(&part).await.unwrap().len() > 0);

        // 第二次：服务器无视 Range，返回 200 OK + 全量数据
        // 此时 download_file 检测到 200（非 206），offset = 0，打开 File::create（截断）
        // 应完整下载覆盖
        download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await
        .unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content, "不支持 Range 的服务器应完整重新下载");

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_resume_from_scratch_when_no_part() {
        let content = b"File content for non-resume test.";
        let url = test_server_resumable(content).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-no-part");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");

        download_file(&client, &url, &path, None, "", "output.bin", &part_path(&path))
            .await
            .unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_resume_exact_boundary() {
        let content = b"boundary test data";
        let url = test_server_resumable(content).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-boundary");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");
        let part = part_path(&path);

        tokio::fs::write(&part, content).await.unwrap();

        let total_size = Some(content.len() as u64);
        download_file(&client, &url, &path, total_size, "", "output.bin", &part_path(&path))
            .await
            .unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_resume_verify_part_content() {
        // 验证中断时 .part 的内容与源文件对应位置完全一致
        let content = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let url = test_server_interruptible(content.to_vec()).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-verify-part");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");
        let part = part_path(&path);

        let result = download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await;
        assert!(result.is_err());

        // 精确验证部分内容
        let partial = tokio::fs::read(&part).await.unwrap();
        assert_eq!(
            partial,
            &content[..partial.len()],
            ".part 内容必须与源文件开头一致（偏移 0）"
        );

        // 续传后整体一致
        download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await
        .unwrap();
        let full = tokio::fs::read(&path).await.unwrap();
        assert_eq!(full, content);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_download_resume_large_file() {
        // 用较大文件（100KB）测试续传健壮性
        let content: Vec<u8> = (0u8..255).cycle().take(100 * 1024).collect();
        let url = test_server_interruptible(content.clone()).await;

        let client = reqwest::Client::new();
        let dir = std::env::temp_dir().join("emby-dl-test-large");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("output.bin");
        let part = part_path(&path);

        let result = download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await;
        assert!(result.is_err());
        let partial_size = tokio::fs::metadata(&part).await.unwrap().len();
        assert!(partial_size > 0 && partial_size < content.len() as u64);

        download_file(
            &client,
            &url,
            &path,
            Some(content.len() as u64),
            "",
            "output.bin",
            &part_path(&path),
        )
        .await
        .unwrap();

        let result = tokio::fs::read(&path).await.unwrap();
        assert_eq!(result, content, "100KB 文件续传后内容必须一致");

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }
}
