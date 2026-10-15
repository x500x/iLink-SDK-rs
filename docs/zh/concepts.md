# 概念与架构

本页解释 SDK 的组成部分以及数据如何流转，以便各模块指南更易理解。

## 分层

整个 crate 分为三层。大多数应用只用最上层。

```text
  WeixinClient  ── 高层句柄（client 模块）
  WeixinApiClient ── 原始 HTTP 传输 + 头部 + JSON（api 模块）
  storage / cdn / messaging / media ── 被上层使用的辅助模块
```

| 层 | 类型 | 职责 |
| --- | --- | --- |
| 高层 | [`WeixinClient`] / [`WeixinClientBuilder`] | 解析账户，暴露 `sender()`、`qr_login()`、引用缓存、启停通知。 |
| 底层 | [`WeixinApiClient`] / [`WeixinApiOptions`] | 构造请求头、附带 `base_info`、应用超时/取消、反序列化线类型。 |
| 辅助 | `cdn`、`messaging`、`storage`、`media` | 加密、登录流程、持久化、MIME 探测。 |

## 两种客户端类型

### `WeixinApiClient`

基于 `reqwest` 的轻量异步 HTTP 客户端。每一次调用对应一个 CGI 端点：

- `get_updates` — 长轮询收件箱
- `send_message` — 发送一条消息项
- `get_upload_url` — 预签名一个 CDN 上传
- `get_config` — 获取输入状态票据
- `send_typing` — 显示/取消输入状态
- `notify_start` / `notify_stop` — 生命周期 ping
- `get_text` / `post_text_unauthenticated` — 二维码状态轮询路径

它是 `Clone`（传输层在 `Arc` 后共享），并支持自定义 [`HttpTransport`] 用于测试。
见 [API 客户端参考](reference/api-client.md)。

### `WeixinClient`

从已存储账户构建的人机友好句柄。它包裹一个 `WeixinApiClient` 与一个解析后的
[`StoredAccount`]，并暴露：

- `sender()` → 用于发送文本/媒体的 [`MessageSender`]
- `qr_login()` → 绑定到同一传输层的 [`QrLogin`]
- `enable_quote_cache()` → 开启本地引用缓存
- `notify_start()` / `notify_stop()` → 生命周期 ping
- `account()`、`account_id()`、`token()`、`api()` → 内省

`WeixinClient` 是 `Clone`，共享开销很低。它设计为由单个任务持有（引用缓存是
`Send` 但非 `Sync`）；分发给其他任务时请克隆，而不是 `&` 共享。

## 消息如何流转

### 出站

```text
MessageSender::send_text ─┐
MessageSender::send_file ─┤
                          ├─► 构建 WeixinMessage
                          │      ├─ 仅文本/项？  ─► api.send_message
                          │      └─ 媒体？ ─► cdn::upload_*（get_upload_url
                          │                    ─► AES-128-ECB 加密
                          │                    ─► 将密文 POST 到 CDN）
                          │                 ─► api.send_message（携带 CdnMedia 引用）
```

### 入站

```text
api.get_updates ─► WeixinMessage
                      ├─ body_from_items()  ─► 文本 / 语音转写 / 媒体占位符
                      ├─ download_item_media() ─► CDN 下载 + AES-128-ECB 解密
                      └─ message_to_context() ─► MsgContext（面向主机的结构）
```

## 核心数据类型

| 类型 | 位置 | 含义 |
| --- | --- | --- |
| [`WeixinMessage`] | `api::types` | 统一消息（`item_list` 为 `MessageItem` 列表）。 |
| [`MessageItem`] | `api::types` | 单个内容项：文本 / 图片 / 视频 / 文件 / 语音 / 工具调用。 |
| [`CdnMedia`] | `api::types` | CDN 引用：`encrypt_query_param` + `aes_key`。 |
| [`QrSession`] | `messaging::auth` | 二维码登录成功后的凭证。 |
| [`StoredAccount`] | `storage::accounts` | 解析后的账户（token + URL + 标志位）。 |
| [`MsgContext`] | `messaging::context` | 面向主机的入站上下文（正文、媒体、引用）。 |
| [`SendOpts`] | `messaging::outbound` | 发送时的公共参数（`context_token`、`run_id`）。 |
| [`SendResult`] | `messaging::outbound` | 客户端 id + 服务端分配的消息 id。 |

## 无损 uint64 id

微信消息 id 可能超过 `i64::MAX`。在线上它们以 JSON 数字形式到达；SDK 在解析前
会对 `message_id` / `msg_id` / `svr_id` 字段加引号（见 [`parse_api_json`]），从而
以 `String` 无损保留。**始终把消息 id 当作字符串处理**——不要强转为 `u64`。

## 会话守卫

当 `getUpdates` 返回 `errcode == -14`（token 失效）时，SDK 的
[会话守卫][`session_guard`] 会暂停该账户的所有 API 调用一小时。这是自动的、
进程级的；重试前可用
[`is_session_paused`](../../src/api/session_guard.rs) 检查。

## 线程模型

- `WeixinApiClient` 是 `Clone + Send + Sync`。
- `WeixinClient` 是 `Clone + Send` 但**不是** `Sync`（引用缓存使用了基于
  `RefCell` 的 SQLite 连接）。请按任务克隆它。
- 二维码注册表与会话守卫注册表是进程级的 `Mutex` 映射。

## 与 TypeScript 插件的关系

这是 TypeScript `openclaw-weixin` channel 插件的一个独立、符合 Rust 习惯的移植，
但**不包含** OpenClaw 网关集成。没有本地 HTTP 服务器、没有 webhook、也没有 IPC——
每个操作都是一次函数调用。线协议（端点、头部、字段、超时、重试语义）刻意保持忠实；
行为差异都有测试覆盖。
