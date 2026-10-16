// Command example is a tiny Go consumer of the iLink SDK bindings.
//
// Run it directly; the native library is resolved automatically (see the
// package documentation):
//
//	cd bindings/go && go run ./examples
package main

import (
	"fmt"
	"os"

	"github.com/x500x/iLink-SDK-rs/bindings/go/v2/ilink"
)

func main() {
	version, err := ilink.Version()
	if err != nil {
		fmt.Fprintln(os.Stderr, "version:", err)
		os.Exit(1)
	}
	fmt.Println("version:", version)

	client := ilink.NewClient(ilink.Config{})

	// Typed helpers return decoded models, not raw JSON.
	info, err := client.MimeInfo("photo.png")
	if err != nil {
		fmt.Fprintln(os.Stderr, "mime.info:", err)
		os.Exit(1)
	}
	fmt.Printf("mime   : %s (image=%v, ext=%s)\n", info.Mime, info.IsImage, info.Extension)

	accounts, err := client.Accounts()
	if err != nil {
		fmt.Fprintln(os.Stderr, "accounts:", err)
		os.Exit(1)
	}
	fmt.Printf("accounts: %d\n", len(accounts))

	// The universal entry point is still available for anything without a
	// dedicated helper.
	if _, err := client.Call("session.status", map[string]any{"account_id": "demo"}); err != nil {
		fmt.Fprintln(os.Stderr, "session.status:", err)
		os.Exit(1)
	}
	fmt.Println("ok")
}
