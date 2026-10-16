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

# 二维码登录
start = client.login_start()
print(start["qrcode_url"])
result = client.login_complete(start["session_key"])    # 阻塞至扫码完成
account_id = result["account_id"]

# 发送消息
client.send_text("someone@im.wechat", "hello from Python")

# 长轮询接收
updates = client.get_updates()
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
[`docs/zh/reference/ffi.md`](../../docs/zh/reference/ffi.md)）；`Client` 为常用操作
提供类型化方法，并把默认路由参数（`account_id`、`base_url`、`cdn_base_url`、
`route_tag`、`bot_agent`、`timeout_ms`）应用到每次调用。

## 测试

```bash
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python -m pytest
# 或者没有 pytest 时：
ILINK_SDK_LIB=../../target/release/ilink_sdk.dll python tests/test_smoke.py
```
