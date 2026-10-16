//go:build ilink_cgo

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

import "unsafe"

// This file is the opt-in cgo path (build tag `ilink_cgo`). It links against
// the shared library at target/release/ of a source checkout, which requires a
// C toolchain. The default, cgo-free build lives in native.go and friends.

// ensureLoaded is a no-op: cgo links the library at build time.
func ensureLoaded() error { return nil }

func nativeVersion() *byte {
	return (*byte)(unsafe.Pointer(C.ilink_version()))
}

func nativeCall(op, paramsJSON string) *byte {
	cop := C.CString(op)
	defer C.free(unsafe.Pointer(cop))

	var cparams *C.char
	if paramsJSON != "" {
		cparams = C.CString(paramsJSON)
		defer C.free(unsafe.Pointer(cparams))
	}
	return (*byte)(unsafe.Pointer(C.ilink_call(cop, cparams)))
}

func nativeFree(ptr *byte) {
	C.ilink_string_free((*C.char)(unsafe.Pointer(ptr)))
}
