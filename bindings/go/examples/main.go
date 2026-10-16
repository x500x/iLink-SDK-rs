// Command example is a tiny Go consumer of the iLink SDK bindings.
//
// Build the library first (from the repository root): cargo build --release
// Then: cd bindings/go && go run ./examples
package main

import (
	"fmt"
	"os"

	"github.com/x500x/iLink-SDK-rs/bindings/go/ilink"
)

func main() {
	version, err := ilink.Version()
	if err != nil {
		fmt.Fprintln(os.Stderr, "version:", err)
		os.Exit(1)
	}
	fmt.Println("version:", version)

	var info struct {
		Mime string `json:"mime"`
	}
	if err := ilink.CallInto("mime.info", map[string]any{"path": "photo.png"}, &info); err != nil {
		fmt.Fprintln(os.Stderr, "mime.info:", err)
		os.Exit(1)
	}
	fmt.Println("mime   :", info.Mime)

	client := ilink.NewClient(ilink.Config{})
	accounts, err := client.Accounts()
	if err != nil {
		fmt.Fprintln(os.Stderr, "accounts:", err)
		os.Exit(1)
	}
	fmt.Printf("accounts: %d\n", len(accounts))
}
