# 存储与配置 API

存储与配置模块 [`../../src/storage/`](../../src/storage/) 负责本地持久化：账户凭据、状态
目录、长轮询游标，以及 `openclaw.json` 中的 `channels.openclaw-weixin` 配置段。
对应 TypeScript 的 `src/auth/accounts.ts`、`src/storage/state-dir.ts`、
`src/storage/sync-buf.ts` 与配置读取。

模块构成：

| 子模块 | 文件 | 职责 |
| --- | --- | --- |
| `accounts` | [`../../src/storage/accounts.rs`](../../src/storage/accounts.rs) | 每账户凭据存储与索引 |
| `state_dir` | [`../../src/storage/state_dir.rs`](../../src/storage/state_dir.rs) | 状态目录解析 |
| `sync_buf` | [`../../src/storage/sync_buf.rs`](../../src/storage/sync_buf.rs) | `get_updates_buf` 游标 |
| `config` | [`../../src/storage/config.rs`](../../src/storage/config.rs) | 配置段读取 |

## `accounts` 子模块

### 常量与核心结构

```rust
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// 统一账户数据：token、base URL、配对 user id。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountData {
    pub token: Option<String>,
    pub saved_at: Option<String>,     // ISO-8601 时间戳
    pub base_url: Option<String>,
    pub user_id: Option<String>,
}
impl AccountData {
    pub fn is_configured(&self) -> bool;            // token 非空
    pub fn effective_base_url(&self) -> String;     // 空则回退 DEFAULT_BASE_URL
}

/// 合并存储凭据与配置后的账户。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccount {
    pub account_id: String,
    pub base_url: String,
    pub cdn_base_url: String,
    pub token: Option<String>,
    pub enabled: bool,
    pub configured: bool,
    pub name: Option<String>,
    pub user_id: Option<String>,
    pub route_tag: Option<String>,
}

/// 部分更新（仅非空字段覆盖存储）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountUpdate {
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
}
```

### 账户 id 规范化

```rust
/// "b0f5860fdecb@im.bot" -> "b0f5860fdecb-im-bot"
pub fn normalize_account_id(raw: &str) -> String;
/// 反向模式匹配："b0f5860fdecb-im-bot" -> Some("b0f5860fdecb@im.bot")；不匹配返回 None
pub fn derive_raw_account_id(normalized_id: &str) -> Option<String>;
```

### 索引与注册

```rust
/// 已注册账户 id 列表（按注册顺序）。
pub fn list_indexed_account_ids() -> Vec<String>;
pub fn list_account_ids() -> Vec<String>;                  // 别名

/// 把 account_id 加入持久索引（已存在则无操作）。
pub fn register_account_id(account_id: &str) -> Result<()>;
/// 从持久索引移除。
pub fn unregister_account_id(account_id: &str) -> Result<()>;
```

索引文件：`<sdk_state_dir>/accounts.json`（JSON 数组）。

### 读写与解析

```rust
/// 读取账户数据（含旧安装的兼容回退）。
pub fn load_account(account_id: &str) -> Option<AccountData>;

/// 持久化账户数据（合并已有文件）。
pub fn save_account(account_id: &str, update: &AccountUpdate) -> Result<AccountData>;

/// 解析账户：合并存储凭据 + 本地配置。
pub fn resolve_account(account_id: &str) -> Result<StoredAccount>;
pub fn resolve_account_with(
    account_id: &str, overrides: Option<&BTreeMap<String, BTreeMap<String, serde_json::Value>>>
) -> Result<StoredAccount>;

/// 删除账户的全部文件（凭据、sync、context-tokens）。
pub fn clear_account(account_id: &str) -> Result<()>;

/// 登录后清除同 user_id 的旧账户。
pub fn clear_stale_accounts_for_user_id(
    current_account_id: &str, user_id: &str, on_clear_context_tokens: impl FnMut(&str)
) -> Result<()>;
```

### `save_account` 的合并语义

- `token`：仅当 `update.token` 非空时才覆盖；空白字符串**不会**清空已有 token。
- `base_url`：仅当非空时存储。
- `user_id`：显式设为 `""` 时清空；`None` 时保留已有值。
- 始终写入 `saved_at`（ISO-8601）。

