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

## Common workflows

`IlinkClient` carries routing defaults and returns the typed models from
[`IlinkModels`](src/main/java/io/ilink/sdk/IlinkModels.java). Every call throws
`IlinkError` on failure.

### QR login

```java
IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");

IlinkModels.LoginStart start = client.loginStart();
System.out.println(start.qrcodeUrl);

// Blocks until the user scans and confirms.
IlinkModels.LoginResult done = client.loginComplete(start.sessionKey, 480, null);
if (done.connected) {
    System.out.println("logged in as " + done.accountId);
}

// Non-blocking: react to poll.kind (continue / needs_verification / expired /
// verify_code_blocked / already_connected / done).
IlinkModels.LoginPoll poll = client.loginPoll(start.sessionKey);
if ("needs_verification".equals(poll.kind)) {
    client.loginSubmitVerifyCode(start.sessionKey, "123456");
} else if ("expired".equals(poll.kind)) {
    start = client.loginRefresh(start.sessionKey);   // new QR code
}
```

`loginWait` is like `loginComplete` but does not persist the credentials.

### Receiving messages

```java
IlinkModels.Updates updates = client.getUpdates(cursor, 0);
if (updates.isError()) {
    System.err.println("server error " + updates.ret);
}
for (JsonValue msg : updates.msgs) {
    System.out.println(msg.get("message_id").asString(""));
}
if (updates.getUpdatesBuf != null) {
    cursor = updates.getUpdatesBuf;
}

// Turn a raw message into a typed context:
IlinkModels.MsgContext ctx = client.contextToMessage(updates.msgs.get(0));
System.out.println(ctx.body + " from " + ctx.from);
```

### Media

```java
IlinkModels.UploadedFile uploaded = client.upload("someone@im.wechat", "big.png", "image");
System.out.println(uploaded.filekey);

IlinkModels.MediaFetch fetched = client.fetch("https://example.com/a.bin", null);
if (fetched.path != null) {
    System.out.println("saved to " + fetched.path);
}
byte[] bytes = fetched.data();          // decoded when returned inline

IlinkModels.InboundMedia media = client.downloadItem(updates.msgs.get(0), "/tmp");
System.out.println(media.paths());
```

### Context tokens, sync cursor and session guard

```java
client.contextSet(accountId, userId, token);
String token2 = client.contextGet(accountId, userId);   // null when absent
client.contextClear(accountId);
long restored = client.contextRestore(accountId);

cursor = client.syncLoad(accountId);
client.syncSave(accountId, cursor);
client.syncClear(accountId);

client.sessionPause(accountId);
IlinkModels.SessionStatus status = client.sessionStatus(accountId);
client.sessionResume(accountId);
```

### Quote cache

```java
IlinkModels.QuotePolicy policy = client.quotePolicy();

IlinkModels.QuoteRecord record = new IlinkModels.QuoteRecord();
record.accountId = accountId;
record.conversationId = conversationId;
record.messageId = messageId;
record.body = "hi";
client.quotePut(record);

IlinkModels.QuoteRecord found = client.quoteFind(accountId, conversationId, messageId);
client.quoteGc();
```

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
