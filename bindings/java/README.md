# iLink SDK — Java

[JNA](https://github.com/java-native-access/jna)-based bindings for the Rust
`ilink-sdk` library. No native compilation step: the wrapper calls the shared
library through JNA.

## Build

```bash
cargo build --release            # from the repository root, produces the library
cd bindings/java
mvn package
```

## Usage

```java
import io.ilink.sdk.IlinkClient;
import io.ilink.sdk.IlinkError;
import io.ilink.sdk.IlinkSdk;

String version = IlinkSdk.version();

// Raw result JSON — parse it with Gson/Jackson if you like.
String info = IlinkSdk.call("mime.info", "{\"path\":\"photo.png\"}");

IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");
String sent = client.sendText("someone@im.wechat", "hello from Java");

try {
    IlinkSdk.call("does.not.exist", "{}");
} catch (IlinkError err) {
    System.err.println(err.getKind() + ": " + err.getMessage());
}
```

Locate the library with `-Djna.library.path=target/release` (or
`-Dilink.sdk.lib=<name>`). On Windows the DLL must also be on `PATH` (or next to
the JVM launcher).

## API

- `IlinkSdk.version()`
- `IlinkSdk.call(op, paramsJson)` — every operation; returns the `result` JSON
  and throws `IlinkError` on failure
- `IlinkClient` — carries routing defaults and adds helpers (`accounts`,
  `loginStart`, `loginComplete`, `sendText`, `sendFile`, `getUpdates`,
  `sendTyping`, `upload`)

## Run the example

```bash
javac -cp jna.jar -d out $(find src/main/java -name '*.java') examples/Example.java
java -cp "out;jna.jar" -Djna.library.path=../../target/release Example
```
