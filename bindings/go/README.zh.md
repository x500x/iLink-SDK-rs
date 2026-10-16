# iLink SDK — Go

> English: [README.md](README.md)

Rust `ilink-sdk` 库的 cgo 绑定，是共享库 JSON C ABI 之上的一层薄封装。

## 构建

```bash
cargo build --release            # 在仓库根目录执行
cd bindings/go
go test ./...
```

Windows 上运行时，产出的 `ilink_sdk.dll` 必须在 `PATH` 中（或放在可执行文件旁边）。

## 用法

```go
package main

import (
    "fmt"

    "github.com/x500x/iLink-SDK-rs/bindings/go/ilink"
)

func main() {
    version, _ := ilink.Version()
    fmt.Println(version)

    var info struct{ Mime string `json:"mime"` }
    _ = ilink.CallInto("mime.info", map[string]any{"path": "photo.png"}, &info)
    fmt.Println(info.Mime)

    client := ilink.NewClient(ilink.Config{AccountID: "cfe9a2017e77-im-bot"})
    accounts, _ := client.Accounts()
    fmt.Println(len(accounts))

    sent, err := client.SendText("someone@im.wechat", "hello from Go", "", "")
    if err != nil {
        panic(err)
    }
    fmt.Println(sent.MessageID)
}
```

## API

- `ilink.Version() (string, error)`
- `ilink.Call(op string, params any) (json.RawMessage, error)` — 全部操作
- `ilink.CallInto(op string, params any, out any) error` — 解析到结构体
- `ilink.NewClient(Config) *Client` — 把 `account_id`、`base_url`、`cdn_base_url`、
  `route_tag`、`bot_agent`、`timeout_ms` 合并进每次调用，并提供类型化方法
  （`Accounts`、`LoginStart`、`LoginComplete`、`SendText`、`SendFile`、
  `GetUpdates`、`SendTyping`、`Upload`）

## 错误

失败返回 `*ilink.Error`，带 `Message` 与 `Kind`：

```go
if apiErr, ok := err.(*ilink.Error); ok && apiErr.Kind == "timeout" {
    // 重试
}
```
