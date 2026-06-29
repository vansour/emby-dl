# Changelog

## v0.0.1 (2025-06-29)

### 初始功能

- **认证系统**：`emby-dl auth` 交互式输入网址、用户名、密码，凭据加密存储到 SQLite
- **媒体库浏览**：`list` 列出所有媒体库，支持 `[movies]`、`[tvshows]` 等类型
- **全文搜索**：`search <关键词>` 跨所有类型（电影/剧集/系列/季/合集等）搜索，无类型过滤
- **逐级浏览**：
  - `series <ID>` — 列出系列的所有季
  - `season <ID>` — 列出某季的所有剧集
- **下载**：
  - `get <ID>` — 按 ID 下载，自动识别类型（电影直下、剧集/季/系列自动展开）
  - `batch <关键词>` — 批量下载搜索结果
  - `link <ID>` — 输出视频直链
- **文件夹目录规范**：
  - 电影：`<输出>/电影名 (年份).ext`
  - 剧集：`<输出>/系列名/Season XX/系列名 - SXXEYY - 集名.ext`
- **断点续传**：已存在文件自动 Range 请求续传
- **进度条**：`indicatif` 显示下载进度
- **原始文件下载**：使用 `Static=true` 直链，不经过服务端转码

### 技术栈

- Rust 2024 edition
- Tokio 异步运行时
- reqwest (rustls TLS)
- clap 4 CLI 框架
- SQLite (rusqlite bundled)
- AES-256-GCM 加密

### 构建

- 支持 10 个目标平台：Linux (x86_64, aarch64, i686, armv7)、Windows (x86_64, aarch64, i686)、macOS (x86_64, aarch64)
