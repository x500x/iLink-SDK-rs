# 多语言绑定

本 crate 是一个 Rust 库，但其功能同样可以从 C、C++、Go、Python、Java 和 Kotlin
调用。它们都走同一个 JSON 进 / JSON 出的 C ABI（见 [C ABI（dll / so / dylib）](ffi.md)），
因此**没有按语言重复实现协议**：每个绑定只负责加载共享库、调用 `ilink_call`、
拆开 `{"ok":true,"result":...}` 信封，并暴露符合该语言习惯的**类型化**辅助方法。
结果会被解码成宿主语言自然的值类型（Python 用 dataclass，Go/C++ 用 struct，
Java/Kotlin 用类），因此常用流程无需手动解析结果 JSON。

| 语言 | 目录 | 运行时依赖 | 状态 |
| --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../../../include/ilink_sdk.h) + [`examples/c`](../../../examples/c) | — | 参考实现 |
| C++ | [`bindings/cpp`](../../../bindings/cpp) | —（header-only） | 已实测 |
| Rust | crate 本体 | — | 原生 |
| Python | [`bindings/python`](../../../bindings/python) | 无（`ctypes`） | 已实测 |
| Go | [`bindings/go`](../../../bindings/go) | cgo | 已实测 |
| Java | [`bindings/java`](../../../bindings/java) | [JNA](https://github.com/java-native-access/jna) | 已实测 |
| Kotlin | [`bindings/kotlin`](../../../bindings/kotlin) | JNA | 已实测 |

## 先构建共享库

```bash
cargo build --release
# Windows -> target/release/ilink_sdk.dll
# Linux   -> target/release/libilink_sdk.so
# macOS   -> target/release/libilink_sdk.dylib
```

## 快速开始

每个绑定都提供同样三样东西：

- 通用入口 `call(op, params)`，可触达**全部**操作；
- 操作结果的类型化模型（见下表）；
- `Client` 辅助类，携带默认路由参数（`account_id`、`base_url`、`cdn_base_url`、
  `route_tag`、`bot_agent`、`timeout_ms`），其常用方法返回类型化模型。

| 语言 | 结果模型 | 类型化 Client |
| --- | --- | --- |
| Python | `ilink_sdk.models` | `ilink_sdk.Client` |
| Go | `ilink` 包 | `ilink.Client` |
| C++ | `ilink_types.hpp` | `ilink::Client` |
| Java | `IlinkModels` | `IlinkClient` |
| Kotlin | `IlinkModels` | `IlinkClient` |

### C

```c
#include "ilink_sdk.h"

char *out = ilink_call("send.text",
                       "{\"to\":\"someone@im.wechat\",\"text\":\"hi\"}");
puts(out);                 /* {"ok":true,"result":{...}} */
ilink_string_free(out);
```

### C++

```cpp
#include "ilink_sdk.hpp"

std::string info = ilink::call("mime.info", R"({"path":"photo.png"})");
ilink::Client client("cfe9a2017e77-im-bot");
ilink::SendResult sent = client.send_text("someone@im.wechat", "hello from C++");
std::cout << sent.message_id;
```

### Rust

```rust,no_run
use ilink_sdk::prelude::*;
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = WeixinClientBuilder::new().build().await?;
client.sender().send_text("someone@im.wechat", "hi", &Default::default()).await?;
# Ok(())
# }
```

### Python

```python
import ilink_sdk

info = ilink_sdk.call("mime.info", {"path": "photo.png"})   # -> dict
client = ilink_sdk.Client(account_id="cfe9a2017e77-im-bot")
sent = client.send_text("someone@im.wechat", "hello from Python")  # models.SendResult
print(sent.message_id)
```

### Go

```go
import "github.com/x500x/iLink-SDK-rs/bindings/go/ilink"

var info struct{ Mime string `json:"mime"` }
_ = ilink.CallInto("mime.info", map[string]any{"path": "photo.png"}, &info)

client := ilink.NewClient(ilink.Config{AccountID: "cfe9a2017e77-im-bot"})
sent, err := client.SendText("someone@im.wechat", "hello from Go", "", "")
fmt.Println(sent.MessageID)
```

### Java

```java
import io.ilink.sdk.IlinkClient;
import io.ilink.sdk.IlinkModels.SendResult;
import io.ilink.sdk.IlinkSdk;

String info = IlinkSdk.call("mime.info", "{\"path\":\"photo.png\"}");
IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");
SendResult sent = client.sendText("someone@im.wechat", "hello from Java");
System.out.println(sent.messageId);
```

### Kotlin

```kotlin
import io.ilink.sdk.IlinkClient
import io.ilink.sdk.IlinkSdk

val info = IlinkSdk.call("mime.info", """{"path":"photo.png"}""")
val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")
val sent = client.sendText("someone@im.wechat", "hello from Kotlin")
println(sent.messageId)
```

## 如何找到共享库

| 语言 | 方式 |
| --- | --- |
| C / C++ | 链接它（`-L target/release -lilink_sdk`），或放到可执行文件旁边 |
| Python | `ILINK_SDK_LIB=/path/to/library`、放到包旁边，或 `target/release` |
| Go | 通过 cgo 编译进去；Windows 上运行时 DLL 需在 `PATH` 中 |
| Java / Kotlin | `-Djna.library.path=target/release`（或 `-Dilink.sdk.lib=<name>`） |

## 错误处理

失败时携带与 Rust API 相同的 [`ErrorKind`](errors.md)
（`dns`、`tcp`、`tls`、`timeout`、`cancelled`、`unknown`）：

```python
except ilink_sdk.IlinkError as err:      # err.kind, err.message
```

```go
if apiErr, ok := err.(*ilink.Error); ok { _ = apiErr.Kind }
```

```java
catch (IlinkError err) { err.getKind(); }
```

```kotlin
catch (err: IlinkError) { err.kind }
```

```cpp
catch (const ilink::Error& err) { err.kind(); }
```

## 操作清单

`call(op, params)` 可触达整个 SDK。全部操作名、参数与返回结构见
[C ABI（dll / so / dylib）](ffi.md#操作列表)。各语言的安装方式、API 说明与可运行
示例位于 [`bindings/`](../../../bindings/README.md)。
