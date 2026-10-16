# Language bindings

> 中文版: [中文文档](../zh/reference/bindings.md)

The crate is a Rust library, but its functionality is also reachable from C,
C++, Go, Python, Java and Kotlin. All of them go through the same JSON-in /
JSON-out C ABI described in [C ABI (dll / so / dylib)](ffi.md), so nothing is
reimplemented per language: each binding loads the shared library, calls
`ilink_call`, unwraps the `{"ok":true,"result":...}` envelope and exposes
idiomatic, **typed** helpers. Results are decoded into the natural value type of
the host language (dataclasses in Python, structs in Go/C++, classes in
Java/Kotlin), so callers never hand-parse result JSON for the common flows.

| Language | Directory | Guide | Install | Status |
| --- | --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../../include/ilink_sdk.h) + [`examples/c`](../../examples/c) | [C ABI](ffi.md) | link the library | reference |
| C++ | [`bindings/cpp`](../../bindings/cpp) | [C++ guide](../../bindings/cpp/README.md) | `find_package(ilink-sdk)` / pkg-config | tested |
| Rust | crate root | [API handbook](api.md) | `cargo add ilink-sdk` | native |
| Python | [`bindings/python`](../../bindings/python) | [Python guide](../../bindings/python/README.md) | `pip install ilink-sdk` | tested |
| Go | [`bindings/go`](../../bindings/go) | [Go guide](../../bindings/go/README.md) | `go get .../bindings/go/ilink` | tested |
| Java | [`bindings/java`](../../bindings/java) | [Java guide](../../bindings/java/README.md) | `io.ilink:ilink-sdk` (Maven) | tested |
| Kotlin | [`bindings/kotlin`](../../bindings/kotlin) | [Kotlin guide](../../bindings/kotlin/README.md) | `io.ilink:ilink-sdk` (Gradle) | tested |

Each per-language guide covers installing/locating the library, a quick start,
the common workflows (QR login, sending and receiving, media, context tokens,
sync cursor, session guard, quote cache) and the full list of typed helpers.

## Installing a binding

The Python, Go, Java and Kotlin packages bundle the native library, so installing
the binding is enough. C and C++ link the library that `cargo build --release`
produces.

| Language | Install | Native library |
| --- | --- | --- |
| Python | `pip install ilink-sdk` | bundled in the platform wheel |
| Go | `go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink` | embedded, loaded with purego |
| Java | Maven `io.ilink:ilink-sdk` | bundled in the JAR, extracted at runtime |
| Kotlin | Gradle `io.ilink:ilink-sdk` | bundled in the JAR, extracted at runtime |
| C++ | `cmake --install bindings/cpp --prefix <dir>` | installed next to the headers |

## Building the native library from source

When you work from a source checkout (or build C/C++ bindings), build the shared
library first:

```bash
cargo build --release
# Windows -> target/release/ilink_sdk.dll
# Linux   -> target/release/libilink_sdk.so
# macOS   -> target/release/libilink_sdk.dylib
```

To package a binding with a locally built library, stage it into the binding's
resources with the shared helper:

```bash
python bindings/scripts/stage_native.py --binding go   # or java / kotlin
```

## Quick start

Every binding exposes the same three things:

- a universal `call(op, params)` that reaches **every** operation,
- typed models for the operation results (see the table below), and
- a `Client` helper that carries default routing parameters
  (`account_id`, `base_url`, `cdn_base_url`, `route_tag`, `bot_agent`,
  `timeout_ms`) and returns typed models from its common-flow methods.

| Language | Result models | Typed client |
| --- | --- | --- |
| Python | `ilink_sdk.models` | `ilink_sdk.Client` |
| Go | package `ilink` | `ilink.Client` |
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

## Locating the shared library

When a binding bundles the library this is automatic. The overrides are:

| Language | How |
| --- | --- |
| C / C++ | link it (`-L target/release -lilink_sdk`), or put it next to the executable; CMake honours `ILINK_SDK_LIBRARY` / `ILINK_SDK_LIB_DIR` |
| Python | `ILINK_SDK_LIB=/path/to/library`, next to the package, or `target/release` |
| Go | embedded copy, then `ILINK_SDK_LIB`, the user cache, or a release download; `-tags ilink_cgo` links it at build time |
| Java / Kotlin | bundled copy, then `-Dilink.sdk.lib=<path-or-name>` / `-Djna.library.path=<dir>` |

## Errors

Failures carry the same [`ErrorKind`](errors.md) as the Rust API
(`dns`, `tcp`, `tls`, `timeout`, `cancelled`, `unknown`):

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

## Operations

`call(op, params)` reaches the entire SDK. The full list of operation names,
their parameters and result shapes is in
[C ABI (dll / so / dylib)](ffi.md#operations). Per-language setup, API notes and
runnable examples live under [`bindings/`](../../bindings/README.md).
