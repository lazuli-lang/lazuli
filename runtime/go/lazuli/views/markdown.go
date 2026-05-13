// Package views provides generator-neutral helpers for future Lazuli view
// surfaces.
package views

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

const (
	frontmatterDelimiter = "---"
)

var (
	// ErrInvalidMarkdownPolicy reports unsupported Markdown rendering policy
	// settings.
	ErrInvalidMarkdownPolicy = errors.New("lazuli/views: invalid markdown policy")

	// ErrInvalidMarkdownDocument reports Markdown input that cannot be safely
	// interpreted by the policy helpers.
	ErrInvalidMarkdownDocument = errors.New("lazuli/views: invalid markdown document")

	// ErrInvalidMarkdownFrontmatter reports malformed frontmatter metadata.
	ErrInvalidMarkdownFrontmatter = errors.New("lazuli/views: invalid markdown frontmatter")
)

// MarkdownFeature is a coarse Markdown capability understood by the policy
// helpers. It is intentionally not a full Markdown AST.
type MarkdownFeature string

const (
	MarkdownFeatureHeading       MarkdownFeature = "heading"
	MarkdownFeatureParagraph     MarkdownFeature = "paragraph"
	MarkdownFeatureEmphasis      MarkdownFeature = "emphasis"
	MarkdownFeatureStrong        MarkdownFeature = "strong"
	MarkdownFeatureLink          MarkdownFeature = "link"
	MarkdownFeatureImage         MarkdownFeature = "image"
	MarkdownFeatureList          MarkdownFeature = "list"
	MarkdownFeatureBlockquote    MarkdownFeature = "blockquote"
	MarkdownFeatureCode          MarkdownFeature = "code"
	MarkdownFeatureTable         MarkdownFeature = "table"
	MarkdownFeatureThematicBreak MarkdownFeature = "thematic_break"
	MarkdownFeatureRawHTML       MarkdownFeature = "raw_html"
)

var markdownFeatureOrder = []MarkdownFeature{
	MarkdownFeatureHeading,
	MarkdownFeatureParagraph,
	MarkdownFeatureEmphasis,
	MarkdownFeatureStrong,
	MarkdownFeatureLink,
	MarkdownFeatureImage,
	MarkdownFeatureList,
	MarkdownFeatureBlockquote,
	MarkdownFeatureCode,
	MarkdownFeatureTable,
	MarkdownFeatureThematicBreak,
	MarkdownFeatureRawHTML,
}

var defaultMarkdownFeatures = []MarkdownFeature{
	MarkdownFeatureHeading,
	MarkdownFeatureParagraph,
	MarkdownFeatureEmphasis,
	MarkdownFeatureStrong,
	MarkdownFeatureLink,
	MarkdownFeatureImage,
	MarkdownFeatureList,
	MarkdownFeatureBlockquote,
	MarkdownFeatureCode,
	MarkdownFeatureTable,
	MarkdownFeatureThematicBreak,
}

// MarkdownSanitizationRequirement describes whether rendered Markdown output
// must pass through an HTML sanitizer before browser delivery.
type MarkdownSanitizationRequirement string

const (
	// MarkdownSanitizationRequired means rendered HTML must be sanitized by the
	// caller before it is sent to a browser. Empty requirements normalize to
	// this value.
	MarkdownSanitizationRequired MarkdownSanitizationRequirement = "required"

	// MarkdownSanitizationTrusted means the caller treats the generated output
	// as trusted and does not require sanitizer enforcement from this policy.
	MarkdownSanitizationTrusted MarkdownSanitizationRequirement = "trusted"
)

var defaultMarkdownURLSchemes = []string{"http", "https", "mailto", "tel"}

// MarkdownSanitizationPolicy records sanitizer expectations for a Markdown
// renderer. These helpers only validate the policy and input; they do not
// sanitize or render Markdown.
type MarkdownSanitizationPolicy struct {
	// Requirement controls whether rendered HTML must be sanitized. Empty
	// defaults to MarkdownSanitizationRequired.
	Requirement MarkdownSanitizationRequirement

	// AllowRawHTML permits raw HTML in Markdown only when Requirement is
	// MarkdownSanitizationRequired and the raw_html feature is allowed.
	AllowRawHTML bool

	// AllowedURLSchemes is the allow-list for absolute link and image URLs.
	// Empty uses http, https, mailto, and tel. Relative URLs are allowed.
	AllowedURLSchemes []string
}

