//go:build !ilink_cgo && windows

package ilink

import "syscall"

// openLibrary loads a DLL at an absolute path (or a name the Windows loader can
// resolve) and returns its handle. Uses the standard library, so the Windows
// build needs no third-party dependency at all.
func openLibrary(path string) (uintptr, error) {
	handle, err := syscall.LoadLibrary(path)
	return uintptr(handle), err
}
