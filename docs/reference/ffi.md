# C ABI (dll / so / dylib)

Building `ilink-sdk` produces, in addition to the Rust `rlib`:

| Platform | Shared library | Static library | Import library |
| --- | --- | --- | --- |
| Windows | `ilink_sdk.dll` | `ilink_sdk.lib` | `ilink_sdk.dll.lib` |
| Linux | `libilink_sdk.so` | `libilink_sdk.a` | — |
| macOS | `libilink_sdk.dylib` | `libilink_sdk.a` | — |

They live in `target/<profile>/` after `cargo build [--release]`.

```bash
cargo build --release
ls target/release/ilink_sdk.dll        # Windows
ls target/release/libilink_sdk.so      # Linux
```

### Linking

**Shared library (recommended).** Link the import library on Windows
(`ilink_sdk.dll.lib` for MSVC, or the `.dll` directly for MinGW) or the
`.so`/`.dylib` elsewhere, and ship the runtime library next to your program:

```bash
# Linux / macOS
cc -I include app.c -L target/release -lilink_sdk -o app

# Windows (MinGW): link the DLL directly
gcc -I include app.c target/release/ilink_sdk.dll -o app.exe

# Windows (MSVC)
cl /I include app.c target/release/ilink_sdk.dll.lib
```

No macros need to be defined for the shared library.

**Static library.** Define `ILINK_SDK_STATIC` before including the header,
otherwise the declarations are marked `dllimport` and the linker looks for
`__imp_ilink_call` instead of `ilink_call`:

```bash
cc -DILINK_SDK_STATIC -I include app.c target/release/libilink_sdk.a -o app
```

On Windows the static library is produced by the MSVC toolchain and can only
be linked by MSVC (it references `__chkstk` and the MSVC C++ type-info
runtime). MinGW/gcc and Go's cgo on Windows must use the shared library.

**Windows runtime dependency.** The Windows binaries are built with
`+crt-static` (see [`.cargo/config.toml`](../../.cargo/config.toml)), so they
statically link the MSVC C runtime and do **not** require the Visual C++
Redistributable to be installed. If you build with a custom toolchain that
does not pick up that config, add
`RUSTFLAGS="-C target-feature=+crt-static"` to get the same result.

CI builds these on Linux, macOS and Windows and attaches them to each GitHub
release (see [`.github/workflows/release.yml`](../../.github/workflows/release.yml)).
On macOS the dylib's install name is rewritten to
`@rpath/libilink_sdk.dylib`, so it is relocatable and can be embedded in an
app bundle; if you build locally and plan to ship the dylib, do the same:

```bash
install_name_tool -id @rpath/libilink_sdk.dylib target/release/libilink_sdk.dylib
```

The public header is [`include/ilink_sdk.h`](../../include/ilink_sdk.h).

## One entry point

Rather than exporting one C function per SDK operation (the Rust API is async,
generic and borrow-heavy), the whole SDK is reachable through a single
JSON-in / JSON-out call:

```c
char *ilink_call(const char *op, const char *params_json);
char *ilink_version(void);
void  ilink_string_free(char *ptr);
```

Every result is a JSON object:

```json
{ "ok": true,  "result": { /* operation specific */ } }
{ "ok": false, "error": "message", "kind": "dns" }
```

`kind` is the [`ErrorKind`](errors.md) identifier (`dns`, `tcp`, `tls`,
`timeout`, `cancelled`, `unknown`). Returned strings are heap-allocated and
must be released with `ilink_string_free`.

## Example

See [`examples/c/ilink_demo.c`](../../examples/c/ilink_demo.c).

```c
#include "ilink_sdk.h"

char *out = ilink_call("mime.info", "{\"path\":\"photo.png\"}");
if (out) {
    puts(out);              /* {"ok":true,"result":{...}} */
    ilink_string_free(out);
}
```

## Operations

