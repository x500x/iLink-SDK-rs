# API 客户端参考

> English: [English](../../reference/api-client.md)

[`WeixinApiClient`](../../../src/api/client.rs) 是底层异步 HTTP 客户端，把 SDK 的每一次
操作映射到一个 iLink CGI 端点。大多数应用改用 [`WeixinClient`](../../../src/client/mod.rs)，
但原始客户端可用于自定义流程、测试以及高级插桩。

## 构造

```rust,no_run
use ilink_sdk::api::{WeixinApiClient, WeixinApiOptions};
use ilink_sdk::storage::accounts::DEFAULT_BASE_URL;

# fn run() -> ilink_sdk::Result<()> {
let api = WeixinApiClient::new(
    WeixinApiOptions::new(DEFAULT_BASE_URL)     // API base URL
        .with_token("bot-token")                 // bot token (Bearer)
        .with_route_tag("r1")                    // SKRouteTag 头
        .with_cdn_base_url("https://cdn/c2c")    // 媒体管道使用
        .with_timeout_ms(15_000),                // 每次调用的默认超时
)?;
# Ok(())
# }
```

- `WeixinApiOptions::new(base_url)` 不带 token 起步（用于登录前的调用）。
- `with_bot_agent(agent)` 覆盖 `bot_agent`（会被净化；见[配置](../reference/configuration.md)）。
- `with_transport(opts, transport)` 注入自定义 [`HttpTransport`] 用于单元测试
  （如内存 mock）。

该客户端是 `Clone`——传输层在 `Arc` 后共享。

## 请求头与 `base_info`

每个已认证的 POST 都携带：

- `iLink-App-Id: bot` ([`ILINK_APP_ID`])
- `iLink-App-ClientVersion`：`build_client_version(VERSION)`（如 `2.4.10` →
  `0x0002040A`）
- `AuthorizationType: ilink_bot_token` ([`AUTHORIZATION_TYPE`])
- `Authorization: Bearer <token>`
- `X-WECHAT-UIN`：随机 `u32` 的 base64
- `SKRouteTag`（当设置时）
- 含 `base_info`（`channel_version` + `bot_agent`）的 JSON 体

`base_info()` 返回当前的 `BaseInfo`；`sanitize_bot_agent` 会应用到 agent 字符串。

## 端点

| 方法 | 端点 | SDK 调用 | 说明 |
| --- | --- | --- | --- |
| POST | `ilink/bot/getupdates` | `get_updates(req, timeout_ms, cancel)` | 长轮询；客户端超时/取消时返回携带相同游标的空 `ret:0`。 |
| POST | `ilink/bot/sendmessage` | `send_message(req)` | `ret != 0` 时 `is_error()` 为 `true`；缺失 `ret` 视为成功。 |
| POST | `ilink/bot/getuploadurl` | `get_upload_url(req)` | 预签名一个 CDN 上传。 |
| POST | `ilink/bot/getconfig` | `get_config(ilink_user_id, context_token)` | 返回输入票据。 |
| POST | `ilink/bot/sendtyping` | `send_typing(req)` | 响应体被忽略。 |
| POST | `ilink/bot/msg/notifystop` | `notify_stop()` | 生命周期 ping。 |
| POST | `ilink/bot/msg/notifystart` | `notify_start()` | 生命周期 ping。 |
| GET | `ilink/bot/get_qrcode_status?...` | `get_text(endpoint, timeout_ms)` | 二维码状态轮询（原始文本）。 |
| POST | `ilink/bot/get_bot_qrcode?...` | `post_text_unauthenticated(endpoint, body, timeout_ms)` | 二维码获取，无 `Authorization`。 |

### 超时

默认超时（可被 `WeixinApiOptions::timeout_ms` 覆盖，它设置全局默认）：

| 常量 | 值 | 用于 |
| --- | --- | --- |
| `DEFAULT_LONG_POLL_TIMEOUT_MS` | 35,000 ms | `get_updates` |
| `DEFAULT_API_TIMEOUT_MS` | 15,000 ms | `send_message`、`get_upload_url` |
| `DEFAULT_CONFIG_TIMEOUT_MS` | 10,000 ms | `get_config`、`send_typing`、`notify_*` |

`get_updates` 超时（或被取消）时**不会报错**——它返回一个携带相同 `get_updates_buf` 的
空 `ret: 0` 响应，因此调用方只需简单重试。

## JSON id 处理

后端把 uint64 消息 id（`message_id`、`msg_id`、`svr_id`）作为 JSON 数字发送，可能超过
`i64::MAX`。[`parse_api_json`] 会在反序列化前对这些字段加引号，从而以 `String` 无损保留。
已经是字符串、`null` 或 `true`/`false` 的 `serde` 值不会被改动。

## 错误分类

传输失败由 [`Error::from_reqwest_wrap`] 映射为 [`ErrorKind`]。完整的分类表与重试建议见
[错误与重试](../reference/errors.md)。

## 不联网的测试

`HttpTransport` 是一个 trait，因此你可以提供一个返回 `(StatusCode, HeaderMap, Vec<u8>)`
的 mock。集成测试 `tests/api_integration.rs` 正是通过这种方式断言请求头、请求体、超时
行为与取消逻辑，而无需触及微信后端。
