# iLink SDK — C++

> English: [README.md](README.md)

共享库 JSON C ABI 之上的 header-only C++ 封装，无第三方依赖。

## 构建

```bash
cargo build --release            # 在仓库根目录执行
g++ -std=c++17 -I include -I bindings/cpp bindings/cpp/example.cpp \
    target/release/ilink_sdk.dll -o ilink_demo.exe
```

Linux/macOS 上链接 `-L target/release -lilink_sdk`（若未安装，再加
`-Wl,-rpath,target/release`）。

## 用法

```cpp
#include "ilink_sdk.hpp"
#include <iostream>

int main() {
    std::cout << ilink::version() << "\n";

    // 返回原始 result JSON —— 用你偏好的 JSON 库解析。
    std::string info = ilink::call("mime.info", R"({"path":"photo.png"})");
    std::cout << info << "\n";

    // ……或让 Client 解码成类型化模型。
    ilink::Client client("cfe9a2017e77-im-bot");
    ilink::MimeInfo mime = client.mime_info("photo.png");
    std::cout << mime.mime << "\n";

    ilink::SendResult sent = client.send_text("someone@im.wechat", "hello from C++");
    std::cout << sent.message_id << "\n";

    try {
        ilink::call("does.not.exist");
    } catch (const ilink::Error& err) {
        std::cerr << err.kind() << ": " << err.what() << "\n";
    }
}
```

## 常用流程

`ilink::Client` 携带路由默认值，并返回
[`ilink_types.hpp`](ilink_types.hpp) 中的类型化 struct。每次调用失败都会抛
`ilink::Error`。

### 二维码登录

```cpp
ilink::Client client("cfe9a2017e77-im-bot");

ilink::LoginStart start = client.login_start();
std::cout << start.qrcode_url << "\n";

// 阻塞至用户扫码确认。
ilink::LoginResult done = client.login_complete(start.session_key, 480);
if (done.connected) {
    std::cout << "已登录为 " << *done.account_id << "\n";
}

// 非阻塞：根据 poll.kind 分支（continue / needs_verification / expired /
// verify_code_blocked / already_connected / done）。
ilink::LoginPoll poll = client.login_poll(start.session_key);
if (poll.kind == "needs_verification") {
    client.login_submit_verify_code(start.session_key, "123456");
} else if (poll.kind == "expired") {
    start = client.login_refresh(start.session_key);   // 新二维码
}
```

`login_wait` 与 `login_complete` 类似，但不持久化凭据。

### 接收消息

```cpp
ilink::Updates updates = client.get_updates(cursor, 0);
if (updates.is_error()) {
    std::cerr << "服务端错误 " << updates.ret.value_or(0) << "\n";
}
for (const auto& msg : updates.msgs) {
    std::cout << msg["message_id"].as_string() << "\n";
}
if (updates.get_updates_buf) {
    cursor = *updates.get_updates_buf;
}

// 把原始消息转成类型化上下文：
ilink::MsgContext ctx = client.context_to_message(updates.msgs[0]);
std::cout << ctx.body << " from " << ctx.from << "\n";
```

### 媒体

```cpp
ilink::UploadedFile uploaded = client.upload("someone@im.wechat", "big.png", "image");
std::cout << uploaded.filekey << "\n";

ilink::MediaFetch fetched = client.fetch("https://example.com/a.bin");
if (fetched.path) std::cout << "已保存到 " << *fetched.path << "\n";

ilink::InboundMedia media = client.download_item(updates.msgs[0], "/tmp");
for (const auto& path : media.paths()) std::cout << path << "\n";
```

### Context token、同步游标与会话守卫

```cpp
client.context_set(account_id, user_id, token);
if (auto token = client.context_get(account_id, user_id)) { /* ... */ }
client.context_clear(account_id);
std::int64_t restored = client.context_restore(account_id);

std::string cursor = client.sync_load(account_id);
client.sync_save(account_id, cursor);
client.sync_clear(account_id);

client.session_pause(account_id);
ilink::SessionStatus status = client.session_status(account_id);
client.session_resume(account_id);
```

### 引用缓存

```cpp
ilink::QuotePolicy policy = client.quote_policy();

ilink::QuoteRecord record;
record.account_id = account_id;
record.conversation_id = conversation_id;
record.message_id = message_id;
record.body = "hi";
client.quote_put(record);

if (auto found = client.quote_find(account_id, conversation_id, message_id)) {
    std::cout << found->body << "\n";
}
client.quote_gc();
```

### 其他操作走原始 JSON

```cpp
ilink::json::Value extension = ilink::call_json("mime.extension", R"({"mime":"image/png"})");
std::cout << extension["extension"].as_string() << "\n";
```

## API

- `ilink::version()`
- `ilink::call(op, params_json = "{}")` — 全部操作；返回 `result` JSON，失败抛
  `ilink::Error`
- `ilink::call_json(op, params_json = "{}")` — 同上，但解析为 `ilink::json::Value`
  （见 [`ilink_json.hpp`](ilink_json.hpp)）
- `ilink::Client` — 携带路由默认值，并返回
  [`ilink_types.hpp`](ilink_types.hpp) 中的类型化模型：`accounts`、
  `resolve_account`、`save_account`、`login_start`、`login_poll`、
  `login_complete`、`send_text`、`send_file`、`send_bytes`、`send_item`、
  `get_updates`、`get_config`、`send_typing`、`upload`、`download`、
  `download_item`、`mime_info`、`session_status`、`quote_policy`、`quote_find` 等

## 错误

`ilink::Error` 继承自 `std::runtime_error`；`kind()` 返回 Rust 的 `ErrorKind`
（`"dns"`、`"timeout"`、`"cancelled"` 等）。
