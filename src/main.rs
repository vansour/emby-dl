mod api;
mod db;
mod download;
mod utils;

use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use db::AuthDb;
use download::DownloadOptions;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "emby-dl", version, about = "从 Emby 服务器下载视频")]
struct Cli {
    /// 下载输出目录
    #[arg(short = 'O', long, default_value = ".")]
    output: PathBuf,

    /// 覆盖已存在的文件
    #[arg(short, long)]
    overwrite: bool,

    /// 仅列出信息，不实际下载
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// 禁用断点续传
    #[arg(long)]
    no_resume: bool,

    /// 下载缓存目录（默认: 系统缓存目录/emby-dl/download-cache）
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 设置服务器认证信息（交互式输入网址、用户名、密码），保存到 SQLite
    Auth,
    /// 测试登录是否正常
    Login,
    /// 列出所有媒体库
    List,
    /// 列出系列的所有季
    Series {
        /// 系列 ID
        id: String,
    },
    /// 列出某季的所有剧集
    Season {
        /// 季 ID
        id: String,
    },
    /// 搜索媒体
    Search {
        /// 搜索关键词
        query: String,
        /// 返回条数上限
        #[arg(short, long, default_value = "20")]
        limit: i32,
        /// 所属媒体库 ID（可选）
        #[arg(long)]
        library: Option<String>,
        /// 媒体类型过滤，如 Movie、Episode、Series
        #[arg(long)]
        item_type: Option<String>,
    },
    /// 按 ID 下载单个媒体
    Get {
        /// 媒体条目 ID
        id: String,
    },
    /// 批量下载搜索匹配的媒体
    Batch {
        /// 搜索关键词
        query: String,
        /// 最大下载数
        #[arg(short, long, default_value = "50")]
        limit: i32,
        /// 所属媒体库 ID（可选）
        #[arg(long)]
        library: Option<String>,
        /// 媒体类型过滤，如 Movie、Episode、Series
        #[arg(long)]
        item_type: Option<String>,
    },
    /// 输出视频下载直链
    Link {
        /// 媒体条目 ID
        id: String,
    },
    /// 配置 HTTP/SOCKS5 代理
    Proxy {
        #[command(subcommand)]
        action: ProxyAction,
    },
    /// 清理下载缓存（.part 临时文件）
    CleanCache,
}

#[derive(Subcommand)]
enum ProxyAction {
    /// 设置代理，如 http://127.0.0.1:8080 或 socks5://127.0.0.1:1080
    Set {
        /// 代理 URL
        url: String,
    },
    /// 查看当前代理
    Show,
    /// 清除代理
    Remove,
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db = AuthDb::open()?;

    let command = match &cli.command {
        Some(cmd) => cmd,
        None => return Ok(()),
    };

    match command {
        Commands::Proxy { action } => {
            match action {
                ProxyAction::Set { url } => {
                    let valid_schemes = ["http://", "https://", "socks5://", "socks5h://"];
                    let has_valid_scheme = valid_schemes.iter().any(|s| url.starts_with(s));
                    if !has_valid_scheme {
                        anyhow::bail!(
                            "无效的代理地址: 缺少协议头，应使用 http://、https://、socks5:// 或 socks5h://"
                        );
                    }
                    reqwest::Proxy::all(url)
                        .map_err(|e| anyhow::anyhow!("无效的代理地址: {}", e))?;
                    db.save_proxy(url)?;
                    info!("代理已保存: {}", url);
                }
                ProxyAction::Show => match db.load_proxy()? {
                    Some(url) => info!("当前代理: {}", url),
                    None => info!("未设置代理"),
                },
                ProxyAction::Remove => {
                    db.remove_proxy()?;
                    info!("代理已清除");
                }
            }
            return Ok(());
        }
        Commands::CleanCache => {
            let cache_dir = default_cache_dir();
            if !cache_dir.exists() {
                info!("缓存目录不存在: {}", cache_dir.display());
                return Ok(());
            }
            let mut total_size = 0u64;
            let mut count = 0u32;
            let mut read_dir = tokio::fs::read_dir(&cache_dir).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("part") {
                    if let Ok(meta) = entry.metadata().await {
                        total_size += meta.len();
                    }
                    tokio::fs::remove_file(&path).await?;
                    count += 1;
                }
            }
            if count == 0 {
                info!("缓存目录已干净 (无 .part 文件)");
            } else {
                let size_mb = total_size as f64 / (1024.0 * 1024.0);
                info!("已清理 {} 个 .part 文件，释放 {:.2} MB", count, size_mb);
            }
            return Ok(());
        }
        Commands::Auth => {
            print!("服务器网址: ");
            io::stdout().flush()?;
            let mut url = String::new();
            io::stdin().read_line(&mut url)?;
            let url = url.trim().to_string();

            print!("用户名: ");
            io::stdout().flush()?;
            let mut username = String::new();
            io::stdin().read_line(&mut username)?;
            let username = username.trim().to_string();

            print!("密码: ");
            io::stdout().flush()?;
            let password = rpassword::read_password()?;
            let password = password.trim().to_string();

            let mut http_builder = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(3600));
            if let Some(proxy_url) = db.load_proxy()? {
                let proxy = reqwest::Proxy::all(&proxy_url)
                    .map_err(|e| anyhow::anyhow!("无效的代理地址: {}", e))?;
                http_builder = http_builder.proxy(proxy);
            }
            let http = http_builder.build()?;

