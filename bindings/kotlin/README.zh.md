# iLink SDK — Kotlin

> English: [README.md](README.md)

基于 [JNA](https://github.com/java-native-access/jna) 的绑定，封装 Rust
`ilink-sdk` 库。API 与 [Java 绑定](../java) 一致。发布到仓库的 JAR 内置了各支持
平台的原生库，运行时会自动解压匹配的那一份，因此无需手动配置库路径。

## 安装

```kotlin
// build.gradle.kts
dependencies {
    implementation("io.ilink:ilink-sdk:2.4.10")
}
```

启动时加载器会挑选当前 OS/CPU 对应的内置库（JAR 内的
`io/ilink/sdk/native/<platform>/`），解压到临时目录后把路径交给 JNA。如需覆盖，
设置 `-Dilink.sdk.lib=<path-or-name>`（或 `-Djna.library.path=<dir>`）。

### 从源码构建

```bash
cargo build --release            # 在仓库根目录执行，产出共享库
python bindings/scripts/stage_native.py --binding kotlin   # 将其内置到 JAR
cd bindings/kotlin
./gradlew run -Dilink.jna.path=../../target/release
```

在 `./gradlew build` 之前，对每个平台各跑一次 `stage_native.py`，即可把多个平台打进
同一个 JAR（CI 会对全部六个发布目标执行）。

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

## 常用流程

`IlinkClient` 携带路由默认值，并返回
[`IlinkModels`](src/main/kotlin/io/ilink/sdk/IlinkModels.kt) 中的类型化 data class。
每次调用失败都会抛 `IlinkError`。

### 二维码登录

```kotlin
val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")

val start = client.loginStart()
println(start.qrcodeUrl)

// 阻塞至用户扫码确认。
val done = client.loginComplete(start.sessionKey, timeoutSecs = 480)
if (done.connected) {
    println("已登录为 ${done.accountId}")
}

// 非阻塞：根据 poll.kind 分支（continue / needs_verification / expired /
// verify_code_blocked / already_connected / done）。
val poll = client.loginPoll(start.sessionKey)
when (poll.kind) {
    "needs_verification" -> client.loginSubmitVerifyCode(start.sessionKey, "123456")
    "expired" -> client.loginRefresh(start.sessionKey)   // 新二维码
}
```

`loginWait` 与 `loginComplete` 类似，但不持久化凭据。

### 接收消息

```kotlin
val updates = client.getUpdates(cursor, timeoutMs = 0)
if (updates.isError) {
    System.err.println("服务端错误 ${updates.ret}")
}
for (msg in updates.msgs) {
    println(msg["message_id"].asString())
}
updates.getUpdatesBuf?.let { cursor = it }

// 把原始消息转成类型化上下文：
val ctx = client.contextToMessage(updates.msgs[0])
println("${ctx.body} from ${ctx.from}")
```

### 媒体

```kotlin
val uploaded = client.upload("someone@im.wechat", "big.png", mediaType = "image")
println(uploaded.filekey)

val fetched = client.fetch("https://example.com/a.bin")
fetched.path?.let { println("已保存到 $it") }
val bytes = fetched.data()          // 内联返回时已解码

val media = client.downloadItem(updates.msgs[0], destDir = "/tmp")
println(media.paths)
```

### Context token、同步游标与会话守卫

```kotlin
client.contextSet(accountId, userId, token)
val token2 = client.contextGet(accountId, userId)   // 不存在时为 null
client.contextClear(accountId)
val restored = client.contextRestore(accountId)

cursor = client.syncLoad(accountId)
client.syncSave(accountId, cursor)
client.syncClear(accountId)

client.sessionPause(accountId)
val status = client.sessionStatus(accountId)
client.sessionResume(accountId)
```

### 引用缓存

```kotlin
import io.ilink.sdk.IlinkModels.QuoteRecord

val policy = client.quotePolicy()

client.quotePut(
    QuoteRecord(
        accountId = accountId,
        conversationId = conversationId,
        messageId = messageId,
        body = "hi",
    ),
)
val found = client.quoteFind(accountId, conversationId, messageId)
client.quoteGc()
```

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
