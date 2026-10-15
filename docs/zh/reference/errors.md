# 错误与重试

SDK 中每个可能失败的操作都返回
[`ilink_sdk::Result<T>`](../../../src/error.rs)，参数化为 [`Error`]。传输层失败
会带有一个分类后的 [`ErrorKind`]，以便调用方按失败原因分支处理，而不是字符串匹配——
这与 TypeScript 的 `classifyFetchError` 一致。

## `ErrorKind`

| 变体 | 何时 | 可重试？ |
| --- | --- | --- |
| `Dns` | 域名解析失败（`ENOTFOUND`、`EAI_AGAIN`、`getaddrinfo`）。 | 是（退避） |
| `Tcp` | 连接被拒 / 重置 / 不可达（`ECONNREFUSED`、`ETIMEDOUT`、`ENETUNREACH`）。 | 是（退避） |
| `Tls` | TLS 握手失败（`SSL`、`CERT`、`handshake`）。 | 视情况（重连） |
| `Timeout` | 客户端超时或长轮询截止。 | 是 |
| `Cancelled` | 外部 `CancellationToken` 触发。 | 否（干净停止） |
| `Unknown` | 其他（含在此处冒泡的业务级 `ret != 0`）。 | 视情况 |

## 关键判定方法

| 方法 | 含义 |
| --- | --- |
| `kind()` | 失败的 [`ErrorKind`]。 |
| `is_abort()` | 当为 `Timeout` **或** `Cancelled` 时为 `true`。这是监听循环用来判断
  `getUpdates` 是否应返回空 `ret: 0` 响应的检查——与 TypeScript 的 `AbortError` 测试一致。 |
| `is_external_cancel()` | 仅当被外部 token 取消（而非内部超时）时为 `true`。 |
| `is_transport()` | 当为连通性问题时（即可重试）为 `true`。 |

## 示例：按原因分支

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use ilink_sdk::{ErrorKind, WeixinApiClient};
use ilink_sdk::api::types::{GetUpdatesReq, MessageItemType};
use ilink_sdk::storage::sync_buf::save_for_account;

let account_id = client.account_id().to_string();
let mut cursor = ilink_sdk::storage::sync_buf::load_for_account(&account_id);

loop {
    let result = client.api().get_updates(
        &GetUpdatesReq { get_updates_buf: cursor.clone(), sync_buf: None },
        None,
        None,
    ).await;

    match result {
        Ok(resp) => {
            if let Some(buf) = resp.get_updates_buf {
                cursor = Some(buf);
                save_for_account(&account_id, &cursor.clone().unwrap_or_default());
            }
            // 处理 resp.msgs ...
            break;
        }
        Err(err) => match err.kind() {
            ErrorKind::Cancelled => {
                // 外部取消：干净地停止循环。
                break;
            }
            ErrorKind::Timeout | ErrorKind::Dns | ErrorKind::Tcp => {
                // 瞬时连通性问题：退避后重试。
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            ErrorKind::Tls => {
                // 关闭、重建客户端，重试一次。
                return Err(err);
            }
            ErrorKind::Unknown => {
                // 可能是业务错误（ret != 0）或未预期失败。
                return Err(err);
            }
        },
    }
}
# Ok(())
# }
```

## 错误如何分类

[`Error::from_reqwest`] 与 [`Error::classify`] 会检查错误的 `Display` 输出及其 source
链，寻找已知的 libuv / OpenSSL / Rust std 错误码：

- `ENOTFOUND` / `EAI_AGAIN` / `getaddrinfo` → `Dns`
- `ECONNREFUSED` → `Tcp`
- `ETIMEDOUT` / `ENETUNREACH` / `EHOSTUNREACH` / `operation timed out` → `Tcp`
- `SSL` / `TLS` / `CERT` / `handshake` / `certificate` → `Tls`

`io::Error` 按 `ErrorKind` 映射（`TimedOut` → `Timeout`，`ConnectionRefused`/
`ConnectionReset` → `Tcp`）。`serde_json`、`url`、`hex`、`base64`、`rusqlite`、`rustls`
的错误变为 `ErrorKind::Unknown`。

## 业务错误不被分类

HTTP 200 但 `ret != 0`（或 `errcode != 0`）的响应**不是**传输错误——它会作为
`ErrorKind::Unknown` 附带描述性消息冒泡，或以类型化响应的形式通过
[`GetUpdatesResp::is_api_error`] / [`SendMessageResp::is_error`] 返回。用这些辅助做协议
级成功检查；把 `ErrorKind` 留给传输/重试决策。

## token 失效 → 会话暂停

当 `getUpdates` 返回 `errcode == -14`（`STALE_TOKEN_ERRCODE`）时，
[会话守卫](../../../src/api/session_guard.rs) 会暂停该账户的所有 API 调用一小时
（`SESSION_PAUSE_DURATION`）。[`is_session_paused`] 让你在重试前检查。请重新认证
（二维码登录）以恢复。
