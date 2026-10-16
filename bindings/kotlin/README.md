# iLink SDK — Kotlin

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
val sent = client.sendText("someone@im.wechat", "hello from Kotlin")

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
- `IlinkClient` — carries routing defaults and adds helpers (`accounts`,
  `loginStart`, `loginComplete`, `sendText`, `sendFile`, `getUpdates`,
  `sendTyping`, `upload`)