> ⚠️ 这个「空白不覆盖」行为很关键：如果你确实要清空某个字段，需要传入 `None`
> 或空字符串的具体规则（如 `user_id=""` 才清空），否则旧值会被保留。

## `state_dir` 子模块

```rust
/// 解析本地状态目录（首个非空值胜出）：
///  1. OPENCLAW_STATE_DIR
///  2. CLAWDBOT_STATE_DIR
///  3. $XDG_STATE_HOME/openclaw（仅 Unix，当设置）
///  4. $HOME/.openclaw
pub fn resolve_state_dir() -> PathBuf;

/// resolve_state_dir()/openclaw-weixin
pub fn sdk_state_dir() -> PathBuf;
/// sdk_state_dir()/accounts
pub fn accounts_dir() -> PathBuf;

/// 创建目录（及父级），返回路径。
pub fn ensure_dir(path: &Path) -> std::io::Result<&Path>;
```

状态目录下的布局：

```text
$OPENCLAW_STATE_DIR/openclaw-weixin/
├── accounts.json                     # 已注册账户 id 索引
├── accounts/
│   ├── <id>.json                     # token、baseUrl、userId
│   ├── <id>.sync.json                # getUpdates 游标
│   └── <id>.context-tokens.json      # 回复上下文 token
├── ref-messages.sqlite               # 引用缓存（见 quote-cache.md）
└── debug-mode.json                   # 每账户调试标志
```

## `sync_buf` 子模块

持久化 `get_updates_buf` 游标，使重启后能续传而非重放整个历史。

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncBufData { pub get_updates_buf: String }
impl SyncBufData { pub fn new(get_updates_buf: impl Into<String>) -> Self; }

/// <accounts_dir>/<account_id>.sync.json
pub fn get_sync_buf_path(account_id: &str) -> PathBuf;

/// 加载游标；回退顺序：给定路径 → 派生的 raw-id 文件名 → 旧安装单账户路径。
pub fn load_get_updates_buf(path: &Path) -> Option<String>;
pub fn load_for_account(account_id: &str) -> Option<String>;

/// 保存游标（创建父目录），值做 JSON 转义。
pub fn save_get_updates_buf(path: &Path, get_updates_buf: &str) -> std::io::Result<()>;
pub fn save_for_account(account_id: &str, get_updates_buf: &str) -> std::io::Result<()>;

/// 删除账户游标。
pub fn clear_for_account(account_id: &str);
```

## `config` 子模块

读取宿主 `openclaw.json` 中的 `channels.openclaw-weixin` 段（与 TS 插件同源，但
Rust SDK 没有宿主配置对象，因此直接从磁盘读取）。

```rust
pub const CHANNEL_CONFIG_SECTION: &str = "openclaw-weixin";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkConfig {
    pub accounts: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    pub reply_progress_messages: bool,
    pub quote_cache: QuoteCacheRaw,
    pub channel_config_updated_at: Option<String>,
    pub route_tag: Option<serde_json::Value>,
    pub bot_agent: Option<String>,
}

pub struct QuoteCacheRaw { /* camelCase 原始值 */ }
impl QuoteCacheRaw {
    /// 归一化为 storage::config::QuoteCachePolicy（无效值回退默认）。
    pub fn to_policy(&self) -> QuoteCachePolicy;
}

pub struct QuoteCachePolicy { /* enabled, retention_ms, max_messages_per_account, media_retention_ms, max_media_bytes_per_account, max_single_media_bytes */ }
impl Default for QuoteCachePolicy { /* 见 config.rs */ }
```

### 配置读取函数

```rust
/// OPENCLAW_CONFIG 优先，否则 <state_dir>/openclaw.json。
pub fn resolve_config_path() -> PathBuf;

pub fn load_local_config() -> SdkConfig;
pub fn load_local_config_from(path: &Path) -> SdkConfig;   // 缺失/损坏返回默认
pub fn resolve_channel_config(overrides: Option<SdkConfig>) -> SdkConfig;

