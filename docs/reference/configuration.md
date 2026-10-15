# Configuration

The SDK reads optional configuration from `openclaw.json` (the same file the
TypeScript plugin uses) plus a few environment variables. There is no required
configuration — sensible defaults cover the common case.

## `openclaw.json` layout

The channel section is `channels.openclaw-weixin`. The file is resolved by
[`resolve_config_path`](../../src/storage/config.rs):

1. `$OPENCLAW_CONFIG` (if set and non-empty)
2. `<state_dir>/openclaw.json`

```jsonc
{
  "channels": {
    "openclaw-weixin": {
      "botAgent": "MyBot/1.0.0",
      "routeTag": "r1",
      "replyProgressMessages": false,
      "quoteCache": {
        "enabled": true,
        "retentionDays": 30,
        "maxMessagesPerAccount": 10000,
        "mediaRetentionDays": 7,
        "maxMediaBytesPerAccount": 268435456,
        "maxSingleMediaBytes": 26214400
      },
      "accounts": {
        "deadbeef-im-bot": {
          "baseUrl": "https://ilinkai.weixin.qq.com",
          "cdnBaseUrl": "https://novac2c.cdn.weixin.qq.com/c2c",
          "enabled": true,
          "name": "primary",
          "routeTag": "r2"
        }
      }
    }
  }
}
```

## Channel-level keys

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `botAgent` | string | `OpenClaw` | Observability identifier sent as `base_info.bot_agent`. Not auth. |
| `routeTag` | string \| number | — | Forwarded as the `SKRouteTag` header. |
| `replyProgressMessages` | bool | `true` | Whether structured tool-call progress messages are sent. |
| `quoteCache` | object | see table | Local quote-cache policy (see below). |
| `accounts.<id>` | object | — | Per-account overrides. |

### `botAgent` sanitization

`botAgent` accepts UA-style `Name/Version` tokens with an optional `(comment)`.
The grammar: `name` and `version` are alphanumeric plus `_ . + -` (max 32 chars
each); a comment is printable ASCII minus `(` `)` (max 64 chars). Invalid tokens
are dropped; an empty result falls back to `OpenClaw`. A result longer than 256
bytes (`BOT_AGENT_MAX_LEN`) is truncated by dropping trailing tokens.

```
MyBot/1.0.0
MyBot/1.0.0 (region=cn;env=prod)
MyBot/1.0.0 LangChain/0.3.5
```

## Per-account keys (`accounts.<id>`)

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `baseUrl` | string | `https://ilinkai.weixin.qq.com` | Per-account API base URL. |
| `cdnBaseUrl` | string | `https://novac2c.cdn.weixin.qq.com/c2c` | Per-account CDN base URL. |
| `enabled` | boolean | `true` | Disable an account without deleting it. |
| `name` | string | — | Friendly name (introspection only). |
| `routeTag` | string \| number | — | Per-account `SKRouteTag` override. |

Account-level `baseUrl` / `cdnBaseUrl` / `routeTag` win over channel-level
values, which win over the built-in defaults.

## Quote-cache keys (`quoteCache`)

| Key | Type | Default |
| --- | --- | --- |
| `enabled` | boolean | `true` |
| `retentionDays` | number | 30 |
| `maxMessagesPerAccount` | number | 10000 |
| `mediaRetentionDays` | number | 7 |
| `maxMediaBytesPerAccount` | number | 256 MiB (268435456) |
| `maxSingleMediaBytes` | number | 25 MiB (26214400) |

Invalid values fall back to their defaults. See
[Storage & state](storage.md).

## Runtime overrides

Most of these can also be set in code without touching disk: the
`WeixinClientBuilder` exposes `base_url`, `cdn_base_url`, `route_tag`,
`bot_agent`, and `timeout_ms`; `WeixinApiOptions` exposes `token`, `timeout_ms`,
`route_tag`, and `cdn_base_url` plus `with_*` builders; and
[`resolve_channel_config`] accepts an in-memory [`SdkConfig`] that overrides
everything on disk.

## Environment variables

| Variable | Effect |
| --- | --- |
| `OPENCLAW_STATE_DIR` | Root state directory (highest priority). |
| `CLAWDBOT_STATE_DIR` | Fallback state directory. |
| `OPENCLAW_CONFIG` | Path to `openclaw.json` (overrides `<state_dir>/openclaw.json`). |
| `XDG_STATE_HOME` | Unix: base for `$XDG_STATE_HOME/openclaw` (used when the above are unset). |
| `HOME` / `USERPROFILE` | Base for `~/.openclaw` (last resort). |

## Feature flags

| Feature | Default | What it enables |
| --- | --- | --- |
| `cli` | on | The `weixin` binary (`clap`, `qrcode`, `directories`, `tracing-subscriber`). |

```bash
cargo build --no-default-features        # library only, no CLI
cargo build --features cli               # library + CLI (default)
```
