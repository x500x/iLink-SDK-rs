# TLS 配置 API

TLS 模块 [`../../src/tls.rs`](../../src/tls.rs) 负责 HTTP 客户端使用的传输层安全配置。服务器
证书通过 `rustls-platform-verifier` 依据**操作系统信任库**校验，因此与浏览器的根证书
一致，也能正确识别企业 MITM 代理；密码套件由 `reqwest` 内置的 `ring` provider 提供。

## `default_tls_config`

```rust
/// 构造默认的 rustls ClientConfig（使用平台证书校验）。
pub fn default_tls_config() -> Result<ClientConfig, RustlsError>;
```

`WeixinApiClient` 与 CDN 上传/下载都在构造 `reqwest::Client` 时调用它：

```rust
let client = reqwest::Client::builder()
    .use_preconfigured_tls(crate::tls::default_tls_config()?)
    .build()?;
```

因此调用方一般不需要直接调用它，除非自己构造 `reqwest::Client` 并想复用 SDK 的校验策略。

## `DangerIgnoreVerifier`

```rust
/// 接受任何证书的校验器（仅测试用）。
#[derive(Debug)]
pub struct DangerIgnoreVerifier;

impl rustls::client::danger::ServerCertVerifier for DangerIgnoreVerifier { /* 全部返回 Ok */ }
```

这是一个**仅用于测试**的「接受所有证书」校验器，对应 TypeScript 测试里的
`NODE_TLS_REJECT_UNAUTHORIZED=0`。生产代码**绝不可**使用，否则会完全丧失 TLS 校验。

示例（测试桩，示意）：

```rust,no_run
# #[cfg(test)]
# mod tests {
use ilink_sdk::tls::DangerIgnoreVerifier;
use rustls::crypto::ring::default_provider;
use rustls::{ClientConfig, RootCertStore};

# fn make_danger_config() -> ClientConfig {
let mut cfg = ClientConfig::builder()
    .with_safe_defaults()
    .with_custom_certificate_verifier(std::sync::Arc::new(DangerIgnoreVerifier))
    .with_no_client_auth();
cfg
# }
# }
```

> 注意：`DangerIgnoreVerifier` 在 SDK 中仅供集成测试内部使用；正式文档之所以列入，是
> 为了说明 TLS 校验的可替换点，以及强调「生产环境不要禁用校验」。

## 设计要点与注意事项

1. **默认信任系统根证书。** 通过 `rustls-platform-verifier`，SDK 自动尊重操作系统的
   信任库与企业代理证书，无需手动配置 `ca-certificates`。
2. **密码套件来自 `ring`。** `reqwest` 默认开启 `rustls` + `ring`，TLS 1.2/1.3 由
   `default_provider()` 提供；SDK 在 `Cargo.toml` 中显式开启了 `tls12` feature。
3. **`default_tls_config` 失败即硬错误。** 它返回 `Result`，如果平台校验器初始化失败
   （极罕见），`WeixinApiClient::new` 也会随之失败；这是预期行为，不应静默吞掉。
4. **不要在生产禁用校验。** `DangerIgnoreVerifier` 仅限测试；任何生产路径使用它都会
   让通信暴露在中间人攻击之下。
5. **与 `reqwest` 的 TLS feature 对齐。** SDK 的 `reqwest` 依赖开启了 `rustls-tls`
   （而非 `native-tls`），因此整个 crate 的 TLS 栈都是 rustls 而非 OpenSSL/SChannel。
