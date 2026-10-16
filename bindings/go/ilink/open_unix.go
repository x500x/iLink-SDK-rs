//go:build !ilink_cgo && !windows

package ilink

import "github.com/ebitengine/purego"

// openLibrary loads a shared library at an absolute path (or a name the OS
// loader can resolve) and returns its handle.
func openLibrary(path string) (uintptr, error) {
	return purego.Dlopen(path, purego.RTLD_NOW|purego.RTLD_GLOBAL)
}
