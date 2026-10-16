// Package ilink provides Go bindings for the Rust iLink (Weixin) SDK.
//
// The bindings call the shared library's JSON C ABI (`ilink_call`): pass an
// operation name and a JSON-serialisable parameter value, and decode the
// operation result. Failures are returned as *Error carrying the Rust error
// `kind`.
//
// # No cgo, no manual setup
//
// The native library is loaded at runtime with purego (or the Windows system
// loader) and, when the module ships it, is extracted from an embedded copy for
// the current OS/CPU. You therefore do not need a C compiler or a hand-built
// library to use the bindings:
//
//	import "github.com/x500x/iLink-SDK-rs/bindings/go/ilink"
//
//	version, _ := ilink.Version()
//
// The library is located in this order:
//
//  1. $ILINK_SDK_LIB (a full path, a directory containing the library, or a
//     bare library name for the OS loader);
//  2. a copy next to the running executable;
//  3. the copy embedded in this module for the current platform;
//  4. a previously extracted/downloaded copy in the user cache;
//  5. a one-time download of the matching release asset into that cache
//     (skip with ILINK_SDK_NO_DOWNLOAD);
//  6. the OS loader path (`ilink_sdk.dll` on Windows, `libilink_sdk.so` /
//     `libilink_sdk.dylib` elsewhere).
//
// Set the build tag `ilink_cgo` to link against the library with cgo instead
// (the original behaviour). That requires a C toolchain and the library at
// target/release/ of a source checkout.
package ilink

import (
	"encoding/json"
	"unsafe"
)

// Error is returned when an operation fails. Kind mirrors ilink_sdk::ErrorKind
// ("dns", "tcp", "tls", "timeout", "cancelled", "unknown", ...).
type Error struct {
	Message string
	Kind    string
}

func (e *Error) Error() string { return e.Message }

// takeString copies a NUL-terminated C string into Go memory. The pointer is
// read-only and must still be freed by the caller.
func takeString(ptr *byte) string {
	if ptr == nil {
		return ""
	}
	var n int
	for *(*byte)(unsafe.Add(unsafe.Pointer(ptr), n)) != 0 {
		n++
	}
	return string(unsafe.Slice(ptr, n))
}

// Version returns the SDK version string.
func Version() (string, error) {
	if err := ensureLoaded(); err != nil {
		return "", err
	}
	ptr := nativeVersion()
	if ptr == nil {
		return "", &Error{Message: "ilink_version returned NULL", Kind: "unknown"}
	}
	defer nativeFree(ptr)
	return takeString(ptr), nil
}

// Call runs one SDK operation and returns the raw `result` JSON.
//
//	raw, err := ilink.Call("mime.info", map[string]any{"path": "photo.png"})
func Call(op string, params any) (json.RawMessage, error) {
	if err := ensureLoaded(); err != nil {
		return nil, err
	}

	var paramsJSON string
	if params != nil {
		encoded, err := json.Marshal(params)
		if err != nil {
			return nil, err
		}
		paramsJSON = string(encoded)
	}

	out := nativeCall(op, paramsJSON)
	if out == nil {
		return nil, &Error{Message: "ilink_call returned NULL", Kind: "unknown"}
	}
	defer nativeFree(out)

	var envelope struct {
		OK     bool            `json:"ok"`
		Result json.RawMessage `json:"result"`
		Error  string          `json:"error"`
		Kind   string          `json:"kind"`
	}
	if err := json.Unmarshal([]byte(takeString(out)), &envelope); err != nil {
		return nil, err
	}
	if !envelope.OK {
		return nil, &Error{Message: envelope.Error, Kind: envelope.Kind}
	}
	return envelope.Result, nil
}

// CallInto runs an operation and decodes `result` into out.
func CallInto(op string, params any, out any) error {
	raw, err := Call(op, params)
	if err != nil {
		return err
	}
	return json.Unmarshal(raw, out)
}
