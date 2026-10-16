package ilink

import (
	"strings"
	"testing"
)

func TestVersion(t *testing.T) {
	version, err := Version()
	if err != nil {
		t.Fatalf("Version: %v", err)
	}
	if !strings.Contains(version, ".") {
		t.Fatalf("unexpected version %q", version)
	}
}

func TestCallPureHelper(t *testing.T) {
	var info struct {
		Mime    string `json:"mime"`
		IsImage bool   `json:"is_image"`
	}
	if err := CallInto("mime.info", map[string]any{"path": "photo.PNG"}, &info); err != nil {
		t.Fatalf("mime.info: %v", err)
	}
	if info.Mime != "image/png" || !info.IsImage {
		t.Fatalf("unexpected info: %+v", info)
	}
}

func TestClientMimeInfo(t *testing.T) {
	client := NewClient(Config{})
	info, err := client.MimeInfo("photo.PNG")
	if err != nil {
		t.Fatalf("MimeInfo: %v", err)
	}
	if info.Mime != "image/png" || !info.IsImage || info.Extension != ".png" {
		t.Fatalf("unexpected info: %+v", info)
	}
}

func TestUnknownOpReturnsKind(t *testing.T) {
	_, err := Call("does.not.exist", nil)
	if err == nil {
		t.Fatal("expected an error")
	}
	apiErr, ok := err.(*Error)
	if !ok {
		t.Fatalf("expected *Error, got %T", err)
	}
	if apiErr.Kind != "unknown" || !strings.Contains(apiErr.Message, "unknown op") {
		t.Fatalf("unexpected error: %+v", apiErr)
	}
}

func TestCryptoRoundTrip(t *testing.T) {
	client := NewClient(Config{})
	const key = "00112233445566778899aabbccddeeff"

	enc, err := client.CryptoEncrypt(key, []byte("hello world"))
	if err != nil {
		t.Fatalf("CryptoEncrypt: %v", err)
	}
	dec, err := client.CryptoDecrypt(key, enc.DataBase64)
	if err != nil {
		t.Fatalf("CryptoDecrypt: %v", err)
	}
	if string(dec) != "hello world" {
		t.Fatalf("round trip mismatch: %q", dec)
	}
}

func TestClientMergesRouting(t *testing.T) {
	client := NewClient(Config{AccountID: "demo"})
	status, err := client.SessionStatus("")
	if err != nil {
		t.Fatalf("SessionStatus: %v", err)
	}
	if status.Paused {
		t.Fatal("expected not paused")
	}
}
