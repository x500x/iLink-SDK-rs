# 命令行工具

`cli` feature（默认开启）会构建一个 `weixin` 二进制，它包装了本库的全部能力，这样
你可以在写代码之前端到端地验证并脚本化整个 SDK。CLI 读取与库相同的状态目录与
`openclaw.json`。

## 构建与运行

```bash
cargo build --release
cargo run -- --help          # 显示所有命令
cargo run -- login           # 二维码登录，保存凭证
cargo run -- accounts        # 列出已注册账户
cargo run -- status          # 解析后的账户设置
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "看这张"
cargo run -- listen          # 长轮询、保存媒体并打印入站消息
cargo run -- logout <account-id>
```

> 使用 `--no-default-features` 时不会构建该二进制。

## 全局选项

所有命令都接受以下选项（多数同时读取对应环境变量）：

| 选项 | 环境变量 | 说明 |
| --- | --- | --- |
| `--log-level <lvl>` | `RUST_LOG_LEVEL` | `trace`…`error`（默认 `info`） |
| `-a, --account <id>` | `WEIXIN_ACCOUNT_ID` | 要操作的账户 |
| `--base-url <url>` | `WEIXIN_BASE_URL` | 覆盖 API base URL |
| `--cdn-base-url <url>` | `WEIXIN_CDN_BASE_URL` | 覆盖 CDN base URL |
| `--route-tag <tag>` | `WEIXIN_ROUTE_TAG` | 作为 `SKRouteTag` 转发 |
| `--bot-agent <ua>` | `WEIXIN_BOT_AGENT` | 可观测性标识 |
| `--timeout-ms <ms>` | `WEIXIN_TIMEOUT_MS` | 单请求超时 |
| `--json` | — | 输出机器可读的 JSON |

## 命令

| 命令 | 描述 |
| --- | --- |
| `login [--timeout <s>] [--show-qr] [--bot-type <t>] [--no-save]` | 运行二维码登录流程并持久化 bot token。 |
| `accounts` | 列出所有已注册账户及其解析后的设置。 |
| `status` | 打印活动账户的解析设置（base URL、token 是否存在、会话守卫状态、路径）。 |
| `logout <account-id>` | 移除账户及其凭证、游标、上下文 token 与引用缓存。 |
| `listen [--duration <s>] [--poll-timeout <ms>] [--media-dir <dir>] [--quote-cache <bool>] [--echo <bool>]` | 长轮询 `getUpdates`，下载入站媒体，填充上下文 token 存储与引用缓存，并打印消息。默认对 `/ping` 回复 `pong`。 |
| `send <to> <text> [--context-token <t>] [--run-id <id>]` | 发送文本消息。 |
| `send-file <to> <path> [--caption <c>] [--context-token <t>]` | 把 `path` 上传到 CDN 并发送（按扩展名判定图片/视频/文件）。 |
| `send-item <to> <json> [--context-token <t>]` | 发送原始 `MessageItem` JSON 对象。 |
| `config [user] [--context-token <t>]` | 带 user 时调用 `getConfig` 并打印 typing ticket；否则打印本地频道配置。 |
| `typing <user> [on\|off] [--context-token <t>]` | 获取 typing ticket 并显示/取消“正在输入”。 |
| `notify <start\|stop>` | 发送 `notifystart` / `notifystop`。 |
| `download [--url <u>] [--param <p>] [--aes-key <k>] [-o <path>]` | 拉取（并可选 AES-ECB 解密）一个 CDN 对象。 |
| `upload <to> <path> [--media-type image\|video\|file\|voice]` | 上传本地文件到 CDN 并打印引用/密钥。 |
| `mime <path>` | 打印路径的 MIME 类型与出站路由。 |
| `id [--kind id\|filekey\|temp] [--prefix <p>] [--ext <.e>]` | 生成 id、CDN file key 或临时文件名。 |
| `quote policy` | 打印生效的引用缓存策略。 |
| `quote find --conversation <c> --message-id <m>` | 查询一条缓存消息。 |
| `quote put --conversation <c> --message-id <m> [--body <b>] [--direction inbound\|outbound] [--media-path <p>] [--media-mime <m>]` | 插入或更新一条缓存消息。 |
| `quote gc` | 运行一次引用缓存 GC。 |
| `quote clear` | 删除该账户的全部缓存消息/媒体。 |
| `quote media-dir` | 打印该账户的托管媒体目录。 |
| `context get <user>` | 读取已存储的回复上下文 token。 |
| `context set <user> <token>` | 存储回复上下文 token。 |
| `context clear` | 删除该账户的全部上下文 token。 |
| `context restore` | 从磁盘重新载入上下文 token。 |
| `sync show` | 打印持久化的 `get_updates_buf` 游标。 |
| `sync set <buf>` | 覆盖持久化游标。 |
| `sync clear` | 删除持久化游标。 |
| `session status` | 打印账户是否被会话守卫暂停。 |
| `session pause` / `session resume` | 暂停一小时 / 提前恢复。 |
| `session assert` | 账户被暂停时报错。 |
| `cdn download-url --param <p>` | 构造 CDN 下载 URL。 |
| `cdn upload-url --param <p> --filekey <k>` | 构造 CDN 上传 URL。 |

## 状态与配置

CLI 使用与库相同的解析规则：

- 状态目录：`OPENCLAW_STATE_DIR` → `CLAWDBOT_STATE_DIR` →
  `$XDG_STATE_HOME/openclaw` → `$HOME/.openclaw`。
- 配置：`OPENCLAW_CONFIG` → `<state_dir>/openclaw.json`，区块
  `channels.openclaw-weixin`。

因此 CLI 执行的二维码登录可立即被你自己的 Rust 代码使用（反之亦然），同一台机器上的
TypeScript 客户端也会共享这些凭证。

## 日志

CLI 会初始化一个 `tracing-subscriber`（经由 `logging` 模块），由 `RUST_LOG` 驱动
（例如 `RUST_LOG=ilink_sdk=debug cargo run -- listen`）。
