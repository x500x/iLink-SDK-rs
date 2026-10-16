# C ABI (dll / so / dylib)

> 中文版: [中文文档](../zh/reference/ffi.md)

> Language bindings for Go, Python, C++, Java and Kotlin wrap this same ABI; see
> [Language bindings](bindings.md).

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

## Two ways in

### Typed functions (recommended)

Every common operation has a C function, with C structs for the results. No
JSON to build or parse:

```c
#include "ilink_sdk.h"

/* Routing defaults — the C counterpart of the other bindings' Client. */
IlinkConfig config;
memset(&config, 0, sizeof config);
config.account_id = "cfe9a2017e77-im-bot";

IlinkMimeInfo info;
if (ilink_mime_info("photo.png", &info) == ILINK_OK) {
    printf("%s %s\n", info.mime, info.extension);
    ilink_mime_info_free(&info);
}

IlinkSendResult sent;
if (ilink_send_text(&config, "u@im.wechat", "hi", NULL, NULL, &sent) == ILINK_OK) {
    printf("sent %s\n", sent.message_id);
    ilink_send_result_free(&sent);
}
```

Conventions:

- every function returns an `IlinkStatus` (`ILINK_OK` on success);
- results are written through an out-pointer and released with the matching
  `ilink_*_free` function, so the library frees its own memory; those are
  NULL-safe and reset the struct, so double-free is safe;
- optional string fields are `NULL` when absent;
- `config` carries the routing defaults (`account_id`, `base_url`,
  `cdn_base_url`, `route_tag`, `bot_agent`, `timeout_ms`); pass `NULL` for the
  built-in defaults;
- on failure, `ilink_last_error()` returns the message for the calling thread
  and `ilink_status_message(status)` describes the code.

Status codes: `ILINK_OK`, `ILINK_ERR_INVALID_ARG`, `ILINK_ERR_NOT_FOUND`,
`ILINK_ERR_DNS`, `ILINK_ERR_TCP`, `ILINK_ERR_TLS`, `ILINK_ERR_TIMEOUT`,
`ILINK_ERR_CANCELLED`, `ILINK_ERR_UNKNOWN`, `ILINK_ERR_PANIC`.

### The JSON escape hatch

Operations without a typed wrapper (QR login, quote cache, `config.load`,
`send.item`, `media.download_item`, ...) are still reachable through one
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

See [`examples/c/ilink_demo.c`](../../examples/c/ilink_demo.c) for a runnable
program that uses the typed API and finishes with the escape hatch.

## Operations

The `op` column is the name for `ilink_call`; the **Typed function** column
names the C function that wraps it (empty when only `ilink_call` reaches it).

