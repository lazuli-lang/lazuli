// Package i18n — per-feature translation catalog loader (Wave 3.5).
//
// Closes the gap in the proposal's §2.E resolution chain L1/L2 where
// per-feature `translation` blocks were authored but never reached the
// resolver's `Catalogs` map. Codegen now emits one `init()` per feature
// that calls `RegisterFeatureTranslationCatalog`; this loader walks the
// supplied embed.FS for `<basePath>/<feature>.<locale>.json` files,
// qualifies bare keys as `<feature>.<bare_key>`, and merges them into
// the process-global default resolver under each locale.
//
// Boundary discipline (`bucket-i18n-cycle.md` §Contexto): wire-thin,
// flat `map[string]string`, no ICU/CLDR/go-i18n. Adapter packs that
// need fancier rendering wrap the resolver — this file just delivers
// authored text to the resolver's L1/L2 lookups.
package i18n

import (
	"embed"
	"encoding/json"
	"fmt"
	"path"
	"strings"
	"sync"
)

// feature_catalog_v1 — re-registering the same feature replaces its
// prior entries so codegen-driven inits are idempotent across boot
// (e.g. a test that reruns init() after SetDefaultResolver(nil)).
var featureCatalogMu sync.Mutex

// RegisterFeatureTranslationCatalog loads every <basePath>/<feature>.<locale>.json
// file from `fs` and merges its entries into the process-global
// default resolver's `Catalogs` map under qualified keys
// (`<feature>.<bare_key>`). Codegen emits one `init()` per feature
// with a `translation` block that calls this function; the runtime
// then has authored text available for the resolver chain's L1
// (per-command MessageKey) and L2 (FeatureErrors.Messages) layers.
//
// Catalog naming convention: `<basePath>/<feature>.<locale>.json` —
// e.g. `i18n/account.pt-BR.json`. Files in `basePath` whose names do
// not match the pattern are ignored without error so adapters can
// layer their own files (e.g. `account.regional.json`) into the same
// directory.
//
// JSON content: flat `{"<bare_key>": "<localized_text>", ...}`. Bare
// keys get qualified to `<feature>.<bare_key>` before insertion into
// `r.Catalogs[locale]` (matching `MessageRef.Qualified()`).
//
// Thread-safe: holds a process-global mutex while merging into the
// resolver. The merge is O(locales × keys) but only fires at process
// boot, so the cost is negligible. Returns an error on malformed JSON
// or non-`*DefaultResolver` resolvers; codegen surfaces the error via
// `panic` in the emitted init() so build-time misconfiguration fails
// loudly.
func RegisterFeatureTranslationCatalog(feature string, fs embed.FS, basePath string) error {
	if feature == "" {
		return fmt.Errorf("lazuli/i18n: RegisterFeatureTranslationCatalog: empty feature name")
	}
	prefix := feature + "."
	suffix := ".json"

	entries, err := fs.ReadDir(basePath)
	if err != nil {
		return fmt.Errorf("lazuli/i18n: read embed dir %q for feature %q: %w", basePath, feature, err)
	}

	// Discover locale → parsed map for every file that matches the
	// `<feature>.<locale>.json` naming convention. Files that don't
	// match are skipped silently — they belong to adapters.
	parsed := map[string]map[string]string{}
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		if !strings.HasPrefix(name, prefix) || !strings.HasSuffix(name, suffix) {
			continue
		}
		locale := strings.TrimSuffix(strings.TrimPrefix(name, prefix), suffix)
		if locale == "" {
			continue
		}
		bytes, err := fs.ReadFile(path.Join(basePath, name))
		if err != nil {
			return fmt.Errorf("lazuli/i18n: read %q for feature %q: %w", name, feature, err)
		}
		catalog := map[string]string{}
		if err := json.Unmarshal(bytes, &catalog); err != nil {
			return fmt.Errorf("lazuli/i18n: parse %q for feature %q: %w", name, feature, err)
		}
		parsed[locale] = catalog
	}

	if len(parsed) == 0 {
		return nil
	}

	featureCatalogMu.Lock()
	defer featureCatalogMu.Unlock()

	r, ok := defaultResolver.(*DefaultResolver)
	if !ok {
		return fmt.Errorf("lazuli/i18n: default resolver is %T, want *DefaultResolver", defaultResolver)
	}
	if r.Catalogs == nil {
		r.Catalogs = map[string]map[string]string{}
	}
	for locale, catalog := range parsed {
		bucket, ok := r.Catalogs[locale]
		if !ok {
			bucket = map[string]string{}
			r.Catalogs[locale] = bucket
		}
		// First drop any prior entries this feature owned under this
		// locale so re-registering replaces rather than accumulates.
		for key := range bucket {
			if strings.HasPrefix(key, prefix) {
				delete(bucket, key)
			}
		}
		for bareKey, text := range catalog {
			bucket[prefix+bareKey] = text
		}
	}
	return nil
}
