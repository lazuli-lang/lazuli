// Package i18n — built-in error catalog loader.
//
// Cell RUNTIME-2 (proposal §11) ships the framework's opinionated
// PT-BR + en-US catalog so a freshly-scaffolded app with zero
// error-vocab authoring still emits a human-readable native message
// on every closed-catalog error code. Without this catalog, layer 3
// of the resolver chain (§2.E) is dark and the runtime falls all the
// way through to the empty pair — restoring the pre-RUNTIME-1 wire
// jargon ("no policy atom matches…"). That regression is the
// hostpoint field incident this whole vocab cycle exists to close.
//
// Format: flat `key → text` JSON, one file per locale, embedded via
// `embed.FS` so the runtime binary carries the strings without a
// filesystem dependency. The JSON keys are the bare error codes
// (`policy_denied`, …); callers that merge into the resolver's
// `Catalogs` (locale → key → text) prepend the `builtin.` namespace —
// the resolver's L3 lookup at `error_resolver.go:93` already does
// that prefixing on its end.
//
// Adapters may ship `builtin.<locale>.json` for additional locales
// (`@plugin/lazuli-i18n-locales-fr`, …) via the adapter contribution
// hook in §5.5 — out of v1 scope; this file ships only the two
// locales the framework guarantees.
package i18n

import (
	"embed"
	"encoding/json"
	"fmt"
)

//go:embed builtin.pt-BR.json builtin.en-US.json
var builtinFS embed.FS

// builtinCatalogs is the parsed, cached form. Populated once by
// `init` from the embedded JSON. Keyed by locale tag (`pt-BR`,
// `en-US`). Each inner map is `code → text` — the catalog is
// returned to callers as a defensive copy so external mutation
// cannot corrupt the cached singleton.
var builtinCatalogs = map[string]map[string]string{}

// builtinFiles enumerates the locale → embedded-filename pairs. New
// locales arrive as adapter contributions (out of v1 scope); the
// core runtime ships only the two locales the framework guarantees.
var builtinFiles = map[string]string{
	"pt-BR": "builtin.pt-BR.json",
	"en-US": "builtin.en-US.json",
}

func init() {
	for locale, name := range builtinFiles {
		bytes, err := builtinFS.ReadFile(name)
		if err != nil {
			// Panic at package init: the file is embedded at compile
			// time, so any read error is a build-time bug, not a
			// runtime condition.
			panic(fmt.Sprintf("lazuli/i18n: builtin catalog %q missing from embed.FS: %v", name, err))
		}
		parsed := map[string]string{}
		if err := json.Unmarshal(bytes, &parsed); err != nil {
			panic(fmt.Sprintf("lazuli/i18n: builtin catalog %q is malformed JSON: %v", name, err))
		}
		builtinCatalogs[locale] = parsed
	}
}

// BuiltinCatalog returns the framework's built-in catalog for the
// requested locale as a flat `code → text` map. Returns nil when the
// locale isn't shipped — callers fall back to en-US per the
// resolver's layer-4 walk (`error_resolver.go:97`).
//
// The returned map is a fresh copy of the cached singleton; callers
// may mutate it freely (e.g. to merge feature catalogs on top of the
// built-in floor before installing into `DefaultResolver.Catalogs`).
func BuiltinCatalog(locale string) map[string]string {
	cached, ok := builtinCatalogs[locale]
	if !ok {
		return nil
	}
	copied := make(map[string]string, len(cached))
	for k, v := range cached {
		copied[k] = v
	}
	return copied
}

// BuiltinLocales returns the list of locale tags the framework ships
// catalogs for. Used by the boot wiring to seed `DefaultResolver
// .Catalogs` with the `builtin.` namespace populated under every
// shipped locale.
func BuiltinLocales() []string {
	out := make([]string, 0, len(builtinCatalogs))
	for locale := range builtinCatalogs {
		out = append(out, locale)
	}
	return out
}
