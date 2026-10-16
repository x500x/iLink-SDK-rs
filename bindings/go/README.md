# iLink SDK — Go

> 中文版: [README.zh.md](README.zh.md)

cgo bindings for the Rust `ilink-sdk` library. The wrapper is a thin layer over
the shared library's JSON C ABI.

## Build

```bash
cargo build --release            # from the repository root
cd bindings/go
go test ./...
```

On Windows the resulting `ilink_sdk.dll` must be on `PATH` (or next to your
executable) when the program runs.

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
- `ilink.Call(op string, params any) (json.RawMessage, error)` — every operation
- `ilink.CallInto(op string, params any, out any) error` — decode into a struct
- `ilink.NewClient(Config) *Client` — merges `account_id`, `base_url`,
  `cdn_base_url`, `route_tag`, `bot_agent`, `timeout_ms` into every call and
  adds typed helpers (`Accounts`, `LoginStart`, `LoginComplete`, `SendText`,
  `SendFile`, `GetUpdates`, `SendTyping`, `Upload`)

## Errors

Failures are `*ilink.Error` with `Message` and `Kind`:

```go
if apiErr, ok := err.(*ilink.Error); ok && apiErr.Kind == "timeout" {
    // retry
}
```
