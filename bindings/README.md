# Language bindings

> 中文版: [README.zh.md](README.zh.md)

The Rust crate builds a native library (`ilink_sdk.dll` / `libilink_sdk.so` /
`libilink_sdk.dylib`) that exposes two layers:

- **typed C functions** — one per operation, with C structs for the results
  (no JSON), plus an `IlinkConfig` for routing defaults; and
- a universal JSON-in / JSON-out call, kept for forward compatibility:

```c
IlinkMimeInfo info;
if (ilink_mime_info("photo.png", &info) == ILINK_OK) {
    printf("%s\n", info.mime);
    ilink_mime_info_free(&info);
}

char *out = ilink_call("login.start", "{}");   /* JSON escape hatch */
```

Every binding below is a thin wrapper over that ABI: it loads the library, calls
into it, unwraps the `{"ok":true,"result":...}` / `{"ok":false,...}` envelope
(where it uses the JSON layer), and exposes idiomatic, **typed** helpers. Nothing
is reimplemented per language: each binding only decodes the shared JSON into the
natural value type of that language (structs in C/Go/C++, dataclasses in Python,
classes in Java/Kotlin) so callers never hand-parse result JSON for the common
flows.

| Language | Directory | Guide | Install | Registry |
| --- | --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../include/ilink_sdk.h), [`examples/c`](../examples/c) | [C ABI](../docs/reference/ffi.md) | link the library | [GitHub Releases](https://github.com/x500x/iLink-SDK-rs/releases) |
| C++ | [`bindings/cpp`](cpp) | [C++ guide](cpp/README.md) | `find_package(ilink-sdk)` / pkg-config | [GitHub Releases](https://github.com/x500x/iLink-SDK-rs/releases) |
| Rust | crate root (`use ilink_sdk`) | [Rust docs](../docs/reference/api.md) | `cargo add ilink-sdk` | [crates.io](https://crates.io/crates/ilink-sdk) |
| Python | [`bindings/python`](python) | [Python guide](python/README.md) | `pip install ilink-sdk` | [PyPI](https://pypi.org/project/ilink-sdk/) |
| Go | [`bindings/go`](go) | [Go guide](go/README.md) | `go get .../bindings/go/v2/ilink` | [pkg.go.dev](https://pkg.go.dev/github.com/x500x/iLink-SDK-rs/bindings/go/v2/ilink) |
| Java | [`bindings/java`](java) | [Java guide](java/README.md) | `io.github.x500x:ilink-sdk` (Maven) | [Maven Central](https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk) |
| Kotlin | [`bindings/kotlin`](kotlin) | [Kotlin guide](kotlin/README.md) | `io.github.x500x:ilink-sdk-kotlin` (Gradle) | [Maven Central](https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk-kotlin) |

Every artifact is published from the same `vX.Y.Z` tag; the release process and
registry URLs are documented in
[Releases & distribution](../docs/reference/distribution.md).

Each guide covers installing/locating the library, a quick start, the common
workflows (QR login, sending and receiving, media, context tokens, sync cursor,
 session guard, quote cache) and the full list of typed helpers.

## Out-of-the-box native library

Except for C and C++, every binding ships the native library inside its package,
so installing the binding is enough — no Rust toolchain, no `cargo build`, no
manual library setup:

| Language | How the native library is delivered | Override |
| --- | --- | --- |
| Python | bundled in the platform wheel, next to the package | `ILINK_SDK_LIB` |
| Go | embedded in the module, loaded with purego (no cgo) | `ILINK_SDK_LIB`, `-tags ilink_cgo` |
| Java | bundled in the JAR, extracted at runtime | `-Dilink.sdk.lib`, `-Djna.library.path` |
| Kotlin | bundled in the JAR, extracted at runtime | `-Dilink.sdk.lib`, `-Djna.library.path` |
| C++ | installed by the CMake package next to the headers | `ILINK_SDK_LIBRARY`, `ILINK_SDK_LIB_DIR` |

When you work from a source checkout, build the library and stage it into a
binding's resources with the shared script:

```bash
cargo build --release
python bindings/scripts/stage_native.py --binding go     # or java / kotlin
```

`stage_native.py` accepts `--platform` and `--library` to stage a cross-built
library, which is how CI assembles one multi-platform JAR.

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
| C | `Ilink*` structs | `IlinkConfig` + one function per operation |
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
