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