// RequiresSanitization reports whether the normalized policy requires rendered
// HTML to be sanitized before browser delivery.
func (p MarkdownSanitizationPolicy) RequiresSanitization() bool {
	normalized, err := normalizeMarkdownSanitizationPolicy(p)
	if err != nil {
		return true
	}
	return normalized.Requirement == MarkdownSanitizationRequired
}

// MarkdownPolicy configures deterministic Markdown validation before a caller
// hands input to a real Markdown renderer.
type MarkdownPolicy struct {
	// AllowedFeatures is the feature allow-list for Markdown input. Empty uses
	// DefaultMarkdownPolicy features, which intentionally exclude raw HTML.
	AllowedFeatures []MarkdownFeature

	// Sanitization records sanitizer requirements and URL scheme constraints.
	Sanitization MarkdownSanitizationPolicy

	// RequiredMetadata lists frontmatter keys that must be present.
	RequiredMetadata []string
}

// DefaultMarkdownPolicy returns a conservative policy for browser-bound
// Markdown. It allows common Markdown features, rejects raw HTML, and requires
// sanitized renderer output.
func DefaultMarkdownPolicy() MarkdownPolicy {
	return MarkdownPolicy{
		AllowedFeatures: append([]MarkdownFeature(nil), defaultMarkdownFeatures...),
		Sanitization: MarkdownSanitizationPolicy{
			Requirement:       MarkdownSanitizationRequired,
			AllowedURLSchemes: append([]string(nil), defaultMarkdownURLSchemes...),
		},
	}
}

// Normalize returns a copy with default features, sanitizer requirement, URL
// schemes, and metadata keys normalized. Validate reports unsupported settings.
func (p MarkdownPolicy) Normalize() MarkdownPolicy {
	normalized, _ := normalizeMarkdownPolicy(p)
	return normalized
}

// Validate checks whether the policy can be applied deterministically.
func (p MarkdownPolicy) Validate() error {
	_, err := normalizeMarkdownPolicy(p)
	return err
}

// Allows reports whether feature is in the normalized allow-list.
func (p MarkdownPolicy) Allows(feature MarkdownFeature) bool {
	normalized, err := normalizeMarkdownPolicy(p)
	if err != nil {
		return false
	}
	feature, err = normalizeMarkdownFeature(feature)
	if err != nil {
		return false
	}
	for _, allowed := range normalized.AllowedFeatures {
		if allowed == feature {
			return true
		}
	}
	return false
}

// RequiresSanitization reports whether the normalized policy requires rendered
// HTML to be sanitized before browser delivery.
func (p MarkdownPolicy) RequiresSanitization() bool {
	return p.Normalize().Sanitization.RequiresSanitization()
}

// ValidateMarkdownPolicy checks whether policy can be applied deterministically.
func ValidateMarkdownPolicy(policy MarkdownPolicy) error {
	return policy.Validate()
}

// MarkdownMetadataField is one normalized frontmatter key/value pair.
type MarkdownMetadataField struct {
	Key   string
	Value string
}

// MarkdownFrontmatter is sorted by key after parsing.
type MarkdownFrontmatter []MarkdownMetadataField

// Value returns the value for key after normalizing the lookup key.
func (m MarkdownFrontmatter) Value(key string) (string, bool) {
	key = normalizeMarkdownMetadataKey(key)
	for _, field := range m {
		if field.Key == key {
			return field.Value, true
		}
	}
	return "", false
}

// Keys returns metadata keys in deterministic sorted order.
func (m MarkdownFrontmatter) Keys() []string {
	keys := make([]string, 0, len(m))
	for _, field := range m {
		keys = append(keys, field.Key)
	}
	sort.Strings(keys)
	return keys
}

// Map returns a copy of metadata as a map for callers that do not need order.
func (m MarkdownFrontmatter) Map() map[string]string {
	out := make(map[string]string, len(m))
	for _, field := range m {
		out[field.Key] = field.Value
	}
	return out
}

// MarkdownLink is a link or image destination discovered during input
// inspection.
type MarkdownLink struct {
	Destination string
	Scheme      string
	Image       bool
}

