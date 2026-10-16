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

// ……或让 Client 解码成类型化 data class。
val mime = client.mimeInfo("photo.png")
println(mime.mime)

val sent = client.sendText("someone@im.wechat", "hello from Kotlin")
println(sent.messageId)

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
- `IlinkSdk.callValue(op, paramsJson)` — 同上，但解析为 `JsonValue`
- `IlinkClient` — 携带路由默认值，并返回
  [`IlinkModels`](src/main/kotlin/io/ilink/sdk/IlinkModels.kt) 中的类型化 data class：
  `accounts`、`resolveAccount`、`saveAccount`、`loginStart`、`loginPoll`、
  `loginWait`、`loginComplete`、`sendText`、`sendFile`、`sendBytes`、`sendItem`、
  `getUpdates`、`getConfig`、`sendTyping`、`upload`、`download`、`downloadItem`、
  `mimeInfo`、`sessionStatus`、`quotePolicy`、`quoteFind` 等

> `IlinkClient.notify(action)` 的参数为必填，因为 `@JvmOverloads` 会生成与
> `Object.notify()` 冲突的 `notify()` 重载。
