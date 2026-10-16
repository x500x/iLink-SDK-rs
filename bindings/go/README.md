# iLink SDK — Go

> 中文版: [README.zh.md](README.zh.md)

Pure-Go bindings for the Rust `ilink-sdk` library. The wrapper is a thin layer
over the shared library's JSON C ABI.

## Install

```bash
go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink
```

That is all you need: the module loads the native library at runtime with
[purego](https://github.com/ebitengine/purego), so **no C compiler and no
`cargo build` are required**. When the module does not carry an embedded copy
for your platform, the matching [release
asset](https://github.com/x500x/iLink-SDK-rs/releases) is downloaded once into
the user cache.

The library is located in this order:

1. `$ILINK_SDK_LIB` — a full path, a directory containing the library, or a
   bare library name for the OS loader;
2. a copy next to your executable;
3. the copy embedded in the module for your OS/CPU (present in a staged build);
4. a previously downloaded copy in the user cache
   (`%LOCALAPPDATA%\ilink-sdk\<version>` / `~/.cache/ilink-sdk/<version>`);
5. a one-time download of the matching release asset into that cache — set
   `ILINK_SDK_NO_DOWNLOAD=1` to skip this step;
6. the OS loader path (`ilink_sdk.dll`, `libilink_sdk.so`, `libilink_sdk.dylib`).

Once a library is cached the download step is never repeated.

### Building from a source checkout

When you work inside this repository, build the library and stage it for the
current platform:

```bash
cargo build --release                    # from the repository root
python bindings/scripts/stage_native.py --binding go
cd bindings/go && go test ./...
```

### Opt-in cgo build

If you would rather link against a library you built yourself, use the
`ilink_cgo` build tag. That restores the original cgo behaviour (a C toolchain
is required and the library is read from `target/release/`):

```bash
go build -tags ilink_cgo ./...
```

## Usage

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

    // Typed helpers return decoded structs, not raw JSON.
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

    // Call / CallInto still reach every operation.
    var raw struct{ Mime string `json:"mime"` }
    _ = ilink.CallInto("mime.info", map[string]any{"path": "photo.png"}, &raw)
}
```

## Common workflows

### QR login

`LoginStart` + `LoginComplete` is the usual path (blocking until the user scans):

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
    fmt.Println("logged in as", done.AccountID)
}
```

To drive the flow yourself, poll for status and react to `poll.Kind`
(`continue`, `needs_verification`, `expired`, `verify_code_blocked`,
`already_connected`, `done`). Always check the error: an inactive session makes
`LoginPoll` return `(nil, err)`.

```go
poll, err := client.LoginPoll(start.SessionKey)
if err != nil {
    panic(err)
}
switch poll.Kind {
case "needs_verification":
    _ = client.LoginSubmitVerifyCode(start.SessionKey, "123456")
case "expired":
    start, err = client.LoginRefresh(start.SessionKey)   // new QR code
    if err != nil {
        panic(err)
    }
}
```

`LoginWait` is like `LoginComplete` but does not persist the credentials.

### Receiving messages

```go
updates, err := client.GetUpdates(cursor, 0)   // *ilink.Updates
if err != nil {
    panic(err)
}
if updates.IsError() {
    fmt.Println("server error", updates.Ret, updates.ErrCode, updates.ErrMsg)
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

// Turn a raw message into a typed context:
ctx, _ := client.ContextToMessage(msg)         // *ilink.MsgContext
fmt.Println(ctx.Body, ctx.From, ctx.To)
```

### Media

```go
uploaded, _ := client.Upload("someone@im.wechat", "big.png", "image")
fmt.Println(uploaded.FileKey, uploaded.AESKeyBase64)

fetched, _ := client.Fetch("https://example.com/a.bin", "")
data, _ := fetched.Data()                      // []byte | nil

media, _ := client.DownloadItem(msg.ItemList[1], "/tmp")
fmt.Println(media.Paths())
```

### Context tokens, sync cursor and session guard

```go
_ = client.ContextSet(accountID, userID, token)
token, _ = client.ContextGet(accountID, userID)        // "" when absent
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

### Quote cache

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
- `ilink.Call(op string, params any) (json.RawMessage, error)` — every operation
- `ilink.CallInto(op string, params any, out any) error` — decode into a struct
- `ilink.NewClient(Config) *Client` — merges `account_id`, `base_url`,
  `cdn_base_url`, `route_tag`, `bot_agent`, `timeout_ms` into every call and
  returns typed models: `Accounts`, `ResolveAccount`, `SaveAccount`,
  `LoginStart`, `LoginPoll`, `LoginWait`, `LoginComplete`, `SendText`,
  `SendFile`, `SendBytes`, `SendItem`, `GetUpdates`, `GetConfig`, `SendTyping`,
  `Upload`, `Download`, `DownloadItem`, `MimeInfo`, `SessionStatus`,
  `QuotePolicy`, `QuoteFind`, ... The models live in
  [`ilink/types.go`](ilink/types.go).

## Errors

Failures are `*ilink.Error` with `Message` and `Kind`:

```go
if apiErr, ok := err.(*ilink.Error); ok && apiErr.Kind == "timeout" {
    // retry
}
```
