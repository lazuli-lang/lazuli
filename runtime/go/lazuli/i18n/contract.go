// Package i18n carries the typed locale + translation contract
// derived from the Lazuli IR. i18n bucket cycle — these types mirror
// `ir::AppLocale` / `ir::LocaleFallback`.
//
// Boundary discipline: this file declares contract types only.
// Concrete ICU message renderers, CLDR plural rule selection, and
// translation memory adapters (Lokalise / Crowdin / Phrase) live in
// sibling packages and adapter packs.
package i18n

import (
	"embed"
	"errors"
)

type Catalog struct {
	Name     string
	FS       embed.FS
	BasePath string
}

// LocaleContract is the lowered `app.locale` block from app.lzi.
type LocaleContract struct {
	Default   string
	Supported []string
	Fallbacks []Fallback
}

// Fallback declares an edge in the fallback graph: when `From` is
// requested but no translation resolves, walk to `To` before
// defaulting to `LocaleContract.Default`.
type Fallback struct {
	From string
	To   string
}

// IsSupported reports whether `tag` appears in the contract's
// supported list. Negotiation middleware uses this for membership
// checks before walking fallbacks.
func (c LocaleContract) IsSupported(tag string) bool {
	for _, s := range c.Supported {
		if s == tag {
			return true
		}
	}
	return false
}

// Resolve walks the fallback graph from `tag` until it hits a
// supported tag or the default. Cycles return `Default`.
func (c LocaleContract) Resolve(tag string) string {
	if c.IsSupported(tag) {
		return tag
	}
	seen := make(map[string]struct{})
	cur := tag
	for {
		if _, ok := seen[cur]; ok {
			return c.Default
		}
		seen[cur] = struct{}{}
		found := false
		for _, fb := range c.Fallbacks {
			if fb.From == cur {
				cur = fb.To
				found = true
				break
			}
		}
		if !found {
			break
		}
		if c.IsSupported(cur) {
			return cur
		}
	}
	return c.Default
}

// ErrLocaleNotSupported signals that the negotiated tag is not in
// `LocaleContract.Supported` and no fallback resolves either.
var ErrLocaleNotSupported = errors.New("lazuli/i18n: requested locale is not supported and no fallback resolves")

// ---------------------------------------------------------------------------
// Error vocabulary contract — IR Error-Vocab proposal §5.1.
//
// These types mirror the IR `FeatureErrors` shape (proposal §3.4) lowered to
// Go. Codegen emits literals; the runtime resolver reads them. No surface
// types here invoke ICU/CLDR/go-i18n; rendering stays a flat map lookup.
// ---------------------------------------------------------------------------

// ErrorExposureDefault is the default exposure for a feature's error
// envelopes — `hide` (catalog-driven 4xx/5xx) or `expose` (verbose,
// dev-mode-friendly). Mirrors `ir::ErrorExposureDefault`.
type ErrorExposureDefault uint8

const (
	// ExposureHide — default: the wire payload only carries fields
	// explicitly listed in `ExposeClient4xx` / `ExposeClient5xx`.
	ExposureHide ErrorExposureDefault = iota
	// ExposureExpose — default: the wire payload carries every field
	// unless an explicit deny list trims it. Matches dev-mode
	// behaviour for legacy features.
	ExposureExpose
)

// MessageRef is a fully-qualified reference to a translation key.
// `Feature` is the owning feature (`account`, `billing`, …);
// `Key` is the bare key name within that feature's catalog
// (`choose_role_signin_required`). The fully-qualified form
// `<feature>.<key>` is what the resolver looks up in catalogs.
type MessageRef struct {
	Feature string
	Key     string
}

// Qualified returns the fully-qualified key form `<feature>.<key>`,
// or just `<key>` when `Feature` is empty (built-in catalog keys).
func (r MessageRef) Qualified() string {
	if r.Feature == "" {
		return r.Key
	}
	return r.Feature + "." + r.Key
}

// IsZero reports whether the ref carries no key. Codegen emits zero
// values for codes that the author did not customize; the resolver
// then falls through to feature- or built-in-catalog defaults.
func (r MessageRef) IsZero() bool { return r.Key == "" }

// ErrorKeys is the per-command closed-catalog of message overrides.
// Codegen emits one literal per generated handler at construction
// time; the runtime resolver reads the field matching `Error.Code`
// during the L1 resolution step.
//
// Adding a new error code → add a new field here AND a matching entry
// in `keyForCode` (error_resolver.go).
//
// DB-INTEGRITY-CATALOG-EXT (2026-05-19): the four `*Violation` fields
// match the codes emitted by `classifyDBError` in the runtime. Authors
// override them with `errors unique_violation message @translation.<key>`
// (etc.) when the feature wants a domain-specific message; otherwise the
// builtin layperson catalog (L3) wins.
type ErrorKeys struct {
	PolicyDenied        MessageRef
	ValidationFailed    MessageRef
	TenantMismatch      MessageRef
	NotFound            MessageRef
	RateLimited         MessageRef
	BadRequest          MessageRef
	MethodNotAllowed    MessageRef
	IntegrationError    MessageRef
	UniqueViolation     MessageRef
	ForeignKeyViolation MessageRef
	NotNullViolation    MessageRef
	CheckViolation      MessageRef
}

// FeatureErrorContract is the lowered `feature.errors` block. Codegen
// emits one per feature that authored an `errors` block; the runtime
// reads it for exposure decisions (`shouldExpose`) and L2 message
// resolution.
type FeatureErrorContract struct {
	// Default controls the baseline exposure for unspecified fields.
	Default ErrorExposureDefault
	// ExposeClient4xx lists envelope fields exposed on 4xx responses.
	// Closed catalog: `message`, `code`, `data`, `message_key`.
	ExposeClient4xx []string
	// ExposeClient5xx lists envelope fields exposed on 5xx responses.
	// Closed catalog: `code`, `data`. `message` is forbidden on 5xx
	// (proposal §2.G — risks leaking internal state).
	ExposeClient5xx []string
	// Messages is the per-code override map. Key is the canonical
	// error code (`policy_denied`, …); value is the translation key
	// that wins when no per-command override fired.
	Messages map[string]MessageRef
}

// AppErrorResolverRegistry is the app-level rollup of every feature's
// error contract plus per-command overrides. Codegen emits a single
// literal at `dist/go/app/error_resolution.gen.go`; the runtime
// installs it at boot via `RegisterAppErrorResolver`.
type AppErrorResolverRegistry struct {
	// Features maps feature name → its lowered error contract. The
	// resolver looks up `SourceTag.Feature` to find the active
	// contract for an outgoing error.
	Features map[string]FeatureErrorContract
	// Commands maps `<feature>.<op>` → the per-command ErrorKeys
	// emitted by codegen. The resolver checks `<feature>.<op>`
	// (built from `SourceTag`) first; misses fall through to
	// `Features`.
	Commands map[string]ErrorKeys
	// PolicyDefaults maps `<feature>` → `<policy-name>` →
	// `MessageRef` for the named-policy `when_denied` layer. v1 leaves
	// this empty until parser/codegen support lands.
	PolicyDefaults map[string]map[string]MessageRef
}
