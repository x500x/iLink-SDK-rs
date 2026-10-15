# 存储与状态

SDK 把一切持久化到本地状态目录——没有云端或注册表服务。本页涵盖目录布局、账户存储、
同步游标与引用缓存。

## 状态目录

[`resolve_state_dir`](../../../src/storage/state_dir.rs) 按以下顺序解析根目录
（第一个非空者胜出）：

1. `OPENCLAW_STATE_DIR`
2. `CLAWDBOT_STATE_DIR`
3. `$XDG_STATE_HOME/openclaw`（Unix，当设置时）—— Rust SDK 为桌面 Linux 新增；可通过
   设置 `OPENCLAW_STATE_DIR` 禁用
4. `$HOME/.openclaw`（Windows 上为 `%USERPROFILE%\.openclaw`）

SDK 自身的数据位于 `<root>/openclaw-weixin/`：

```text
<root>/openclaw-weixin/
├── accounts.json                      # 已注册账户 id（按注册顺序）
├── accounts/
│   ├── <id>.json                      # token、baseUrl、userId、savedAt
│   ├── <id>.sync.json                 # getUpdates 游标
│   └── <id>.context-tokens.json       # 回复上下文 token
├── ref-messages.sqlite                # 引用缓存
└── debug-mode.json                    # 每账户调试标志（主机管理）
```

这与 TypeScript 插件的布局一致，因此同一台机器上的 Rust 与 TypeScript 客户端可以
共享凭证。

## 账户

[`storage::accounts`](../../../src/storage/accounts.rs) 是凭证存储。

### 账户 id 规范化

微信账户 id（`b0f5860fdecb@im.bot`）会被转换为文件系统安全键
（`b0f5860fdecb-im-bot`），对应函数是 [`normalize_account_id`]。逆向函数是
[`derive_raw_account_id`]（`-im-bot` → `@im.bot`，`-im-wechat` → `@im.wechat`）。

### 读取 / 写入

| 函数 | 用途 |
| --- | --- |
| `save_account(id, &AccountUpdate)` | 合并并持久化 token/base_url/user_id（Unix 下权限 0600）。 |
| `register_account_id(id)` | 把 `id` 加入索引（已存在则为空操作）。 |
| `unregister_account_id(id)` | 从索引中移除 `id`。 |
| `list_indexed_account_ids()` / `list_account_ids()` | 所有已注册 id。 |
| `load_account(id)` | 加载 `AccountData`，含旧版回退。 |
| `resolve_account(id)` | 合并存储数据与 `openclaw.json` → [`StoredAccount`]。 |
| `clear_account(id)` | 删除账户的所有文件。 |
| `clear_stale_accounts_for_user_id(...)` | 裁剪绑定到同一微信用户的其他账户。 |

`AccountUpdate` 的合并规则：非空的 `token` 覆盖；非空的 `base_url` 覆盖；`user_id`
显式为 `""` 时清除，为 `None` 时保留。

### `StoredAccount`

账户的解析视图：`account_id`、`base_url`、`cdn_base_url`（默认 [`CDN_BASE_URL`]）、
`token`、`enabled`、`configured`、`name`、`user_id`、`route_tag`。`WeixinClient::account()`
返回它。

## 同步游标（长轮询续传）

[`storage::sync_buf`](../../../src/storage/sync_buf.rs) 持久化 `get_updates_buf` 游标，
以便重启后续传，而不是重放整个历史。

```rust,no_run
# fn run(account_id: &str) {
use openclaw_weixin::storage::sync_buf::{load_for_account, save_for_account, get_sync_buf_path};

let cursor = load_for_account(account_id);            // Option<String>
if let Some(buf) = &cursor {
    let _ = save_for_account(account_id, buf);          // 每次轮询后持久化
}
let _ = get_sync_buf_path(account_id);                 // <accounts>/<id>.sync.json
# }
```

兼容读取顺序：规范化账户文件 → 原始 id 文件 → 旧版单账户文件。

## 引用缓存

较新的微信客户端在引用消息时只发送一个服务端消息 id。本地引用缓存
（[`messaging::quote_cache`]，即 `ref-messages.sqlite`）会存储每条消息的正文与受管
媒体，以便后续引用恢复上下文。

### 打开

```rust,no_run
# fn run() -> openclaw_weixin::Result<()> {
use openclaw_weixin::messaging::quote_cache::QuoteStore;

let store = QuoteStore::open_default()?;   // <root>/openclaw-weixin/ref-messages.sqlite
# Ok(())
# }
```

或在 [`WeixinClient`] 上开启：

```rust,no_run
# fn run(client: &mut openclaw_weixin::WeixinClient) -> openclaw_weixin::Result<()> {
client.enable_quote_cache()?;
let _ = client.quote_store();
# Ok(())
# }
```

### 记录

- [`QuoteMessageRecord`] —— 一条缓存消息（账户、会话、id、方向、正文、媒体路径/MIME/名称/大小、`created_at`）。
- [`QuoteMessageInput`] —— 你传给 [`QuoteStore::put`] 的内容（结构相同，外加一个
  `source_media_path`，会被注册到受管媒体根目录）。
- [`Direction`] —— `Inbound`（默认）或 `Outbound`。

### 策略与默认值

缓存由 [`QuoteCachePolicy`] 控制。默认值（以及覆盖它们的 `openclaw.json` 键）：

| 字段 | 默认 | `openclaw.json` 键 |
| --- | --- | --- |
| `enabled` | `true` | `quoteCache.enabled` |
| `retention_ms` | 30 天 | `quoteCache.retentionDays`（天） |
| `max_messages_per_account` | 10,000 | `quoteCache.maxMessagesPerAccount` |
| `media_retention_ms` | 7 天 | `quoteCache.mediaRetentionDays`（天） |
| `max_media_bytes_per_account` | 256 MiB | `quoteCache.maxMediaBytesPerAccount` |
| `max_single_media_bytes` | 25 MiB | `quoteCache.maxSingleMediaBytes` |

非法值会回退到默认值（与 TypeScript 的 `positiveNumber` 辅助一致）。GC 在每累计
`GC_WRITE_INTERVAL`（100）次写入以及每小时定时器上运行。受管媒体位于
`media/inbound/openclaw-weixin-quotes/<account>/`（`QUOTE_MEDIA_SUBDIR`）。

### 禁用

在 channel 配置中设置 `quoteCache.enabled: false`，或以 `enabled: false` 构造存储。
被禁用的缓存不存储任何内容，查询一律未命中。