// MarkdownDocument is the normalized result of inspecting Markdown input.
type MarkdownDocument struct {
	Metadata MarkdownFrontmatter
	Body     string
	Features []MarkdownFeature
	Links    []MarkdownLink
}

// HasFeature reports whether the inspected document uses feature.
func (d MarkdownDocument) HasFeature(feature MarkdownFeature) bool {
	feature, err := normalizeMarkdownFeature(feature)
	if err != nil {
		return false
	}
	for _, current := range d.Features {
		if current == feature {
			return true
		}
	}
	return false
}

// ParseMarkdownFrontmatter parses a small frontmatter subset at the start of
// source. Supported fields are single-line "key: value" pairs. The returned
// body uses normalized LF line endings and excludes the frontmatter block.
func ParseMarkdownFrontmatter(source string) (MarkdownFrontmatter, string, error) {
	source = normalizeMarkdownNewlines(source)
	source = strings.TrimPrefix(source, "\ufeff")
	if first, rest, ok := strings.Cut(source, "\n"); ok {
		if strings.TrimSpace(first) != frontmatterDelimiter {
			return nil, source, nil
		}
		return parseMarkdownFrontmatterBlock(rest)
	}
	if strings.TrimSpace(source) == frontmatterDelimiter {
		return nil, "", fmt.Errorf("%w: missing closing delimiter", ErrInvalidMarkdownFrontmatter)
	}
	return nil, source, nil
}

// InspectMarkdown parses frontmatter and reports coarse Markdown features and
// link destinations. It does not render Markdown.
func InspectMarkdown(source string) (MarkdownDocument, error) {
	metadata, body, err := ParseMarkdownFrontmatter(source)
	if err != nil {
		return MarkdownDocument{}, err
	}
	if err := validateMarkdownBody(body); err != nil {
		return MarkdownDocument{}, err
	}

	features, links, err := inspectMarkdownBody(body)
	if err != nil {
		return MarkdownDocument{}, err
	}
	return MarkdownDocument{
		Metadata: metadata,
		Body:     body,
		Features: features,
		Links:    links,
	}, nil
}

// ValidateMarkdown inspects source and validates it against policy. The
// returned document is normalized and safe to pass to a renderer selected by
// the caller.
func ValidateMarkdown(source string, policy MarkdownPolicy) (MarkdownDocument, error) {
	normalized, err := normalizeMarkdownPolicy(policy)
	if err != nil {
		return MarkdownDocument{}, err
	}

	document, err := InspectMarkdown(source)
	if err != nil {
		return MarkdownDocument{}, err
	}

	var errs []error
	allowed := make(map[MarkdownFeature]struct{}, len(normalized.AllowedFeatures))
	for _, feature := range normalized.AllowedFeatures {
		allowed[feature] = struct{}{}
	}
	for _, feature := range document.Features {
		if _, ok := allowed[feature]; !ok {
			errs = append(errs, fmt.Errorf("%w: feature %q is not allowed", ErrInvalidMarkdownDocument, feature))
		}
	}
	if document.HasFeature(MarkdownFeatureRawHTML) && !normalized.Sanitization.AllowRawHTML {
		errs = append(errs, fmt.Errorf("%w: raw HTML requires AllowRawHTML", ErrInvalidMarkdownDocument))
	}

	for _, key := range normalized.RequiredMetadata {
		if _, ok := document.Metadata.Value(key); !ok {
			errs = append(errs, fmt.Errorf("%w: metadata %q is required", ErrInvalidMarkdownDocument, key))
		}
	}

	allowedSchemes := make(map[string]struct{}, len(normalized.Sanitization.AllowedURLSchemes))
	for _, scheme := range normalized.Sanitization.AllowedURLSchemes {
		allowedSchemes[scheme] = struct{}{}
	}
	for i, link := range document.Links {
		if link.Destination == "" {
			errs = append(errs, fmt.Errorf("%w: link[%d] destination is required", ErrInvalidMarkdownDocument, i))
			continue
		}
		if link.Scheme == "" {
			continue
		}
		if _, ok := allowedSchemes[link.Scheme]; !ok {
			errs = append(errs, fmt.Errorf("%w: link[%d] scheme %q is not allowed", ErrInvalidMarkdownDocument, i, link.Scheme))
		}
	}

	return document, errors.Join(errs...)
}

