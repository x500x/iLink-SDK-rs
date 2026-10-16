# iLink SDK — Python

> English: [README.md](README.md)

无第三方依赖（`ctypes`）的 Python 绑定，封装 Rust `ilink-sdk` 库。所有调用都走共享库的
JSON C ABI，因此你只需要一个构建产物：`ilink_sdk.dll` / `libilink_sdk.so` /
`libilink_sdk.dylib`。

## 安装

```bash
cargo build --release            # 在仓库根目录执行，产出共享库
pip install ./bindings/python
```

加载器按以下顺序查找库：

1. `$ILINK_SDK_LIB`（共享库完整路径），
2. 已安装包旁边，
3. 源码检出目录的 `target/release/`，
4. 系统加载路径。

## 用法

```python
import ilink_sdk

print(ilink_sdk.version())                              # '2.4.10'
print(ilink_sdk.call("mime.info", {"path": "a.png"}))   # 已解析的结果对象

client = ilink_sdk.Client(account_id="cfe9a2017e77-im-bot")

# 类型化方法返回 ilink_sdk.models 中的 dataclass。
info = client.mime_info("a.png")            # models.MimeInfo
print(info.mime, info.is_image)

# 二维码登录
start = client.login_start()                # models.LoginStart
print(start.qrcode_url)
result = client.login_complete(start.session_key)   # 阻塞至扫码完成
account_id = result.account_id

# 发送消息
sent = client.send_text("someone@im.wechat", "hello from Python")
print(sent.message_id)

# 长轮询接收
updates = client.get_updates()              # models.Updates
for message in updates.messages:
    print(message.message_id)
```

## 常用流程

### 媒体

```python
# 上传文件到 CDN（media_type: image | video | file | voice）
uploaded = client.upload("someone@im.wechat", "big.png", media_type="image")
print(uploaded.filekey, uploaded.aes_key_base64)

# 下载并解密 CDN 引用，写入磁盘
got = client.download({"encrypt_query_param": "...", "aes_key": "..."},
                      dest_path="/tmp/out.png")
print(got.bytes, got.path)

# 下载 URL；不传 dest_path 时字节内联返回（自动 base64 解码）
inline = client.fetch("https://example.com/a.bin")
raw = inline.data()                      # bytes | None

# 下载某条消息里的全部媒体
media = client.download_item(msg.item_list[1], dest_dir="/tmp")
print(media.paths())                     # ['/tmp/...']
```

### Context token 与同步游标

```python
client.context_set(account_id, user_id, token)
token = client.context_get(account_id, user_id)   # str | None
client.context_clear(account_id)
client.context_restore(account_id)                # -> int（恢复条数）

cursor = client.sync_load(account_id)
client.sync_save(account_id, cursor)
client.sync_clear(account_id)
```

### 会话守卫

```python
client.session_pause(account_id)
status = client.session_status(account_id)        # models.SessionStatus
print(status.paused, status.remaining_ms)
client.session_resume(account_id)
client.session_assert(account_id)                 # 暂停时抛异常
```

### 引用缓存

```python
policy = client.quote_policy()                    # models.QuotePolicy
client.quote_put(account_id, conversation_id, message_id, body="hi")
record = client.quote_find(account_id, conversation_id, message_id)  # models.QuoteRecord | None
client.quote_gc()
print(client.quote_media_dir(account_id))
```

### 任意操作也能类型化

```python
paths = client.call_into("paths", ilink_sdk.models.Paths)
config = client.config_load()                     # models.ConfigLoad
raw = client.call("mime.extension", {"mime": "image/png"})   # 普通 dict
```

## 错误

失败时抛出 `ilink_sdk.IlinkError`，并附带 Rust 的 `kind`：

```python
try:
    client.send_text("someone@im.wechat", "hi")
except ilink_sdk.IlinkError as err:
    if err.kind in ("timeout", "cancelled"):
        ...   # 重试或干净退出
    raise
```

## API

`call(op, params=None)` 可触达**全部**操作（见
[`docs/zh/reference/ffi.md`](../../docs/zh/reference/ffi.md)）并返回已解码的 Python
数据；`Client` 为常用流程提供类型化方法，返回
[`ilink_sdk.models`](ilink_sdk/models.py) 中的 dataclass（`Account`、`LoginStart`、
`LoginResult`、`SendResult`、`Updates`、`MsgContext`、`UploadedFile`、
`InboundMedia`、`MimeInfo` 等），并把默认路由参数（`account_id`、`base_url`、
`cdn_base_url`、`route_tag`、`bot_agent`、`timeout_ms`）应用到每次调用。
可用 `Client.call_into(op, Model, params)` 把任意操作解码成模型。

## 测试

```bash
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python -m pytest
# 或者没有 pytest 时：
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python tests/test_smoke.py
```
