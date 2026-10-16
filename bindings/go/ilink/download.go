//go:build !ilink_cgo

package ilink

import (
	"archive/zip"
	"bytes"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"time"
)

// downloadTimeout bounds the whole fetch so a slow or offline network cannot
// block startup for long. The download is a last resort: the module normally
// embeds the library, so this is only reached from a source checkout that was
// not staged.
const downloadTimeout = 20 * time.Second

// cachedLibrary is where a downloaded library for this platform lives.
func cachedLibrary(fileName string) (string, error) {
	dir, err := cacheDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, fileName), nil
}

// cacheDir returns a writable per-user directory for downloaded libraries.
func cacheDir() (string, error) {
	base, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	dir := filepath.Join(base, "ilink-sdk", libraryVersion())
	return dir, os.MkdirAll(dir, 0o755)
}

// downloadLibrary fetches the release archive for the current platform and
// extracts the shared library into the user cache. It is only reached when the
// module ships no embedded copy and nothing is cached yet. Set
// ILINK_SDK_NO_DOWNLOAD=1 to skip it and fall back to the OS loader instead.
func downloadLibrary(fileName string) (string, error) {
	if os.Getenv("ILINK_SDK_NO_DOWNLOAD") != "" {
		return "", fmt.Errorf("downloads disabled by ILINK_SDK_NO_DOWNLOAD")
	}

	dir, err := cacheDir()
	if err != nil {
		return "", err
	}

	url := fmt.Sprintf(
		"https://github.com/%s/releases/download/v%s/ilink-sdk-%s.zip",
		repoSlug, libraryVersion(), hostPlatform(),
	)

	client := &http.Client{Timeout: downloadTimeout}
	resp, err := client.Get(url)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("downloading %s: unexpected status %s", url, resp.Status)
	}

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}

	reader, err := zip.NewReader(bytes.NewReader(body), int64(len(body)))
	if err != nil {
		return "", err
	}
	for _, entry := range reader.File {
		if filepath.Base(entry.Name) != fileName {
			continue
		}
		src, err := entry.Open()
		if err != nil {
			return "", err
		}
		data, err := io.ReadAll(src)
		src.Close()
		if err != nil {
			return "", err
		}

		// Write to a temporary file, then rename so a partial download never
		// looks like a valid cached library.
		target := filepath.Join(dir, fileName)
		tmp, err := os.CreateTemp(dir, fileName+".tmp-*")
		if err != nil {
			return "", err
		}
		if _, err := tmp.Write(data); err != nil {
			tmp.Close()
			os.Remove(tmp.Name())
			return "", err
		}
		tmp.Close()
		if err := os.Chmod(tmp.Name(), 0o755); err != nil {
			os.Remove(tmp.Name())
			return "", err
		}
		if err := os.Rename(tmp.Name(), target); err != nil {
			os.Remove(tmp.Name())
			return "", err
		}
		return target, nil
	}
	return "", fmt.Errorf("%s does not contain %s", url, fileName)
}