| Area | `op` | Typed function | Parameters | Result |
| --- | --- | --- | --- | --- |
| Meta | `version` | `ilink_version_string` | — | version string |
| Meta | `paths` | `ilink_paths` | — | state/config paths |
| Accounts | `accounts.list` | `ilink_accounts_list` | — | array of resolved accounts |
| Accounts | `account.resolve` | `ilink_account_resolve` | `account_id` | resolved account |
| Accounts | `account.save` | `ilink_account_save` | `account_id`, `token?`, `base_url?`, `user_id?` | resolved account |
| Accounts | `account.clear` | `ilink_account_clear` | `account_id` | `{account_id}` |
| Login | `login.start` | | `base_url?`, `bot_type?`, `session_key?` | `{qrcode, qrcode_url, session_key}` |
| Login | `login.poll` | | `session_key` | `{kind, ...}` |
| Login | `login.submit_verify_code` | | `session_key`, `code` | `{submitted}` |
| Login | `login.refresh` | | `session_key` | `{qrcode, qrcode_url, session_key}` |
| Login | `login.wait` | | `session_key`, `timeout_secs?`, `verify_code?` | wait result |
| Login | `login.complete` | | `session_key`, `timeout_secs?`, `verify_code?` | wait result + saved `account_id` |
| Send | `send.text` | `ilink_send_text` | `to`, `text`, `context_token?`, `run_id?` | `{message_id, server_message_id}` |
| Send | `send.file` | `ilink_send_file` | `to`, `path`, `caption?`, `context_token?` | `{message_id, server_message_id}` |
| Send | `send.item` | | `to`, `item`, `context_token?` | `{message_id, server_message_id}` |
| Send | `send.bytes` | `ilink_send_bytes` | `to`, `file_name`, `data_base64`, `caption?` | `{message_id, server_message_id}` |
| API | `get_updates` | `ilink_get_updates` | `get_updates_buf?`, `timeout_ms?` | `GetUpdatesResp` |
| API | `get_config` | `ilink_get_config` | `user_id`, `context_token?` | `GetConfigResp` |
| API | `send_typing` | `ilink_send_typing` | `user_id`, `typing_ticket`, `status?` | `{sent}` |
| API | `notify` | `ilink_notify` | `action` (`start`\|`stop`) | `{notified}` |
| API | `context.to_message` | | `message`, `account_id?` | `MsgContext` |
| API | `message.body` | `ilink_message_body` | `items` | `{body}` |
| Media | `media.fetch` | `ilink_fetch` | `url`, `dest_path?` | `{bytes, path?}` or `{bytes, data_base64}` |
| Media | `media.download` | `ilink_download` | `media`, `aes_key?`, `cdn_base_url?`, `dest_path?` | `{bytes, path?}` |
| Media | `media.download_item` | | `item`, `cdn_base_url?`, `dest_dir?` | `InboundMedia` |
| Media | `media.upload` | `ilink_upload` | `to`, `path`, `media_type?`, `cdn_base_url?` | `UploadedFile` |
| Quote | `quote.policy` | | — | effective policy |
| Quote | `quote.find` | | `account_id`, `conversation_id`, `message_id` | record or `null` |
| Quote | `quote.put` | | `account_id`, `conversation_id`, `message_id`, `body?`, ... | `{stored}` |
| Quote | `quote.gc` | | — | `{gc}` |
| Quote | `quote.delete_account` | | `account_id` | `{deleted}` |
| Quote | `quote.media_dir` | | `account_id` | `{path}` |
| Context | `context.get` | `ilink_context_get` | `account_id`, `user_id` | `{token}` or `null` |
| Context | `context.set` | `ilink_context_set` | `account_id`, `user_id`, `token` | `{stored}` |
| Context | `context.clear` | `ilink_context_clear` | `account_id` | `{cleared}` |
| Context | `context.restore` | `ilink_context_restore` | `account_id` | `{restored}` |
| Sync | `sync.load` | `ilink_sync_load` | `account_id` | `{get_updates_buf}` |
| Sync | `sync.save` | `ilink_sync_save` | `account_id`, `buf` | `{saved}` |
| Sync | `sync.clear` | `ilink_sync_clear` | `account_id` | `{cleared}` |
| Session | `session.status` | `ilink_session_status` | `account_id` | `{paused, remaining_ms}` |
| Session | `session.pause` / `session.resume` | `ilink_session_pause` / `ilink_session_resume` | `account_id` | `{paused}` / `{resumed}` |
| Session | `session.assert` | `ilink_session_assert` | `account_id` | `{active}` or error |
| Helpers | `mime.info` | `ilink_mime_info` | `path` | `{mime, kind, extension, ...}` |
| Helpers | `mime.extension` | `ilink_mime_extension` | `mime` | `{extension}` |
| Helpers | `config.load` | | — | `{config, quote_cache_policy}` |
| Helpers | `util.generate_id` | `ilink_generate_id` | `prefix?` | `{id}` |
| Helpers | `util.file_key` | `ilink_file_key` | — | `{file_key}` |
| Helpers | `util.temp_name` | `ilink_temp_name` | `prefix?`, `ext?` | `{name}` |
| Helpers | `cdn.download_url` | `ilink_cdn_download_url` | `encrypted_query_param`, `cdn_base_url?` | `{url}` |
| Helpers | `cdn.upload_url` | `ilink_cdn_upload_url` | `upload_param`, `filekey`, `cdn_base_url?` | `{url}` |
| Crypto | `crypto.encrypt` | `ilink_crypto_encrypt` | `key_hex`, `data_base64` | `{data_base64, padded_size}` |
| Crypto | `crypto.decrypt` | `ilink_crypto_decrypt` | `key_hex`, `data_base64` | `{data_base64}` |

For the typed functions the routing overrides are fields of `IlinkConfig`
rather than `params` entries. `ilink_call` still accepts them as `account_id`,
`base_url`, `cdn_base_url`, `route_tag`, `bot_agent` and `timeout_ms` in the
parameter object.

## Threading

The library owns a lazily created Tokio runtime and can be called from any
thread that is **not** already running a Tokio runtime. Calls block until the
operation completes. The runtime uses between two and four worker threads
(capped at four regardless of core count) to keep the thread count and
resident memory modest for embedded callers.

## Panics

`ilink_call` catches Rust panics and returns
`{"ok":false,"error":"panic inside ilink_call","kind":"panic"}` instead of
unwinding across the FFI boundary.