func normalizeMarkdownPolicy(policy MarkdownPolicy) (MarkdownPolicy, error) {
	features, featuresErr := normalizeMarkdownFeatures(policy.AllowedFeatures)
	sanitization, sanitizationErr := normalizeMarkdownSanitizationPolicy(policy.Sanitization)
	requiredMetadata, metadataErr := normalizeMarkdownRequiredMetadata(policy.RequiredMetadata)

	normalized := MarkdownPolicy{
		AllowedFeatures:  features,
		Sanitization:     sanitization,
		RequiredMetadata: requiredMetadata,
	}

	var errs []error
	if featuresErr != nil {
		errs = append(errs, featuresErr)
	}
	if sanitizationErr != nil {
		errs = append(errs, sanitizationErr)
	}
	if metadataErr != nil {
		errs = append(errs, metadataErr)
	}
	if hasMarkdownFeature(features, MarkdownFeatureRawHTML) && !sanitization.AllowRawHTML {
		errs = append(errs, fmt.Errorf("%w: raw_html feature requires AllowRawHTML", ErrInvalidMarkdownPolicy))
	}
	return normalized, errors.Join(errs...)
}

func normalizeMarkdownFeatures(features []MarkdownFeature) ([]MarkdownFeature, error) {
	if len(features) == 0 {
		return append([]MarkdownFeature(nil), defaultMarkdownFeatures...), nil
	}

	seen := make(map[MarkdownFeature]int, len(features))
	normalized := make([]MarkdownFeature, 0, len(features))
	var errs []error
	for i, feature := range features {
		clean, err := normalizeMarkdownFeature(feature)
		if err != nil {
			errs = append(errs, fmt.Errorf("%w: allowed feature[%d] %q is unknown", ErrInvalidMarkdownPolicy, i, feature))
			continue
		}
		if previous, ok := seen[clean]; ok {
			errs = append(errs, fmt.Errorf("%w: allowed feature[%d] duplicates feature[%d]", ErrInvalidMarkdownPolicy, i, previous))
			continue
		}
		seen[clean] = i
		normalized = append(normalized, clean)
	}
	sortMarkdownFeatures(normalized)
	return normalized, errors.Join(errs...)
}

func normalizeMarkdownFeature(feature MarkdownFeature) (MarkdownFeature, error) {
	clean := MarkdownFeature(strings.ToLower(strings.TrimSpace(string(feature))))
	if hasMarkdownFeature(markdownFeatureOrder, clean) {
		return clean, nil
	}
	return "", ErrInvalidMarkdownPolicy
}

func normalizeMarkdownSanitizationPolicy(policy MarkdownSanitizationPolicy) (MarkdownSanitizationPolicy, error) {
	requirement := MarkdownSanitizationRequirement(strings.ToLower(strings.TrimSpace(string(policy.Requirement))))
	if requirement == "" {
		requirement = MarkdownSanitizationRequired
	}

	schemes, schemesErr := normalizeMarkdownURLSchemes(policy.AllowedURLSchemes)
	normalized := MarkdownSanitizationPolicy{
		Requirement:       requirement,
		AllowRawHTML:      policy.AllowRawHTML,
		AllowedURLSchemes: schemes,
	}

	var errs []error
	switch requirement {
	case MarkdownSanitizationRequired, MarkdownSanitizationTrusted:
	default:
		errs = append(errs, fmt.Errorf("%w: unknown sanitization requirement %q", ErrInvalidMarkdownPolicy, policy.Requirement))
	}
	if schemesErr != nil {
		errs = append(errs, schemesErr)
	}
	if requirement == MarkdownSanitizationTrusted && policy.AllowRawHTML {
		errs = append(errs, fmt.Errorf("%w: raw HTML requires sanitizer enforcement", ErrInvalidMarkdownPolicy))
	}
	return normalized, errors.Join(errs...)
}

