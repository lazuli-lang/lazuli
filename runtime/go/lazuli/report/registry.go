// Auto-mount registry for `report <name>` declarations.
//
// Generated `<feature>/reports.gen.go` `init()` calls Register() to
// declare each contract; `report.Mount(mux)` walks the registry and
// attaches `GET /api/reports/<name>.<format>` routes for every
// (contract × format) pair.
//
// Runner is the runtime hook the route handler invokes per request.
// Generated code emits a stub runner returning a clear "wire source +
// store" error; user code overrides the per-report `Runner` variable
// once the SourceFn / ObjectStore are wired (typically in `func main`
// or a feature-local bootstrap helper). The closure form means the
// registry stays decoupled from concrete sources — wire-thin.

package report

import (
	"context"
	"fmt"
	"sync"
)

// Runner executes a registered report and returns the resolved URL
// (signed, public, or private). The HTTP handler returned by Mount()
// reads `Visibility` off the contract to decide how to surface the URL
// (302 vs. JSON envelope).
type Runner func(ctx context.Context, format Format) (string, error)

// Registration pairs a Contract with its Runner. Mount() iterates a
// snapshot to mount routes deterministically.
type Registration struct {
	Contract Contract
	Runner   Runner
}

var registryMu sync.RWMutex
var registry = make(map[string]Registration)

// Register adds a (contract, runner) entry under contract.Name. Panics
// on duplicate name — the doctor `REPORT-PATH-COLLISION-001` rule
// already enforces uniqueness at compile time; a runtime duplicate
// means generated code drifted from authored sources.
func Register(c Contract, runner Runner) {
	registryMu.Lock()
	defer registryMu.Unlock()
	if _, exists := registry[c.Name]; exists {
		panic(fmt.Sprintf("lazuli/report: contract %q registered twice", c.Name))
	}
	if runner == nil {
		runner = stubRunner(c.Name)
	}
	registry[c.Name] = Registration{Contract: c, Runner: runner}
}

// SetRunner replaces the runner for a previously registered contract.
// User code calls this once source + store are wired (e.g. inside
// `func main` after `lazuli.Boot`). Returns false when the contract
// is not registered.
func SetRunner(name string, runner Runner) bool {
	registryMu.Lock()
	defer registryMu.Unlock()
	reg, ok := registry[name]
	if !ok {
		return false
	}
	reg.Runner = runner
	registry[name] = reg
	return true
}

// Registered returns a stable snapshot of every registered report. The
// Mount() helper consumes this; tests inspect it.
func Registered() []Registration {
	registryMu.RLock()
	defer registryMu.RUnlock()
	out := make([]Registration, 0, len(registry))
	for _, reg := range registry {
		out = append(out, reg)
	}
	return out
}

// stubRunner is the default runner installed when generated code
// registers without a wired source/store. Returns a clear actionable
// error so the HTTP 501 envelope guides the user to the fix.
func stubRunner(name string) Runner {
	return func(_ context.Context, _ Format) (string, error) {
		return "", ErrRunnerNotWired(name)
	}
}

// ErrRunnerNotWired returns the canonical error used when the
// auto-mounted route is invoked before the user has overridden the
// per-report `<Name>Runner` variable in the generated package.
// Generated `reports.gen.go` references this so the error envelope
// stays consistent across features.
func ErrRunnerNotWired(name string) error {
	return fmt.Errorf(
		"lazuli/report: %q has no runner wired — set `<feature>.%sRunner = func(ctx, format) { return <feature>.Run%s(ctx, format, source, store) }` from main",
		name, toPascal(name), toPascal(name),
	)
}

// toPascal converts a snake_case identifier to PascalCase for the
// user-facing error envelope. Mirrors the codegen casing so the hint
// matches the generated identifier exactly.
func toPascal(s string) string {
	var out []byte
	upNext := true
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c == '_' || c == '-' {
			upNext = true
			continue
		}
		if upNext && c >= 'a' && c <= 'z' {
			c -= 32
		}
		out = append(out, c)
		upNext = false
	}
	return string(out)
}

// Reset clears the registry. Test-only; production code must not call.
func Reset() {
	registryMu.Lock()
	defer registryMu.Unlock()
	registry = make(map[string]Registration)
}
