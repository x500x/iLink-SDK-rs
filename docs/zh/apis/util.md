# 工具函数 API

工具函数模块 [`../../src/util/`](../../src/util/) 提供 ID 生成、日志脱敏与时间辅助，供
SDK 内部与调用方共用。

模块构成：

| 子模块 | 文件 | 职责 |
| --- | --- | --- |
| `random` | [`../../src/util/random.rs`](../../src/util/random.rs) | 唯一 ID 与随机值 |
| `redact` | [`../../src/util/redact.rs`](../../src/util/redact.rs) | 诊断输出的脱敏 |
| `time` | [`../../src/util/time.rs`](../../src/util/time.rs) | 时间戳 |

`util` 模块的公开再导出见 [`../../src/util/mod.rs`](../../src/util/mod.rs)。

## `random` 子模块

```rust
/// 前缀化的唯一 ID： "<prefix>:<毫秒时间戳>-<4字节hex>"
pub fn generate_id(prefix: &str) -> String;

/// 临时文件名： "<prefix>-<毫秒时间戳>-<4字节hex><ext>"
pub fn temp_file_name(prefix: &str, ext: &str) -> String;

/// 16 字节随机值，渲染为 32 个字符的 hex（CDN filekey 用）。
pub fn file_key() -> String;

/// 随机 u32，渲染为十进制字符串（X-WECHAT-UIN 用）。
pub fn random_u32() -> u32;
```

格式示例：

- `generate_id("openclaw-weixin")` → `"openclaw-weixin:1761000000123-a1b2c3d4"`
- `temp_file_name("weixin-remote", ".png")` →
  `"weixin-remote-1761000000123-a1b2c3d4.png"`
- `file_key()` → 32 个 hex 字符

这些函数都基于 `rand::rng()` 的密码学安全随机源（实际为 `rand 0.9` 默认后端）。

用法示例：

```rust,no_run
use ilink_sdk::util::random::{generate_id, temp_file_name, file_key};

# fn example() {
let id = generate_id("openclaw-weixin");
let name = temp_file_name("weixin-out", ".jpg");
let key = file_key();
println!("{}, {}, {}", id, name, key);
# }
```

## `redact` 子模块

诊断日志在打印前会经脱敏，避免泄露 token、授权头或签名 query。

```rust
pub const DEFAULT_TOKEN_PREFIX_LEN: usize = 6;
pub const DEFAULT_BODY_MAX_LEN: usize = 200;

/// 截断字符串，超出时追加长度指示（如 "abc…(len=18)"）；空串返回空串。
pub fn truncate(s: &str, max: usize) -> String;

/// 脱敏 token/secret：仅保留前 prefix_len 个字符 + 总长度；缺失返回 "(none)"。
pub fn redact_token(token: Option<&str>, prefix_len: Option<usize>) -> String;

/// 先脱敏 JSON 敏感字段，再 truncate 用于日志。
/// 等价于 redact_json_fields(body) 后截断到 max_len（默认 200）。
pub fn redact_body(body: &str, max_len: Option<usize>) -> String;

/// 把 JSON 中敏感 key 的值替换为 "<redacted>"（不破坏其它文本）。
pub fn redact_json_fields(input: &str) -> String;

/// 去掉 URL 的 query 串（常含签名/token），保留 scheme/host/path。
pub fn redact_url(url: &str) -> String;
```

### 敏感字段

`redact_json_fields` 仅重写以下 JSON key 的值（使用小型手写扫描器，不会误伤字符串
内部「看起来像 key」的文本）：

`context_token`、`bot_token`、`token`、`authorization`、`Authorization`。

### 示例

```rust,no_run
use ilink_sdk::util::redact::{redact_token, redact_body, redact_url};

# fn example() {
assert_eq!(redact_token(None, None), "(none)");
assert_eq!(redact_token(Some("abcdef1234"), None), "abcdef…(len=10)");

let body = r#"{"context_token":"secret","token":"t","other":"keep"}"#;
let out = redact_body(body, None);
assert!(out.contains(r#""context_token":"<redacted>""#));
assert!(out.contains(r#""other":"keep""#));

assert_eq!(
    redact_url("https://ilinkai.weixin.qq.com/ilink/bot/getupdates?token=abc"),
    "https://ilinkai.weixin.qq.com/ilink/bot/getupdates?<redacted>"
);
# }
```

- `truncate` 按**字符**计数（非字节），因此中文等也能正确截断并标注真实长度。
- `redact_json_fields` 只在 `"key" : "value"` 这种确切的 key/value 结构上替换，文本中
  提到 "the token is here" 这种不会被误替换。

## `time` 子模块

```rust
/// 当前 Unix 毫秒时间戳（u128）。
pub fn now_millis() -> u128;

/// 当前 ISO-8601（UTC）字符串，毫秒精度（对应 JS 的 toISOString）。
pub fn now_iso8601() -> String;
```

`now_iso8601` 使用 Howard Hinnant 的 `civil_from_days` 算法从「自 1970-01-01 的天数」
换算 `(year, month, day)`，再组装为 `YYYY-MM-DDTHH:MM:SS.mmmZ` 形态：

```rust,no_run
use ilink_sdk::util::time::{now_iso8601, now_millis};

# fn example() {
let s = now_iso8601();
let ms = now_millis();
println!("{} ({} ms)", s, ms);
// 例如：2026-01-01T00:00:00.000Z
# }
```

## 设计要点与注意事项

1. **`generate_id` / `temp_file_name` 含毫秒时间戳，理论上单机内高并发仍可能碰撞**
   （碰撞概率极低，因为有 4 字节随机后缀）。如需全局唯一，调用方应自行加盐。
2. **`redact_token` 默认保留前 6 个字符。** 足以在日志中区分不同 token 而不泄露
   敏感部分；可传 `prefix_len` 覆盖。
3. **`redact_url` 对无效 URL 回退到截断。** 无法解析的 URL 直接 `truncate(url, 80)`，
   不会报错。
4. **`now_millis` 返回 `u128`。** 与 `i64::MAX` 无溢出风险（远超 2262 年）；与后端
   uint64 时间戳对接时以 `String` 保存，避免截断（见 [api-client.md](api-client.md)）。
5. **时间换算支持公元前日期。** `civil_from_days` 对负天数（1970 之前）同样正确，
   便于测试边界。