func normalizeMarkdownURLSchemes(schemes []string) ([]string, error) {
	if len(schemes) == 0 {
		return append([]string(nil), defaultMarkdownURLSchemes...), nil
	}

	seen := make(map[string]int, len(schemes))
	normalized := make([]string, 0, len(schemes))
	var errs []error
	for i, scheme := range schemes {
		clean := strings.TrimSuffix(strings.ToLower(strings.TrimSpace(scheme)), ":")
		if !validMarkdownURLScheme(clean) {
			errs = append(errs, fmt.Errorf("%w: url scheme[%d] %q is invalid", ErrInvalidMarkdownPolicy, i, scheme))
			continue
		}
		if unsafeMarkdownURLScheme(clean) {
			errs = append(errs, fmt.Errorf("%w: url scheme[%d] %q is unsafe", ErrInvalidMarkdownPolicy, i, scheme))
			continue
		}
		if previous, ok := seen[clean]; ok {
			errs = append(errs, fmt.Errorf("%w: url scheme[%d] duplicates scheme[%d]", ErrInvalidMarkdownPolicy, i, previous))
			continue
		}
		seen[clean] = i
		normalized = append(normalized, clean)
	}
	sort.Strings(normalized)
	return normalized, errors.Join(errs...)
}

func normalizeMarkdownRequiredMetadata(keys []string) ([]string, error) {
	if len(keys) == 0 {
		return nil, nil
	}

	seen := make(map[string]int, len(keys))
	normalized := make([]string, 0, len(keys))
	var errs []error
	for i, key := range keys {
		clean := normalizeMarkdownMetadataKey(key)
		if !validMarkdownMetadataKey(clean) {
			errs = append(errs, fmt.Errorf("%w: required metadata[%d] %q is invalid", ErrInvalidMarkdownPolicy, i, key))
			continue
		}
		if previous, ok := seen[clean]; ok {
			errs = append(errs, fmt.Errorf("%w: required metadata[%d] duplicates metadata[%d]", ErrInvalidMarkdownPolicy, i, previous))
			continue
		}
		seen[clean] = i
		normalized = append(normalized, clean)
	}
	sort.Strings(normalized)
	return normalized, errors.Join(errs...)
}

func parseMarkdownFrontmatterBlock(rest string) (MarkdownFrontmatter, string, error) {
	var lines []string
	for {
		line, next, ok := strings.Cut(rest, "\n")
		trimmed := strings.TrimSpace(line)
		if trimmed == frontmatterDelimiter || trimmed == "..." {
			metadata, err := parseMarkdownMetadata(lines)
			if err != nil {
				return nil, "", err
			}
			return metadata, next, nil
		}
		if !ok {
			return nil, "", fmt.Errorf("%w: missing closing delimiter", ErrInvalidMarkdownFrontmatter)
		}
		lines = append(lines, line)
		rest = next
	}
}

func parseMarkdownMetadata(lines []string) (MarkdownFrontmatter, error) {
	seen := make(map[string]int, len(lines))
	metadata := make(MarkdownFrontmatter, 0, len(lines))
	var errs []error
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}

		key, value, ok := strings.Cut(trimmed, ":")
		if !ok {
			errs = append(errs, fmt.Errorf("%w: line %d must be key: value", ErrInvalidMarkdownFrontmatter, i+1))
			continue
		}
		key = normalizeMarkdownMetadataKey(key)
		if !validMarkdownMetadataKey(key) {
			errs = append(errs, fmt.Errorf("%w: line %d key is invalid", ErrInvalidMarkdownFrontmatter, i+1))
			continue
		}
		if previous, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: line %d duplicates line %d", ErrInvalidMarkdownFrontmatter, i+1, previous+1))
			continue
		}

		value = unquoteMarkdownMetadataValue(strings.TrimSpace(value))
		if hasMarkdownControl(value) {
			errs = append(errs, fmt.Errorf("%w: line %d value contains control characters", ErrInvalidMarkdownFrontmatter, i+1))
			continue
		}

		seen[key] = i
		metadata = append(metadata, MarkdownMetadataField{Key: key, Value: value})
	}
	sort.Slice(metadata, func(i, j int) bool {
		return metadata[i].Key < metadata[j].Key
	})
	return metadata, errors.Join(errs...)
}

func unquoteMarkdownMetadataValue(value string) string {
	if len(value) < 2 {
		return value
	}
	if (value[0] == '"' && value[len(value)-1] == '"') || (value[0] == '\'' && value[len(value)-1] == '\'') {
		return strings.TrimSpace(value[1 : len(value)-1])
	}
	return value
}

