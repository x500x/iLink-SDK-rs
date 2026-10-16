# 多语言绑定

> English: [English](../../reference/bindings.md)

本 crate 是一个 Rust 库，但其功能同样可以从 C、C++、Go、Python、Java 和 Kotlin
调用。它们都走同一个 JSON 进 / JSON 出的 C ABI（见 [C ABI（dll / so / dylib）](ffi.md)），
因此**没有按语言重复实现协议**：每个绑定只负责加载共享库、调用 `ilink_call`、
拆开 `{"ok":true,"result":...}` 信封，并暴露符合该语言习惯的**类型化**辅助方法。
结果会被解码成宿主语言自然的值类型（Python 用 dataclass，Go/C++ 用 struct，
Java/Kotlin 用类），因此常用流程无需手动解析结果 JSON。

| 语言 | 目录 | 指南 | 安装方式 | 状态 |
| --- | --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../../../include/ilink_sdk.h) + [`examples/c`](../../../examples/c) | [C ABI](ffi.md) | 链接库 | 参考实现 |
| C++ | [`bindings/cpp`](../../../bindings/cpp) | [C++ 指南](../../../bindings/cpp/README.zh.md) | `find_package(ilink-sdk)` / pkg-config | 已实测 |
| Rust | crate 本体 | [API 手册](api.md) | `cargo add ilink-sdk` | 原生 |
| Python | [`bindings/python`](../../../bindings/python) | [Python 指南](../../../bindings/python/README.zh.md) | `pip install ilink-sdk` | 已实测 |
| Go | [`bindings/go`](../../../bindings/go) | [Go 指南](../../../bindings/go/README.zh.md) | `go get .../bindings/go/ilink` | 已实测 |
| Java | [`bindings/java`](../../../bindings/java) | [Java 指南](../../../bindings/java/README.zh.md) | `io.github.x500x:ilink-sdk`（Maven） | 已实测 |
| Kotlin | [`bindings/kotlin`](../../../bindings/kotlin) | [Kotlin 指南](../../../bindings/kotlin/README.zh.md) | `io.github.x500x:ilink-sdk`（Gradle） | 已实测 |

每份语言指南都涵盖库的安装/定位、快速开始、常用流程（二维码登录、收发消息、媒体、
context token、同步游标、会话守卫、引用缓存）以及全部类型化辅助方法。

## 安装绑定

Python、Go、Java、Kotlin 包都内置了原生库，因此装好绑定就能用。C 和 C++ 则链接
`cargo build --release` 产出的库。所有产物都从同一个 `vX.Y.Z` tag 发布——各仓库地址与
发版流程见 [发布与分发](distribution.md)。

| 语言 | 安装 | 仓库 | 原生库 |
| --- | --- | --- | --- |
| Python | `pip install ilink-sdk` | [PyPI](https://pypi.org/project/ilink-sdk/) | 打包进平台 wheel |
| Go | `go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink` | [pkg.go.dev](https://pkg.go.dev/github.com/x500x/iLink-SDK-rs/bindings/go/ilink) | 嵌入模块，用 purego 加载 |
| Java | Maven `io.github.x500x:ilink-sdk` | [Maven Central](https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk) | 打包进 JAR，运行时解压 |
| Kotlin | Gradle `io.github.x500x:ilink-sdk-kotlin` | [Maven Central](https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk-kotlin) | 打包进 JAR，运行时解压 |
| C++ | `cmake --install bindings/cpp --prefix <dir>` | [GitHub Releases](https://github.com/x500x/iLink-SDK-rs/releases) | 安装到头文件旁边 |
| Rust | `cargo add ilink-sdk` | [crates.io](https://crates.io/crates/ilink-sdk) | 从源码构建 |

## 从源码构建原生库

在源码检出目录中开发（或构建 C/C++ 绑定时），先构建共享库：

```bash
cargo build --release
# Windows -> target/release/ilink_sdk.dll
# Linux   -> target/release/libilink_sdk.so
# macOS   -> target/release/libilink_sdk.dylib
```

若要把本地构建的库打包进某个绑定，用共用脚本写入该绑定的资源目录：

```bash
python bindings/scripts/stage_native.py --binding go   # 或 java / kotlin
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

绑定内置了库时这一步是自动的。可用的覆盖方式：

| 语言 | 方式 |
| --- | --- |
| C / C++ | 链接它（`-L target/release -lilink_sdk`），或放到可执行文件旁边；CMake 支持 `ILINK_SDK_LIBRARY` / `ILINK_SDK_LIB_DIR` |
| Python | `ILINK_SDK_LIB=/path/to/library`、放到包旁边，或 `target/release` |
| Go | 先内置副本，再 `ILINK_SDK_LIB`、用户缓存、release 下载；`-tags ilink_cgo` 则在构建期链接 |
| Java / Kotlin | 先内置副本，再 `-Dilink.sdk.lib=<path-or-name>` / `-Djna.library.path=<dir>` |

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
