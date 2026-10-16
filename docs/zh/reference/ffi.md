# C ABI（dll / so / dylib）

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

## 单一入口

Rust API 是异步、泛型且大量借用参数的，因此不为每个操作导出一个 C 函数，而是用一个
JSON 进 / JSON 出的调用暴露整个 SDK：

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

见 [`examples/c/ilink_demo.c`](../../../examples/c/ilink_demo.c)。

```c
#include "ilink_sdk.h"

char *out = ilink_call("mime.info", "{\"path\":\"photo.png\"}");
if (out) {
    puts(out);              /* {"ok":true,"result":{...}} */
    ilink_string_free(out);
}
```

## 操作列表

| 领域 | `op` | 参数 | 结果 |
| --- | --- | --- | --- |
| 元信息 | `version` | — | 版本字符串 |
| 元信息 | `paths` | — | 状态/配置路径 |
| 账户 | `accounts.list` | — | 已解析账户数组 |
| 账户 | `account.resolve` | `account_id` | 已解析账户 |
| 账户 | `account.save` | `account_id`, `token?`, `base_url?`, `user_id?` | 已解析账户 |
| 账户 | `account.clear` | `account_id` | `{account_id}` |
| 登录 | `login.start` | `base_url?`, `bot_type?`, `session_key?` | `{qrcode, qrcode_url, session_key}` |
| 登录 | `login.poll` | `session_key` | `{kind, ...}` |
| 登录 | `login.submit_verify_code` | `session_key`, `code` | `{submitted}` |
| 登录 | `login.refresh` | `session_key` | `{qrcode, qrcode_url, session_key}` |
| 登录 | `login.wait` | `session_key`, `timeout_secs?`, `verify_code?` | 等待结果 |
| 登录 | `login.complete` | `session_key`, `timeout_secs?`, `verify_code?` | 等待结果 + 已保存的 `account_id` |
| 发送 | `send.text` | `to`, `text`, `context_token?`, `run_id?` | `{message_id, server_message_id}` |
| 发送 | `send.file` | `to`, `path`, `caption?`, `context_token?` | `{message_id, server_message_id}` |
| 发送 | `send.item` | `to`, `item`, `context_token?` | `{message_id, server_message_id}` |
| 发送 | `send.bytes` | `to`, `file_name`, `data_base64`, `caption?` | `{message_id, server_message_id}` |
| API | `get_updates` | `get_updates_buf?`, `timeout_ms?` | `GetUpdatesResp` JSON |
| API | `get_config` | `user_id`, `context_token?` | `GetConfigResp` JSON |
| API | `send_typing` | `user_id`, `typing_ticket`, `status?` | `{sent}` |
| API | `notify` | `action`（`start`\|`stop`） | `{notified}` |
| API | `context.to_message` | `message`, `account_id?` | `MsgContext` JSON |
| API | `message.body` | `items` | `{body}` |
| 媒体 | `media.fetch` | `url`, `dest_path?` | `{bytes, path?}` 或 `{bytes, data_base64}` |
| 媒体 | `media.download` | `media`, `aes_key?`, `cdn_base_url?`, `dest_path?` | `{bytes, path?}` |
| 媒体 | `media.download_item` | `item`, `cdn_base_url?`, `dest_dir?` | `InboundMedia` JSON |
| 媒体 | `media.upload` | `to`, `path`, `media_type?`（`image`\|`video`\|`file`\|`voice`）, `cdn_base_url?` | `UploadedFile` JSON |
| 引用 | `quote.policy` | — | 生效策略 |
| 引用 | `quote.find` | `account_id`, `conversation_id`, `message_id` | 记录或 `null` |
| 引用 | `quote.put` | `account_id`, `conversation_id`, `message_id`, `body?`, `direction?`, `source_media_path?`, `media_mime?`, `media_name?`, `created_at?` | `{stored}` |
| 引用 | `quote.gc` | — | `{gc}` |
| 引用 | `quote.delete_account` | `account_id` | `{deleted}` |
| 引用 | `quote.media_dir` | `account_id` | `{path}` |
| 上下文 | `context.get` | `account_id`, `user_id` | `{token}` 或 `null` |
| 上下文 | `context.set` | `account_id`, `user_id`, `token` | `{stored}` |
| 上下文 | `context.clear` | `account_id` | `{cleared}` |
| 上下文 | `context.restore` | `account_id` | `{restored}` |
| 游标 | `sync.load` | `account_id` | `{get_updates_buf}` |
| 游标 | `sync.save` | `account_id`, `buf` | `{saved}` |
| 游标 | `sync.clear` | `account_id` | `{cleared}` |
| 会话 | `session.status` | `account_id` | `{paused, remaining_ms}` |
| 会话 | `session.pause` / `session.resume` | `account_id` | `{paused}` / `{resumed}` |
| 会话 | `session.assert` | `account_id` | `{active}` 或错误 |
| 辅助 | `mime.info` | `path` | `{mime, kind, extension, ...}` |
| 辅助 | `mime.extension` | `mime` | `{extension}` |
| 辅助 | `config.load` | — | `{config, quote_cache_policy}` |
| 辅助 | `util.generate_id` | `prefix?` | `{id}` |
| 辅助 | `util.file_key` | — | `{file_key}` |
| 辅助 | `util.temp_name` | `prefix?`, `ext?` | `{name}` |
| 辅助 | `cdn.download_url` | `encrypted_query_param`, `cdn_base_url?` | `{url}` |
| 辅助 | `cdn.upload_url` | `upload_param`, `filekey`, `cdn_base_url?` | `{url}` |
| 加密 | `crypto.encrypt` | `key_hex`, `data_base64` | `{data_base64, padded_size}` |
| 加密 | `crypto.decrypt` | `key_hex`, `data_base64` | `{data_base64}` |

所有需要访问账户的操作都接受可选的路由覆盖参数 `account_id`、`base_url`、
`cdn_base_url`、`route_tag`、`bot_agent` 与 `timeout_ms`，与 CLI 的全局选项一致。

## 线程

库内部持有懒创建的 Tokio 运行时，可从任意**未运行 Tokio 运行时**的线程调用。调用会
阻塞至操作完成。运行时使用 2~4 个 worker 线程（无论核数多少都封顶为 4），以控制嵌入
场景下的线程数和常驻内存。

## panic

`ilink_call` 会捕获 Rust panic，返回
`{"ok":false,"error":"panic inside ilink_call","kind":"panic"}`，而不会让 panic 跨
FFI 边界展开。