func inspectMarkdownBody(body string) ([]MarkdownFeature, []MarkdownLink, error) {
	features := make(map[MarkdownFeature]struct{})
	links := make([]MarkdownLink, 0)
	var errs []error
	var fence string

	lines := strings.Split(body, "\n")
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if fence != "" {
			if isMarkdownFenceClose(trimmed, fence) {
				fence = ""
			}
			continue
		}
		if trimmed == "" {
			continue
		}
		if marker, ok := markdownFenceMarker(trimmed); ok {
			features[MarkdownFeatureCode] = struct{}{}
			fence = marker
			continue
		}

		blockOnly := isMarkdownBlockLine(trimmed)
		lineFeatures, lineLinks := inspectMarkdownLine(line)
		for _, feature := range lineFeatures {
			features[feature] = struct{}{}
		}
		links = append(links, lineLinks...)

		if !blockOnly {
			features[MarkdownFeatureParagraph] = struct{}{}
		}

		for j, link := range lineLinks {
			if link.Destination == "" {
				errs = append(errs, fmt.Errorf("%w: line %d link[%d] destination is required", ErrInvalidMarkdownDocument, i+1, j))
			}
		}
	}
	if fence != "" {
		errs = append(errs, fmt.Errorf("%w: unclosed code fence", ErrInvalidMarkdownDocument))
	}

	out := make([]MarkdownFeature, 0, len(features))
	for _, feature := range markdownFeatureOrder {
		if _, ok := features[feature]; ok {
			out = append(out, feature)
		}
	}
	return out, links, errors.Join(errs...)
}

func inspectMarkdownLine(line string) ([]MarkdownFeature, []MarkdownLink) {
	trimmed := strings.TrimSpace(line)
	features := make(map[MarkdownFeature]struct{})

	switch {
	case isMarkdownHeading(trimmed):
		features[MarkdownFeatureHeading] = struct{}{}
	case isMarkdownThematicBreak(trimmed):
		features[MarkdownFeatureThematicBreak] = struct{}{}
	case strings.HasPrefix(trimmed, ">"):
		features[MarkdownFeatureBlockquote] = struct{}{}
	case isMarkdownListItem(trimmed):
		features[MarkdownFeatureList] = struct{}{}
	}
	if isMarkdownTableLine(trimmed) {
		features[MarkdownFeatureTable] = struct{}{}
	}

	inline := maskMarkdownCodeSpans(line)
	if containsMarkdownHTML(inline) {
		features[MarkdownFeatureRawHTML] = struct{}{}
	}
	if containsMarkdownStrong(inline) {
		features[MarkdownFeatureStrong] = struct{}{}
	}
	if containsMarkdownEmphasis(inline) {
		features[MarkdownFeatureEmphasis] = struct{}{}
	}
	if strings.Contains(inline, "`") {
		features[MarkdownFeatureCode] = struct{}{}
	}

	links := inspectMarkdownLinks(inline)
	for _, link := range links {
		if link.Image {
			features[MarkdownFeatureImage] = struct{}{}
		} else {
			features[MarkdownFeatureLink] = struct{}{}
		}
	}

	out := make([]MarkdownFeature, 0, len(features))
	for _, feature := range markdownFeatureOrder {
		if _, ok := features[feature]; ok {
			out = append(out, feature)
		}
	}
	return out, links
}

func inspectMarkdownLinks(line string) []MarkdownLink {
	links := make([]MarkdownLink, 0)
	for i := 0; i < len(line); i++ {
		image := false
		open := i
		if line[i] == '!' && i+1 < len(line) && line[i+1] == '[' {
			image = true
			open = i + 1
		} else if line[i] != '[' {
			continue
		}

		close := strings.Index(line[open+1:], "](")
		if close < 0 {
			continue
		}
		close += open + 1
		destStart := close + 2
		destEnd := findMarkdownLinkDestinationEnd(line, destStart)
		if destEnd < 0 {
			continue
		}
		destination := normalizeMarkdownLinkDestination(line[destStart:destEnd])
		links = append(links, MarkdownLink{
			Destination: destination,
			Scheme:      markdownURLScheme(destination),
			Image:       image,
		})
		i = destEnd
	}

	for _, destination := range inspectMarkdownAutolinks(line) {
		links = append(links, MarkdownLink{
			Destination: destination,
			Scheme:      markdownURLScheme(destination),
		})
	}
	return links
}

