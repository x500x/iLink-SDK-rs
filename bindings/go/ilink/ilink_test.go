package ilink

import (
	"encoding/json"
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
	const key = "00112233445566778899aabbccddeeff"
	var enc struct {
		DataBase64 string `json:"data_base64"`
	}
	if err := CallInto("crypto.encrypt", map[string]any{"key_hex": key, "data_base64": "aGVsbG8gd29ybGQ="}, &enc); err != nil {
		t.Fatalf("encrypt: %v", err)
	}
	var dec struct {
		DataBase64 string `json:"data_base64"`
	}
	if err := CallInto("crypto.decrypt", map[string]any{"key_hex": key, "data_base64": enc.DataBase64}, &dec); err != nil {
		t.Fatalf("decrypt: %v", err)
	}
	if dec.DataBase64 != "aGVsbG8gd29ybGQ=" {
		t.Fatalf("round trip mismatch: %s", dec.DataBase64)
	}
}

func TestClientMergesRouting(t *testing.T) {
	client := NewClient(Config{AccountID: "demo"})
	raw, err := client.Call("session.status", nil)
	if err != nil {
		t.Fatalf("session.status: %v", err)
	}
	var status struct {
		Paused bool `json:"paused"`
	}
	if err := json.Unmarshal(raw, &status); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if status.Paused {
		t.Fatal("expected not paused")
	}
}
