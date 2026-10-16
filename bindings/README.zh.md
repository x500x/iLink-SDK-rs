# 多语言绑定

> English: [README.md](README.md)

Rust crate 会产出一个原生库（`ilink_sdk.dll` / `libilink_sdk.so` /
`libilink_sdk.dylib`），其全部能力都通过一个 JSON 进 / JSON 出的 C 函数暴露：

```c
char *ilink_call(const char *op, const char *params_json);
char *ilink_version(void);
void  ilink_string_free(char *ptr);
```

下面每个绑定都是这套 ABI 的薄封装：加载库、调用 `ilink_call`、拆开
`{"ok":true,"result":...}` / `{"ok":false,...}` 信封，并暴露符合该语言习惯的
辅助方法。**没有按语言重复实现协议。**

| 语言 | 目录 | 运行时依赖 | 状态 |
| --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../include/ilink_sdk.h)、[`examples/c`](../examples/c) | — | 参考实现 |
| C++ | [`bindings/cpp`](cpp) | —（header-only） | 已实测 |
| Rust | crate 本体（`use ilink_sdk`） | — | 原生 |
| Python | [`bindings/python`](python) | 无（`ctypes`） | 已实测 |
| Go | [`bindings/go`](go) | cgo | 已实测 |
| Java | [`bindings/java`](java) | [JNA](https://github.com/java-native-access/jna) | 已实测 |
| Kotlin | [`bindings/kotlin`](kotlin) | JNA | 已实测 |

## 先构建共享库

在仓库根目录：

```bash
cargo build --release
# -> target/release/ilink_sdk.dll      (Windows)
#    target/release/libilink_sdk.so    (Linux)
#    target/release/libilink_sdk.dylib (macOS)
```

然后让绑定找到它：

| 语言 | 如何定位库 |
| --- | --- |
| C / C++ | 链接它 / `-I include`，或放到可执行文件旁边 |
| Python | `ILINK_SDK_LIB=/path/to/library`，或装到包旁边 |
| Go | 通过 cgo 编译进去；运行时 DLL 需在 `PATH` 中 |
| Java / Kotlin | `-Djna.library.path=target/release` |

## 操作清单

每个绑定都能触达全部操作；完整的 `op` 名称、参数与返回结构见
[`docs/zh/reference/ffi.md`](../docs/zh/reference/ffi.md)。

## 错误处理

每个绑定都会附带 Rust 的 `ErrorKind`
（`dns`、`tcp`、`tls`、`timeout`、`cancelled`、`unknown`）：

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