func inspectMarkdownAutolinks(line string) []string {
	var links []string
	for i := 0; i < len(line); i++ {
		if line[i] != '<' {
			continue
		}
		end := strings.IndexByte(line[i+1:], '>')
		if end < 0 {
			continue
		}
		end += i + 1
		destination := strings.TrimSpace(line[i+1 : end])
		scheme := markdownURLScheme(destination)
		if scheme == "http" || scheme == "https" || scheme == "mailto" {
			links = append(links, destination)
		}
		i = end
	}
	return links
}

func findMarkdownLinkDestinationEnd(line string, start int) int {
	depth := 0
	for i := start; i < len(line); i++ {
		switch line[i] {
		case '(':
			depth++
		case ')':
			if depth == 0 {
				return i
			}
			depth--
		}
	}
	return -1
}

func normalizeMarkdownLinkDestination(raw string) string {
	raw = strings.TrimSpace(raw)
	if strings.HasPrefix(raw, "<") {
		if end := strings.IndexByte(raw, '>'); end >= 0 {
			return strings.TrimSpace(raw[1:end])
		}
	}
	for i, r := range raw {
		if unicode.IsSpace(r) {
			return strings.TrimSpace(raw[:i])
		}
	}
	return raw
}

func markdownURLScheme(destination string) string {
	for i, r := range destination {
		switch {
		case r == ':':
			if i == 0 {
				return ""
			}
			scheme := strings.ToLower(destination[:i])
			if validMarkdownURLScheme(scheme) {
				return scheme
			}
			return ""
		case r == '/', r == '?', r == '#':
			return ""
		case i == 0 && !isMarkdownSchemeLetter(byte(r)):
			return ""
		case i > 0 && !isMarkdownSchemeChar(byte(r)):
			return ""
		}
	}
	return ""
}

func isMarkdownHeading(line string) bool {
	count := 0
	for count < len(line) && line[count] == '#' {
		count++
	}
	return count > 0 && count <= 6 && (count == len(line) || unicode.IsSpace(rune(line[count])))
}

func isMarkdownThematicBreak(line string) bool {
	count := 0
	var marker rune
	for _, r := range line {
		if unicode.IsSpace(r) {
			continue
		}
		if marker == 0 {
			if r != '-' && r != '*' && r != '_' {
				return false
			}
			marker = r
		}
		if r != marker {
			return false
		}
		count++
	}
	return count >= 3
}

func isMarkdownListItem(line string) bool {
	if len(line) >= 2 {
		if (line[0] == '-' || line[0] == '*' || line[0] == '+') && unicode.IsSpace(rune(line[1])) {
			return true
		}
	}

	i := 0
	for i < len(line) && line[i] >= '0' && line[i] <= '9' {
		i++
	}
	return i > 0 && i+1 < len(line) && (line[i] == '.' || line[i] == ')') && unicode.IsSpace(rune(line[i+1]))
}

func isMarkdownTableLine(line string) bool {
	if !strings.Contains(line, "|") {
		return false
	}
	cells := strings.Split(line, "|")
	nonEmpty := 0
	for _, cell := range cells {
		cell = strings.TrimSpace(cell)
		if cell != "" {
			nonEmpty++
		}
	}
	return nonEmpty >= 2
}

func markdownFenceMarker(line string) (string, bool) {
	if len(line) < 3 {
		return "", false
	}
	marker := line[0]
	if marker != '`' && marker != '~' {
		return "", false
	}
	count := 0
	for count < len(line) && line[count] == marker {
		count++
	}
	if count < 3 {
		return "", false
	}
	return strings.Repeat(string(marker), count), true
}

func isMarkdownFenceClose(line, fence string) bool {
	return strings.HasPrefix(line, fence)
}

func isMarkdownBlockLine(line string) bool {
	if isMarkdownHeading(line) || isMarkdownThematicBreak(line) || strings.HasPrefix(line, ">") || isMarkdownListItem(line) || isMarkdownTableLine(line) {
		return true
	}
	return strings.HasPrefix(line, "<") && containsMarkdownHTML(line)
}

func maskMarkdownCodeSpans(line string) string {
	var b strings.Builder
	b.Grow(len(line))
	inCode := false
	for _, r := range line {
		if r == '`' {
			inCode = !inCode
			b.WriteRune(r)
			continue
		}
		if inCode {
			b.WriteByte(' ')
			continue
		}
		b.WriteRune(r)
	}
	return b.String()
}

