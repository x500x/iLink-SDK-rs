# iLink SDK — C++

> English: [README.md](README.md)

共享库 JSON C ABI 之上的 header-only C++ 封装，除 C++17 编译器和原生 `ilink_sdk`
库外无其它依赖。

## 安装

封装本身是 header-only，唯一构建产物是原生库。预编译压缩包在
[releases 页面](https://github.com/x500x/iLink-SDK-rs/releases)（每平台一个，内含共享库/
静态库与 `ilink_sdk.h`）。

从源码接入最省事的方式是用本目录里的 CMake 包：

```bash
cargo build --release                    # 在仓库根目录执行
cmake -S bindings/cpp -B build
cmake --install build --prefix /your/prefix
```

这会把头文件、原生库和 CMake 包安装到 `/your/prefix`。随后在任意项目中：

```cmake
find_package(ilink-sdk REQUIRED)
target_link_libraries(app PRIVATE ilink::ilink)
```

同时会安装 `pkg-config` 文件（`ilink-sdk.pc`），因此
`pkg-config --cflags --libs ilink-sdk` 也可用。安装目录可整体搬移：它会在包旁边
解析原生库。

## 不用 CMake 直接编译

如果更愿意直接调用编译器，把两个头文件目录都指上并链接原生库：

```bash
cargo build --release            # 在仓库根目录执行
g++ -std=c++17 -I include -I bindings/cpp bindings/cpp/example.cpp \
    target/release/ilink_sdk.dll -o ilink_demo.exe
```

Linux/macOS 上链接 `-L target/release -lilink_sdk`（若未安装，再加
`-Wl,-rpath,target/release`）。Windows + MinGW 用
`target/release/ilink_sdk.dll.lib` 作为导入库；MSVC 则链接 `ilink_sdk.dll.lib`，
并在运行时把 `ilink_sdk.dll` 放在可执行文件旁边。

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
