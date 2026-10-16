# iLink SDK — Rust SDK 文档

一个用于腾讯 **微信 (iLink)** 后端协议的 Rust SDK：二维码登录、长轮询消息收发、
以及 CDN 媒体上传/下载，打包为一个纯库，不内嵌 HTTP 服务器、也不依赖网关。

本 `docs/` 文件夹与顶层 [`README.md`](../README.md) 互补，提供实用指南与模块参考。

> 英文版（主文档）见 [`docs/zh/../README.md`](../README.md) — English version:
> [English](../README.md). 项目首页中文版见 [README.zh.md](../../README.zh.md)。

| 指南 | 内容 |
| --- | --- |
| [快速开始](getting-started.md) | 引入依赖，跑通第一个二维码登录 + 发送示例。 |
| [概念与架构](concepts.md) | 模块地图、数据流、核心类型。 |
| [认证（二维码登录）](guides/authentication.md) | 二维码登录流程与会话处理。 |
| [消息收发](guides/messaging.md) | 长轮询接收、发送文本/媒体、输入状态、上下文 token。 |
| [媒体（CDN）](guides/media.md) | AES-128-ECB 上传/下载、语音转码。 |
| [存储与状态](guides/storage.md) | 账户、状态目录、同步游标、引用缓存。 |
| [错误与重试](reference/errors.md) | `ErrorKind` 分类与重试策略。 |
| [配置](reference/configuration.md) | `openclaw.json`、环境变量、feature 开关。 |
| [命令行工具](reference/cli.md) | 内置的 `weixin` 二进制。 |
| [C ABI（dll / so / dylib）](reference/ffi.md) | 通过共享/静态库从 C/C++/Go 调用本 SDK。 |
| [多语言绑定](reference/bindings.md) | 从 C、C++、Go、Python、Java、Kotlin 使用本 SDK。 |
| [发布与分发](reference/distribution.md) | 各产物发布到哪里，以及如何发版。 |
| [API 客户端参考](reference/api-client.md) | 底层 `WeixinApiClient`。 |
| [API 手册](reference/api.md) | 逐个列出每个公开类型、函数与常量的完整清单。 |
| [分板块 API 手册](apis/README.md) | 按领域逐一详述各 API 的作用与用法。 |

> 需要某个公开项的确切签名时，优先查阅 [API 手册](reference/api.md)；
> 它与 `src/lib.rs` 及每个模块文件一一对应。若想要叙事性、以用法为主的解说，
> 参见 [分板块 API 手册](apis/README.md)。

## 目录结构

```text
ilink_sdk/
├── api            HTTP 客户端 + 线类型 + 会话守卫
├── cdn            AES-128-ECB 加密 + CDN 上传/下载
├── client         高层 WeixinClient / Builder
├── error          Error / ErrorKind / Result
├── ffi            C ABI（dll/so/dylib）JSON 分发
├── logging        tracing 订阅器辅助（cli feature）
├── media          MIME 探测
├── messaging      auth（二维码）、outbound、inbound context、typing、quote cache
├── storage        accounts、state dir、sync buf、config
├── tls            rustls 配置
└── util           random / redact / time 辅助
```

## 最低要求

| | 要求 |
| --- | --- |
| Rust | `1.82` 及以上（edition 2021） |
| 异步运行时 | Tokio（`rt-multi-thread`） |
| Feature 开关 | `cli` 默认开启 |

```bash
cargo build --no-default-features   # 仅库 + C ABI，不含 CLI
cargo build --release               # 同时产出 ilink_sdk.dll / .so / .dylib 与 .lib / .a
cargo doc --no-deps --open          # 本地构建本参考文档
```
