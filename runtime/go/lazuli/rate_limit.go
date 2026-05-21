package lazuli

import "os"

// RateLimit is the env-qualified throttle policy emitted by codegen
// for each Command / Api / Agent / Report. Generated `*.gen.go`
// literals (`dist/go/<feature>/<feature>.gen.go`) populate `Default`
// from the unqualified `rate_limit "..."` source line and `ByEnv`
// from each `rate_limit "..." in <env_list>` override. The runtime
// resolves the active limit per request via `Resolve()`; the
// resolution is a single linear scan against `LAZULI_ENV`, no engine.
//
// Backward-compat: legacy single-line `rate_limit "X per Y per Z"`
// lowers to `RateLimit{Default: "X per Y per Z"}` (zero-length
// `ByEnv`); `Resolve()` returns `Default` regardless of env. Existing
// 0.15.0 fixtures emit this shape unchanged after the LZIR_SCHEMA
// bump (proposal §8).
//
// See `docs/proposals/ir-rate-limit-env-aware.md` §4.1 + §7.
type RateLimit struct {
	Default string
	ByEnv   []RateLimitByEnv
}

// RateLimitByEnv is one env-qualified override entry. `Envs` carries
// lowercase env names matching the closed catalog (`production`,
// `staging`, `test`, `dev`, `local`); `Limit` is the spec literal
// (empty string == unlimited; the proposal-defined `"unlimited"`
// keyword lowers to empty at parse time per §4.4).
type RateLimitByEnv struct {
	Envs  []string
	Limit string
}

// Resolve returns the rate-limit string active for the current
// `LAZULI_ENV`. Scans `ByEnv` linearly (catalog has at most 5
// entries; per-command lists are short) and falls back to `Default`
// when no env matches.
//
// Reads `os.Getenv("LAZULI_ENV")` per request — cheap enough not to
// cache, and lets test fixtures swap the env mid-run without
// re-instantiating handler chains. Empty `LAZULI_ENV` is treated as
// `"local"` (matching the existing CORS convention at
// `runtime/go/lazuli/http_cors.go:34`).
//
// Per RULE-VOCAB-03: zero branches on user input. The scan is a
// static equality match against the env value; no interpreter, no
// DSL evaluation.
func (r RateLimit) Resolve() string {
	env := os.Getenv("LAZULI_ENV")
	if env == "" {
		env = "local"
	}
	for _, entry := range r.ByEnv {
		for _, name := range entry.Envs {
			if name == env {
				return entry.Limit
			}
		}
	}
	return r.Default
}

// IsUnlimited reports whether the resolved limit disables throttling
// entirely (empty string == no limit). Backward-compat: the legacy
// shape `RateLimit{Default: ""}` reads as unlimited (it already did
// — the middleware skipped enforcement on empty strings before
// `ir-rate-limit-env-aware`).
func (r RateLimit) IsUnlimited() bool {
	return r.Resolve() == ""
}

// RateLimitFromDefault returns a `RateLimit` with only `Default`
// populated. Provided as a backward-compat shim for legacy callers
// that hand-constructed a string-typed `RateLimit` before the
// env-aware refactor.
//
// Generated code does not call this — codegen emits the struct
// literal directly per `format_rate_limit_struct` in
// `crates/lazuli_codegen_go/src/emitter/command.rs`.
func RateLimitFromDefault(s string) RateLimit {
	return RateLimit{Default: s}
}