func containsMarkdownHTML(line string) bool {
	for i := 0; i < len(line); i++ {
		if line[i] != '<' || i+1 >= len(line) {
			continue
		}
		next := line[i+1]
		if next == '!' {
			return true
		}
		if next == '/' {
			i++
			if i+1 >= len(line) || !isMarkdownHTMLNameStart(line[i+1]) {
				continue
			}
		} else if !isMarkdownHTMLNameStart(next) {
			continue
		}

		j := i + 1
		if line[j] == '/' {
			j++
		}
		for j < len(line) && isMarkdownHTMLNameChar(line[j]) {
			j++
		}
		if j < len(line) && (line[j] == '>' || line[j] == '/' || unicode.IsSpace(rune(line[j]))) {
			return true
		}
	}
	return false
}

func containsMarkdownStrong(line string) bool {
	return containsMarkdownDelimiter(line, "**") || containsMarkdownDelimiter(line, "__")
}

func containsMarkdownEmphasis(line string) bool {
	return containsMarkdownSingleDelimiter(line, '*') || containsMarkdownSingleDelimiter(line, '_')
}

func containsMarkdownDelimiter(line, delimiter string) bool {
	first := strings.Index(line, delimiter)
	if first < 0 {
		return false
	}
	second := strings.Index(line[first+len(delimiter):], delimiter)
	return second >= 0
}

func containsMarkdownSingleDelimiter(line string, delimiter byte) bool {
	for i := 0; i < len(line); i++ {
		if line[i] != delimiter {
			continue
		}
		if i+1 < len(line) && line[i+1] == delimiter {
			i++
			continue
		}
		for j := i + 1; j < len(line); j++ {
			if line[j] == delimiter {
				return true
			}
		}
	}
	return false
}

func validateMarkdownBody(body string) error {
	for _, r := range body {
		switch r {
		case '\n', '\t':
			continue
		}
		if unicode.IsControl(r) {
			return fmt.Errorf("%w: body contains control characters", ErrInvalidMarkdownDocument)
		}
	}
	return nil
}

func normalizeMarkdownNewlines(value string) string {
	value = strings.ReplaceAll(value, "\r\n", "\n")
	return strings.ReplaceAll(value, "\r", "\n")
}

func normalizeMarkdownMetadataKey(key string) string {
	return strings.ToLower(strings.TrimSpace(key))
}

func validMarkdownMetadataKey(key string) bool {
	if key == "" {
		return false
	}
	for i := 0; i < len(key); i++ {
		c := key[i]
		switch {
		case c >= 'a' && c <= 'z':
		case c >= '0' && c <= '9':
		case i > 0 && (c == '-' || c == '_' || c == '.'):
		default:
			return false
		}
	}
	return true
}

func hasMarkdownControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func validMarkdownURLScheme(scheme string) bool {
	if scheme == "" || !isMarkdownSchemeLetter(scheme[0]) {
		return false
	}
	for i := 1; i < len(scheme); i++ {
		if !isMarkdownSchemeChar(scheme[i]) {
			return false
		}
	}
	return true
}

func unsafeMarkdownURLScheme(scheme string) bool {
	switch scheme {
	case "data", "file", "javascript", "vbscript":
		return true
	default:
		return false
	}
}

func isMarkdownSchemeLetter(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isMarkdownSchemeChar(c byte) bool {
	return isMarkdownSchemeLetter(c) || (c >= '0' && c <= '9') || c == '+' || c == '-' || c == '.'
}

func isMarkdownHTMLNameStart(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isMarkdownHTMLNameChar(c byte) bool {
	return isMarkdownHTMLNameStart(c) || (c >= '0' && c <= '9') || c == '-'
}

func hasMarkdownFeature(features []MarkdownFeature, feature MarkdownFeature) bool {
	for _, current := range features {
		if current == feature {
			return true
		}
	}
	return false
}

func sortMarkdownFeatures(features []MarkdownFeature) {
	sort.Slice(features, func(i, j int) bool {
		return markdownFeatureRank(features[i]) < markdownFeatureRank(features[j])
	})
}

func markdownFeatureRank(feature MarkdownFeature) int {
	for i, current := range markdownFeatureOrder {
		if current == feature {
			return i
		}
	}
	return len(markdownFeatureOrder)
}
