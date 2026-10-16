# iLink SDK — Kotlin

> 中文版: [README.zh.md](README.zh.md)

[JNA](https://github.com/java-native-access/jna)-based bindings for the Rust
`ilink-sdk` library. The API mirrors the [Java binding](../java); it is a
dependency-free wrapper over the shared library's JSON C ABI.

## Build

```bash
cargo build --release            # from the repository root, produces the library
cd bindings/kotlin
./gradlew run -Dilink.jna.path=../../target/release
```

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
