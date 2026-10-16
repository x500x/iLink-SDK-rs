# 日志 API

> English: [English](../../apis/logging.md)

日志模块 [`../../src/logging.rs`](../../../src/logging.rs) 基于 `tracing`。SDK 内部大量使用
`tracing` 记录 span 与事件，但**未安装全局 subscriber 时这些记录会被静默丢弃**——
因此下游应用可以自行选择 `env_logger`、`tracing_subscriber` 或完全不接入。

当启用 `cli` feature 时，`init_tracing` 会安装一个遵循 `RUST_LOG` 的默认 subscriber。

## 再导出的宏

```rust
pub use tracing::{debug, error, info, trace, warn};
```

调用方可以像使用 `tracing` 一样直接 `info!("...")`、`warn!(...)`，无需额外依赖
`tracing` crate（前提是 SDK 的 `tracing` 版本在依赖图中可见；通常已传递可用）。

## 常量与类型

```rust
/// 每个 SDK span 携带的账户 id 字段名。
pub const ACCOUNT_FIELD: &str = "account";

/// CLI 使用的日志级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level { Trace, Debug, Info, Warn, Error }
impl Level {
    pub fn as_str(&self) -> &'static str;
}
impl std::fmt::Display for Level { /* 委托 as_str */ }
impl std::str::FromStr for Level { type Err = String; }   // "warning" -> Warn
```

## `init_tracing`

```rust
#[cfg(feature = "cli")]
pub fn init_tracing(level: Level);

#[cfg(not(feature = "cli"))]
pub fn init_tracing(_level: Level) {}   // 无操作（未启用 cli 时）
```

`init_tracing` 仅在 `cli` feature 开启时编译进真正的 subscriber 安装逻辑；否则为空
函数。它遵循 `RUST_LOG` 环境变量（默认级别由传入的 `level` 决定，典型为 `Info`）。

## 用法示例

### CLI 应用（启用 cli feature）

```rust,no_run
use ilink_sdk::logging::{init_tracing, Level};

fn main() {
    init_tracing(Level::Info);   // 遵循 RUST_LOG，默认 Info
    // ...其余逻辑，SDK 内部 info!/warn! 现在可见
}
```

运行：

```bash
RUST_LOG=debug cargo run --bin weixin listen
```

### 库应用（不使用 cli feature，自行接入 tracing）

```rust,no_run
use ilink_sdk::logging::ACCOUNT_FIELD;
use tracing::info;

# fn example() {
// 调用方自己装 subscriber（tracing-subscriber / opentelemetry / …）
info!(account = "acct", "处理入站消息");
let _ = ACCOUNT_FIELD;
# }
```

SDK 内部 span（如 `getUpdates` 轮询、CDN 上传）会携带 `account` 字段，便于按账户过滤。

## 设计要点与注意事项

1. **未安装 subscriber 不会报错。** SDK 的所有日志都是通过 `tracing` 发出；没有全局
   订阅器时记录被丢弃，不影响业务正确性。
2. **`init_tracing` 依赖 `cli` feature。** 纯库（构建时 `--no-default-features`）里它
   是空函数；若要 SDK 帮你装好 subscriber，请用默认 features 构建。
3. **`ACCOUNT_FIELD` 用于跨 span 关联。** 如果你在自己的 `tracing` 代码里用相同的字段名
   记录账户 id，日志系统能把 SDK 内部事件与你的事件按账户聚合。
4. **日志中的敏感数据已脱敏。** SDK 在记录响应体、URL、token 时都先经 `util::redact`
   处理（见 [util.md](util.md)），因此直接开启 `debug` 不会泄露凭据。
5. **不要在热路径手动构造 subscriber。** 每个进程应只安装一次全局 subscriber；
   `init_tracing` 内部已处理重复安装（按 `tracing-subscriber` 的默认行为）。
