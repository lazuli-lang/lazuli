package lazuli

// LazuliGoVersion mirrors `runtime/go/VERSION` (the source of truth
// gated by `crates/lazuli_codegen_go/tests/version_pin.rs`).
const LazuliGoVersion = "v0.1.0"

// LazuliCommit is filled at build time via -ldflags
// "-X lazuli.dev/runtime/lazuli.LazuliCommit=<hash>".
// Falls back to "unknown" for dev builds.
var LazuliCommit = "unknown"

// LazuliBuildTime is filled at build time the same way.
var LazuliBuildTime = "unknown"

// BuildInfo returns a struct surfaced via /healthz "version" field.
func BuildInfo() BuildInfoData {
	return BuildInfoData{
		Version:   LazuliGoVersion,
		Commit:    LazuliCommit,
		BuildTime: LazuliBuildTime,
	}
}

type BuildInfoData struct {
	Version   string `json:"version"`
	Commit    string `json:"commit"`
	BuildTime string `json:"build_time"`
}