            let auth_info = api::auth::authenticate(&http, &url, &username, &password).await?;
            db.save_auth(
                &url,
                &auth_info.username,
                &auth_info.access_token,
                &auth_info.user_id,
            )?;
            info!("认证成功，已保存到数据库 (用户: {})", auth_info.username);
            return Ok(());
        }
        _ => {}
    }

    let mut http_builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30));
    if let Some(proxy_url) = db.load_proxy()? {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|e| anyhow::anyhow!("无效的代理地址: {}", e))?;
        http_builder = http_builder.proxy(proxy);
        info!("使用代理: {}", proxy_url);
    }
    let http = http_builder.build()?;

    let auth_info = if let Some(stored) = db.load_auth()? {
        info!("使用已保存的认证信息 (用户: {})", stored.username);
        api::auth::AuthInfo {
            access_token: stored.access_token,
            user_id: stored.user_id,
            server_url: stored.server_url,
            username: stored.username,
        }
    } else {
        let cred = db
            .load_credentials()?
            .ok_or_else(|| anyhow::anyhow!("未找到认证信息，请先运行 emby-dl auth"))?;
        let info = api::auth::authenticate(&http, &cred.server_url, &cred.username, &cred.password)
            .await?;
        db.save_auth(
            &cred.server_url,
            &info.username,
            &info.access_token,
            &info.user_id,
        )?;
        info!("登录成功: {} (用户: {})", cred.server_url, info.username);
        info
    };

    let client = api::client::EmbyClient::new(http, auth_info)?;

    let cache_dir = cli.cache_dir.unwrap_or_else(default_cache_dir);
    info!("使用缓存目录: {}", cache_dir.display());
    let opts = DownloadOptions {
        output_dir: cli.output,
        overwrite: cli.overwrite,
        dry_run: cli.dry_run,
        no_resume: cli.no_resume,
        batch_current: None,
        batch_total: None,
        cache_dir: Some(cache_dir),
    };

    match command {
        Commands::Login => {
            info!("认证成功!");
        }

        Commands::Series { id } => {
            let seasons = client.get_series_seasons(id).await?;
            if seasons.is_empty() {
                info!("该系列暂无季");
            } else {
                for s in &seasons {
                    let num = s.index_number.unwrap_or(0);
                    info!("  [{}] 第{}季 - {}", s.id, num, s.name);
                }
            }
        }

        Commands::Season { id } => {
            let season_item = client.get_item(id).await?;
            let season_number = season_item.index_number.unwrap_or(1);
            let episodes = client.get_child_items(id, "Episode").await?;
            if episodes.is_empty() {
                info!("该季暂无剧集");
            } else {
                for ep in &episodes {
                    let ep_num = ep.index_number.unwrap_or(1);
                    info!(
                        "  [{}] S{:02}E{:02} - {}",
                        ep.id, season_number, ep_num, ep.name
                    );
                }
            }
        }

        Commands::List => {
            let views = client.list_views().await?;
            info!("媒体库列表:");
            for v in &views {
                let ctype = v.collection_type.as_deref().unwrap_or("未知");
                info!("  {} [{}] ({})", v.name, ctype, v.id);
            }
        }

        Commands::Search {
            query,
            limit,
            library,
            item_type,
        } => {
            let items = client
                .search_items(query, library.as_deref(), *limit, item_type.as_deref())
                .await?;
            if items.is_empty() {
                info!("未找到匹配结果");
            } else {
                for item in &items {
                    print_item(item);
                }
            }
        }

        Commands::Get { id } => {
            let item = client.get_item(id).await?;
            print_item(&item);
            if !opts.dry_run {
                download::download_item(&client, &item, &opts).await?;
            }
        }

        Commands::Batch {
            query,
            limit,
            library,
            item_type,
        } => {
            let items = client
                .search_items(query, library.as_deref(), *limit, item_type.as_deref())
                .await?;
            if items.is_empty() {
                info!("未找到匹配结果");
                return Ok(());
            }
            let total = items.len();
            info!("找到 {} 个条目", total);
            if !opts.dry_run {
                info!("开始下载...");
                for (i, item) in items.iter().enumerate() {
                    let mut batch_opts = opts.clone();
                    batch_opts.batch_current = Some(i + 1);
                    batch_opts.batch_total = Some(total);
                    if let Err(e) = download::download_item(&client, item, &batch_opts).await {
                        error!("下载失败 [{}]: {}", item.name, e);
                    }
                }
            }
        }

        Commands::Link { id } => {
            let item = client.get_item(id).await?;
            let sources = client.get_playback_info(&item.id).await?;
            let source = sources
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("未找到可用的媒体源"))?;
            let container = download::extract_container(&source);
            let filename = utils::filename::build_item_filename(&item, &container, None);
            let url = download::build_download_url(&client, &item.id, &source);
            info!("文件名: {}", filename);
            info!("{}", url);
        }

        Commands::Auth | Commands::Proxy { .. } | Commands::CleanCache => unreachable!(),
    }

    Ok(())
}

