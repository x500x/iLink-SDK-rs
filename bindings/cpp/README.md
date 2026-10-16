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

## Common workflows

`ilink::Client` carries routing defaults and returns the typed structs from
[`ilink_types.hpp`](ilink_types.hpp). Every call throws `ilink::Error` on
failure.

### QR login

```cpp
ilink::Client client("cfe9a2017e77-im-bot");

ilink::LoginStart start = client.login_start();
std::cout << start.qrcode_url << "\n";

// Blocks until the user scans and confirms.
ilink::LoginResult done = client.login_complete(start.session_key, 480);
if (done.connected) {
    std::cout << "logged in as " << *done.account_id << "\n";
}

// Non-blocking: react to poll.kind (continue / needs_verification / expired /
// verify_code_blocked / already_connected / done).
ilink::LoginPoll poll = client.login_poll(start.session_key);
if (poll.kind == "needs_verification") {
    client.login_submit_verify_code(start.session_key, "123456");
} else if (poll.kind == "expired") {
    start = client.login_refresh(start.session_key);   // new QR code
}
```

`login_wait` is like `login_complete` but does not persist the credentials.

### Receiving messages

```cpp
ilink::Updates updates = client.get_updates(cursor, 0);
if (updates.is_error()) {
    std::cerr << "server error " << updates.ret.value_or(0) << "\n";
}
for (const auto& msg : updates.msgs) {
    std::cout << msg["message_id"].as_string() << "\n";
}
if (updates.get_updates_buf) {
    cursor = *updates.get_updates_buf;
}

// Turn a raw message into a typed context:
ilink::MsgContext ctx = client.context_to_message(updates.msgs[0]);
std::cout << ctx.body << " from " << ctx.from << "\n";
```

### Media

```cpp
ilink::UploadedFile uploaded = client.upload("someone@im.wechat", "big.png", "image");
std::cout << uploaded.filekey << "\n";

ilink::MediaFetch fetched = client.fetch("https://example.com/a.bin");
if (fetched.path) std::cout << "saved to " << *fetched.path << "\n";

ilink::InboundMedia media = client.download_item(updates.msgs[0], "/tmp");
for (const auto& path : media.paths()) std::cout << path << "\n";
```

### Context tokens, sync cursor and session guard

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

### Quote cache

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

### Raw JSON for anything else

```cpp
ilink::json::Value extension = ilink::call_json("mime.extension", R"({"mime":"image/png"})");
std::cout << extension["extension"].as_string() << "\n";
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
