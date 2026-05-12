package observability

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"runtime"
	"testing"
)

func TestBuildInfoSnapshotReturnsRuntimeBuildInfo(t *testing.T) {
	snapshot := BuildInfoSnapshot()

	if snapshot.ModulePath == "" {
		t.Fatal("module_path is empty")
	}
	if snapshot.GoVersion != runtime.Version() {
		t.Fatalf("go_version = %q, want %q", snapshot.GoVersion, runtime.Version())
	}
	if snapshot.LazuliVersion != lazuliVersion {
		t.Fatalf("lazuli_version = %q, want %q", snapshot.LazuliVersion, lazuliVersion)
	}
	if snapshot.Settings == nil {
		t.Fatal("settings is nil, want an empty or populated slice")
	}
	for _, setting := range snapshot.Settings {
		if setting.Key == "" {
			t.Fatalf("setting with empty key: %#v", setting)
		}
	}
}

func TestBuildInfoHandlerWritesJSONSnapshot(t *testing.T) {
	rec := httptest.NewRecorder()
	BuildInfoHandler().ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/debug/buildinfo", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}

	var raw map[string]json.RawMessage
	if err := json.Unmarshal(rec.Body.Bytes(), &raw); err != nil {
		t.Fatalf("decode raw response: %v", err)
	}

	expectedKeys := []string{
		"module_path",
		"version",
		"go_version",
		"lazuli_version",
		"settings",
	}
	if len(raw) != len(expectedKeys) {
		t.Fatalf("keys = %v, want exactly %v", raw, expectedKeys)
	}
	for _, key := range expectedKeys {
		if _, ok := raw[key]; !ok {
			t.Fatalf("missing response key %q in %v", key, raw)
		}
	}

	var snapshot BuildInfo
	if err := json.Unmarshal(rec.Body.Bytes(), &snapshot); err != nil {
		t.Fatalf("decode snapshot: %v", err)
	}
	if snapshot.ModulePath == "" {
		t.Fatal("module_path is empty")
	}
	if snapshot.GoVersion != runtime.Version() {
		t.Fatalf("go_version = %q, want %q", snapshot.GoVersion, runtime.Version())
	}
	if snapshot.LazuliVersion != lazuliVersion {
		t.Fatalf("lazuli_version = %q, want %q", snapshot.LazuliVersion, lazuliVersion)
	}
	if snapshot.Settings == nil {
		t.Fatal("settings is nil, want an empty or populated slice")
	}
}
