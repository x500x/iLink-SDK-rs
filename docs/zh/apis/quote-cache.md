# 引用缓存 API

新版微信客户端在引用消息时只发送服务器消息 id，因此所需的文本与媒体元数据需要
本地保存，以便后续引用能还原上下文。引用缓存位于
[`../../src/messaging/quote_cache.rs`](../../../src/messaging/quote_cache.rs)，使用 SQLite
（`rusqlite`，内置 SQLite）持久化。当策略中 `enabled == false` 时整体禁用。

数据库位置：`<state_dir>/openclaw-weixin/ref-messages.sqlite`；受管媒体目录：
`<state_dir>/media/inbound/openclaw-weixin-quotes/<account>/`。

## `QuoteCachePolicy`

```rust
pub struct QuoteCachePolicy {
    pub enabled: bool,
    pub retention_ms: u64,
    pub max_messages_per_account: u64,
    pub media_retention_ms: u64,
    pub max_media_bytes_per_account: u64,
    pub max_single_media_bytes: u64,
}
```

> ⚠️ 注意：存在**两个** `QuoteCachePolicy` 类型——
> `messaging::quote_cache::QuoteCachePolicy`（本节的运行时策略）与
> `storage::config::QuoteCachePolicy`（配置归一化，字段形态相同）。二者不要混淆。

默认值（`impl Default`）：

```rust
QuoteCachePolicy {
    enabled: true,
    retention_ms: 30 * 24 * 60 * 60 * 1000,       // 文本保留 30 天
    max_messages_per_account: 10_000,
    media_retention_ms: 7 * 24 * 60 * 60 * 1000,  // 媒体保留 7 天
    max_media_bytes_per_account: 256 * 1024 * 1024, // 256 MiB
    max_single_media_bytes: 25 * 1024 * 1024,       // 25 MiB
}
```

相关常量：

```rust
pub const DEFAULT_RETENTION_DAYS: u64 = 30;
pub const DEFAULT_MAX_MESSAGES_PER_ACCOUNT: u64 = 10_000;
pub const DEFAULT_MEDIA_RETENTION_DAYS: u64 = 7;
pub const DEFAULT_MAX_MEDIA_BYTES_PER_ACCOUNT: u64 = 256 * 1024 * 1024;
pub const DEFAULT_MAX_SINGLE_MEDIA_BYTES: u64 = 25 * 1024 * 1024;
pub const QUOTE_MEDIA_SUBDIR: &str = "media/inbound/openclaw-weixin-quotes";
```

## `Direction`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction { #[default] Inbound, Outbound }

impl Direction {
    fn as_str(self) -> &'static str;   // "inbound" / "outbound"
    fn from_str(raw: &str) -> Self;    // "outbound"->Outbound，其余->Inbound
}
```

## `QuoteMessageInput` 与 `QuoteMessageRecord`

```rust
pub struct QuoteMessageInput {
    pub account_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub direction: Direction,
    pub body: String,
    pub source_media_path: Option<String>,   // 源媒体路径，写入时登记进受管根目录
    pub media_mime: Option<String>,
    pub media_name: Option<String>,
    pub created_at: i64,
}

pub struct QuoteMessageRecord {
    pub account_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub direction: Direction,
    pub body: String,
    pub media_path: Option<String>,    // 受管根目录下的路径
    pub media_mime: Option<String>,
    pub media_name: Option<String>,
    pub media_size: Option<i64>,
    pub created_at: i64,
}
```

## `QuoteStore`

```rust
pub struct QuoteStore;  // SQLite 连接 + 受管媒体根 + 策略 + 写计数

impl QuoteStore {
    /// 按策略打开（创建）默认引用数据库。policy.enabled==false 时返回 Ok(None)。
    pub fn open(policy: QuoteCachePolicy) -> Result<Option<QuoteStore>>;

    /// 用默认策略打开（policy 必为 enabled，否则 panic via expect）。
    pub fn open_default() -> Result<QuoteStore>;

    /// 在显式位置打开（root_dir 放 sqlite，media_root 放媒体）。
    pub fn open_at(policy: QuoteCachePolicy, root_dir: &Path, media_root: &Path) -> Result<QuoteStore>;

    pub fn set_policy(&mut self, policy: QuoteCachePolicy);   // 替换策略并触发 GC
    pub fn policy(&self) -> &QuoteCachePolicy;

    /// 查找缓存消息；过期记录在读取时删除，返回 None。
    pub fn find(&self, account_id: &str, conversation_id: &str, message_id: &str)
        -> Option<QuoteMessageRecord>;

    /// 插入或更新消息；空 message_id 或「空 body 且无媒体」会被忽略。
    pub fn put(&mut self, input: &QuoteMessageInput) -> Result<()>;

    /// 删除账户的全体消息与受管媒体文件。
    pub fn delete_account(&mut self, account_id: &str);

    /// 完整 GC 遍历。
    pub fn gc(&mut self);

    pub fn account_media_dir(&self, account_id: &str) -> PathBuf;
    pub fn ensure_account_media_dir(&self, account_id: &str) -> Result<PathBuf>;
}
```

### 自由函数

```rust
/// 一个账户的受管媒体目录名（可读前缀 + 48 位哈希，路径安全）。
pub fn account_media_dir_name(account_id: &str) -> String;

