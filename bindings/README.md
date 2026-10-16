# Language bindings

> 中文版: [README.zh.md](README.zh.md)

The Rust crate builds a native library (`ilink_sdk.dll` / `libilink_sdk.so` /
`libilink_sdk.dylib`) whose entire surface is one JSON-in / JSON-out C function:

```c
char *ilink_call(const char *op, const char *params_json);
char *ilink_version(void);
void  ilink_string_free(char *ptr);
```

Every binding below is a thin wrapper over that ABI: it loads the library, calls
`ilink_call`, unwraps the `{"ok":true,"result":...}` / `{"ok":false,...}`
envelope, and exposes idiomatic, **typed** helpers. Nothing is reimplemented per
language: each binding only decodes the shared JSON into the natural value type
of that language (dataclasses in Python, structs in Go/C++, classes in
Java/Kotlin) so callers never hand-parse result JSON for the common flows.

| Language | Directory | Guide | Runtime dependency | Status |
| --- | --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../include/ilink_sdk.h), [`examples/c`](../examples/c) | [C ABI](../docs/reference/ffi.md) | — | reference |
| C++ | [`bindings/cpp`](cpp) | [C++ guide](cpp/README.md) | — (header-only) | tested |
| Rust | crate root (`use ilink_sdk`) | [Rust docs](../docs/reference/api.md) | — | native |
| Python | [`bindings/python`](python) | [Python guide](python/README.md) | none (`ctypes`) | tested |
| Go | [`bindings/go`](go) | [Go guide](go/README.md) | cgo | tested |
| Java | [`bindings/java`](java) | [Java guide](java/README.md) | [JNA](https://github.com/java-native-access/jna) | tested |
| Kotlin | [`bindings/kotlin`](kotlin) | [Kotlin guide](kotlin/README.md) | JNA | tested |

Each guide covers installing/locating the library, a quick start, the common
workflows (QR login, sending and receiving, media, context tokens, sync cursor,
session guard, quote cache) and the full list of typed helpers.

## Build the library first

From the repository root:

```bash
cargo build --release
# -> target/release/ilink_sdk.dll   (Windows)
#    target/release/libilink_sdk.so (Linux)
#    target/release/libilink_sdk.dylib (macOS)
```

Then point the binding at it:

| Language | How to locate the library |
| --- | --- |
| C / C++ | link it / `-I include`, or put it next to the executable |
| Python | `ILINK_SDK_LIB=/path/to/library` or install it next to the package |
| Go | compiled in via cgo; the DLL must be on `PATH` at runtime |
| Java / Kotlin | `-Djna.library.path=target/release` |

## Operation catalogue

Every binding can reach every operation; the full list of `op` names, their
parameters and result shapes is in
[`docs/reference/ffi.md`](../docs/reference/ffi.md).

## Error handling

Each binding reports failures with the Rust `ErrorKind`
(`dns`, `tcp`, `tls`, `timeout`, `cancelled`, `unknown`) attached to the error:

```python
except ilink_sdk.IlinkError as err:   # err.kind, err.message
```

```go
if apiErr, ok := err.(*ilink.Error); ok { _ = apiErr.Kind }
```

```java
catch (IlinkError err) { err.getKind(); }
```

```cpp
catch (const ilink::Error& err) { err.kind(); }
```

## Typed results

The universal `call(op, params)` still reaches every operation, but each binding
also exposes typed models and a `Client` whose helpers return them:

| Language | Result type | Client |
| --- | --- | --- |
| Python | `ilink_sdk.models.*` dataclasses | `ilink_sdk.Client` |
| Go | structs in package `ilink` | `ilink.Client` |
| C++ | structs in `ilink_types.hpp` | `ilink::Client` |
| Java | `IlinkModels.*` | `IlinkClient` |
| Kotlin | `IlinkModels.*` data classes | `IlinkClient` |

```python
info = ilink_sdk.Client().mime_info("photo.png")   # models.MimeInfo
info.mime, info.is_image
```

```go
info, err := ilink.NewClient(ilink.Config{}).MimeInfo("photo.png")
info.Mime
```

```cpp
ilink::Client client;
ilink::MimeInfo info = client.mime_info("photo.png");
```

```java
IlinkModels.MimeInfo info = new IlinkClient().mimeInfo("photo.png");
```

```kotlin
val info = IlinkClient().mimeInfo("photo.png")
```
