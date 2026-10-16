# iLink SDK — Python

> 中文版: [README.zh.md](README.zh.md)

Dependency-free (`ctypes`) Python bindings for the Rust `ilink-sdk` library.
Everything goes through the shared library's JSON C ABI, so the only build
artifact you need is `ilink_sdk.dll` / `libilink_sdk.so` / `libilink_sdk.dylib`.

## Install

```bash
cargo build --release            # from the repository root, produces the library
pip install ./bindings/python
```

The loader looks for the library in this order:

1. `$ILINK_SDK_LIB` (full path to the shared library),
2. next to the installed package,
3. `target/release/` of a source checkout,
4. the system loader path.

## Usage

```python
import ilink_sdk

print(ilink_sdk.version())                              # '2.4.10'
print(ilink_sdk.call("mime.info", {"path": "a.png"}))   # decoded result object

client = ilink_sdk.Client(account_id="cfe9a2017e77-im-bot")

# QR login
start = client.login_start()
print(start["qrcode_url"])
result = client.login_complete(start["session_key"])    # blocks until scanned
account_id = result["account_id"]

# Send a message
client.send_text("someone@im.wechat", "hello from Python")

# Long-poll for inbound messages
updates = client.get_updates()
```

## Errors

Failures raise `ilink_sdk.IlinkError` with the Rust `kind` attached:

```python
try:
    client.send_text("someone@im.wechat", "hi")
except ilink_sdk.IlinkError as err:
    if err.kind in ("timeout", "cancelled"):
        ...   # retry or stop cleanly
    raise
```

## API

`call(op, params=None)` reaches **every** operation (see
[`docs/reference/ffi.md`](../../docs/reference/ffi.md)); `Client` adds typed
helpers for the common ones and applies default routing parameters
(`account_id`, `base_url`, `cdn_base_url`, `route_tag`, `bot_agent`,
`timeout_ms`) to every call.

## Test

```bash
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python -m pytest
# or, with no pytest:
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python tests/test_smoke.py
```
