# C ABI（dll / so / dylib）

> English: [English](../../reference/ffi.md)

> Go、Python、C++、Java、Kotlin 的绑定都封装同一套 ABI，见 [多语言绑定](bindings.md)。

构建 `ilink-sdk` 时，除了 Rust `rlib` 之外还会产出：

| 平台 | 共享库 | 静态库 | 导入库 |
| --- | --- | --- | --- |
| Windows | `ilink_sdk.dll` | `ilink_sdk.lib` | `ilink_sdk.dll.lib` |
| Linux | `libilink_sdk.so` | `libilink_sdk.a` | — |
| macOS | `libilink_sdk.dylib` | `libilink_sdk.a` | — |

`cargo build [--release]` 之后它们位于 `target/<profile>/`。

```bash
cargo build --release
ls target/release/ilink_sdk.dll        # Windows
ls target/release/libilink_sdk.so      # Linux
```

### 链接方式

**共享库（推荐）。** Windows 上链接导入库（MSVC 用 `ilink_sdk.dll.lib`，MinGW 可直接
链接 `.dll`），其他平台链接 `.so`/`.dylib`，并把运行库放到程序旁边：

```bash
# Linux / macOS
cc -I include app.c -L target/release -lilink_sdk -o app

# Windows（MinGW）：直接链接 DLL
gcc -I include app.c target/release/ilink_sdk.dll -o app.exe

# Windows（MSVC）
cl /I include app.c target/release/ilink_sdk.dll.lib
```

链接共享库不需要定义任何宏。

**静态库。** 必须在包含头文件前定义 `ILINK_SDK_STATIC`，否则声明会被标记为
`dllimport`，链接器会去找 `__imp_ilink_call` 而不是 `ilink_call`：

```bash
cc -DILINK_SDK_STATIC -I include app.c target/release/libilink_sdk.a -o app
```

Windows 上的静态库由 MSVC 工具链产出，只能由 MSVC 链接（会引用 `__chkstk` 以及
MSVC 的 C++ type-info 运行时）。MinGW/gcc 和 Windows 上的 Go cgo 请改用共享库。

**Windows 运行库依赖。** Windows 产物使用 `+crt-static` 构建（见
[`.cargo/config.toml`](../../../.cargo/config.toml)），静态链接 MSVC C 运行库，**不需要**
安装 Visual C++ Redistributable。如果使用未读取该配置的自定义工具链，加上
`RUSTFLAGS="-C target-feature=+crt-static"` 可获得同样效果。

公开头文件为 [`include/ilink_sdk.h`](../../../include/ilink_sdk.h)。

## 两种调用方式

### 类型化函数（推荐）

每个常用操作都有一个 C 函数，结果用 C 结构体承载，无需拼接或解析 JSON：

