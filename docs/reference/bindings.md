# Language bindings

The crate is a Rust library, but its functionality is also reachable from C,
C++, Go, Python, Java and Kotlin. All of them go through the same JSON-in /
JSON-out C ABI described in [C ABI (dll / so / dylib)](ffi.md), so nothing is
reimplemented per language: each binding loads the shared library, calls
`ilink_call`, unwraps the `{"ok":true,"result":...}` envelope and exposes
idiomatic helpers.

| Language | Directory | Runtime dependency | Status |
| --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../../include/ilink_sdk.h) + [`examples/c`](../../examples/c) | — | reference |
| C++ | [`bindings/cpp`](../../bindings/cpp) | — (header-only) | tested |
| Rust | crate root | — | native |
| Python | [`bindings/python`](../../bindings/python) | none (`ctypes`) | tested |
| Go | [`bindings/go`](../../bindings/go) | cgo | tested |
| Java | [`bindings/java`](../../bindings/java) | [JNA](https://github.com/java-native-access/jna) | tested |
| Kotlin | [`bindings/kotlin`](../../bindings/kotlin) | JNA | tested |

## Build the library first

```bash
cargo build --release
# Windows -> target/release/ilink_sdk.dll
# Linux   -> target/release/libilink_sdk.so
# macOS   -> target/release/libilink_sdk.dylib
```

## Quick start

Every binding exposes the same two things:

- a universal `call(op, params)` that reaches **every** operation, and
- a `Client` helper that carries default routing parameters
  (`account_id`, `base_url`, `cdn_base_url`, `route_tag`, `bot_agent`,
  `timeout_ms`) and adds typed helpers for the common flows.

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
std::string sent = client.send_text("someone@im.wechat", "hello from C++");
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
client.send_text("someone@im.wechat", "hello from Python")
```

### Go

```go
import "github.com/x500x/iLink-SDK-rs/bindings/go/ilink"

var info struct{ Mime string `json:"mime"` }
_ = ilink.CallInto("mime.info", map[string]any{"path": "photo.png"}, &info)

client := ilink.NewClient(ilink.Config{AccountID: "cfe9a2017e77-im-bot"})
sent, err := client.SendText("someone@im.wechat", "hello from Go", "", "")
```

### Java

```java
import io.ilink.sdk.IlinkClient;
import io.ilink.sdk.IlinkSdk;

String info = IlinkSdk.call("mime.info", "{\"path\":\"photo.png\"}");
IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");
String sent = client.sendText("someone@im.wechat", "hello from Java");
```

### Kotlin

```kotlin
import io.ilink.sdk.IlinkClient
import io.ilink.sdk.IlinkSdk

val info = IlinkSdk.call("mime.info", """{"path":"photo.png"}""")
val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")
val sent = client.sendText("someone@im.wechat", "hello from Kotlin")
```

## Locating the shared library

| Language | How |
| --- | --- |
| C / C++ | link it (`-L target/release -lilink_sdk`), or put it next to the executable |
| Python | `ILINK_SDK_LIB=/path/to/library`, next to the package, or `target/release` |
| Go | compiled in via cgo; the DLL must be on `PATH` at runtime (Windows) |
| Java / Kotlin | `-Djna.library.path=target/release` (or `-Dilink.sdk.lib=<name>`) |

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
