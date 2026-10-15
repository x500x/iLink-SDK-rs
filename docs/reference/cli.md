# CLI

The `cli` feature (enabled by default) builds a small `weixin` binary that wraps
the library, so you can exercise the protocol end to end before writing code.
The CLI reads the same state directory and `openclaw.json` as the library.

## Build & run

```bash
cargo run -- --help          # show all commands
cargo run -- login           # QR login, saves credentials
cargo run -- accounts        # list registered accounts
cargo run -- status          # resolved account settings
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "look"
cargo run -- listen          # long-poll and print inbound messages
cargo run -- logout <account-id>
```

> With `--no-default-features`, the binary is not built.

## Commands

| Command | Description |
| --- | --- |
| `login` | Run the QR login flow and persist the resulting bot token. Renders the QR code for scanning. |
| `accounts` | List all registered account ids (from `accounts.json`). |
| `status` | Print the resolved settings for the active account (base URL, CDN URL, enabled, route tag, config source). |
| `send <to> <text> [--context-token <t>]` | Send a text message to `to`. |
| `send-file <to> <path> [--caption <c>]` | Upload `path` to the CDN and send it (image/video/file by extension). |
| `listen` | Long-poll `getUpdates` and print inbound messages as they arrive. |
| `logout <account-id>` | Remove an account and its files from the state directory. |

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
