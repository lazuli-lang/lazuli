// Package i18n carries the typed locale + translation contract
// derived from the Lazuli IR. i18n bucket cycle — these types mirror
// `ir::AppLocale` / `ir::LocaleFallback`.
//
// Boundary discipline: this file declares contract types only.
// Concrete ICU message renderers, CLDR plural rule selection, and
// translation memory adapters (Lokalise / Crowdin / Phrase) live in
// sibling packages and adapter packs.
package i18n

import "errors"

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
