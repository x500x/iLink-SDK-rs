//go:build !ilink_cgo

package ilink

import (
	"sync"

	"github.com/ebitengine/purego"
)

// The native library is loaded at runtime (no cgo) and its three exported
// symbols are bound to Go function values. Finding the library is handled by
// resolveLibrary (see resolve.go), which also downloads a matching build into
// the user cache when the module itself does not ship one.
var (
	nativeOnce sync.Once
	nativeErr  error

	nativeCall    func(op string, paramsJSON string) *byte
	nativeVersion func() *byte
	nativeFree    func(ptr *byte)
)

// ensureLoaded loads the native library exactly once and binds its symbols.
func ensureLoaded() error {
	nativeOnce.Do(func() {
		path, err := resolveLibrary()
		if err != nil {
			nativeErr = err
			return
		}
		handle, err := openLibrary(path)
		if err != nil {
			nativeErr = &Error{
				Message: "cannot load the iLink native library (" + path + "): " + err.Error(),
				Kind:    "unknown",
			}
			return
		}
		purego.RegisterLibFunc(&nativeCall, handle, "ilink_call")
		purego.RegisterLibFunc(&nativeVersion, handle, "ilink_version")
		purego.RegisterLibFunc(&nativeFree, handle, "ilink_string_free")
	})
	return nativeErr
}
