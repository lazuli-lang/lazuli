package lazuli

import (
	"sync"

	"github.com/microcosm-cc/bluemonday"
)

// HTML sanitization — wire-thin glue over `bluemonday`, the canonical
// Go HTML sanitizer. A field declared `validate sanitize_html(<profile>)`
// in the DSL lowers to `FieldConstraints.sanitize_html` in the IR; codegen
// emits the column→profile mapping into `Resource.SanitizeColumns`; the
// runtime walks that map at the write boundary (`applyCreates` /
// `applyUpdates`, see `sanitize_wire.go`) and rewrites each bound string
// value through the matching bluemonday policy BEFORE it reaches the
// driver. The stored column therefore never holds attacker-controlled
// markup — closing the stored-XSS hole the analyzer used to lower into a
// no-op.
//
// Founding wire-thin principle: zero HTML parsing knowledge lives here.
// bluemonday owns tokenization, tag/attribute allow-listing, URL scheme
// vetting, and entity handling. This file is the policy registry + a
// single `Sanitize` entry point.

// SanitizeHTMLProfile is the closed catalog of sanitization presets,
// mirroring the IR `SanitizeHtmlProfile` enum (serde snake_case). The
// string values are exactly what codegen emits into
// `Resource.SanitizeColumns` so the runtime can resolve a policy from
// the column metadata.
type SanitizeHTMLProfile string

const (
	// SanitizeStrict strips ALL tags, leaving plain text. Maps to
	// `bluemonday.StrictPolicy()`. Use for fields that briefly accept
	// rich input from a rich-text editor but persist as plain text.
	SanitizeStrict SanitizeHTMLProfile = "strict"

	// SanitizeBasic allows a small set of safe inline / formatting tags
	// (`b`, `i`, `em`, `strong`, `a[href]`, `br`, `p`, `span`) and
	// strips everything dangerous (script/style/iframe/object/embed,
	// event-handler attributes, `javascript:` URLs). Built on
	// bluemonday's hardened UGC policy, narrowed to the inline subset
	// the IR doc-comment advertises.
	SanitizeBasic SanitizeHTMLProfile = "basic"

	// SanitizeMarkdownSafe is appropriate for HTML produced by a
	// trusted markdown renderer: it keeps the structural tags markdown
	// emits (headings, lists, blockquote, code/pre, tables, links,
	// images) while still stripping script/style/iframe and unsafe URL
	// schemes. Built on bluemonday's `UGCPolicy()` (the library's
	// documented "user-generated content / rendered markdown" preset).
	SanitizeMarkdownSafe SanitizeHTMLProfile = "markdown_safe"
)

// policyRegistry lazily builds + caches one *bluemonday.Policy per
// profile. bluemonday policies are immutable + goroutine-safe once
// constructed (the library documents `Sanitize` as concurrency-safe),
// so a single cached instance per profile is reused across requests.
var (
	policyOnce     sync.Once
	policyByName   map[SanitizeHTMLProfile]*bluemonday.Policy
	policyInitFail string
)

func buildPolicies() {
	policyByName = map[SanitizeHTMLProfile]*bluemonday.Policy{
		// strict — strip every tag, decode entities. StrictPolicy is
		// bluemonday's "allow nothing" preset.
		SanitizeStrict: bluemonday.StrictPolicy(),

		// basic — start from the hardened UGC policy (which already
		// drops script/style/iframe/object/embed, on* handlers, and
		// vets URL schemes) then narrow to the safe inline/formatting
		// subset the IR advertises so block/structural markup is also
		// stripped.
		SanitizeBasic: basicPolicy(),

		// markdown_safe — the library's UGCPolicy is purpose-built for
		// "content from a markdown renderer or rich-text editor": it
		// permits headings, lists, blockquote, code/pre, tables, links
		// (with rel="nofollow" + scheme vetting) and images, while
		// stripping script/style and unsafe schemes.
		SanitizeMarkdownSafe: bluemonday.UGCPolicy(),
	}
}

// basicPolicy constructs the `basic` profile: a deliberately small
// allow-list of inline + minimal formatting tags. Everything not listed
// (including script/style/iframe and all event-handler attributes) is
// stripped by bluemonday's default-deny posture. URL schemes on `<a>`
// are restricted to http/https/mailto so `javascript:` payloads are
// dropped.
func basicPolicy() *bluemonday.Policy {
	p := bluemonday.NewPolicy()
	p.AllowStandardURLs()
	p.AllowAttrs("href").OnElements("a")
	p.RequireParseableURLs(true)
	p.AllowURLSchemes("http", "https", "mailto")
	p.AllowElements("b", "i", "em", "strong", "br", "p", "span")
	return p
}

// policyFor resolves the cached policy for a profile, building the
// registry on first use. An unknown profile is treated as the safest
// option (strict) rather than silently passing markup through —
// fail-closed is the correct posture for a security primitive.
func policyFor(profile SanitizeHTMLProfile) *bluemonday.Policy {
	policyOnce.Do(buildPolicies)
	if p, ok := policyByName[profile]; ok {
		return p
	}
	return policyByName[SanitizeStrict]
}

// SanitizeHTML runs `input` through the bluemonday policy for `profile`
// and returns the sanitized string. Exported so generated code and
// authored `@fn` extensions that set a sanitized field can apply the
// exact same transform the write boundary applies. Wire-thin: one call
// into `bluemonday.Policy.Sanitize`.
func SanitizeHTML(profile SanitizeHTMLProfile, input string) string {
	return policyFor(profile).Sanitize(input)
}
