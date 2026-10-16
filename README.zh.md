# iLink SDK (ilink-sdk)

[![License: MIT](https://img.shields.io/badge/license-MIT-green)](./LICENSE)
[![CI](https://github.com/x500x/iLink-SDK-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/x500x/iLink-SDK-rs/actions/workflows/ci.yml)

> English: [README.md](README.md)

面向腾讯**微信 (iLink)** 后端协议的 Rust SDK。

本 crate 是 TypeScript
[`openclaw-weixin`](https://github.com/Tencent/openclaw-weixin) channel 插件的忠实、
符合 Rust 习惯的移植，去掉了 OpenClaw 网关集成。剩下的部分是一个可以嵌入任意
Rust 应用的纯库：

- 二维码登录 + 本地凭证存储
- 长轮询消息投递（`getUpdates`）
- 文本、图片、视频、文件与语音消息
- CDN 媒体上传 / 下载，AES-128-ECB 解密
- 输入状态（typing）指示
- 本地引用缓存，用于仅含 ID 的入站引用

**不内嵌 HTTP 服务器。** 一切都是对 crate 的函数调用；没有守护进程、没有 webhook
端点、也不需要搭 IPC。

| | 要求 |
| --- | --- |
| Rust | `1.82` 及以上（edition 2021） |
| Tokio | 必需（异步运行时） |
| 微信后端 | 与 TypeScript 插件相同的 iLink 端点 |

## 快速开始

```toml
[dependencies]
ilink-sdk = "2.4"
tokio = { version = "1", features = ["full"] }
```

```rust,no_run
use ilink_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 启动二维码登录并展示 URL。
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = ilink_sdk::messaging::QrLogin::new(&api);
    let start = qr.start(None).await?;
    println!("请扫描此二维码：{}", start.qrcode_url);

    // 2. 等待用户在手机上确认。
    let result = qr
        .wait(&start.session_key, std::time::Duration::from_secs(480), |_retry| async {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(line.trim().to_string())
        }, |_| async {})
        .await?;

    let session = result.session.expect("登录必须成功");
    let account_id = ilink_sdk::storage::accounts::normalize_account_id(&session.bot_id);

    ilink_sdk::storage::accounts::save_account(
        &account_id,
        &ilink_sdk::storage::accounts::AccountUpdate {
            token: Some(session.token),
            base_url: Some(session.base_url),
            user_id: Some(session.user_id),
        },
    )?;
    ilink_sdk::storage::accounts::register_account_id(&account_id)?;

    // 3. 回发一条消息。
    let client = WeixinClientBuilder::new().account_id(&account_id).build().await?;
    client
        .sender()
        .send_text(
            "someone@im.wechat",
            "hello from Rust",
            &ilink_sdk::messaging::SendOpts::default(),
        )
        .await?;

    Ok(())
}
```

## 设计说明

### 忠实于线协议

请求/响应的结构、头部、字段语义、超时与重试策略都对齐 TypeScript 实现。凡有差异之处，
都是有意为之，并有测试覆盖：

| 方面 | TypeScript | Rust |
| --- | --- | --- |
| uint64 消息 id | 在 `JSON.parse` 前改写为字符串 | 在 `serde_json` 解析前加引号 |
| `getUpdates` 超时 | 返回 `ret: 0` + 相同游标 | 一致 |
| `sendMessage` 的 `ret` | 仅非零时为错误 | 一致 |
| AES-128-ECB | `node:crypto` | 逐块 ECB（见下） |
| 引用缓存 | `node:sqlite` | `rusqlite`（内置 SQLite） |

**ECB 是刻意逐块实现的。** `aes` 没有独立的 ECB 模式，而零 IV 的 CBC 并**不**等价：
它会把每个明文块与前一个密文块异或，导致相同的明文块产生不同的密文。微信后端期望的是
真正的 ECB，因此 [`encrypt_aes_ecb`](src/cdn/aes_ecb.rs) 独立地变换每个 16 字节块。
有一个测试断言其输出与 Node 的 `aes-128-ecb` 逐字节一致。

### 存储位置

所有状态都放在 TypeScript 插件使用的同一目录下，因此同一台机器上的 Rust 客户端与
TypeScript 客户端可以共享凭证：

```text
$OPENCLAW_STATE_DIR/openclaw-weixin/
├── accounts.json                     # 已注册账户 id 列表
├── accounts/
│   ├── <id>.json                     # token、baseUrl、userId
│   ├── <id>.sync.json                # getUpdates 游标
│   └── <id>.context-tokens.json      # 回复上下文 token
├── ref-messages.sqlite               # 引用缓存
└── debug-mode.json                   # 按账户的调试开关
```

用 `OPENCLAW_STATE_DIR`（或 `CLAWDBOT_STATE_DIR`）覆盖根目录。配置从
`$OPENCLAW_CONFIG` 读取，否则读 `<state_dir>/openclaw.json` 的
`channels.openclaw-weixin` 段。

### 错误处理

每个可能失败的调用都返回 [`Result<T>`](src/error.rs)。传输失败带有 [`ErrorKind`]，
让调用方可以按原因分支，而不是做字符串匹配：

```rust
match client.sender().send_text(to, text, &opts).await {
    Ok(result) => println!("sent {}", result.message_id),
    Err(err) => match err.kind() {
        ErrorKind::Timeout | ErrorKind::Cancelled => { /* 重试或干净退出 */ }
        ErrorKind::Dns | ErrorKind::Tcp => { /* 退避后重试 */ }
        _ => { /* 记录并告知用户 */ }
    },
}
```

`err.is_abort()` 是监控循环使用的判断：客户端超时*或*外部取消时为真，与 TypeScript 的
`AbortError` 测试一致。

## 命令行工具（CLI）

`cli` feature（默认开启）会构建一个 `weixin` 二进制，封装整个库，便于在写代码前
端到端地演练（并可脚本化）：

```bash
cargo run -- login                 # 二维码登录，保存凭证
cargo run -- accounts              # 列出已注册账户
cargo run -- status                # 解析后的账户设置
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "look"
cargo run -- send-item <@im.wechat> '{"type":1,"text_item":{"text":"hi"}}'
cargo run -- listen                # 长轮询、保存媒体、打印入站消息
cargo run -- typing <@im.wechat> on
cargo run -- notify start
cargo run -- upload <@im.wechat> ./photo.png --media-type image
cargo run -- download --param <p> --aes-key <k> -o ./out.jpg
cargo run -- quote policy && cargo run -- quote gc
cargo run -- context set <@im.wechat> <token>
cargo run -- sync show
cargo run -- session status
cargo run -- mime ./photo.png
cargo run -- logout <account-id>
```

每个命令都接受 `--json` 以输出机器可读结果，以及全局路由参数 `--account`、
`--base-url`、`--cdn-base-url`、`--route-tag`、`--bot-agent`、`--timeout-ms`。
完整命令参考见 [docs/reference/cli.md](docs/reference/cli.md)。

## 共享库（dll / so / dylib）

crate 会同时构建为 `rlib`、`cdylib` 与 `staticlib`，因此 `cargo build --release`
会在 Rust rlib 之外产出一个原生共享/静态库：

| 平台 | 产物 |
| --- | --- |
| Windows | `ilink_sdk.dll`、`ilink_sdk.dll.lib`（导入库）、`ilink_sdk.lib`（静态） |
| Linux | `libilink_sdk.so`、`libilink_sdk.a` |
| macOS | `libilink_sdk.dylib`、`libilink_sdk.a` |

整个 SDK 都可通过 [`include/ilink_sdk.h`](include/ilink_sdk.h) 中声明的单一
JSON 进 / JSON 出调用，从 C/C++/Go/… 访问：

```c
char *out = ilink_call("send.text",
                       "{\"to\":\"someone@im.wechat\",\"text\":\"hi\"}");
puts(out);                 /* {"ok":true,"result":{...}} */
ilink_string_free(out);
```

可运行的示例在 [`examples/c/ilink_demo.c`](examples/c/ilink_demo.c)；操作清单位于
[docs/reference/ffi.md](docs/reference/ffi.md)。

**默认链接共享库。** Windows 产物使用 `+crt-static` 构建，因此不依赖 Visual C++
Redistributable。Windows 的*静态*库仅限 MSVC，包含头文件时需要 `-DILINK_SDK_STATIC`；
MinGW 或 Go 的 cgo 请改用 DLL。确切参数见
[docs/reference/ffi.md](docs/reference/ffi.md#linking)。

CI 在 Linux、macOS 和 Windows（x86_64 与 aarch64）上构建并测试，并把库作为 release
产物发布（见 [`.github/workflows/ci.yml`](.github/workflows/ci.yml) 与
[`.github/workflows/release.yml`](.github/workflows/release.yml)）。macOS 上会把 dylib
的 install name 改写为 `@rpath/libilink_sdk.dylib`，以便打包进 app bundle。

## 多语言绑定

除 Rust 和 C 外，还提供了 Go、Python、C++、Java、Kotlin 的薄绑定。它们都是同一套
JSON C ABI 的封装，没有按语言重复实现协议。每个绑定都提供通用入口 `call(op, params)`
触达全部操作，并为常用流程提供**类型化模型** —— 见
[`bindings/`](bindings/README.zh.md)。

Python、Go、Java、Kotlin 包都内置了原生库，因此装好绑定就能用（不需要 Rust 工具链，
也不需要手动配置库）：

```bash
pip install ilink-sdk                    # Python
go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink   # Go（纯 Go，无需 cgo）
```

| 语言 | 目录 | 安装 | 说明 |
| --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](include/ilink_sdk.h) | 链接库 | 头文件 + [`examples/c`](examples/c) |
| C++ | [`bindings/cpp`](bindings/cpp) | `find_package(ilink-sdk)` | header-only，RAII，类型化模型 |
| Rust | crate 本体 | `cargo add ilink-sdk` | 原生 |
| Python | [`bindings/python`](bindings/python) | `pip install ilink-sdk` | `ctypes`，无依赖，dataclass |
| Go | [`bindings/go`](bindings/go) | `go get .../bindings/go/ilink` | 纯 Go（purego），类型化 struct |
| Java | [`bindings/java`](bindings/java) | Maven `io.ilink:ilink-sdk` | JNA，类型化模型 |
| Kotlin | [`bindings/kotlin`](bindings/kotlin) | Gradle `io.ilink:ilink-sdk` | JNA，类型化 data class |

```python
# Python
import ilink_sdk
ilink_sdk.call("mime.info", {"path": "photo.png"})
ilink_sdk.Client().mime_info("photo.png").mime          # 类型化方法
```

```go
// Go
ilink.Call("mime.info", map[string]any{"path": "photo.png"})
info, _ := ilink.NewClient(ilink.Config{}).MimeInfo("photo.png")  // 类型化方法
```

## 配置

`openclaw.json` 的 `channels.openclaw-weixin` 段中的可选设置：

| 键 | 类型 | 默认 | 用途 |
| --- | --- | --- | --- |
| `botAgent` | string | `OpenClaw` | 作为 `base_info.bot_agent` 发送的可观测性标识。不是认证凭证。 |
| `routeTag` | string \| number | — | 作为 `SKRouteTag` 头转发。 |
| `accounts.<id>.baseUrl` | string | `https://ilinkai.weixin.qq.com` | 按账户的 API base URL。 |
| `accounts.<id>.cdnBaseUrl` | string | `https://novac2c.cdn.weixin.qq.com/c2c` | 按账户的 CDN base URL。 |
| `accounts.<id>.enabled` | boolean | `true` | 禁用某账户但不删除它。 |
| `quoteCache` | object | 见下 | 本地引用缓存策略。 |

`botAgent` 接受 UA 风格的 `Name/Version` token，可带 `(comment)`。非法 token 会被丢弃；
结果为空时回退到 `OpenClaw`。超过 256 字节的值会通过丢弃尾部 token 来截断。

引用缓存键：`enabled`、`retentionDays`（30）、`maxMessagesPerAccount`（10000）、
`mediaRetentionDays`（7）、`maxMediaBytesPerAccount`（256 MiB）、
`maxSingleMediaBytes`（25 MiB）。非法值回退到默认值。

## Feature 开关

| Feature | 默认 | 作用 |
| --- | --- | --- |
| `cli` | 开 | `weixin` 二进制（`clap`、`qrcode`、`directories`、`tracing-subscriber`） |

`src/ffi.rs` 中的 C ABI 始终编译；它没有额外依赖。

```bash
cargo build --no-default-features   # 仅库 + C ABI，不含 CLI
```

## Release 构建配置

`[profile.release]` 针对小巧、自包含的产物做了调优：

- `lto = true`、`codegen-units = 1`、`strip = true`；
- `opt-level = "s"` —— 热点路径受网络/TLS 限制，`ring` 的汇编不受影响，因此这几乎不
  损失 CPU，却让产物缩小约 30%（1 MiB 的 AES-ECB 往返约慢 20%，折算到 25 MiB 媒体
  文件约 12 ms）。若更看重吞吐而非体积，可设为 `opt-level = 3`；
- Windows 上使用 `+crt-static`（见 [`.cargo/config.toml`](.cargo/config.toml)），
  去除对 Visual C++ Redistributable 的依赖。

HTTP/2 已禁用（客户端不配置 ALPN，走 HTTP/1.1），CLI 解析 `RUST_LOG` 的全局级别而
不引入 `env-filter` 正则引擎；两者都让产物更小。

## 测试

```bash
cargo test                 # 单元 + 集成
cargo test --no-default-features   # 仅库
```

- 单元测试覆盖加密（含与 Node 的逐字节对比）、`<SECRET_2885aeee>`、markdown
  保留（其行为由 TypeScript 测试套件定义，而非猜测），以及引用缓存 GC。
- [`tests/api_integration.rs`](tests/api_integration.rs) 中的集成测试针对 `wiremock`
  服务器跑真实 HTTP 路径，断言头部、请求体、超时处理与取消。

## 文档

| 需求 | 从这里开始 |
| --- | --- |
| 线协议 | 上游 [`docs/protocol.md`](https://github.com/Tencent/openclaw-weixin/blob/main/docs/protocol.md) |
| 模块地图 | [`src/lib.rs`](src/lib.rs) |
| 错误 | [`src/error.rs`](src/error.rs) |
| 媒体 | [`src/cdn`](src/cdn) |
| 中文文档 | [`docs/zh/README.md`](docs/zh/README.md) |

## 与 TypeScript 插件的关系

这是一个独立实现，而非绑定。TypeScript 插件实现的是 OpenClaw channel 契约（网关
生命周期、回复分发、hook 集成、配对）；本 SDK 有意省略所有这些，因此不依赖任何网关。
如果你需要完整的 channel 插件，请使用 TypeScript 项目。如果你想从 Rust 与微信后端
通信 —— 服务、CLI、机器人、测试工具 —— 用这个。

## 许可证

MIT。本 Rust SDK 是上游
[openclaw-weixin](https://github.com/Tencent/openclaw-weixin) 项目的移植，并保留其
版权声明。

- 上游 `openclaw-weixin`：Copyright (C) 2026 Tencent. All rights reserved.
- Rust 移植（`iLink-SDK-rs`）：Copyright (C) 2026 x500x. All rights reserved.

完整文本见 [LICENSE](./LICENSE)。