```c
#include "ilink_sdk.h"

/* 路由默认值 —— 相当于其它绑定里的 Client。 */
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

约定：

- 所有函数都返回 `IlinkStatus`（成功为 `ILINK_OK`）；
- 结果通过出参返回，用配对的 `ilink_*_free` 释放，由库负责释放自己分配的内存；这些
  free 函数对 NULL 安全，且会重置结构体，因此重复释放也安全；
- 可选字符串字段在缺失时为 `NULL`；
- `config` 携带路由默认值（`account_id`、`base_url`、`cdn_base_url`、`route_tag`、
  `bot_agent`、`timeout_ms`）；传 `NULL` 即用内置默认值；
- 失败时用 `ilink_last_error()` 取当前线程的错误消息，
  `ilink_status_message(status)` 取状态码描述。

状态码：`ILINK_OK`、`ILINK_ERR_INVALID_ARG`、`ILINK_ERR_NOT_FOUND`、
`ILINK_ERR_DNS`、`ILINK_ERR_TCP`、`ILINK_ERR_TLS`、`ILINK_ERR_TIMEOUT`、
`ILINK_ERR_CANCELLED`、`ILINK_ERR_UNKNOWN`、`ILINK_ERR_PANIC`。

### JSON 逃生通道

没有类型化封装的操作（二维码登录、引用缓存、`config.load`、`send.item`、
`media.download_item` 等）仍可通过这一个 JSON 进 / JSON 出的调用触达：

```c
char *ilink_call(const char *op, const char *params_json);
char *ilink_version(void);
void  ilink_string_free(char *ptr);
```

返回值始终是一个 JSON 对象：

```json
{ "ok": true,  "result": { /* 各操作不同 */ } }
{ "ok": false, "error": "message", "kind": "dns" }
```

`kind` 是 [`ErrorKind`](errors.md) 的标识（`dns`、`tcp`、`tls`、`timeout`、
`cancelled`、`unknown`）。返回的字符串由堆分配，必须用 `ilink_string_free` 释放。

## 示例

见 [`examples/c/ilink_demo.c`](../../../examples/c/ilink_demo.c)，其中使用了类型化
API，并在末尾演示逃生通道。

## 操作列表

`op` 列是 `ilink_call` 使用的名称；**类型化函数**列是对应的 C 函数（空白表示只能通过
`ilink_call` 触达）。

| 领域 | `op` | 类型化函数 | 参数 | 结果 |
| --- | --- | --- | --- | --- |
| 元信息 | `version` | `ilink_version_string` | — | 版本字符串 |
| 元信息 | `paths` | `ilink_paths` | — | 状态/配置路径 |
| 账户 | `accounts.list` | `ilink_accounts_list` | — | 已解析账户数组 |
| 账户 | `account.resolve` | `ilink_account_resolve` | `account_id` | 已解析账户 |
| 账户 | `account.save` | `ilink_account_save` | `account_id`, `token?`, `base_url?`, `user_id?` | 已解析账户 |
| 账户 | `account.clear` | `ilink_account_clear` | `account_id` | `{account_id}` |
| 登录 | `login.start` | | `base_url?`, `bot_type?`, `session_key?` | `{qrcode, qrcode_url, session_key}` |
| 登录 | `login.poll` | | `session_key` | `{kind, ...}` |
| 登录 | `login.submit_verify_code` | | `session_key`, `code` | `{submitted}` |
| 登录 | `login.refresh` | | `session_key` | `{qrcode, qrcode_url, session_key}` |
| 登录 | `login.wait` | | `session_key`, `timeout_secs?`, `verify_code?` | 等待结果 |
| 登录 | `login.complete` | | `session_key`, `timeout_secs?`, `verify_code?` | 等待结果 + 已保存的 `account_id` |
| 发送 | `send.text` | `ilink_send_text` | `to`, `text`, `context_token?`, `run_id?` | `{message_id, server_message_id}` |
| 发送 | `send.file` | `ilink_send_file` | `to`, `path`, `caption?`, `context_token?` | `{message_id, server_message_id}` |
| 发送 | `send.item` | | `to`, `item`, `context_token?` | `{message_id, server_message_id}` |
| 发送 | `send.bytes` | `ilink_send_bytes` | `to`, `file_name`, `data_base64`, `caption?` | `{message_id, server_message_id}` |
| API | `get_updates` | `ilink_get_updates` | `get_updates_buf?`, `timeout_ms?` | `GetUpdatesResp` |
| API | `get_config` | `ilink_get_config` | `user_id`, `context_token?` | `GetConfigResp` |
| API | `send_typing` | `ilink_send_typing` | `user_id`, `typing_ticket`, `status?` | `{sent}` |
| API | `notify` | `ilink_notify` | `action`（`start`\|`stop`） | `{notified}` |
| API | `context.to_message` | | `message`, `account_id?` | `MsgContext` |
| API | `message.body` | `ilink_message_body` | `items` | `{body}` |
| 媒体 | `media.fetch` | `ilink_fetch` | `url`, `dest_path?` | `{bytes, path?}` 或 `{bytes, data_base64}` |
| 媒体 | `media.download` | `ilink_download` | `media`, `aes_key?`, `cdn_base_url?`, `dest_path?` | `{bytes, path?}` |
| 媒体 | `media.download_item` | | `item`, `cdn_base_url?`, `dest_dir?` | `InboundMedia` |
| 媒体 | `media.upload` | `ilink_upload` | `to`, `path`, `media_type?`, `cdn_base_url?` | `UploadedFile` |
| 引用 | `quote.policy` | | — | 生效策略 |
| 引用 | `quote.find` | | `account_id`, `conversation_id`, `message_id` | 记录或 `null` |
| 引用 | `quote.put` | | `account_id`, `conversation_id`, `message_id`, `body?`, ... | `{stored}` |
| 引用 | `quote.gc` | | — | `{gc}` |
| 引用 | `quote.delete_account` | | `account_id` | `{deleted}` |
| 引用 | `quote.media_dir` | | `account_id` | `{path}` |
| 上下文 | `context.get` | `ilink_context_get` | `account_id`, `user_id` | `{token}` 或 `null` |
| 上下文 | `context.set` | `ilink_context_set` | `account_id`, `user_id`, `token` | `{stored}` |
| 上下文 | `context.clear` | `ilink_context_clear` | `account_id` | `{cleared}` |
| 上下文 | `context.restore` | `ilink_context_restore` | `account_id` | `{restored}` |
| 游标 | `sync.load` | `ilink_sync_load` | `account_id` | `{get_updates_buf}` |
| 游标 | `sync.save` | `ilink_sync_save` | `account_id`, `buf` | `{saved}` |
| 游标 | `sync.clear` | `ilink_sync_clear` | `account_id` | `{cleared}` |
| 会话 | `session.status` | `ilink_session_status` | `account_id` | `{paused, remaining_ms}` |
| 会话 | `session.pause` / `session.resume` | `ilink_session_pause` / `ilink_session_resume` | `account_id` | `{paused}` / `{resumed}` |
| 会话 | `session.assert` | `ilink_session_assert` | `account_id` | `{active}` 或错误 |
| 辅助 | `mime.info` | `ilink_mime_info` | `path` | `{mime, kind, extension, ...}` |
| 辅助 | `mime.extension` | `ilink_mime_extension` | `mime` | `{extension}` |
| 辅助 | `config.load` | | — | `{config, quote_cache_policy}` |
| 辅助 | `util.generate_id` | `ilink_generate_id` | `prefix?` | `{id}` |
| 辅助 | `util.file_key` | `ilink_file_key` | — | `{file_key}` |
| 辅助 | `util.temp_name` | `ilink_temp_name` | `prefix?`, `ext?` | `{name}` |
| 辅助 | `cdn.download_url` | `ilink_cdn_download_url` | `encrypted_query_param`, `cdn_base_url?` | `{url}` |
| 辅助 | `cdn.upload_url` | `ilink_cdn_upload_url` | `upload_param`, `filekey`, `cdn_base_url?` | `{url}` |
| 加密 | `crypto.encrypt` | `ilink_crypto_encrypt` | `key_hex`, `data_base64` | `{data_base64, padded_size}` |
| 加密 | `crypto.decrypt` | `ilink_crypto_decrypt` | `key_hex`, `data_base64` | `{data_base64}` |

使用类型化函数时，路由覆盖参数是 `IlinkConfig` 的字段，而不是 `params` 中的键。
`ilink_call` 仍接受 `account_id`、`base_url`、`cdn_base_url`、`route_tag`、
`bot_agent` 与 `timeout_ms` 作为参数对象中的键，与 CLI 的全局选项一致。

## 线程

库内部持有懒创建的 Tokio 运行时，可从任意**未运行 Tokio 运行时**的线程调用。调用会
阻塞至操作完成。运行时使用 2~4 个 worker 线程（无论核数多少都封顶为 4），以控制嵌入
场景下的线程数和常驻内存。

## panic

`ilink_call` 会捕获 Rust panic，返回
`{"ok":false,"error":"panic inside ilink_call","kind":"panic"}`，而不会让 panic 跨
FFI 边界展开。
