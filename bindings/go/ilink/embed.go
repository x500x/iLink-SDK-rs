//go:build !ilink_cgo

package ilink

import (
	"embed"
	"io/fs"
	"os"
	"path/filepath"
)

// nativeFiles holds the native libraries staged into native/ by
// bindings/scripts/stage_native.py (and by CI before a release). When the
// directory only contains its placeholder, extraction finds nothing and the
// resolver falls back to the cache or a download.
//
//go:embed all:native
var nativeFiles embed.FS

// extractEmbedded writes the embedded library for the current platform into the
// user cache (once per version) and returns its path. It reports false when no
// such library was embedded.
func extractEmbedded(fileName string) (string, bool) {
	resource := "native/" + hostPlatform() + "/" + fileName
	data, err := fs.ReadFile(nativeFiles, resource)
	if err != nil || len(data) == 0 {
		return "", false
	}

	target, err := cachedLibrary(fileName)
	if err != nil {
		// Fall back to a temporary file when there is no writable cache dir.
		target, err = writeTemp(fileName, data)
		if err != nil {
			return "", false
		}
		return target, true
	}

	// Reuse an existing extraction of the same size; otherwise (re)write it.
	if info, err := os.Stat(target); err == nil && info.Size() == int64(len(data)) {
		return target, true
	}
	if err := writeFile(target, data); err != nil {
		return "", false
	}
	return target, true
}

// writeTemp writes data to a fresh temporary file and returns its path.
func writeTemp(fileName string, data []byte) (string, error) {
	dir, err := os.MkdirTemp("", "ilink-sdk-")
	if err != nil {
		return "", err
	}
	target := filepath.Join(dir, fileName)
	if err := writeFile(target, data); err != nil {
		os.RemoveAll(dir)
		return "", err
	}
	return target, nil
}

// writeFile writes data atomically so a concurrent reader never sees a partial
// library.
func writeFile(target string, data []byte) error {
	tmp, err := os.CreateTemp(filepath.Dir(target), filepath.Base(target)+".tmp-*")
	if err != nil {
		return err
	}
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		os.Remove(tmp.Name())
		return err
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmp.Name())
		return err
	}
	if err := os.Chmod(tmp.Name(), 0o755); err != nil {
		os.Remove(tmp.Name())
		return err
	}
	return os.Rename(tmp.Name(), target)
}
