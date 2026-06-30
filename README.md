# emby-dl

从 Emby 媒体服务器下载视频的命令行工具。支持原始文件下载、断点续传、加密认证存储、代理等特性。

## 安装

### 从预编译二进制安装（推荐）

从 [Releases](https://github.com/vansour/emby-dl/releases) 下载对应平台的压缩包，解压后即可使用。

### 一键安装脚本（Linux）

```bash
curl -sSfL https://github.com/vansour/emby-dl/releases/latest/download/install.sh | bash
```

### 从源码编译

```bash
git clone https://github.com/vansour/emby-dl
cd emby-dl
cargo build --release
```

## 使用

### 1. 认证

```bash
emby-dl auth
```

按提示输入服务器网址、用户名和密码。认证信息加密保存到 `~/.config/emby-dl/auth.db`，后续无需重复输入。

### 2. 浏览与搜索

```bash
# 列出所有媒体库
emby-dl list

# 搜索媒体
emby-dl search "让子弹飞"

# 搜索并指定类型过滤
emby-dl search "黑客帝国" --item-type Movie

# 搜索限定在某个媒体库内
emby-dl search "女子监狱" --library <LibraryID> --item-type Series

# 列出系列的所有季
emby-dl series <SeriesID>

# 列出某季的所有剧集
emby-dl season <SeasonID>
```

### 3. 下载

```bash
# 下载单个媒体（自动识别类型：电影直下，剧集/季/系列自动展开）
emby-dl get <ID>

# 下载到指定目录
emby-dl -O /path/to/dir get <ID>

# 批量下载搜索结果
emby-dl batch "黑客帝国" --limit 20

# 批量下载（带类型和媒体库过滤）
emby-dl batch "女子监狱" --item-type Episode --limit 50

# 仅查看不下载
emby-dl -n get <ID>
```

### 4. 获取下载直链

```bash
emby-dl link <ID>
```

可用于第三方下载器（如 aria2、IDM 等）。

### 5. 代理设置

```bash
# 设置 HTTP 代理
emby-dl proxy set http://127.0.0.1:8080

# 设置 SOCKS5 代理
emby-dl proxy set socks5://127.0.0.1:1080

# 查看当前代理
emby-dl proxy show

# 清除代理
emby-dl proxy remove
```

### 6. 测试登录

```bash
emby-dl login
```

## 全局选项

| 选项 | 说明 |
|---|---|
| `-O, --output <PATH>` | 下载输出目录（默认当前目录） |
| `-n, --dry-run` | 仅列出信息，不实际下载 |
| `-f, --overwrite` | 覆盖已存在的文件 |
| `--no-resume` | 禁用断点续传，强制重新下载 |
| `-h, --help` | 打印帮助 |
| `-V, --version` | 打印版本 |

## 子命令详情

| 命令 | 参数 | 说明 |
|---|---|---|
| `auth` | — | 交互式设置服务器认证信息 |
| `login` | — | 测试登录是否正常 |
| `list` | — | 列出所有媒体库 |
| `series <id>` | — | 列出系列的所有季 |
| `season <id>` | — | 列出某季的所有剧集 |
| `search <query>` | `--limit`, `--library`, `--item-type` | 搜索媒体 |
| `get <id>` | — | 按 ID 下载（自动展开系列/季） |
| `batch <query>` | `--limit`, `--library`, `--item-type` | 批量下载搜索结果 |
| `link <id>` | — | 输出视频下载直链 |
| `proxy set <url>` | — | 设置 HTTP/SOCKS5 代理 |
| `proxy show` | — | 查看当前代理 |
| `proxy remove` | — | 清除代理 |

## 下载目录结构

```
<输出目录>/
├── 系列名/
│   ├── Season 01/
│   │   ├── 系列名 - S01E01 - 集标题.mkv
│   │   ├── 系列名 - S01E02 - 集标题.mkv
│   │   └── ...
│   ├── Season 02/
│   └── ...
├── 电影名 (年份)/
│   └── 电影名 (年份).mkv
└── ...
```

### 命名规则

- **剧集**: `系列名 - S{季号}E{集号} - 集标题.{ext}`
- **电影**: `电影名 (年份).{ext}`（放在同名目录内）

## 下载策略

- **原始文件下载** — 使用 Emby `Static=true` 参数请求原始媒体文件，服务端不进行转码，保留源文件的格式和画质
- **严格单文件串行** — 每次只下载一个文件，每个文件使用单 HTTP 连接（不分块），避免并发连接给服务器造成压力
- **断点续传** — 通过 HTTP `Range` 头实现，中断后自动续传；服务器不支持 Range 时自动回退为完整重新下载
- **进度显示** — 已知文件大小时显示百分比 + 速度 + 预估剩余时间，未知大小时显示已下载字节数

## 数据存储

| 文件 | 位置 | 说明 |
|---|---|---|
| `auth.db` | `~/.config/emby-dl/auth.db` | 加密的认证信息和代理配置 |
| `key` | `~/.config/emby-dl/key` | AES-256-GCM 密钥（仅本地，不外传） |

首次运行 `auth` 自动创建以上文件。密钥为 256 位随机数，权限设为 `600`（仅当前用户可读）。

## 技术栈

| 组件 | 选型 |
|---|---|
| 语言 | Rust 2024 edition |
| 异步运行时 | Tokio (multi-thread) |
| HTTP 客户端 | reqwest 0.13 (rustls TLS, brotli) |
| CLI 框架 | clap 4 (derive) |
| 数据库 | SQLite (rusqlite bundled) |
| 加密 | AES-256-GCM (aes-gcm 0.11) |
| 进度条 | indicatif 0.18 |
| 错误处理 | anyhow |
| 日志 | tracing + tracing-subscriber |

## 模块结构

```
src/
├── main.rs          # CLI 入口，子命令分发
├── db.rs            # SQLite 数据库（加密存储/代理）
├── api/
│   ├── mod.rs       # 模块声明
│   ├── auth.rs      # Emby AuthenticateByName 认证
│   ├── client.rs    # HTTP 客户端封装
│   ├── items.rs     # 数据模型
│   └── stream.rs    # 播放信息获取
├── download/
│   ├── mod.rs       # 下载调度（自动展开系列/季）
│   └── direct.rs    # 文件下载（断点续传）
└── utils/
    ├── mod.rs       # 模块声明
    ├── filename.rs  # 文件名生成与清理
    └── progress.rs  # 进度条显示
```

## 常见问题

**Q: 下载中断后如何续传？**  
无需任何操作，再次运行同样的下载命令，会自动检测 `.part` 临时文件并续传。

**Q: 如何查看服务端剧集编号是否正确？**  
```bash
emby-dl season <SeasonID>
```
如果显示 S01E01 但期望的是 S02E01，请在 Emby 管理后台检查该集的元数据。

**Q: 如何清除认证信息重新登录？**  
```bash
rm -rf ~/.config/emby-dl
emby-dl auth
```
