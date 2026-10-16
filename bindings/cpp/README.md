# iLink SDK — C++

> 中文版: [README.zh.md](README.zh.md)

Header-only C++ wrapper around the shared library's JSON C ABI. No dependencies.

## Build

```bash
cargo build --release            # from the repository root
g++ -std=c++17 -I include -I bindings/cpp bindings/cpp/example.cpp \
    target/release/ilink_sdk.dll -o ilink_demo.exe
```

On Linux/macOS link `-L target/release -lilink_sdk` (add
`-Wl,-rpath,target/release` if you do not install it).

## Usage

```cpp
#include "ilink_sdk.hpp"
#include <iostream>

int main() {
    std::cout << ilink::version() << "\n";

    // Raw result JSON — parse it with your JSON library of choice.
    std::string info = ilink::call("mime.info", R"({"path":"photo.png"})");
    std::cout << info << "\n";

    // ...or let the client decode it into a typed model.
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
- `ilink::call(op, params_json = "{}")` — every operation; returns the `result`
  JSON and throws `ilink::Error` on failure
- `ilink::call_json(op, params_json = "{}")` — same, but parsed into an
  `ilink::json::Value` (see [`ilink_json.hpp`](ilink_json.hpp))
- `ilink::Client` — carries routing defaults and returns typed models from
  [`ilink_types.hpp`](ilink_types.hpp): `accounts`, `resolve_account`,
  `save_account`, `login_start`, `login_poll`, `login_complete`, `send_text`,
  `send_file`, `send_bytes`, `send_item`, `get_updates`, `get_config`,
  `send_typing`, `upload`, `download`, `download_item`, `mime_info`,
  `session_status`, `quote_policy`, `quote_find`, ...

## Errors

`ilink::Error` derives from `std::runtime_error`; `kind()` returns the Rust
`ErrorKind` (`"dns"`, `"timeout"`, `"cancelled"`, ...).
