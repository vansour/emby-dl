# Changelog

## v0.0.5 (2025-06-30)

### 新增功能

- **Scoop 安装支持**：新增 `scoop/emby-dl.json` 清单，支持 `scoop bucket add` 安装
- **Homebrew 安装支持**：新增 `Formula/emby-dl.rb`，支持 `brew install` 安装

### 工程改进

- 版本号统一更新至 0.0.5
- Release 工作流自动生成 Scoop 清单和 Homebrew formula（含正确 SHA256）
- README 补充各平台配置路径说明

## v0.0.4 (2025-06-30)

### Bug 修复

- **search / get 缺失季号**：`search_items` 和 `get_item` 未传 `Fields` 参数，Emby 不返回 `ParentIndexNumber` 和 `IndexNumber`，导致所有剧集季号默认 `Season 01`
- **ParentId 查询不返回 ParentIndexNumber**：Emby API 在使用 `ParentId` 过滤查询时不返回 `ParentIndexNumber`，从季节对象获取 IndexNumber 手动赋值
- **代理 URL 缺乏校验**：无效代理地址（如缺少协议头）被静默保存，使用时才报错且信息模糊
- **proxy 子命令功能不完整**：缺少查看当前代理的命令

### 功能变更

- **代理 URL 保存时校验**：`proxy set` 时立即校验 URL 格式，非法地址直接拒绝并给出明确提示
- **新增 `proxy show` 命令**：查看当前代理配置
- **完善 README**：补充完整的子命令文档、目录结构、命名规则、常见问题

### 工程改进

- 版本号统一更新至 0.0.4（Cargo.toml / Emby Authorization header / X-Emby-Client-Version）
- `EmbyClient` 和 `DownloadOptions` 增加 Clone 派生

## v0.0.3 (2025-06-30)

### Bug 修复

- **剧集文件名缺失 SxxExx**：下载系列/季时剧集文件被命名为电影格式（`剧集名 (年份).ext`）而非剧集格式（`系列名 - SxxExx - 剧集名.ext`）
- **Release Changelog 为空**：`release.yml` 中 awk 精确匹配 `## v0.0.2` 失败，因 CHANGELOG 标题含日期后缀，改为行首前缀匹配

### 功能变更

- **电影下载新增独立文件夹**：每个电影文件放入同名目录，避免根目录文件堆积（`The Matrix (1999)/The Matrix (1999).mkv`）

### 工程改进

- 版本号统一更新至 0.0.3（Cargo.toml / Emby Authorization header / X-Emby-Client-Version）

## v0.0.2 (2025-06-29)

### 新增功能

- **代理支持**：`proxy set <URL>` 设置 HTTP/SOCKS5 代理，`proxy remove` 清除，地址持久化到 SQLite
- **系列逐级下载**：`get <系列ID>` 自动下载所有季的所有剧集；`get <季ID>` 下载该季所有剧集
- **结果类型标签**：搜索结果显示 `[电影]`、`[剧集]`、`[系列]`、`[季]` 等中文类型标识

### Bug 修复

- **断点续传误判**：未下载完成的文件中断后重新下载被跳过的问题，改为比较文件大小
- **Brotli 压缩支持**：部分 Emby 服务器（通过 FRP 代理）返回 `Content-Encoding: br`，reqwest 未启用 brotli 导致解析为二进制乱码

### 依赖升级

- `rusqlite` 0.31 → 0.40（bundled SQLite）
- `aes-gcm` 0.10 → 0.11（AEAD 0.6 API 适配）
- `getrandom` 0.2 → 0.4（`getrandom()` → `fill()`）

### 工程改进

- 升级 Rust edition 2024
- 添加 `CHANGELOG.md` 版本管理
- GitHub Actions Release 增加 `contents: write` 权限和 changelog 分段提取
- 认证改为标准输入读取，不再依赖 `/dev/tty`，支持非交互式环境

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
