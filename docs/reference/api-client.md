# API client reference

[`WeixinApiClient`](../../src/api/client.rs) is the low-level async HTTP client
that maps each SDK operation to one iLink CGI endpoint. Most applications use
[`WeixinClient`](../../src/client/mod.rs) instead, but the raw client is
available for custom flows, testing, and advanced instrumentation.

## Construction

```rust,no_run
use ilink_sdk::api::{WeixinApiClient, WeixinApiOptions};
use ilink_sdk::storage::accounts::DEFAULT_BASE_URL;

# fn run() -> ilink_sdk::Result<()> {
let api = WeixinApiClient::new(
    WeixinApiOptions::new(DEFAULT_BASE_URL)     // API base URL
        .with_token("bot-token")                 // bot token (Bearer)
        .with_route_tag("r1")                    // SKRouteTag header
        .with_cdn_base_url("https://cdn/c2c")    // used by the media pipeline
        .with_timeout_ms(15_000),                // default per-call timeout
)?;
# Ok(())
# }
```

- `WeixinApiOptions::new(base_url)` starts with no token (for pre-login calls).
- `with_bot_agent(agent)` overrides the `bot_agent` (sanitized; see
  [Configuration](configuration.md)).
- `with_transport(opts, transport)` injects a custom [`HttpTransport`] for unit
  tests (e.g. an in-memory mock).

The client is `Clone` — the transport is shared behind `Arc`.

## Headers & `base_info`

Every authenticated POST carries:

- `iLink-App-Id: bot` ([`ILINK_APP_ID`])
- `iLink-App-ClientVersion`: `build_client_version(VERSION)` (e.g. `2.4.10` →
  `0x0002040A`)
- `AuthorizationType: ilink_bot_token` ([`AUTHORIZATION_TYPE`])
- `Authorization: Bearer <token>`
- `X-WECHAT-UIN`: base64 of a random `u32`
- `SKRouteTag` (when set)
- A JSON body containing `base_info` (`channel_version` + `bot_agent`)

`base_info()` returns the current `BaseInfo`; `sanitize_bot_agent` is applied to
the agent string.

## Endpoints

| Method | Endpoint | SDK call | Notes |
| --- | --- | --- | --- |
| POST | `ilink/bot/getupdates` | `get_updates(req, timeout_ms, cancel)` | Long poll; returns empty `ret:0` on client timeout/cancel with the same cursor. |
| POST | `ilink/bot/sendmessage` | `send_message(req)` | `is_error()` when `ret != 0`; absent `ret` is OK. |
| POST | `ilink/bot/getuploadurl` | `get_upload_url(req)` | Pre-signs a CDN upload. |
| POST | `ilink/bot/getconfig` | `get_config(ilink_user_id, context_token)` | Returns the typing ticket. |
| POST | `ilink/bot/sendtyping` | `send_typing(req)` | Response body ignored. |
| POST | `ilink/bot/msg/notifystop` | `notify_stop()` | Lifecycle ping. |
| POST | `ilink/bot/msg/notifystart` | `notify_start()` | Lifecycle ping. |
| GET | `ilink/bot/get_qrcode_status?...` | `get_text(endpoint, timeout_ms)` | QR status polling (raw text). |
| POST | `ilink/bot/get_bot_qrcode?...` | `post_text_unauthenticated(endpoint, body, timeout_ms)` | QR fetch, no `Authorization`. |

### Timeouts

Default timeouts (overridable via `WeixinApiOptions::timeout_ms`, which sets a
global default):

| Constant | Value | Used by |
| --- | --- | --- |
| `DEFAULT_LONG_POLL_TIMEOUT_MS` | 35,000 ms | `get_updates` |
| `DEFAULT_API_TIMEOUT_MS` | 15,000 ms | `send_message`, `get_upload_url` |
| `DEFAULT_CONFIG_TIMEOUT_MS` | 10,000 ms | `get_config`, `send_typing`, `notify_*` |

A `get_updates` call that times out (or is cancelled) does **not** error — it
returns an empty `ret: 0` response carrying the same `get_updates_buf`, so the
caller simply retries.

## JSON id handling

The backend sends uint64 message ids (`message_id`, `msg_id`, `svr_id`) as JSON
numbers that can exceed `i64::MAX`. [`parse_api_json`] quotes those fields before
deserializing, so they survive losslessly as `String`. A `serde` value that is
already a string, `null`, or `true`/`false` is left untouched.

## Error classification

Transport failures are mapped to [`ErrorKind`] by
[`Error::from_reqwest_wrap`]. See [Errors & retries](errors.md) for
the full classification table and the retry guidance.

## Testing without the network

`HttpTransport` is a trait, so you can supply a mock that returns
`(StatusCode, HeaderMap, Vec<u8>)`. This is how the integration tests in
`tests/api_integration.rs` assert on headers, request bodies, timeout behavior,
and cancellation without touching the Weixin backend.
