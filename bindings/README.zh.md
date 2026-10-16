# 多语言绑定

> English: [README.md](README.md)

Rust crate 会产出一个原生库（`ilink_sdk.dll` / `libilink_sdk.so` /
`libilink_sdk.dylib`），它暴露两层接口：

- **类型化 C 函数** —— 每个操作一个函数，结果用 C 结构体（无需 JSON），
  另有 `IlinkConfig` 携带路由默认值；以及
- 一个万能的 JSON 进 / JSON 出调用，用于向前兼容：

```c
IlinkMimeInfo info;
if (ilink_mime_info("photo.png", &info) == ILINK_OK) {
    printf("%s\n", info.mime);
    ilink_mime_info_free(&info);
}

char *out = ilink_call("login.start", "{}");   /* JSON 逃生通道 */
```

下面每个绑定都是这套 ABI 的薄封装：加载库、调用它（用到 JSON 层时）拆开
`{"ok":true,"result":...}` / `{"ok":false,...}` 信封，并暴露符合该语言习惯的
**类型化**辅助方法。**没有按语言重复实现协议**：每个绑定只是把同一份 JSON 解码成该语言
自然的值类型（C/Go/C++ 用 struct，Python 用 dataclass，Java/Kotlin 用类），因此常用流程
无需手动解析结果 JSON。

| 语言 | 目录 | 指南 | 安装方式 | 仓库 |
| --- | --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](../include/ilink_sdk.h)、[`examples/c`](../examples/c) | [C ABI](../docs/zh/reference/ffi.md) | 链接库 | [GitHub Releases](https://github.com/x500x/iLink-SDK-rs/releases) |
| C++ | [`bindings/cpp`](cpp) | [C++ 指南](cpp/README.zh.md) | `find_package(ilink-sdk)` / pkg-config | [GitHub Releases](https://github.com/x500x/iLink-SDK-rs/releases) |
| Rust | crate 本体（`use ilink_sdk`） | [Rust 文档](../docs/zh/reference/api.md) | `cargo add ilink-sdk` | [crates.io](https://crates.io/crates/ilink-sdk) |
| Python | [`bindings/python`](python) | [Python 指南](python/README.zh.md) | `pip install ilink-sdk` | [PyPI](https://pypi.org/project/ilink-sdk/) |
| Go | [`bindings/go`](go) | [Go 指南](go/README.zh.md) | `go get .../bindings/go/v2/ilink` | [pkg.go.dev](https://pkg.go.dev/github.com/x500x/iLink-SDK-rs/bindings/go/v2/ilink) |
| Java | [`bindings/java`](java) | [Java 指南](java/README.zh.md) | `io.github.x500x:ilink-sdk`（Maven） | [Maven Central](https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk) |
| Kotlin | [`bindings/kotlin`](kotlin) | [Kotlin 指南](kotlin/README.zh.md) | `io.github.x500x:ilink-sdk-kotlin`（Gradle） | [Maven Central](https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk-kotlin) |

所有产物都从同一个 `vX.Y.Z` tag 发布；发版流程与各仓库地址见
[发布与分发](../docs/zh/reference/distribution.md)。

每份指南都涵盖库的安装/定位、快速开始、常用流程（二维码登录、收发消息、媒体、
context token、同步游标、会话守卫、引用缓存）以及全部类型化辅助方法。

## 开箱即用的原生库

除 C 和 C++ 外，每个绑定都把原生库随包一起分发，因此装好绑定就能用——不需要 Rust
工具链、不需要 `cargo build`、也不需要手动配置库路径：

| 语言 | 原生库的交付方式 | 覆盖方式 |
| --- | --- | --- |
| Python | 打包进平台 wheel，位于包内 | `ILINK_SDK_LIB` |
| Go | 嵌入模块，用 purego 加载（无需 cgo） | `ILINK_SDK_LIB`、`-tags ilink_cgo` |
| Java | 打包进 JAR，运行时解压 | `-Dilink.sdk.lib`、`-Djna.library.path` |
| Kotlin | 打包进 JAR，运行时解压 | `-Dilink.sdk.lib`、`-Djna.library.path` |
| C++ | 由 CMake 包安装到头文件旁边 | `ILINK_SDK_LIBRARY`、`ILINK_SDK_LIB_DIR` |

在源码检出目录中开发时，构建库并把它写入某个绑定的资源目录（共用一个脚本）：

```bash
cargo build --release
python bindings/scripts/stage_native.py --binding go     # 或 java / kotlin
```

`stage_native.py` 支持 `--platform` 与 `--library`，用于写入交叉编译的库，CI 正是借此
组装出支持多平台的单个 JAR。

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
| C | `Ilink*` 结构体 | `IlinkConfig` + 每个操作一个函数 |
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
