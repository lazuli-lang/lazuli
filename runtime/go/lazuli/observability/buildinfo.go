package observability

import (
	"encoding/json"
	"net/http"
	"runtime"
	"runtime/debug"
)

// BuildInfo is a JSON-safe snapshot of the Go build metadata embedded in the
// running binary.
type BuildInfo struct {
	// ModulePath is the path of the main module in the running binary.
	ModulePath string `json:"module_path"`
	// Version is the version of the main module in the running binary.
	Version string `json:"version"`
	// GoVersion is the Go toolchain version used to build the running binary.
	GoVersion string `json:"go_version"`
	// LazuliVersion is the Lazuli runtime version.
	LazuliVersion string `json:"lazuli_version"`
	// Settings are Go build settings such as GOOS, GOARCH, and VCS metadata.
	Settings []BuildInfoSetting `json:"settings"`
}

// BuildInfoSetting is a single Go build setting reported by runtime/debug.
type BuildInfoSetting struct {
	// Key is the setting name.
	Key string `json:"key"`
	// Value is the setting value.
	Value string `json:"value"`
}

// BuildInfoSnapshot returns build metadata for the running binary.
func BuildInfoSnapshot() BuildInfo {
	info, ok := debug.ReadBuildInfo()
	if !ok || info == nil {
		return BuildInfo{
			GoVersion:     runtime.Version(),
			LazuliVersion: lazuliVersion,
			Settings:      []BuildInfoSetting{},
		}
	}

	settings := make([]BuildInfoSetting, 0, len(info.Settings))
	for _, setting := range info.Settings {
		settings = append(settings, BuildInfoSetting{
			Key:   setting.Key,
			Value: setting.Value,
		})
	}

	return BuildInfo{
		ModulePath:    info.Main.Path,
		Version:       info.Main.Version,
		GoVersion:     info.GoVersion,
		LazuliVersion: lazuliVersion,
		Settings:      settings,
	}
}

// BuildInfoHandler returns an http.Handler that writes a JSON BuildInfo
// snapshot with HTTP 200.
func BuildInfoHandler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_ = json.NewEncoder(w).Encode(BuildInfoSnapshot())
	})
}