/// 媒体存储子目录（含 QUOTE_MEDIA_SUBDIR 前缀）。
pub fn quote_media_subdir(account_id: &str) -> String;
```

## `find` 的细节

- `message_id` 为空直接返回 `None`。
- 读取时若记录已超出 `retention_ms`（`now - created_at > retention_ms`），会在读取
  路径上删除并返回 `None`。
- 命中则返回 `QuoteMessageRecord`，其 `media_path` 指向受管根目录下的文件。

## `put` 的细节

- `message_id` 为空，或「`body` 为空且 `source_media_path` 为 `None`」时直接忽略。
- `source_media_path` 非空时，会尝试**登记**进受管媒体根目录（见下方安全约束）。
- 使用 upsert：主键 `(account_id, conversation_id, message_id)`；已有行时仅以
  `COALESCE` 方式补充缺失的媒体字段，不覆盖已有值。
- 写满 `GC_WRITE_INTERVAL`（100）次后自动触发一次 `gc()`；媒体写入后还会立即执行
  媒体预算检查。

### 媒体登记的安全约束

`register_managed_media` 只接纳**位于账户受管根目录内**的文件；根目录之外的路径会被
拒绝（缓存永远不会接管无关路径）。它要求：文件真实存在、是文件、大小不超过
`max_single_media_bytes`。因此调用方应先把媒体下载/移动到 `account_media_dir()`
下，再在 `source_media_path` 中引用。

## `gc` 的细节

一次完整 GC 会：

1. 删除 `created_at` 早于 `now - retention_ms` 的消息。
2. 对每个有数据的账户：
   - 裁剪到 `max_messages_per_account`（保留最新者，按 `created_at DESC, rowid
     DESC`，`LIMIT -1 OFFSET cap`）。
   - 对每类媒体，超过 `media_retention_ms` 的从行中解绑（detach）并删除孤立文件。
   - 执行每账户媒体预算（`max_media_bytes_per_account`）：按 `newest_at ASC` 顺序
     删除超标媒体，直到总额达标。
3. 最后删除受管根目录下无任何行引用的孤立媒体文件。

## 用法示例

### 开启并使用（配合 WeixinClient）

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::quote_cache::{QuoteStore, QuoteMessageInput, Direction};

# fn example() -> ilink_sdk::Result<()> {
let mut store = QuoteStore::open_default()?;  // 默认策略，enabled

let mut input = QuoteMessageInput {
    account_id: "acct".into(),
    conversation_id: "u@im.wechat".into(),
    message_id: "42".into(),
    direction: Direction::Inbound,
    body: "原始引用内容".into(),
    source_media_path: None,
    media_mime: None,
    media_name: None,
    created_at: 0,   // <=0 时用 now_ms() 填充
};
store.put(&input)?;

if let Some(rec) = store.find("acct", "u@im.wechat", "42") {
    println!("found: body={}, dir={:?}", rec.body, rec.direction);
}
# Ok(())
# }
```

### 登记受管媒体

```rust,no_run
# use ilink_sdk::prelude::*;
# use ilink_sdk::messaging::quote_cache::{QuoteStore, QuoteMessageInput, Direction};
# fn example(mut store: QuoteStore) -> ilink_sdk::Result<()> {
// 1) 先把媒体放到受管根目录
let dir = store.ensure_account_media_dir("acct")?;
let file = dir.join("quote.png");
std::fs::write(&file, b"imagedata")?;

// 2) 在 input 中引用该路径
let mut input = QuoteMessageInput {
    account_id: "acct".into(),
    conversation_id: "u@im.wechat".into(),
    message_id: "7".into(),
    direction: Direction::Inbound,
    body: String::new(),                 // 允许空 body，只要有媒体
    source_media_path: Some(file.to_string_lossy().into_owned()),
    media_mime: Some("image/png".into()),
    media_name: None,
    created_at: 0,
};
store.put(&input)?;

if let Some(rec) = store.find("acct", "u@im.wechat", "7") {
    assert!(rec.media_path.is_some());
}
# Ok(())
# }
```

### 删除账户与手动 GC

```rust,no_run
# use ilink_sdk::prelude::*;
# use ilink_sdk::messaging::quote_cache::QuoteStore;
# fn example(mut store: QuoteStore) {
store.delete_account("acct");   // 删行 + 删该账户受管媒体目录
store.gc();                    // 随时可手动触发完整 GC
# }
```

## 设计要点与注意事项

1. **受管媒体「只进自己家」。**
   `register_managed_media` 严格校验路径在 `account_media_dir()` 之内；根目录外的
   文件会被拒绝，因此务必先 `ensure_account_media_dir` 再落盘。
2. **`put` 对「空 body 且无媒体」静默忽略。** 不要指望用空 input「占位」或清缓存。
3. **`find` 会删除过期记录。** 调用方不应假设「曾经 put 就一定能 find 到」——超过
   保留期会在读取时清理。
4. **`open(policy)` 在 `enabled==false` 时返回 `Ok(None)`。** 调用方需处理 `None`；
   `open_default()` 则假定默认策略（enabled）并 `expect`，不要对禁用策略用它。
5. **`QuoteStore` 不是 `Sync`。** 与 `WeixinClient` 类似，受管媒体由 SQLite
   连接持有，应以 clone/单任务方式使用，或每个任务持有独立 `QuoteStore`。
6. **写入触发自动 GC。** 每 100 次写自动 GC；若写入量极大且有实时性要求，可观察
   这点对延迟的影响。
