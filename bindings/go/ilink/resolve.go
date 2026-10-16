//go:build !ilink_cgo

package ilink

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// resolveLibrary finds the native library to load. The order is:
//
//  1. $ILINK_SDK_LIB — a full path, a directory containing the library, or a
//     bare library name for the OS loader;
//  2. a copy next to the running executable;
//  3. the copy embedded in this module for the current platform (if any);
//  4. a previously downloaded copy in the user cache;
//  5. a fresh download of the matching release asset into that cache (skipped
//     when ILINK_SDK_NO_DOWNLOAD is set);
//  6. the bare library name, resolved by the OS loader.
func resolveLibrary() (string, error) {
	name := libraryFileName(hostPlatform())

	if override := os.Getenv("ILINK_SDK_LIB"); override != "" {
		if resolved, ok := resolveOverride(override, name); ok {
			return resolved, nil
		}
	}

	if exe, err := os.Executable(); err == nil {
		candidate := filepath.Join(filepath.Dir(exe), name)
		if fileExists(candidate) {
			return candidate, nil
		}
	}

	if embedded, ok := extractEmbedded(name); ok {
		return embedded, nil
	}

	if cached, err := cachedLibrary(name); err == nil && fileExists(cached) {
		return cached, nil
	}

	if downloaded, err := downloadLibrary(name); err == nil && fileExists(downloaded) {
		return downloaded, nil
	}

	// Let the OS loader look on its own search path.
	return libraryName(), nil
}

// resolveOverride interprets $ILINK_SDK_LIB. It returns false when the value
// looks like a name we should hand to the OS loader instead.
func resolveOverride(value, fileName string) (string, bool) {
	// A bare name (no path separator) is a loader name, e.g. "ilink_sdk".
	if !strings.ContainsAny(value, `/\`) {
		// Still allow "ilink_sdk.dll" style names to be used verbatim.
		return value, true
	}
	info, err := os.Stat(value)
	if err != nil {
		return "", false
	}
	if info.IsDir() {
		candidate := filepath.Join(value, fileName)
		return candidate, fileExists(candidate)
	}
	return value, true
}

// libraryName is the base name the OS loader resolves.
func libraryName() string {
	switch runtime.GOOS {
	case "windows":
		return "ilink_sdk.dll"
	case "darwin":
		return "libilink_sdk.dylib"
	default:
		return "libilink_sdk.so"
	}
}

// libraryFileName is the file name of the shared library for a platform tag.
func libraryFileName(platform string) string {
	switch platform {
	case "windows-x86_64", "windows-aarch64":
		return "ilink_sdk.dll"
	case "macos-x86_64", "macos-aarch64":
		return "libilink_sdk.dylib"
	default:
		return "libilink_sdk.so"
	}
}

// hostPlatform maps the running OS/CPU to the release artifact name.
func hostPlatform() string {
	aarch64 := runtime.GOARCH == "arm64"
	switch runtime.GOOS {
	case "windows":
		if aarch64 {
			return "windows-aarch64"
		}
		return "windows-x86_64"
	case "darwin":
		if aarch64 {
			return "macos-aarch64"
		}
		return "macos-x86_64"
	default:
		if aarch64 {
			return "linux-aarch64"
		}
		return "linux-x86_64"
	}
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}
