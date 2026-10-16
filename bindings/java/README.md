# iLink SDK — Java

> 中文版: [README.zh.md](README.zh.md)

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
import io.ilink.sdk.IlinkModels.MimeInfo;
import io.ilink.sdk.IlinkModels.SendResult;
import io.ilink.sdk.IlinkSdk;
import io.ilink.sdk.JsonValue;

String version = IlinkSdk.version();

// Raw result JSON — parse it with Gson/Jackson if you like.
String info = IlinkSdk.call("mime.info", "{\"path\":\"photo.png\"}");

IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");

// ...or let the client decode it into a typed model.
MimeInfo mime = client.mimeInfo("photo.png");
System.out.println(mime.mime);

SendResult sent = client.sendText("someone@im.wechat", "hello from Java");
System.out.println(sent.messageId);

// The parsed-JSON entry point is available too.
JsonValue parsed = IlinkSdk.callValue("mime.extension", "{\"mime\":\"image/png\"}");

IlinkSdk.call("does.not.exist", "{}");   // throws IlinkError
```

Locate the library with `-Djna.library.path=target/release` (or
`-Dilink.sdk.lib=<name>`). On Windows the DLL must also be on `PATH` (or next to
the JVM launcher).

## API

- `IlinkSdk.version()`
- `IlinkSdk.call(op, paramsJson)` — every operation; returns the `result` JSON
  and throws `IlinkError` on failure
- `IlinkSdk.callValue(op, paramsJson)` — same, but parsed into a `JsonValue`
- `IlinkClient` — carries routing defaults and returns typed models from
  [`IlinkModels`](src/main/java/io/ilink/sdk/IlinkModels.java): `accounts`,
  `resolveAccount`, `saveAccount`, `loginStart`, `loginPoll`, `loginWait`,
  `loginComplete`, `sendText`, `sendFile`, `sendBytes`, `sendItem`, `getUpdates`,
  `getConfig`, `sendTyping`, `upload`, `download`, `downloadItem`, `mimeInfo`,
  `sessionStatus`, `quotePolicy`, `quoteFind`, ...

## Run the example

```bash
javac -cp jna.jar -d out $(find src/main/java -name '*.java') examples/Example.java
java -cp "out;jna.jar" -Djna.library.path=../../target/release Example
```
