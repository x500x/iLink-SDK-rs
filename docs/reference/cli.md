# CLI

> 中文版: [中文文档](../zh/reference/cli.md)

The `cli` feature (enabled by default) builds a `weixin` binary that wraps the
whole library, so you can exercise every capability of the SDK end to end —
and script it — before writing code. The CLI reads the same state directory and
`openclaw.json` as the library.

## Build & run

```bash
cargo build --release
cargo run -- --help          # show all commands
cargo run -- login           # QR login, saves credentials
cargo run -- accounts        # list registered accounts
cargo run -- status          # resolved account settings
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "look"
cargo run -- listen          # long-poll, save media, print inbound messages
cargo run -- logout <account-id>
```

> With `--no-default-features`, the binary is not built.

## Global options

All commands accept these flags (most also read the matching environment
variable):

| Flag | Env | Purpose |
| --- | --- | --- |
| `--log-level <lvl>` | `RUST_LOG_LEVEL` | `trace`…`error` (default `info`) |
| `-a, --account <id>` | `WEIXIN_ACCOUNT_ID` | account to operate on |
| `--base-url <url>` | `WEIXIN_BASE_URL` | override the API base URL |
| `--cdn-base-url <url>` | `WEIXIN_CDN_BASE_URL` | override the CDN base URL |
| `--route-tag <tag>` | `WEIXIN_ROUTE_TAG` | forwarded as `SKRouteTag` |
| `--bot-agent <ua>` | `WEIXIN_BOT_AGENT` | observability identifier |
| `--timeout-ms <ms>` | `WEIXIN_TIMEOUT_MS` | per-request timeout |
| `--json` | — | emit machine-readable JSON |

## Commands

| Command | Description |
| --- | --- |
| `login [--timeout <s>] [--show-qr] [--bot-type <t>] [--no-save]` | Run the QR login flow and persist the resulting bot token. |
| `accounts` | List every registered account with its resolved settings. |
| `status` | Print the resolved settings for the active account (base URLs, token presence, session-guard state, paths). |
| `logout <account-id>` | Remove an account and its credentials, cursor, context tokens and quote cache. |
| `listen [--duration <s>] [--poll-timeout <ms>] [--media-dir <dir>] [--quote-cache <bool>] [--echo <bool>]` | Long-poll `getUpdates`, download inbound media, fill the context-token store and quote cache, and print messages. Replies `pong` to `/ping` unless `--echo false`. |
| `send <to> <text> [--context-token <t>] [--run-id <id>]` | Send a text message. |
| `send-file <to> <path> [--caption <c>] [--context-token <t>]` | Upload `path` to the CDN and send it (image/video/file by extension). |
| `send-item <to> <json> [--context-token <t>]` | Send a raw `MessageItem` JSON object. |
| `config [user] [--context-token <t>]` | With a user: call `getConfig` and print the typing ticket. Without: print the local channel config. |
| `typing <user> [on\|off] [--context-token <t>]` | Fetch the typing ticket and show/cancel the typing indicator. |
| `notify <start\|stop>` | Send the `notifystart` / `notifystop` call. |
| `download [--url <u>] [--param <p>] [--aes-key <k>] [-o <path>]` | Fetch (and optionally AES-ECB decrypt) a CDN object. |
| `upload <to> <path> [--media-type image\|video\|file\|voice]` | Upload a local file to the CDN and print the reference/key. |
| `mime <path>` | Print the MIME type and outbound routing for a path. |
| `id [--kind id\|filekey\|temp] [--prefix <p>] [--ext <.e>]` | Generate an id, CDN file key or temp file name. |
| `quote policy` | Print the effective quote-cache policy. |
| `quote find --conversation <c> --message-id <m>` | Look up a cached message. |
| `quote put --conversation <c> --message-id <m> [--body <b>] [--direction inbound\|outbound] [--media-path <p>] [--media-mime <m>]` | Insert or update a cached message. |
| `quote gc` | Run a quote-cache garbage-collection pass. |
| `quote clear` | Delete every cached message/media for the account. |
| `quote media-dir` | Print the account's managed media directory. |
| `context get <user>` | Read a stored reply context token. |
| `context set <user> <token>` | Store a reply context token. |
| `context clear` | Remove every context token for the account. |
| `context restore` | Reload persisted context tokens from disk. |
| `sync show` | Print the persisted `get_updates_buf` cursor. |
| `sync set <buf>` | Overwrite the persisted cursor. |
| `sync clear` | Delete the persisted cursor. |
| `session status` | Print whether the account is paused by the session guard. |
| `session pause` / `session resume` | Pause for one hour / clear the pause. |
| `session assert` | Error when the account is paused. |
| `cdn download-url --param <p>` | Build a CDN download URL. |
| `cdn upload-url --param <p> --filekey <k>` | Build a CDN upload URL. |

### Session guard

The SDK's session guard is in-process. Because every CLI invocation is a new
process, the CLI also **persists** the pause to
`<accounts_dir>/<account-id>.session-pause.json`, so `session pause` survives
across commands: while an account is paused, every account command (`send`,
`send-file`, `listen`, `config`, `typing`, `notify`, `upload`, `status`, …)
refuses to run until `session resume`. `login` is never blocked, so you can
always re-authenticate.

## State & config

The CLI uses the same resolution rules as the library:

- State directory: `OPENCLAW_STATE_DIR` → `CLAWDBOT_STATE_DIR` →
  `$XDG_STATE_HOME/openclaw` → `$HOME/.openclaw`.
- Config: `OPENCLAW_CONFIG` → `<state_dir>/openclaw.json`, section
  `channels.openclaw-weixin`.

So a QR login performed by the CLI is immediately usable by your own Rust code
(and vice versa), and a TypeScript client on the same machine shares the
credentials.

## Tracing

The CLI initializes a `tracing-subscriber` (via the `logging` module) driven by
`RUST_LOG` (e.g. `RUST_LOG=ilink_sdk=debug cargo run -- listen`).
