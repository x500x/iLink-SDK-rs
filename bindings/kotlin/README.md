# iLink SDK — Kotlin

> 中文版: [README.zh.md](README.zh.md)

[JNA](https://github.com/java-native-access/jna)-based bindings for the Rust
`ilink-sdk` library. The API mirrors the [Java binding](../java). The published
JAR bundles the native library for every supported platform and extracts the
right one at runtime, so no manual library setup is required.

## Install

```kotlin
// build.gradle.kts
dependencies {
    implementation("io.ilink:ilink-sdk-kotlin:2.4.10")
}
```

Published on Maven Central:
<https://central.sonatype.com/artifact/io.ilink/ilink-sdk-kotlin>.

At startup the loader picks the bundled library for the current OS/CPU
(`io/ilink/sdk/native/<platform>/` inside the JAR), extracts it to a temporary
directory and hands the path to JNA. To override, set
`-Dilink.sdk.lib=<path-or-name>` (or `-Djna.library.path=<dir>`).

### Building from source

```bash
cargo build --release            # from the repository root, produces the library
python bindings/scripts/stage_native.py --binding kotlin   # bundle it into the JAR
cd bindings/kotlin
./gradlew run -Dilink.jna.path=../../target/release
```

Run `stage_native.py` once per platform before `./gradlew build` to bundle
several platforms into one JAR (CI does this for all six release targets).

## Usage

```kotlin
import io.ilink.sdk.IlinkClient
import io.ilink.sdk.IlinkError
import io.ilink.sdk.IlinkSdk

val version = IlinkSdk.version()

// Raw result JSON — parse it with kotlinx.serialization / Gson / Jackson.
val info = IlinkSdk.call("mime.info", """{"path":"photo.png"}""")

val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")

// ...or let the client decode it into a typed data class.
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

Locate the library with `-Djna.library.path=<dir>` (or `-Dilink.sdk.lib=<name>`).
On Windows the DLL must also be on `PATH`.

## Common workflows

`IlinkClient` carries routing defaults and returns the typed data classes from
[`IlinkModels`](src/main/kotlin/io/ilink/sdk/IlinkModels.kt). Every call throws
`IlinkError` on failure.

### QR login

```kotlin
val client = IlinkClient(accountId = "cfe9a2017e77-im-bot")

val start = client.loginStart()
println(start.qrcodeUrl)

// Blocks until the user scans and confirms.
val done = client.loginComplete(start.sessionKey, timeoutSecs = 480)
if (done.connected) {
    println("logged in as ${done.accountId}")
}

// Non-blocking: react to poll.kind (continue / needs_verification / expired /
// verify_code_blocked / already_connected / done).
val poll = client.loginPoll(start.sessionKey)
when (poll.kind) {
    "needs_verification" -> client.loginSubmitVerifyCode(start.sessionKey, "123456")
    "expired" -> client.loginRefresh(start.sessionKey)   // new QR code
}
```

`loginWait` is like `loginComplete` but does not persist the credentials.

### Receiving messages

```kotlin
val updates = client.getUpdates(cursor, timeoutMs = 0)
if (updates.isError) {
    System.err.println("server error ${updates.ret}")
}
for (msg in updates.msgs) {
    println(msg["message_id"].asString())
}
updates.getUpdatesBuf?.let { cursor = it }

// Turn a raw message into a typed context:
val ctx = client.contextToMessage(updates.msgs[0])
println("${ctx.body} from ${ctx.from}")
```

### Media

```kotlin
val uploaded = client.upload("someone@im.wechat", "big.png", mediaType = "image")
println(uploaded.filekey)

val fetched = client.fetch("https://example.com/a.bin")
fetched.path?.let { println("saved to $it") }
val bytes = fetched.data()          // decoded when returned inline

val media = client.downloadItem(updates.msgs[0], destDir = "/tmp")
println(media.paths)
```

### Context tokens, sync cursor and session guard

```kotlin
client.contextSet(accountId, userId, token)
val token2 = client.contextGet(accountId, userId)   // null when absent
client.contextClear(accountId)
val restored = client.contextRestore(accountId)

cursor = client.syncLoad(accountId)
client.syncSave(accountId, cursor)
client.syncClear(accountId)

client.sessionPause(accountId)
val status = client.sessionStatus(accountId)
client.sessionResume(accountId)
```

### Quote cache

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
- `IlinkSdk.call(op, paramsJson)` — every operation; returns the `result` JSON
  and throws `IlinkError` on failure
- `IlinkSdk.callValue(op, paramsJson)` — same, but parsed into a `JsonValue`
- `IlinkClient` — carries routing defaults and returns typed data classes from
  [`IlinkModels`](src/main/kotlin/io/ilink/sdk/IlinkModels.kt): `accounts`,
  `resolveAccount`, `saveAccount`, `loginStart`, `loginPoll`, `loginWait`,
  `loginComplete`, `sendText`, `sendFile`, `sendBytes`, `sendItem`, `getUpdates`,
  `getConfig`, `sendTyping`, `upload`, `download`, `downloadItem`, `mimeInfo`,
  `sessionStatus`, `quotePolicy`, `quoteFind`, ...

> `IlinkClient.notify(action)` takes a required argument because
> `@JvmOverloads` would otherwise generate a `notify()` overload clashing with
> `Object.notify()`.
