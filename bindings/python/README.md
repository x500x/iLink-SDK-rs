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

# Typed helpers return dataclasses from ilink_sdk.models.
info = client.mime_info("a.png")            # models.MimeInfo
print(info.mime, info.is_image)

# QR login
start = client.login_start()                # models.LoginStart
print(start.qrcode_url)
result = client.login_complete(start.session_key)   # blocks until scanned
account_id = result.account_id

# Send a message
sent = client.send_text("someone@im.wechat", "hello from Python")
print(sent.message_id)

# Long-poll for inbound messages
updates = client.get_updates()              # models.Updates
for message in updates.messages:
    print(message.message_id)
```

## Common workflows

### Media

```python
# Upload a file to the CDN (media_type: image | video | file | voice)
uploaded = client.upload("someone@im.wechat", "big.png", media_type="image")
print(uploaded.filekey, uploaded.aes_key_base64)

# Download and decrypt a CDN reference, writing it to disk
got = client.download({"encrypt_query_param": "...", "aes_key": "..."},
                      dest_path="/tmp/out.png")
print(got.bytes, got.path)

# Fetch a URL; without dest_path the bytes come back inline, base64 decoded
inline = client.fetch("https://example.com/a.bin")
raw = inline.data()                      # bytes | None

# Download every media referenced by an inbound message item
media = client.download_item(msg.item_list[1], dest_dir="/tmp")
print(media.paths())                     # ['/tmp/...']
```

### Context tokens & sync cursor

```python
client.context_set(account_id, user_id, token)
token = client.context_get(account_id, user_id)   # str | None
client.context_clear(account_id)
client.context_restore(account_id)                # -> int (restored count)

cursor = client.sync_load(account_id)
client.sync_save(account_id, cursor)
client.sync_clear(account_id)
```

### Session guard

```python
client.session_pause(account_id)
status = client.session_status(account_id)        # models.SessionStatus
print(status.paused, status.remaining_ms)
client.session_resume(account_id)
client.session_assert(account_id)                 # raises when paused
```

### Quote cache

```python
policy = client.quote_policy()                    # models.QuotePolicy
client.quote_put(account_id, conversation_id, message_id, body="hi")
record = client.quote_find(account_id, conversation_id, message_id)  # models.QuoteRecord | None
client.quote_gc()
print(client.quote_media_dir(account_id))
```

### Any operation, typed

```python
paths = client.call_into("paths", ilink_sdk.models.Paths)
config = client.config_load()                     # models.ConfigLoad
raw = client.call("mime.extension", {"mime": "image/png"})   # plain dict
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
[`docs/reference/ffi.md`](../../docs/reference/ffi.md)) and returns decoded
Python data. `Client` adds typed helpers for the common flows, returning
[`ilink_sdk.models`](ilink_sdk/models.py) dataclasses (`Account`, `LoginStart`,
`LoginResult`, `SendResult`, `Updates`, `MsgContext`, `UploadedFile`,
`InboundMedia`, `MimeInfo`, ...), and applies default routing parameters
(`account_id`, `base_url`, `cdn_base_url`, `route_tag`, `bot_agent`,
`timeout_ms`) to every call. Use `Client.call_into(op, Model, params)` to decode
any operation into a model.

## Test

```bash
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python -m pytest
# or, with no pytest:
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python tests/test_smoke.py
```
