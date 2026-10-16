# iLink SDK — Kotlin

> English: [README.md](README.md)

基于 [JNA](https://github.com/java-native-access/jna) 的绑定，封装 Rust
`ilink-sdk` 库。API 与 [Java 绑定](../java) 一致，是共享库 JSON C ABI 之上无
额外依赖的封装。

## 构建

```bash
cargo build --release            # 在仓库根目录执行，产出共享库
cd bindings/kotlin
./gradlew run -Dilink.jna.path=../../target/release
```

## 用法

```kotlin
import io.ilink.sdk.IlinkClient
import io.ilink.sdk.IlinkError
import io.ilink.sdk.IlinkSdk

val version = IlinkSdk.version()

// 返回原始 result JSON —— 可用 kotlinx.serialization / Gson / Jackson 解析。
val info = IlinkSdk.call("mime.info", """{"path":"photo.png"}""")

val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")
val sent = client.sendText("someone@im.wechat", "hello from Kotlin")

try {
    IlinkSdk.call("does.not.exist", "{}")
} catch (err: IlinkError) {
    System.err.println("${err.kind}: ${err.message}")
}
```

用 `-Djna.library.path=<dir>`（或 `-Dilink.sdk.lib=<name>`）定位库。
Windows 上 DLL 还需在 `PATH` 中。

## API

- `IlinkSdk.version()`
- `IlinkSdk.call(op, paramsJson)` — 全部操作；返回 `result` JSON，失败抛
  `IlinkError`
- `IlinkClient` — 携带路由默认值，并提供辅助方法（`accounts`、`loginStart`、
  `loginComplete`、`sendText`、`sendFile`、`getUpdates`、`sendTyping`、`upload`）