pub fn load_route_tag(account_id: &str) -> Option<String>;
pub fn load_bot_agent() -> Option<String>;
```

### 配置项（顶层 `channels.openclaw-weixin`）

| 键 | 类型 | 默认 | 用途 |
| --- | --- | --- | --- |
| `botAgent` | string | `OpenClaw` | 可观测标识（非鉴权），经 sanitize |
| `routeTag` | string \| number | — | 作为 `SKRouteTag` 转发 |
| `accounts.<id>.baseUrl` | string | `https://ilinkai.weixin.qq.com` | 每账户 API base URL |
| `accounts.<id>.cdnBaseUrl` | string | `https://novac2c.cdn.weixin.qq.com/c2c` | 每账户 CDN base URL |
| `accounts.<id>.enabled` | boolean | `true` | 禁用账户而不删除 |
| `quoteCache` | object | 见下 | 本地引用缓存策略 |

`quoteCache` 子键：`enabled`、`retentionDays`(30)、`maxMessagesPerAccount`(10000)、
`mediaRetentionDays`(7)、`maxMediaBytesPerAccount`(256 MiB)、`maxSingleMediaBytes`
(25 MiB)。无效值回退默认（与 TS 的 `positiveNumber` 行为一致）。

## 用法示例

### 登录后保存并注册账户

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::storage::accounts::{self, AccountUpdate};

# fn example(session: QrSession) -> ilink_sdk::Result<()> {
let account_id = accounts::normalize_account_id(&session.bot_id);
accounts::save_account(&account_id, &AccountUpdate {
    token: Some(session.token),
    base_url: Some(session.base_url),
    user_id: Some(session.user_id),
})?;
accounts::register_account_id(&account_id)?;
println!("saved account {}", account_id);
# Ok(())
# }
```

### 列出账户并解析

```rust,no_run
# use ilink_sdk::prelude::*;
# fn example() -> ilink_sdk::Result<()> {
for id in ilink_sdk::storage::accounts::list_indexed_account_ids() {
    let acc = ilink_sdk::storage::accounts::resolve_account(&id)?;
    println!("{} enabled={} base={}", acc.account_id, acc.enabled, acc.base_url);
}
# Ok(())
# }
```

### 持久化长轮询游标

```rust,no_run
# use ilink_sdk::prelude::*;
# use ilink_sdk::storage::sync_buf;
# fn example(buf: &str) -> std::io::Result<()> {
sync_buf::save_for_account("acct", buf)?;
if let Some(next) = sync_buf::load_for_account("acct") {
    println!("resume cursor len = {}", next.len());
}
# Ok(())
# }
```

### 读取每账户 route tag

```rust,no_run
# use ilink_sdk::prelude::*;
# fn example() {
let tag = ilink_sdk::storage::config::load_route_tag("acct");
println!("route tag: {:?}", tag);
# }
```

## 设计要点与注意事项

1. **账户目录与 TS 插件共享。** 状态根与 TS 完全一致的子路径，因此同一台机器上
   Rust 与 TS 客户端可共享凭据。
2. **`save_account` 的「空白不覆盖」语义。** 要清空字段请遵循规则（`user_id=""`
   才清空；`token`/`base_url` 空白不覆盖）。Logic 上这是为了不意外擦掉已登录凭据。
3. **`resolve_account` 失败即硬错误。** 空 `account_id`、文件不存在、无 token 都会
   返回 `Err`；`WeixinClientBuilder::build()` 内部依赖它。
4. **游标多回退路径。** `load_get_updates_buf` 会尝试 raw-id 文件名与旧单账户路径，
   因此升级迁移不会丢游标。
5. **配置缺失/损坏不致命。** `load_local_config*` 在文件缺失或 JSON 损坏时返回
   `SdkConfig::default()`，而非错误。
6. **环境变量覆盖状态目录。** `OPENCLAW_STATE_DIR` / `CLAWDBOT_STATE_DIR` 优先；空串
   被忽略，继续回退。
7. **`botAgent` 会被 sanitize。** 非法 UA 串在写入 `base_info` 前被清洗（见
   [api-client.md](api-client.md)），配置里写的值同理。
