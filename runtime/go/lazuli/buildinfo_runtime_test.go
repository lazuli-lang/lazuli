package lazuli

import (
	"encoding/json"
	"testing"
)

func TestBuildInfoReturnsExpectedShape(t *testing.T) {
	info := BuildInfo()

	if info.Version != LazuliGoVersion {
		t.Fatalf("Version = %q, want %q", info.Version, LazuliGoVersion)
	}
	if info.Commit != LazuliCommit {
		t.Fatalf("Commit = %q, want %q", info.Commit, LazuliCommit)
	}
	if info.BuildTime != LazuliBuildTime {
		t.Fatalf("BuildTime = %q, want %q", info.BuildTime, LazuliBuildTime)
	}

	raw, err := json.Marshal(info)
	if err != nil {
		t.Fatalf("marshal BuildInfoData: %v", err)
	}

	var fields map[string]string
	if err := json.Unmarshal(raw, &fields); err != nil {
		t.Fatalf("decode BuildInfoData JSON: %v", err)
	}

	expected := map[string]string{
		"version":    LazuliGoVersion,
		"commit":     LazuliCommit,
		"build_time": LazuliBuildTime,
	}
	if len(fields) != len(expected) {
		t.Fatalf("fields = %v, want exactly %v", fields, expected)
	}
	for key, want := range expected {
		if got, ok := fields[key]; !ok || got != want {
			t.Fatalf("field %q = %q, present %v; want %q", key, got, ok, want)
		}
	}
}
