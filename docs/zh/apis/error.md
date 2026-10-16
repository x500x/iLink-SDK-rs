# 错误处理 API

> English: [English](../../apis/error.md)

`ilink_sdk` 的所有可能失败的操作都返回 `Result<T>`（即
`std::result::Result<T, Error>`）。错误处理模块位于 [`../../src/error.rs`](../../../src/error.rs)，
它把传输层失败统一分类成 `ErrorKind`，让调用方可以基于错误类别做分支，而不必
对错误字符串做匹配。

## 类型总览

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

## `enum ErrorKind`

传输层失败的分类。`business` 级别的失败（HTTP 200 但 `ret != 0`）**不会**出现
在这个枚举里——它们以 `ErrorKind::Unknown` 形式呈现（附带业务错误信息在
`Error::message` 中）。

| 变体 | 含义 | 典型触发 |
| --- | --- | --- |
| `Dns` | DNS 解析失败 | `ENOTFOUND`、`EAI_AGAIN`、`getaddrinfo` |
| `Tcp` | TCP 连接 / 连接重置失败 | `ECONNREFUSED`、`ETIMEDOUT`、`ENETUNREACH` |
| `Tls` | TLS 握手错误 | `SSL`、`CERT`、证书校验失败 |
| `Timeout` | 客户端侧超时 | 请求超过 `timeout_ms` |
| `Cancelled` | 外部取消信号触发 | 传入的 `CancellationToken` 被取消 |
| `Unknown` | 其他网络级错误 | 无法分类的失败，以及所有业务错误 |

### 方法

```rust
impl ErrorKind {
    /// 稳定的小写标识符，便于日志与指标。例如 "dns"、"tcp"、"tls"。
    pub fn as_str(&self) -> &'static str;

    /// 人类可读的英文描述，措辞与 TypeScript 实现一致。
    pub fn description(&self) -> &'static str;
}

impl std::fmt::Display for ErrorKind { /* 委托给 as_str */ }
```

示例：

```rust
use ilink_sdk::error::ErrorKind;
assert_eq!(ErrorKind::Dns.as_str(), "dns");
assert_eq!(ErrorKind::Tls.description(), "TLS handshake error");
```

## `struct Error`

SDK 返回的单一错误类型。

```rust
#[derive(Debug, Clone)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    external_cancel: bool,
}
```

### 构造与转换

```rust
impl Error {
    /// 用给定类别与消息构造错误。
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self;

    /// 便捷构造：`ErrorKind::Unknown`。
    pub fn unknown(message: impl Into<String>) -> Self;

    /// 标记为「外部取消」导致（区别于内部超时）。
    pub fn with_external_cancel(mut self) -> Self;
}
```

`Error` 也实现了 `From`，把常见底层错误自动转换为 SDK 错误，因此 `?`
运算符可直接用于这些类型：

| 来源类型 | 转换方式 |
| --- | --- |
| `std::io::Error` | `TimedOut`→`Timeout`；`ConnectionRefused`/`ConnectionReset`/`NotConnected`/`NetworkDown`→`Tcp`；否则按字符串分类 |
| `serde_json::Error` | `Unknown`（包成 `"json: ..."`） |
| `reqwest::Error` | 见 `from_reqwest` / `from_reqwest_wrap` |
| `reqwest::header::InvalidHeaderValue` / `ToStrError` | `Unknown` |
| `rustls::Error` | `Unknown`（包成 `"tls: ..."`） |
| `url::ParseError` | `Unknown` |
| `hex::FromHexError` | `Unknown` |
| `base64::DecodeError` | `Unknown` |
| `rusqlite::Error` | `Unknown`（包成 `"sqlite: ..."`） |

### 判定方法

```rust
impl Error {
    /// 失败类别。
    pub fn kind(&self) -> ErrorKind;

    /// 是否由外部取消信号导致。
    pub fn is_external_cancel(&self) -> bool;

    /// 是否为客户端超时 **或** 外部取消。对应 TS 的 AbortError 检查。
    pub fn is_abort(&self) -> bool;

    /// 是否为「可重试的连接问题」（相对协议/业务错误）。
    pub fn is_transport(&self) -> bool;
}
```

`is_abort()` 是长轮询循环用来判断「静默重试」的关键：`get_updates` 在内部超时
或外部取消时返回空 `ret:0` 响应（不会报错），但其它路径上若遇到 `is_abort()`
为真的错误，调用方同样可以当作「继续轮询」处理。

示例——基于类别分支（来自顶层 README）：

```rust
match client.sender().send_text(to, text, &opts).await {
    Ok(result) => println!("已发送 {}", result.message_id),
    Err(err) => match err.kind() {
        ErrorKind::Timeout | ErrorKind::Cancelled => { /* 重试或干净退出 */ }
        ErrorKind::Dns | ErrorKind::Tcp => { /* 退避后重试 */ }
        _ => { /* 记录并呈现给用户 */ }
    },
}
```

## 分类器

传输层在底层使用两个分类函数，外部一般不需要直接调用，但理解它们有助于预期
错误类别：

```rust
impl Error {
    /// 从任意 `std::error::Error` 链分类，遍历 Display + source 链，
    /// 匹配 TypeScript `classifyFetchError` 的已知错误码。
    pub fn classify<E: std::error::Error>(err: &E) -> Self;

    /// 从 `reqwest::Error` 分类，保留 reqwest 已判定的 Timeout/Cancelled。
    pub fn from_reqwest_wrap(err: reqwest::Error) -> Self;
}
```

`classify_fetch_error_string` 是字符串匹配的核心（非公开函数，行为如下）：

- 含 `ENOTFOUND` / `EAI_AGAIN` / `getaddrinfo` → `Dns`
- 含 `ECONNREFUSED` → `Tcp`
- 含 `UND_ERR_CONNECT_TIMEOUT` / `ETIMEDOUT` / `ENETUNREACH` / `EHOSTUNREACH`
  / `operation timed out` / `timed out` → `Tcp`
- 含 `UND_ERR_SOCKET` / `SSL` / `TLS` / `CERT` / `UNABLE_TO_VERIFY` /
  `DEPTH_ZERO` / `certificate` / `handshake` → `Tls`
- 其余 → `Unknown`

## 设计要点与注意事项

1. **业务错误不在 `ErrorKind` 中分流。** `send_message` 在 `ret != 0` 时返回
   `ErrorKind::Unknown`，但 `message` 里包含 `ret=<code> errmsg=...`。如果需要
   按业务码分支，请在调用后读取响应体（如 `SendMessageResp`），而不是依赖
   `ErrorKind`。
2. **`is_transport()` 不等于「非 `Unknown`」。**
   `is_transport()` 的定义是「不是 `Unknown` 或 `Cancelled`，或者是由外部取消
   触发」——所以 `Unknown` 也**可能**被判定为可重试（当 `external_cancel` 为真
   时）。做退避策略时优先用 `is_abort()` + `kind()` 组合判断，避免误把业务错误
   当成可重试。
3. **`is_abort()` 是监控循环的判定依据。** 它的语义与 TypeScript 的
   `AbortError` 检查一致：客户端超时或外部取消。
4. **错误可安全打印与序列化。** `Error` 实现 `Display` 与 `std::error::Error`；
   `message` 中可能包含部分响应体（已通过 `redact_body` 截断/脱敏）。
