# iLink SDK — Java

> English: [README.md](README.md)

基于 [JNA](https://github.com/java-native-access/jna) 的绑定，封装 Rust
`ilink-sdk` 库。发布到仓库的 JAR 内置了各支持平台的原生库，运行时会自动解压匹配的
那一份，因此无需本地编译，也无需手动配置库路径。

## 安装

在构建中加入依赖：

```xml
<dependency>
  <groupId>io.github.x500x</groupId>
  <artifactId>ilink-sdk</artifactId>
  <version>2.4.11</version>
</dependency>
```

发布于 Maven Central：
<https://central.sonatype.com/artifact/io.github.x500x/ilink-sdk>。

启动时加载器会挑选当前 OS/CPU 对应的内置库（JAR 内的
`io/ilink/sdk/native/<platform>/`），解压到临时目录后把路径交给 JNA。如需覆盖：

- `-Dilink.sdk.lib=/abs/path/to/libilink_sdk.so` —— 直接使用该文件；
- `-Dilink.sdk.lib=ilink_sdk` —— 交给 JNA 按名称解析（会遵循
  `-Djna.library.path` 和系统加载路径）。

### 从源码构建

```bash
cargo build --release            # 在仓库根目录执行，产出共享库
python bindings/scripts/stage_native.py --binding java   # 将其内置到 JAR
cd bindings/java
mvn package
```

在 `mvn package` 之前，对每个平台各跑一次 `stage_native.py`，即可把多个平台打进同一个
JAR（CI 会对全部六个发布目标执行）。

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

## 常用流程

`IlinkClient` 携带路由默认值，并返回
[`IlinkModels`](src/main/java/io/ilink/sdk/IlinkModels.java) 中的类型化模型。
每次调用失败都会抛 `IlinkError`。

### 二维码登录

```java
IlinkClient client = new IlinkClient("cfe9a2017e77-im-bot");

IlinkModels.LoginStart start = client.loginStart();
System.out.println(start.qrcodeUrl);

// 阻塞至用户扫码确认。
IlinkModels.LoginResult done = client.loginComplete(start.sessionKey, 480, null);
if (done.connected) {
    System.out.println("已登录为 " + done.accountId);
}

// 非阻塞：根据 poll.kind 分支（continue / needs_verification / expired /
// verify_code_blocked / already_connected / done）。
IlinkModels.LoginPoll poll = client.loginPoll(start.sessionKey);
if ("needs_verification".equals(poll.kind)) {
    client.loginSubmitVerifyCode(start.sessionKey, "123456");
} else if ("expired".equals(poll.kind)) {
    start = client.loginRefresh(start.sessionKey);   // 新二维码
}
```

`loginWait` 与 `loginComplete` 类似，但不持久化凭据。

### 接收消息

```java
IlinkModels.Updates updates = client.getUpdates(cursor, 0);
if (updates.isError()) {
    System.err.println("服务端错误 " + updates.ret);
}
for (JsonValue msg : updates.msgs) {
    System.out.println(msg.get("message_id").asString(""));
}
if (updates.getUpdatesBuf != null) {
    cursor = updates.getUpdatesBuf;
}

// 把原始消息转成类型化上下文：
IlinkModels.MsgContext ctx = client.contextToMessage(updates.msgs.get(0));
System.out.println(ctx.body + " from " + ctx.from);
```

### 媒体

```java
IlinkModels.UploadedFile uploaded = client.upload("someone@im.wechat", "big.png", "image");
System.out.println(uploaded.filekey);

IlinkModels.MediaFetch fetched = client.fetch("https://example.com/a.bin", null);
if (fetched.path != null) {
    System.out.println("已保存到 " + fetched.path);
}
byte[] bytes = fetched.data();          // 内联返回时已解码

IlinkModels.InboundMedia media = client.downloadItem(updates.msgs.get(0), "/tmp");
System.out.println(media.paths());
```

### Context token、同步游标与会话守卫

```java
client.contextSet(accountId, userId, token);
String token2 = client.contextGet(accountId, userId);   // 不存在时为 null
client.contextClear(accountId);
long restored = client.contextRestore(accountId);

cursor = client.syncLoad(accountId);
client.syncSave(accountId, cursor);
client.syncClear(accountId);

client.sessionPause(accountId);
IlinkModels.SessionStatus status = client.sessionStatus(accountId);
client.sessionResume(accountId);
```

### 引用缓存

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
