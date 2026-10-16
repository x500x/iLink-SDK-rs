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
**类型化**辅助方法。**没有按语言重复实现协议**：每个绑定只是把同一份 JSON 解码成该语言
自然的值类型（Python 用 dataclass，Go/C++ 用 struct，Java/Kotlin 用类），因此常用流程
无需手动解析结果 JSON。

| 语言 | 目录 | 指南 | 运行时依赖 | 状态 |
| --- | --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../include/ilink_sdk.h)、[`examples/c`](../examples/c) | [C ABI](../docs/zh/reference/ffi.md) | — | 参考实现 |
| C++ | [`bindings/cpp`](cpp) | [C++ 指南](cpp/README.zh.md) | —（header-only） | 已实测 |
| Rust | crate 本体（`use ilink_sdk`） | [Rust 文档](../docs/zh/reference/api.md) | — | 原生 |
| Python | [`bindings/python`](python) | [Python 指南](python/README.zh.md) | 无（`ctypes`） | 已实测 |
| Go | [`bindings/go`](go) | [Go 指南](go/README.zh.md) | cgo | 已实测 |
| Java | [`bindings/java`](java) | [Java 指南](java/README.zh.md) | [JNA](https://github.com/java-native-access/jna) | 已实测 |
| Kotlin | [`bindings/kotlin`](kotlin) | [Kotlin 指南](kotlin/README.zh.md) | JNA | 已实测 |

每份指南都涵盖库的安装/定位、快速开始、常用流程（二维码登录、收发消息、媒体、
context token、同步游标、会话守卫、引用缓存）以及全部类型化辅助方法。

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

## 类型化返回值

通用入口 `call(op, params)` 仍可触达全部操作，但每个绑定同时提供类型化模型，以及返回
这些模型的 `Client` 辅助方法：

| 语言 | 返回类型 | Client |
| --- | --- | --- |
| Python | `ilink_sdk.models.*` dataclass | `ilink_sdk.Client` |
| Go | `ilink` 包内的 struct | `ilink.Client` |
| C++ | `ilink_types.hpp` 内的 struct | `ilink::Client` |
| Java | `IlinkModels.*` | `IlinkClient` |
| Kotlin | `IlinkModels.*` data class | `IlinkClient` |

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
