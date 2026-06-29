mod api;
mod db;
mod download;
mod utils;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use db::AuthDb;
use download::DownloadOptions;

#[derive(Parser)]
#[command(name = "emby-dl", version, about = "从 Emby 服务器下载视频")]
struct Cli {
    /// 下载输出目录
    #[arg(short = 'O', long, default_value = ".")]
    output: PathBuf,

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
    },
    /// 输出视频下载直链
    Link {
        /// 媒体条目 ID
        id: String,
    },
}

async fn run() -> anyhow::Result<()> {
    use std::io::{self, Write};

    let cli = Cli::parse();
    let db = AuthDb::open()?;
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Auth subcommand: interactive setup, no existing auth needed
    if let Some(Commands::Auth) = &cli.command {
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
        let mut password = String::new();
        io::stdin().read_line(&mut password)?;
        let password = password.trim().to_string();

        let auth_info = api::auth::authenticate(&http, &url, &username, &password).await?;
        db.save_auth(&url, &auth_info.username, &auth_info.access_token, &auth_info.user_id)?;
        eprintln!("认证成功，已保存到数据库 (用户: {})", auth_info.username);
        return Ok(());
    }

    let auth_info = if let Some(stored) = db.load_auth()? {
        eprintln!("使用已保存的认证信息 (用户: {})", stored.username);
        api::auth::AuthInfo {
            access_token: stored.access_token,
            user_id: stored.user_id,
            server_url: stored.server_url,
            username: stored.username,
        }
    } else {
        let cred = db.load_credentials()?
            .ok_or_else(|| anyhow::anyhow!("未找到认证信息，请先运行 emby-dl auth"))?;
        let info = api::auth::authenticate(&http, &cred.server_url, &cred.username, &cred.password).await?;
        db.save_auth(&cred.server_url, &info.username, &info.access_token, &info.user_id)?;
        eprintln!("登录成功: {} (用户: {})", cred.server_url, info.username);
        info
    };

    let client = api::client::EmbyClient::new(http, auth_info);

    let opts = DownloadOptions {
        output_dir: cli.output,
        overwrite: false,
    };

    let command = match &cli.command {
        Some(cmd) => cmd,
        None => return Ok(()),
    };

    match command {
        Commands::Auth => unreachable!(),

        Commands::Login => {
            eprintln!("认证成功!");
        }

        Commands::Series { id } => {
            let seasons = client.get_series_seasons(id).await?;
            if seasons.is_empty() {
                println!("该系列暂无季");
            } else {
                for s in &seasons {
                    let num = s.index_number.unwrap_or(0);
                    println!("  [{}] 第{}季 - {}", s.id, num, s.name);
                }
            }
        }

        Commands::Season { id } => {
            let episodes = client.get_child_items(id, "Episode").await?;
            if episodes.is_empty() {
                println!("该季暂无剧集");
            } else {
                for ep in &episodes {
                    let ep_num = ep.index_number.unwrap_or(1);
                    let season_num = ep.parent_index_number.unwrap_or(1);
                    println!(
                        "  [{}] S{:02}E{:02} - {}",
                        ep.id, season_num, ep_num, ep.name
                    );
                }
            }
        }

        Commands::List => {
            let views = client.list_views().await?;
            println!("媒体库列表:");
            for v in &views {
                let ctype = v.collection_type.as_deref().unwrap_or("未知");
                println!("  {} [{}] ({})", v.name, ctype, v.id);
            }
        }

        Commands::Search {
            query,
            limit,
            library,
        } => {
            let items = client
                .search_items(query, library.as_deref(), *limit)
                .await?;
            if items.is_empty() {
                println!("未找到匹配结果");
            } else {
                for item in &items {
                    print_item(item);
                }
            }
        }

        Commands::Get { id } => {
            let item = client.get_item(id).await?;
            print_item(&item);
            download::download_item(&client, &item, &opts).await?;
        }

        Commands::Batch {
            query,
            limit,
            library,
        } => {
            let items = client
                .search_items(query, library.as_deref(), *limit)
                .await?;
            if items.is_empty() {
                println!("未找到匹配结果");
                return Ok(());
            }
            println!("找到 {} 个条目，开始下载...", items.len());
            for item in &items {
                if let Err(e) = download::download_item(&client, item, &opts).await {
                    eprintln!("下载失败 [{}]: {}", item.name, e);
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
            let filename = utils::filename::build_item_filename(&item, &container);
            let url = download::build_download_url(&client, &item.id, &source);
            eprintln!("文件名: {}", filename);
            println!("{}", url);
        }
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
        println!(
            "  {} [{}] {} - S{:02}E{:02} - {}",
            type_tag, item.id, s, season, episode, item.name
        );
    } else if is_season && let Some(s) = series {
        let num = item.index_number.unwrap_or(0);
        println!(
            "  {} [{}] {} - 第{}季 - {}",
            type_tag, item.id, s, num, item.name
        );
    } else if let Some(s) = series {
        let season = item.parent_index_number.unwrap_or(1);
        let episode = item.index_number.unwrap_or(1);
        println!(
            "  {} [{}] {} - S{:02}E{:02} - {}",
            type_tag, item.id, s, season, episode, item.name
        );
    } else {
        let year = item
            .production_year
            .map(|y| format!(" ({})", y))
            .unwrap_or_default();
        println!("  {} [{}] {}{}", type_tag, item.id, item.name, year);
    }
}

fn main() {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("无法创建运行时: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(run()) {
        eprintln!("错误: {}", e);
        std::process::exit(1);
    }
}
