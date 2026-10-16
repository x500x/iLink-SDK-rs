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

    client := ilink.NewClient(ilink.Config{AccountID: "cfe9a2017e77-im-bot"})

    // 类型化方法返回已解码的 struct，而非原始 JSON。
    info, _ := client.MimeInfo("photo.png")
    fmt.Println(info.Mime)

    accounts, _ := client.Accounts()
    fmt.Println(len(accounts))

    sent, err := client.SendText("someone@im.wechat", "hello from Go", "", "")
    if err != nil {
        panic(err)
    }
    fmt.Println(sent.MessageID)

    updates, _ := client.GetUpdates("", 0)
    for _, message := range updates.Messages() {
        fmt.Println(message.MessageID)
    }

    // Call / CallInto 仍可触达全部操作。
    var raw struct{ Mime string `json:"mime"` }
    _ = ilink.CallInto("mime.info", map[string]any{"path": "photo.png"}, &raw)
}
```

## 常用流程

### 二维码登录

`LoginStart` + `LoginComplete` 是常用路径（阻塞至用户扫码）：

```go
start, err := client.LoginStart("")            // *ilink.LoginStart
if err != nil {
    panic(err)
}
fmt.Println(start.QRCodeURL)

done, err := client.LoginComplete(start.SessionKey, 480, "")   // *ilink.LoginResult
if err != nil {
    panic(err)
}
if done.Connected {
    fmt.Println("已登录为", done.AccountID)
}
```

若要自己驱动流程，可轮询状态并根据 `poll.Kind` 分支
（`continue`、`needs_verification`、`expired`、`verify_code_blocked`、
`already_connected`、`done`）。务必检查错误：会话不存在时 `LoginPoll` 会返回
`(nil, err)`。

```go
poll, err := client.LoginPoll(start.SessionKey)
if err != nil {
    panic(err)
}
switch poll.Kind {
case "needs_verification":
    _ = client.LoginSubmitVerifyCode(start.SessionKey, "123456")
case "expired":
    start, err = client.LoginRefresh(start.SessionKey)   // 新二维码
    if err != nil {
        panic(err)
    }
}
```

`LoginWait` 与 `LoginComplete` 类似，但不持久化凭据。

### 接收消息

```go
updates, err := client.GetUpdates(cursor, 0)   // *ilink.Updates
if err != nil {
    panic(err)
}
if updates.IsError() {
    fmt.Println("服务端错误", updates.Ret, updates.ErrCode, updates.ErrMsg)
}
for _, msg := range updates.Messages() {
    fmt.Println(msg.MessageID, msg.FromUserID)
    if text, ok := msg.ItemList[0].Text(); ok {
        fmt.Println("  text:", text)
    }
}
if updates.GetUpdatesBuf != nil {
    cursor = *updates.GetUpdatesBuf
}

// 把原始消息转成类型化上下文：
ctx, _ := client.ContextToMessage(msg)         // *ilink.MsgContext
fmt.Println(ctx.Body, ctx.From, ctx.To)
```

### 媒体

```go
uploaded, _ := client.Upload("someone@im.wechat", "big.png", "image")
fmt.Println(uploaded.FileKey, uploaded.AESKeyBase64)

fetched, _ := client.Fetch("https://example.com/a.bin", "")
data, _ := fetched.Data()                      // []byte | nil

media, _ := client.DownloadItem(msg.ItemList[1], "/tmp")
fmt.Println(media.Paths())
```

### Context token、同步游标与会话守卫

```go
_ = client.ContextSet(accountID, userID, token)
token, _ = client.ContextGet(accountID, userID)        // 不存在时为 ""
_ = client.ContextClear(accountID)
n, _ := client.ContextRestore(accountID)

cursor, _ = client.SyncLoad(accountID)
_ = client.SyncSave(accountID, cursor)
_ = client.SyncClear(accountID)

_ = client.SessionPause(accountID)
status, _ := client.SessionStatus(accountID)           // *ilink.SessionStatus
fmt.Println(status.Paused, status.RemainingMS)
_ = client.SessionResume(accountID)
```

### 引用缓存

```go
policy, _ := client.QuotePolicy()                      // *ilink.QuotePolicy
_ = client.QuotePut(ilink.QuoteRecord{
    AccountID: accountID, ConversationID: convID, MessageID: msgID, Body: "hi",
})
record, _ := client.QuoteFind(accountID, convID, msgID) // *ilink.QuoteRecord | nil
_ = client.QuoteGC()
```

## API

- `ilink.Version() (string, error)`
- `ilink.Call(op string, params any) (json.RawMessage, error)` — 全部操作
- `ilink.CallInto(op string, params any, out any) error` — 解析到结构体
- `ilink.NewClient(Config) *Client` — 把 `account_id`、`base_url`、`cdn_base_url`、
  `route_tag`、`bot_agent`、`timeout_ms` 合并进每次调用，并返回类型化模型：
  `Accounts`、`ResolveAccount`、`SaveAccount`、`LoginStart`、`LoginPoll`、
  `LoginWait`、`LoginComplete`、`SendText`、`SendFile`、`SendBytes`、`SendItem`、
  `GetUpdates`、`GetConfig`、`SendTyping`、`Upload`、`Download`、`DownloadItem`、
  `MimeInfo`、`SessionStatus`、`QuotePolicy`、`QuoteFind` 等。模型定义见
  [`ilink/types.go`](ilink/types.go)。

## 错误

失败返回 `*ilink.Error`，带 `Message` 与 `Kind`：

```go
if apiErr, ok := err.(*ilink.Error); ok && apiErr.Kind == "timeout" {
    // 重试
}
```