| Area | `op` | Parameters | Result |
| --- | --- | --- | --- |
| Meta | `version` | — | version string |
| Meta | `paths` | — | state/config paths |
| Accounts | `accounts.list` | — | array of resolved accounts |
| Accounts | `account.resolve` | `account_id` | resolved account |
| Accounts | `account.save` | `account_id`, `token?`, `base_url?`, `user_id?` | resolved account |
| Accounts | `account.clear` | `account_id` | `{account_id}` |
| Login | `login.start` | `base_url?`, `bot_type?`, `session_key?` | `{qrcode, qrcode_url, session_key}` |
| Login | `login.poll` | `session_key` | `{kind, ...}` |
| Login | `login.submit_verify_code` | `session_key`, `code` | `{submitted}` |
| Login | `login.refresh` | `session_key` | `{qrcode, qrcode_url, session_key}` |
| Login | `login.wait` | `session_key`, `timeout_secs?`, `verify_code?` | wait result |
| Login | `login.complete` | `session_key`, `timeout_secs?`, `verify_code?` | wait result + saved `account_id` |
| Send | `send.text` | `to`, `text`, `context_token?`, `run_id?` | `{message_id, server_message_id}` |
| Send | `send.file` | `to`, `path`, `caption?`, `context_token?` | `{message_id, server_message_id}` |
| Send | `send.item` | `to`, `item`, `context_token?` | `{message_id, server_message_id}` |
| Send | `send.bytes` | `to`, `file_name`, `data_base64`, `caption?` | `{message_id, server_message_id}` |
| API | `get_updates` | `get_updates_buf?`, `timeout_ms?` | `GetUpdatesResp` JSON |
| API | `get_config` | `user_id`, `context_token?` | `GetConfigResp` JSON |
| API | `send_typing` | `user_id`, `typing_ticket`, `status?` | `{sent}` |
| API | `notify` | `action` (`start`\|`stop`) | `{notified}` |
| API | `context.to_message` | `message`, `account_id?` | `MsgContext` JSON |
| API | `message.body` | `items` | `{body}` |
| Media | `media.fetch` | `url`, `dest_path?` | `{bytes, path?}` or `{bytes, data_base64}` |
| Media | `media.download` | `media`, `aes_key?`, `cdn_base_url?`, `dest_path?` | `{bytes, path?}` |
| Media | `media.download_item` | `item`, `cdn_base_url?`, `dest_dir?` | `InboundMedia` JSON |
| Media | `media.upload` | `to`, `path`, `media_type?` (`image`\|`video`\|`file`\|`voice`), `cdn_base_url?` | `UploadedFile` JSON |
| Quote | `quote.policy` | — | effective policy |
| Quote | `quote.find` | `account_id`, `conversation_id`, `message_id` | record or `null` |
| Quote | `quote.put` | `account_id`, `conversation_id`, `message_id`, `body?`, `direction?`, `source_media_path?`, `media_mime?`, `media_name?`, `created_at?` | `{stored}` |
| Quote | `quote.gc` | — | `{gc}` |
| Quote | `quote.delete_account` | `account_id` | `{deleted}` |
| Quote | `quote.media_dir` | `account_id` | `{path}` |
| Context | `context.get` | `account_id`, `user_id` | `{token}` or `null` |
| Context | `context.set` | `account_id`, `user_id`, `token` | `{stored}` |
| Context | `context.clear` | `account_id` | `{cleared}` |
| Context | `context.restore` | `account_id` | `{restored}` |
| Sync | `sync.load` | `account_id` | `{get_updates_buf}` |
| Sync | `sync.save` | `account_id`, `buf` | `{saved}` |
| Sync | `sync.clear` | `account_id` | `{cleared}` |
| Session | `session.status` | `account_id` | `{paused, remaining_ms}` |
| Session | `session.pause` / `session.resume` | `account_id` | `{paused}` / `{resumed}` |
| Session | `session.assert` | `account_id` | `{active}` or error |
| Helpers | `mime.info` | `path` | `{mime, kind, extension, ...}` |
| Helpers | `mime.extension` | `mime` | `{extension}` |
| Helpers | `config.load` | — | `{config, quote_cache_policy}` |
| Helpers | `util.generate_id` | `prefix?` | `{id}` |
| Helpers | `util.file_key` | — | `{file_key}` |
| Helpers | `util.temp_name` | `prefix?`, `ext?` | `{name}` |
| Helpers | `cdn.download_url` | `encrypted_query_param`, `cdn_base_url?` | `{url}` |
| Helpers | `cdn.upload_url` | `upload_param`, `filekey`, `cdn_base_url?` | `{url}` |
| Crypto | `crypto.encrypt` | `key_hex`, `data_base64` | `{data_base64, padded_size}` |
| Crypto | `crypto.decrypt` | `key_hex`, `data_base64` | `{data_base64}` |

Every operation that talks to an account accepts the optional routing
overrides `account_id`, `base_url`, `cdn_base_url`, `route_tag`, `bot_agent`
and `timeout_ms`, exactly like the CLI's global flags.

## Threading

The library owns a lazily created Tokio runtime and can be called from any
thread that is **not** already running a Tokio runtime. Calls block until the
operation completes.

## Panics

`ilink_call` catches Rust panics and returns
`{"ok":false,"error":"panic inside ilink_call","kind":"panic"}` instead of
unwinding across the FFI boundary.
