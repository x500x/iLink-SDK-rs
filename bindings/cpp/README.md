# iLink SDK — C++

Header-only C++ wrapper around the shared library's JSON C ABI. No dependencies.

## Build

```bash
cargo build --release            # from the repository root
g++ -std=c++17 -I include bindings/cpp/example.cpp \
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

    ilink::Client client("cfe9a2017e77-im-bot");
    std::cout << client.send_text("someone@im.wechat", "hello from C++") << "\n";

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
- `ilink::Client` — carries routing defaults and adds helpers (`accounts`,
  `login_start`, `login_complete`, `send_text`, `send_file`, `get_updates`,
  `send_typing`, `upload`)

## Errors

`ilink::Error` derives from `std::runtime_error`; `kind()` returns the Rust
`ErrorKind` (`"dns"`, `"timeout"`, `"cancelled"`, ...).
