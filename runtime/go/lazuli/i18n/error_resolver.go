// Package i18n — error message resolver.
//
// The resolver renders the human-readable string for an outgoing error
// envelope by walking the four-layer chain documented in
// `docs/proposals/ir-error-messages-vocab.md` §2.E:
//
//  L1. command-level override (per-command `policy_when_denied` etc.) —
//      caller supplies the already-resolved `MessageKey` on the error.
//  L2. feature-level catch-all (`feature.errors.<code>.message`) —
//      looked up via the active feature's contract.
//  L3. built-in catalog at <locale>.
//  L4. built-in catalog at en-US (last-resort floor).
//
// The resolver is wire-thin: catalogs are `map[string]string`. No
// ICU/CLDR/go-i18n in this file — adapters wrap the resolver if they
// need fancier rendering. Boundary discipline per
// `bucket-i18n-cycle.md` §Contexto.
package i18n

import (
	"strings"
)

// ErrorRequest is the input shape the resolver needs. The HTTP
// boundary builds one of these from `*lazuli.Error` plus context.
// Keeping the resolver de-coupled from the `lazuli` package avoids
// a circular import (lazuli → i18n for resolution, i18n → lazuli for
// the Error type).
type ErrorRequest struct {
	// Code is the canonical error code (`policy_denied`, …). Used for
	// L2 feature lookup and L3/L4 builtin lookup.
	Code string
	// MessageKey is the L1 pre-resolved key set by codegen (or by a
	// runtime producer like `policy.go`). Empty when no per-command
	// override fired.
	MessageKey string
	// Feature is the owning feature, read from `SourceTag.Feature` on
	// the request context. Used to look up the active
	// `FeatureErrorContract` for L2 resolution.
	Feature string
	// Locale is the negotiated locale tag (`pt-BR`, `en-US`, …),
	// already resolved against the supported list by the negotiation
	// middleware. The resolver does not re-negotiate.
	Locale string
}

// ErrorResolver renders the final error message string.
type ErrorResolver interface {
	// Resolve walks the four-layer chain and returns the final string
	// plus the key that fired (for logs / wire `message_key`).
	// Returns ("", "") when no catalog covers the code in any locale;
	// caller falls back to its own literal.
	Resolve(req ErrorRequest) (text string, firedKey string)
}

// DefaultResolver implements the four-layer chain over flat
// `map[string]string` catalogs.
type DefaultResolver struct {
	// Registry carries per-feature contracts and per-command keys
	// emitted by codegen. May be zero — the resolver still falls
	// through to the built-in catalog.
	Registry AppErrorResolverRegistry
	// Catalogs is `locale → key → text`. Both feature catalogs and
	// the built-in catalog merge into this single map at boot.
	// Resolver does not own the loader — boot wiring populates it.
	Catalogs map[string]map[string]string
	// FallbackLocale is the last-resort locale walked when the
	// requested locale has no entry. Conventionally `en-US`.
	FallbackLocale string
}

// Resolve implements ErrorResolver. See package doc for the chain.
func (r *DefaultResolver) Resolve(req ErrorRequest) (string, string) {
	// L1 — per-command override key already on the error.
	if req.MessageKey != "" {
		if text, ok := r.lookup(req.MessageKey, req.Locale); ok {
			return text, req.MessageKey
		}
	}
	// L2 — feature-level catch-all by error code.
	if req.Feature != "" && req.Code != "" {
		if contract, ok := r.Registry.Features[req.Feature]; ok {
			if ref, ok := contract.Messages[req.Code]; ok && !ref.IsZero() {
				key := ref.Qualified()
				if text, ok := r.lookup(key, req.Locale); ok {
					return text, key
				}
			}
		}
	}
	// L3 — built-in catalog at the requested locale.
	if req.Code != "" {
		builtinKey := "builtin." + req.Code
		if text, ok := r.lookup(builtinKey, req.Locale); ok {
			return text, builtinKey
		}
		// L4 — built-in catalog at the fallback locale.
		if r.FallbackLocale != "" && !strings.EqualFold(r.FallbackLocale, req.Locale) {
			if text, ok := r.lookup(builtinKey, r.FallbackLocale); ok {
				return text, builtinKey
			}
		}
	}
	return "", ""
}

// lookup returns the catalog entry for (key, locale). Returns ("", false)
// on miss.
func (r *DefaultResolver) lookup(key, locale string) (string, bool) {
	if locale == "" {
		return "", false
	}
	catalog, ok := r.Catalogs[locale]
	if !ok {
		return "", false
	}
	text, ok := catalog[key]
	return text, ok
}

// defaultResolver is the process-global resolver substituted at the
// HTTP boundary. Boot wiring installs the catalog-merged variant via
// SetDefaultResolver; tests reset it via SetDefaultResolver(nil).
var defaultResolver ErrorResolver = &DefaultResolver{}

// SetDefaultResolver installs the process-global resolver. Passing nil
// reinstalls the empty default (no catalogs — every Resolve returns
// the zero pair so callers fall back to literal Error.Message).
func SetDefaultResolver(r ErrorResolver) {
	if r == nil {
		defaultResolver = &DefaultResolver{}
		return
	}
	defaultResolver = r
}

// Default returns the process-global resolver. The HTTP boundary
// (lazuli.writeError) consults this on every error response.
func Default() ErrorResolver { return defaultResolver }
