# Language bindings

The Rust crate builds a native library (`ilink_sdk.dll` / `libilink_sdk.so` /
`libilink_sdk.dylib`) whose entire surface is one JSON-in / JSON-out C function:

```c
char *ilink_call(const char *op, const char *params_json);
char *ilink_version(void);
void  ilink_string_free(char *ptr);
```

Every binding below is a thin wrapper over that ABI: it loads the library, calls
`ilink_call`, unwraps the `{"ok":true,"result":...}` / `{"ok":false,...}`
envelope, and exposes idiomatic helpers. Nothing is reimplemented per language.

| Language | Directory | Runtime dependency | Status |
| --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../include/ilink_sdk.h), [`examples/c`](../examples/c) | — | reference |
| C++ | [`bindings/cpp`](cpp) | — (header-only) | tested |
| Rust | crate root (`use ilink_sdk`) | — | native |
| Python | [`bindings/python`](python) | none (`ctypes`) | tested |
| Go | [`bindings/go`](go) | cgo | tested |
| Java | [`bindings/java`](java) | [JNA](https://github.com/java-native-access/jna) | tested |
| Kotlin | [`bindings/kotlin`](kotlin) | JNA | source |

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