fn type_label(t: Option<&str>) -> &'static str {
    match t {
        Some("Movie") => "[电影]",
        Some("Episode") => "[剧集]",
        Some("Series") => "[系列]",
        Some("Season") => "[季]",
        Some("BoxSet") => "[合集]",
        Some("MusicArtist") => "[音乐人]",
        Some("MusicAlbum") => "[专辑]",
        Some("Audio") => "[音频]",
        Some("Playlist") => "[播放列表]",
        Some("Person") => "[人物]",
        Some("Video") => "[视频]",
        Some("Photo") => "[照片]",
        Some("PhotoAlbum") => "[相册]",
        Some("Trailer") => "[预告]",
        Some("Book") => "[书籍]",
        Some("Program") => "[节目]",
        _ => "",
    }
}

fn print_item(item: &api::items::EmbyItem) {
    let type_tag = type_label(item.item_type.as_deref());
    let series = item.series_name.as_deref();
    let is_episode = item.item_type.as_deref() == Some("Episode");
    let is_season = item.item_type.as_deref() == Some("Season");

    if is_episode && let Some(s) = series {
        let season = item.parent_index_number.unwrap_or(1);
        let episode = item.index_number.unwrap_or(1);
        info!(
            "  {} [{}] {} - S{:02}E{:02} - {}",
            type_tag, item.id, s, season, episode, item.name
        );
    } else if is_season && let Some(s) = series {
        let num = item.index_number.unwrap_or(0);
        info!(
            "  {} [{}] {} - 第{}季 - {}",
            type_tag, item.id, s, num, item.name
        );
    } else if let Some(s) = series {
        let season = item.parent_index_number.unwrap_or(1);
        let episode = item.index_number.unwrap_or(1);
        info!(
            "  {} [{}] {} - S{:02}E{:02} - {}",
            type_tag, item.id, s, season, episode, item.name
        );
    } else {
        let year = item
            .production_year
            .map(|y| format!(" ({})", y))
            .unwrap_or_default();
        info!("  {} [{}] {}{}", type_tag, item.id, item.name, year);
    }
}

fn default_cache_dir() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("emby-dl");
    path.push("download-cache");
    path
}

fn main() {
    tracing_subscriber::fmt::init();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!("无法创建运行时: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(run()) {
        error!("错误: {}", e);
        std::process::exit(1);
    }
}
