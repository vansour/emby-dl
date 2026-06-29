# emby-dl

从 Emby 服务器下载视频的命令行工具。

## 特性

- **下载原始源文件** — 通过 `Static=true` 直链下载，不经过服务端转码，保留原始画质和格式
- **严格单线程下载** — 每个文件串行下载，不占用过多带宽/连接资源
- 认证信息加密保存到 SQLite，无需重复输入
- 列出所有媒体库
- 搜索电影/剧集
- 按 ID 下载单个媒体
- 批量下载搜索结果
- 输出视频直链
- 支持断点续传
- 进度条显示

## 安装

```bash
git clone <repo-url>
cd emby-dl
cargo build --release
```

## 使用

### 首次使用：设置服务器认证

```bash
emby-dl auth
```

按提示输入服务器网址、用户名和密码，认证信息加密保存到 `~/.config/emby-dl/auth.db`，后续使用无需再次输入。

### 选项

| 选项 | 说明 |
|---|---|
| `-O, --output <PATH>` | 下载输出目录（默认 `.`） |
| `-h, --help` | 打印帮助 |
| `-V, --version` | 打印版本 |

### 子命令

| 命令 | 说明 |
|---|---|
| `auth` | 设置服务器认证信息（交互式） |
| `login` | 测试登录是否正常 |
| `list` | 列出所有媒体库 |
| `search <QUERY>` | 搜索媒体 |
| `get <ID>` | 按 ID 下载单个媒体 |
| `batch <QUERY>` | 批量下载搜索匹配的媒体 |
| `link <ID>` | 输出视频下载直链 |

### 示例

```bash
# 设置认证（交互式输入网址、用户名、密码）
emby-dl auth

# 列出媒体库
emby-dl list

# 搜索电影
emby-dl search "让子弹飞"

# 下载到指定目录
emby-dl -O /path/to/dir get 12345

# 批量下载搜索结果
emby-dl batch "黑客帝国" --limit 10

# 输出直链（可用于第三方下载器）
emby-dl link 12345
```

`search` 和 `batch` 命令支持 `--library <ID>` 限定搜索范围。

## 下载策略

- **单线程下载**：每个下载任务严格串行执行，避免并发连接给服务器造成压力，也适合带宽有限的环境
- **原始文件**：使用 Emby 的 `Static=true` 参数请求原始媒体文件，服务端不进行任何转码，确保下载内容与源文件完全一致
- **断点续传**：已存在的文件自动尝试 `Range` 请求续传，服务器不支持时重新下载

## 技术栈

- **语言**: Rust (edition 2021)
- **运行时**: Tokio (异步)
- **HTTP 客户端**: reqwest 0.13 (rustls TLS)
- **CLI 框架**: clap 4 (derive)
- **进度条**: indicatif
- **数据库**: SQLite (rusqlite)
- **加密**: AES-256-GCM

## 模块结构

```
src/
├── main.rs          # CLI 入口，子命令分发
├── db.rs            # SQLite 数据库（认证信息加密存储）
├── api/
│   ├── auth.rs      # Emby 认证 (AuthenticateByName)
│   ├── client.rs    # HTTP 客户端封装
│   ├── items.rs     # 数据模型
│   └── stream.rs    # 播放信息获取
├── download/
│   ├── mod.rs       # 下载调度
│   └── direct.rs    # 文件下载（断点续传）
└── utils/
    ├── filename.rs  # 文件名生成
    └── progress.rs  # 进度条显示
```
