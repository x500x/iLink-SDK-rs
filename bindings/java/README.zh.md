# iLink SDK — Java

> English: [README.md](README.md)

基于 [JNA](https://github.com/java-native-access/jna) 的绑定，封装 Rust
`ilink-sdk` 库。无需本地编译：封装层通过 JNA 调用共享库。

## 构建

```bash
cargo build --release            # 在仓库根目录执行，产出共享库
cd bindings/java
mvn package
```

## 用法

```java
import io.ilink.sdk.IlinkClient;
import io.ilink.sdk.IlinkError;
import io.ilink.sdk.IlinkModels.MimeInfo;
import io.ilink.sdk.IlinkModels.SendResult;
import io.ilink.sdk.IlinkSdk;
import io.ilink.sdk.JsonValue;

String version = IlinkSdk.version();

// 返回原始 result JSON —— 可用 Gson/Jackson 解析。
String info = IlinkSdk.call("mime.info", "{\"path\":\"photo.png\"}");

IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");

// ……或让 Client 解码成类型化模型。
MimeInfo mime = client.mimeInfo("photo.png");
System.out.println(mime.mime);

SendResult sent = client.sendText("someone@im.wechat", "hello from Java");
System.out.println(sent.messageId);

// 也可用返回已解析 JSON 的入口。
JsonValue parsed = IlinkSdk.callValue("mime.extension", "{\"mime\":\"image/png\"}");

IlinkSdk.call("does.not.exist", "{}");   // 抛出 IlinkError
```

用 `-Djna.library.path=target/release`（或 `-Dilink.sdk.lib=<name>`）定位库。
Windows 上 DLL 还需在 `PATH` 中（或放在 JVM 启动器旁边）。

## API

- `IlinkSdk.version()`
- `IlinkSdk.call(op, paramsJson)` — 全部操作；返回 `result` JSON，失败抛
  `IlinkError`
- `IlinkSdk.callValue(op, paramsJson)` — 同上，但解析为 `JsonValue`
- `IlinkClient` — 携带路由默认值，并返回
  [`IlinkModels`](src/main/java/io/ilink/sdk/IlinkModels.java) 中的类型化模型：
  `accounts`、`resolveAccount`、`saveAccount`、`loginStart`、`loginPoll`、
  `loginWait`、`loginComplete`、`sendText`、`sendFile`、`sendBytes`、`sendItem`、
  `getUpdates`、`getConfig`、`sendTyping`、`upload`、`download`、`downloadItem`、
  `mimeInfo`、`sessionStatus`、`quotePolicy`、`quoteFind` 等

## 运行示例

```bash
javac -cp jna.jar -d out $(find src/main/java -name '*.java') examples/Example.java
java -cp "out;jna.jar" -Djna.library.path=../../target/release Example
```
