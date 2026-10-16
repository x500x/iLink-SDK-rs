// Package ilink provides Go bindings for the Rust iLink (Weixin) SDK.
//
// The bindings are a thin cgo wrapper around the shared library's JSON C ABI
// (`ilink_call`): pass an operation name and a JSON-serialisable parameter
// value, and decode the operation result. Failures are returned as *Error
// carrying the Rust error `kind`.
//
// Build the shared library first (from the repository root):
//
//	cargo build --release
//
// On Windows the resulting ilink_sdk.dll must be on PATH (or next to your
// executable) when the program runs.
package ilink

/*
#cgo CFLAGS: -I${SRCDIR}/../../../include
#cgo linux LDFLAGS: -L${SRCDIR}/../../../target/release -lilink_sdk -Wl,-rpath,${SRCDIR}/../../../target/release
#cgo darwin LDFLAGS: -L${SRCDIR}/../../../target/release -lilink_sdk -Wl,-rpath,${SRCDIR}/../../../target/release
#cgo windows LDFLAGS: ${SRCDIR}/../../../target/release/ilink_sdk.dll
#include <stdlib.h>
#include "ilink_sdk.h"
*/
import "C"

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

// Version returns the SDK version string.
func Version() (string, error) {
	ptr := C.ilink_version()
	if ptr == nil {
		return "", &Error{Message: "ilink_version returned NULL", Kind: "unknown"}
	}
	defer C.ilink_string_free(ptr)
	return C.GoString(ptr), nil
}

// Call runs one SDK operation and returns the raw `result` JSON.
//
//	raw, err := ilink.Call("mime.info", map[string]any{"path": "photo.png"})
func Call(op string, params any) (json.RawMessage, error) {
	cop := C.CString(op)
	defer C.free(unsafe.Pointer(cop))

	var cparams *C.char
	if params != nil {
		encoded, err := json.Marshal(params)
		if err != nil {
			return nil, err
		}
		cparams = C.CString(string(encoded))
		defer C.free(unsafe.Pointer(cparams))
	}

	out := C.ilink_call(cop, cparams)
	if out == nil {
		return nil, &Error{Message: "ilink_call returned NULL", Kind: "unknown"}
	}
	defer C.ilink_string_free(out)

	var envelope struct {
		OK     bool            `json:"ok"`
		Result json.RawMessage `json:"result"`
		Error  string          `json:"error"`
		Kind   string          `json:"kind"`
	}
	if err := json.Unmarshal([]byte(C.GoString(out)), &envelope); err != nil {
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
