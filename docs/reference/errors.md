# Errors & retries

Every fallible operation in the SDK returns
[`ilink_sdk::Result<T>`](../../src/error.rs), parameterised by [`Error`].
Transport-level failures carry a classified [`ErrorKind`] so callers can branch
on the failure cause instead of string matching — mirroring the TypeScript
`classifyFetchError`.

## `ErrorKind`

| Variant | When | Retryable? |
| --- | --- | --- |
| `Dns` | Hostname resolution failed (`ENOTFOUND`, `EAI_AGAIN`, `getaddrinfo`). | yes (back off) |
| `Tcp` | Connection refused / reset / unreachable (`ECONNREFUSED`, `ETIMEDOUT`, `ENETUNREACH`). | yes (back off) |
| `Tls` | TLS handshake failed (`SSL`, `CERT`, `handshake`). | maybe (reconnect) |
| `Timeout` | Client-side timeout or long-poll deadline reached. | yes |
| `Cancelled` | An external `CancellationToken` fired. | no (stop cleanly) |
| `Unknown` | Anything else (incl. business-level `ret != 0` surfaced here). | depends |

## Key predicates

| Method | Meaning |
| --- | --- |
| `kind()` | The [`ErrorKind`] of the failure. |
| `is_abort()` | `true` for `Timeout` **or** `Cancelled`. This is the check the monitor loop uses to decide whether `getUpdates` should return an empty `ret: 0` response — matching the TypeScript `AbortError` test. |
| `is_external_cancel()` | `true` only when cancelled by an external token (not an internal timeout). |
| `is_transport()` | `true` for connectivity problems (i.e. retryable). |

## Example: branch on the cause

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
            // process resp.msgs ...
            break;
        }
        Err(err) => match err.kind() {
            ErrorKind::Cancelled => {
                // External cancellation: stop the loop cleanly.
                break;
            }
            ErrorKind::Timeout | ErrorKind::Dns | ErrorKind::Tcp => {
                // Transient connectivity issue: back off and retry.
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            ErrorKind::Tls => {
                // Close, re-create the client, and retry once.
                return Err(err);
            }
            ErrorKind::Unknown => {
                // Could be a business error (ret != 0) or an unexpected failure.
                return Err(err);
            }
        },
    }
}
# Ok(())
# }
```

## How errors are classified

[`Error::from_reqwest`] and [`Error::classify`] inspect the error's `Display`
output plus its source chain for well-known libuv / OpenSSL / Rust std codes:

- `ENOTFOUND` / `EAI_AGAIN` / `getaddrinfo` → `Dns`
- `ECONNREFUSED` → `Tcp`
- `ETIMEDOUT` / `ENETUNREACH` / `EHOSTUNREACH` / `operation timed out` → `Tcp`
- `SSL` / `TLS` / `CERT` / `handshake` / `certificate` → `Tls`

`io::Error` is mapped by `ErrorKind` (`TimedOut` → `Timeout`,
`ConnectionRefused`/`ConnectionReset` → `Tcp`). `serde_json`, `url`, `hex`,
`base64`, `rusqlite`, and `rustls` errors become `ErrorKind::Unknown`.

## Business errors are not classified

An HTTP 200 response whose `ret != 0` (or `errcode != 0`) is **not** a
transport error — it surfaces as `ErrorKind::Unknown` with a descriptive
message, or as a typed response via [`GetUpdatesResp::is_api_error`] /
[`SendMessageResp::is_error`]. Use those helpers for protocol-level success
checks; keep `ErrorKind` for transport/retry decisions.

## Stale token → session pause

When `getUpdates` reports `errcode == -14` (`STALE_TOKEN_ERRCODE`), the
[session guard](../../src/api/session_guard.rs) pauses all API calls for that
account for one hour (`SESSION_PAUSE_DURATION`). [`is_session_paused`] lets you
check before retrying. Re-authenticate (QR login) to resume.
