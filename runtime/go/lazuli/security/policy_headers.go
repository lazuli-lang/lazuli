package security

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"unicode"
)

const (
	// HeaderXFrameOptions is the legacy clickjacking response header.
	HeaderXFrameOptions = "X-Frame-Options"
	// HeaderXContentTypeOptions is the MIME sniffing response header.
	HeaderXContentTypeOptions = "X-Content-Type-Options"
	// HeaderReferrerPolicy is the referrer disclosure response header.
	HeaderReferrerPolicy = "Referrer-Policy"
	// HeaderPermissionsPolicy is the browser feature permissions response
	// header.
	HeaderPermissionsPolicy = "Permissions-Policy"

	XFrameOptionsDeny       = "DENY"
	XFrameOptionsSameOrigin = "SAMEORIGIN"

	XContentTypeOptionsNoSniff = "nosniff"

	ReferrerPolicyNoReferrer                  = "no-referrer"
	ReferrerPolicyNoReferrerWhenDowngrade     = "no-referrer-when-downgrade"
	ReferrerPolicyOrigin                      = "origin"
	ReferrerPolicyOriginWhenCrossOrigin       = "origin-when-cross-origin"
	ReferrerPolicySameOrigin                  = "same-origin"
	ReferrerPolicyStrictOrigin                = "strict-origin"
	ReferrerPolicyStrictOriginWhenCrossOrigin = "strict-origin-when-cross-origin"
	ReferrerPolicyUnsafeURL                   = "unsafe-url"

	PermissionsPolicyAll  = "*"
	PermissionsPolicySelf = "self"
)

// ErrInvalidPolicyHeader is returned when a browser security policy header
// value cannot be safely rendered.
var ErrInvalidPolicyHeader = errors.New("lazuli/security: invalid_policy_header")

// XFrameOptionsPolicy configures an X-Frame-Options header value.
type XFrameOptionsPolicy struct {
	Value string
}

// XFrameOptionsBuilder builds XFrameOptionsPolicy values with a fluent API.
type XFrameOptionsBuilder struct {
	value string
}

// NewXFrameOptionsBuilder returns an empty X-Frame-Options builder.
func NewXFrameOptionsBuilder() XFrameOptionsBuilder {
	return XFrameOptionsBuilder{}
}

// Deny denies all framing of the response.
func (b XFrameOptionsBuilder) Deny() XFrameOptionsBuilder {
	b.value = XFrameOptionsDeny
	return b
}

// SameOrigin allows same-origin framing of the response.
func (b XFrameOptionsBuilder) SameOrigin() XFrameOptionsBuilder {
	b.value = XFrameOptionsSameOrigin
	return b
}

// Value sets a raw X-Frame-Options value. Build validates and normalizes it.
func (b XFrameOptionsBuilder) Value(value string) XFrameOptionsBuilder {
	b.value = value
	return b
}

// Build returns a normalized XFrameOptionsPolicy.
func (b XFrameOptionsBuilder) Build() (XFrameOptionsPolicy, error) {
	return normalizeXFrameOptionsPolicy(XFrameOptionsPolicy{Value: b.value})
}

// HeaderName returns the X-Frame-Options header name.
func (b XFrameOptionsBuilder) HeaderName() string {
	return HeaderXFrameOptions
}

// HeaderValue renders the builder's policy value.
func (b XFrameOptionsBuilder) HeaderValue() (string, error) {
	policy, err := b.Build()
	if err != nil {
		return "", err
	}
	return policy.HeaderValue()
}

// Header returns the X-Frame-Options header name and rendered value.
func (b XFrameOptionsBuilder) Header() (string, string, error) {
	value, err := b.HeaderValue()
	return b.HeaderName(), value, err
}

// Validate reports whether policy can be rendered as an X-Frame-Options
// header.
func (p XFrameOptionsPolicy) Validate() error {
	_, err := normalizeXFrameOptionsPolicy(p)
	return err
}

// HeaderName returns the X-Frame-Options header name.
func (p XFrameOptionsPolicy) HeaderName() string {
	return HeaderXFrameOptions
}

// HeaderValue renders policy as an X-Frame-Options header value.
func (p XFrameOptionsPolicy) HeaderValue() (string, error) {
	return XFrameOptionsHeaderValue(p)
}

// Header returns the X-Frame-Options header name and rendered value.
func (p XFrameOptionsPolicy) Header() (string, string, error) {
	value, err := p.HeaderValue()
	return p.HeaderName(), value, err
}

// XFrameOptionsHeaderValue renders policy as an X-Frame-Options header value.
func XFrameOptionsHeaderValue(policy XFrameOptionsPolicy) (string, error) {
	normalized, err := normalizeXFrameOptionsPolicy(policy)
	if err != nil {
		return "", err
	}
	return normalized.Value, nil
}

// ValidateXFrameOptionsPolicy reports whether policy can be rendered safely.
func ValidateXFrameOptionsPolicy(policy XFrameOptionsPolicy) error {
	return policy.Validate()
}

// XContentTypeOptionsPolicy configures an X-Content-Type-Options header value.
type XContentTypeOptionsPolicy struct {
	Value string
}

// XContentTypeOptionsBuilder builds XContentTypeOptionsPolicy values with a
// fluent API.
type XContentTypeOptionsBuilder struct {
	value string
}

// NewXContentTypeOptionsBuilder returns an empty X-Content-Type-Options
// builder.
func NewXContentTypeOptionsBuilder() XContentTypeOptionsBuilder {
	return XContentTypeOptionsBuilder{}
}

// NoSniff disables browser MIME type sniffing.
func (b XContentTypeOptionsBuilder) NoSniff() XContentTypeOptionsBuilder {
	b.value = XContentTypeOptionsNoSniff
	return b
}

// Value sets a raw X-Content-Type-Options value. Build validates and
// normalizes it.
func (b XContentTypeOptionsBuilder) Value(value string) XContentTypeOptionsBuilder {
	b.value = value
	return b
}

// Build returns a normalized XContentTypeOptionsPolicy.
func (b XContentTypeOptionsBuilder) Build() (XContentTypeOptionsPolicy, error) {
	return normalizeXContentTypeOptionsPolicy(XContentTypeOptionsPolicy{Value: b.value})
}

// HeaderName returns the X-Content-Type-Options header name.
func (b XContentTypeOptionsBuilder) HeaderName() string {
	return HeaderXContentTypeOptions
}

// HeaderValue renders the builder's policy value.
func (b XContentTypeOptionsBuilder) HeaderValue() (string, error) {
	policy, err := b.Build()
	if err != nil {
		return "", err
	}
	return policy.HeaderValue()
}

// Header returns the X-Content-Type-Options header name and rendered value.
func (b XContentTypeOptionsBuilder) Header() (string, string, error) {
	value, err := b.HeaderValue()
	return b.HeaderName(), value, err
}

// Validate reports whether policy can be rendered as an
// X-Content-Type-Options header.
func (p XContentTypeOptionsPolicy) Validate() error {
	_, err := normalizeXContentTypeOptionsPolicy(p)
	return err
}

// HeaderName returns the X-Content-Type-Options header name.
func (p XContentTypeOptionsPolicy) HeaderName() string {
	return HeaderXContentTypeOptions
}

// HeaderValue renders policy as an X-Content-Type-Options header value.
func (p XContentTypeOptionsPolicy) HeaderValue() (string, error) {
	return XContentTypeOptionsHeaderValue(p)
}

// Header returns the X-Content-Type-Options header name and rendered value.
func (p XContentTypeOptionsPolicy) Header() (string, string, error) {
	value, err := p.HeaderValue()
	return p.HeaderName(), value, err
}

// XContentTypeOptionsHeaderValue renders policy as an X-Content-Type-Options
// header value.
func XContentTypeOptionsHeaderValue(policy XContentTypeOptionsPolicy) (string, error) {
	normalized, err := normalizeXContentTypeOptionsPolicy(policy)
	if err != nil {
		return "", err
	}
	return normalized.Value, nil
}

// ValidateXContentTypeOptionsPolicy reports whether policy can be rendered
// safely.
func ValidateXContentTypeOptionsPolicy(policy XContentTypeOptionsPolicy) error {
	return policy.Validate()
}

// ReferrerPolicy configures a Referrer-Policy header. Multiple values are
// rendered in insertion order as a comma-separated fallback list.
type ReferrerPolicy struct {
	Values []string
}

// ReferrerPolicyBuilder builds ReferrerPolicy values with a fluent API.
type ReferrerPolicyBuilder struct {
	values []string
}

// NewReferrerPolicyBuilder returns an empty Referrer-Policy builder.
func NewReferrerPolicyBuilder() ReferrerPolicyBuilder {
	return ReferrerPolicyBuilder{}
}

// Policy appends a raw Referrer-Policy value. Build validates and normalizes
// it.
func (b ReferrerPolicyBuilder) Policy(policy string) ReferrerPolicyBuilder {
	b.values = append(b.values, policy)
	return b
}

// NoReferrer appends no-referrer.
func (b ReferrerPolicyBuilder) NoReferrer() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyNoReferrer)
}

// NoReferrerWhenDowngrade appends no-referrer-when-downgrade.
func (b ReferrerPolicyBuilder) NoReferrerWhenDowngrade() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyNoReferrerWhenDowngrade)
}

// Origin appends origin.
func (b ReferrerPolicyBuilder) Origin() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyOrigin)
}

// OriginWhenCrossOrigin appends origin-when-cross-origin.
func (b ReferrerPolicyBuilder) OriginWhenCrossOrigin() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyOriginWhenCrossOrigin)
}

// SameOrigin appends same-origin.
func (b ReferrerPolicyBuilder) SameOrigin() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicySameOrigin)
}

// StrictOrigin appends strict-origin.
func (b ReferrerPolicyBuilder) StrictOrigin() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyStrictOrigin)
}

// StrictOriginWhenCrossOrigin appends strict-origin-when-cross-origin.
func (b ReferrerPolicyBuilder) StrictOriginWhenCrossOrigin() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyStrictOriginWhenCrossOrigin)
}

// UnsafeURL appends unsafe-url.
func (b ReferrerPolicyBuilder) UnsafeURL() ReferrerPolicyBuilder {
	return b.Policy(ReferrerPolicyUnsafeURL)
}

// Build returns a normalized ReferrerPolicy.
func (b ReferrerPolicyBuilder) Build() (ReferrerPolicy, error) {
	return normalizeReferrerPolicy(ReferrerPolicy{Values: b.values})
}

// HeaderName returns the Referrer-Policy header name.
func (b ReferrerPolicyBuilder) HeaderName() string {
	return HeaderReferrerPolicy
}

// HeaderValue renders the builder's policy value.
func (b ReferrerPolicyBuilder) HeaderValue() (string, error) {
	policy, err := b.Build()
	if err != nil {
		return "", err
	}
	return policy.HeaderValue()
}

// Header returns the Referrer-Policy header name and rendered value.
func (b ReferrerPolicyBuilder) Header() (string, string, error) {
	value, err := b.HeaderValue()
	return b.HeaderName(), value, err
}

// Validate reports whether policy can be rendered as a Referrer-Policy header.
func (p ReferrerPolicy) Validate() error {
	_, err := normalizeReferrerPolicy(p)
	return err
}

// HeaderName returns the Referrer-Policy header name.
func (p ReferrerPolicy) HeaderName() string {
	return HeaderReferrerPolicy
}

// HeaderValue renders policy as a Referrer-Policy header value.
func (p ReferrerPolicy) HeaderValue() (string, error) {
	return ReferrerPolicyHeaderValue(p)
}

// Header returns the Referrer-Policy header name and rendered value.
func (p ReferrerPolicy) Header() (string, string, error) {
	value, err := p.HeaderValue()
	return p.HeaderName(), value, err
}

// ReferrerPolicyHeaderValue renders policy as a Referrer-Policy header value.
func ReferrerPolicyHeaderValue(policy ReferrerPolicy) (string, error) {
	normalized, err := normalizeReferrerPolicy(policy)
	if err != nil {
		return "", err
	}
	return strings.Join(normalized.Values, ", "), nil
}

// ValidateReferrerPolicy reports whether policy can be rendered safely.
func ValidateReferrerPolicy(policy ReferrerPolicy) error {
	return policy.Validate()
}

// PermissionsPolicyDirective configures one Permissions-Policy directive.
// Empty Allowlist renders as an empty allowlist, disabling the feature.
type PermissionsPolicyDirective struct {
	Feature   string
	Allowlist []string
}

// PermissionsPolicy configures a Permissions-Policy header. Directives are
// normalized and rendered deterministically by HeaderValue.
type PermissionsPolicy struct {
	Directives []PermissionsPolicyDirective
}

// PermissionsPolicyBuilder builds PermissionsPolicy values with a fluent API.
type PermissionsPolicyBuilder struct {
	directives []PermissionsPolicyDirective
}

// NewPermissionsPolicyBuilder returns an empty Permissions-Policy builder.
func NewPermissionsPolicyBuilder() PermissionsPolicyBuilder {
	return PermissionsPolicyBuilder{}
}

// Directive appends a Permissions-Policy directive. Duplicate directive names
// and allowlist values are merged by Build.
func (b PermissionsPolicyBuilder) Directive(feature string, allowlist ...string) PermissionsPolicyBuilder {
	b.directives = append(b.directives, PermissionsPolicyDirective{
		Feature:   feature,
		Allowlist: clonePolicyHeaderStrings(allowlist),
	})
	return b
}

// Allow appends a directive with an explicit allowlist.
func (b PermissionsPolicyBuilder) Allow(feature string, allowlist ...string) PermissionsPolicyBuilder {
	return b.Directive(feature, allowlist...)
}

// AllowSelf appends directives allowing each feature for the same origin.
func (b PermissionsPolicyBuilder) AllowSelf(features ...string) PermissionsPolicyBuilder {
	for _, feature := range features {
		b = b.Directive(feature, PermissionsPolicySelf)
	}
	return b
}

// Disable appends directives with empty allowlists for the supplied features.
func (b PermissionsPolicyBuilder) Disable(features ...string) PermissionsPolicyBuilder {
	for _, feature := range features {
		b = b.Directive(feature)
	}
	return b
}

// Build returns a normalized PermissionsPolicy.
func (b PermissionsPolicyBuilder) Build() (PermissionsPolicy, error) {
	return normalizePermissionsPolicy(PermissionsPolicy{Directives: b.directives})
}

// HeaderName returns the Permissions-Policy header name.
func (b PermissionsPolicyBuilder) HeaderName() string {
	return HeaderPermissionsPolicy
}

// HeaderValue renders the builder's policy value.
func (b PermissionsPolicyBuilder) HeaderValue() (string, error) {
	policy, err := b.Build()
	if err != nil {
		return "", err
	}
	return policy.HeaderValue()
}

// Header returns the Permissions-Policy header name and rendered value.
func (b PermissionsPolicyBuilder) Header() (string, string, error) {
	value, err := b.HeaderValue()
	return b.HeaderName(), value, err
}

// Validate reports whether policy can be rendered as a Permissions-Policy
// header.
func (p PermissionsPolicy) Validate() error {
	_, err := normalizePermissionsPolicy(p)
	return err
}

// HeaderName returns the Permissions-Policy header name.
func (p PermissionsPolicy) HeaderName() string {
	return HeaderPermissionsPolicy
}

// HeaderValue renders policy as a Permissions-Policy header value.
func (p PermissionsPolicy) HeaderValue() (string, error) {
	return PermissionsPolicyHeaderValue(p)
}

// Header returns the Permissions-Policy header name and rendered value.
func (p PermissionsPolicy) Header() (string, string, error) {
	value, err := p.HeaderValue()
	return p.HeaderName(), value, err
}

// PermissionsPolicyHeaderValue renders policy as a deterministic
// Permissions-Policy header value.
func PermissionsPolicyHeaderValue(policy PermissionsPolicy) (string, error) {
	normalized, err := normalizePermissionsPolicy(policy)
	if err != nil {
		return "", err
	}

	parts := make([]string, 0, len(normalized.Directives))
	for _, directive := range normalized.Directives {
		switch {
		case len(directive.Allowlist) == 0:
			parts = append(parts, directive.Feature+"=()")
		case len(directive.Allowlist) == 1 && directive.Allowlist[0] == PermissionsPolicyAll:
			parts = append(parts, directive.Feature+"=*")
		default:
			parts = append(parts, directive.Feature+"=("+strings.Join(directive.Allowlist, " ")+")")
		}
	}
	return strings.Join(parts, ", "), nil
}

// ValidatePermissionsPolicy reports whether policy can be rendered safely.
func ValidatePermissionsPolicy(policy PermissionsPolicy) error {
	return policy.Validate()
}

// ValidatePermissionsPolicyDirective reports whether directive can be rendered
// safely.
func ValidatePermissionsPolicyDirective(directive PermissionsPolicyDirective) error {
	_, _, err := normalizePermissionsPolicyDirective(directive)
	return err
}

func normalizeXFrameOptionsPolicy(policy XFrameOptionsPolicy) (XFrameOptionsPolicy, error) {
	value := strings.ToUpper(strings.TrimSpace(policy.Value))
	switch value {
	case XFrameOptionsDeny, XFrameOptionsSameOrigin:
		return XFrameOptionsPolicy{Value: value}, nil
	default:
		return XFrameOptionsPolicy{}, fmt.Errorf("%w: invalid X-Frame-Options value %q", ErrInvalidPolicyHeader, policy.Value)
	}
}

func normalizeXContentTypeOptionsPolicy(policy XContentTypeOptionsPolicy) (XContentTypeOptionsPolicy, error) {
	value := strings.ToLower(strings.TrimSpace(policy.Value))
	if value != XContentTypeOptionsNoSniff {
		return XContentTypeOptionsPolicy{}, fmt.Errorf("%w: invalid X-Content-Type-Options value %q", ErrInvalidPolicyHeader, policy.Value)
	}
	return XContentTypeOptionsPolicy{Value: value}, nil
}

func normalizeReferrerPolicy(policy ReferrerPolicy) (ReferrerPolicy, error) {
	if len(policy.Values) == 0 {
		return ReferrerPolicy{}, fmt.Errorf("%w: at least one Referrer-Policy value is required", ErrInvalidPolicyHeader)
	}

	values := make([]string, 0, len(policy.Values))
	seen := make(map[string]struct{}, len(policy.Values))
	for i, value := range policy.Values {
		normalized, err := normalizeReferrerPolicyValue(value)
		if err != nil {
			return ReferrerPolicy{}, fmt.Errorf("%w: value[%d]: %v", ErrInvalidPolicyHeader, i, err)
		}
		if _, ok := seen[normalized]; ok {
			continue
		}
		seen[normalized] = struct{}{}
		values = append(values, normalized)
	}
	return ReferrerPolicy{Values: values}, nil
}

func normalizeReferrerPolicyValue(value string) (string, error) {
	value = strings.ToLower(strings.TrimSpace(value))
	switch value {
	case ReferrerPolicyNoReferrer,
		ReferrerPolicyNoReferrerWhenDowngrade,
		ReferrerPolicyOrigin,
		ReferrerPolicyOriginWhenCrossOrigin,
		ReferrerPolicySameOrigin,
		ReferrerPolicyStrictOrigin,
		ReferrerPolicyStrictOriginWhenCrossOrigin,
		ReferrerPolicyUnsafeURL:
		return value, nil
	default:
		return "", fmt.Errorf("%w: invalid Referrer-Policy value %q", ErrInvalidPolicyHeader, value)
	}
}

type permissionsPolicyDirectiveState struct {
	denied    bool
	allowlist map[string]struct{}
}

func normalizePermissionsPolicy(policy PermissionsPolicy) (PermissionsPolicy, error) {
	if len(policy.Directives) == 0 {
		return PermissionsPolicy{}, fmt.Errorf("%w: at least one Permissions-Policy directive is required", ErrInvalidPolicyHeader)
	}

	byFeature := make(map[string]*permissionsPolicyDirectiveState, len(policy.Directives))
	for i, directive := range policy.Directives {
		feature, allowlist, err := normalizePermissionsPolicyDirective(directive)
		if err != nil {
			return PermissionsPolicy{}, fmt.Errorf("%w: directive[%d]: %v", ErrInvalidPolicyHeader, i, err)
		}

		state, ok := byFeature[feature]
		if !ok {
			state = &permissionsPolicyDirectiveState{allowlist: make(map[string]struct{}, len(allowlist))}
			byFeature[feature] = state
		}
		if len(allowlist) == 0 {
			if len(state.allowlist) > 0 {
				return PermissionsPolicy{}, fmt.Errorf("%w: %s mixes empty allowlist with allowed origins", ErrInvalidPolicyHeader, feature)
			}
			state.denied = true
			continue
		}
		if state.denied {
			return PermissionsPolicy{}, fmt.Errorf("%w: %s mixes empty allowlist with allowed origins", ErrInvalidPolicyHeader, feature)
		}
		for _, value := range allowlist {
			state.allowlist[value] = struct{}{}
		}
	}

	features := make([]string, 0, len(byFeature))
	for feature := range byFeature {
		features = append(features, feature)
	}
	sort.Strings(features)

	directives := make([]PermissionsPolicyDirective, 0, len(features))
	for _, feature := range features {
		state := byFeature[feature]
		allowlist := make([]string, 0, len(state.allowlist))
		for value := range state.allowlist {
			allowlist = append(allowlist, value)
		}
		if len(allowlist) > 1 && containsPolicyHeaderValue(allowlist, PermissionsPolicyAll) {
			return PermissionsPolicy{}, fmt.Errorf("%w: %s mixes * with other allowlist values", ErrInvalidPolicyHeader, feature)
		}
		sort.Slice(allowlist, func(i, j int) bool {
			iRank := permissionsPolicyAllowlistRank(allowlist[i])
			jRank := permissionsPolicyAllowlistRank(allowlist[j])
			if iRank != jRank {
				return iRank < jRank
			}
			return allowlist[i] < allowlist[j]
		})
		directives = append(directives, PermissionsPolicyDirective{
			Feature:   feature,
			Allowlist: allowlist,
		})
	}

	return PermissionsPolicy{Directives: directives}, nil
}

func normalizePermissionsPolicyDirective(directive PermissionsPolicyDirective) (string, []string, error) {
	feature, err := normalizePermissionsPolicyFeature(directive.Feature)
	if err != nil {
		return "", nil, err
	}

	allowlist := make([]string, 0, len(directive.Allowlist))
	for i, value := range directive.Allowlist {
		normalized, err := normalizePermissionsPolicyAllowlistValue(value)
		if err != nil {
			return "", nil, fmt.Errorf("%s allowlist[%d]: %w", feature, i, err)
		}
		allowlist = append(allowlist, normalized)
	}
	if len(allowlist) > 1 && containsPolicyHeaderValue(allowlist, PermissionsPolicyAll) {
		return "", nil, fmt.Errorf("%w: %s mixes * with other allowlist values", ErrInvalidPolicyHeader, feature)
	}
	return feature, allowlist, nil
}

func normalizePermissionsPolicyFeature(feature string) (string, error) {
	feature = strings.ToLower(strings.TrimSpace(feature))
	if feature == "" {
		return "", fmt.Errorf("%w: Permissions-Policy feature is required", ErrInvalidPolicyHeader)
	}
	for i := 0; i < len(feature); i++ {
		c := feature[i]
		if (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-' {
			continue
		}
		return "", fmt.Errorf("%w: Permissions-Policy feature %q contains invalid character %q", ErrInvalidPolicyHeader, feature, c)
	}
	return feature, nil
}

func normalizePermissionsPolicyAllowlistValue(value string) (string, error) {
	value = strings.TrimSpace(value)
	switch strings.ToLower(value) {
	case PermissionsPolicyAll:
		return PermissionsPolicyAll, nil
	case PermissionsPolicySelf:
		return PermissionsPolicySelf, nil
	case "":
		return "", fmt.Errorf("%w: Permissions-Policy allowlist value is required", ErrInvalidPolicyHeader)
	}
	return normalizePermissionsPolicyOrigin(value)
}

func normalizePermissionsPolicyOrigin(origin string) (string, error) {
	origin = strings.TrimSpace(origin)
	if strings.HasPrefix(origin, `"`) || strings.HasSuffix(origin, `"`) {
		if len(origin) < 2 || !strings.HasPrefix(origin, `"`) || !strings.HasSuffix(origin, `"`) {
			return "", fmt.Errorf("%w: malformed quoted origin %q", ErrInvalidPolicyHeader, origin)
		}
		origin = origin[1 : len(origin)-1]
	}
	if containsPolicyHeaderUnsafeOriginChar(origin) {
		return "", fmt.Errorf("%w: unsafe origin %q", ErrInvalidPolicyHeader, origin)
	}

	parsed, err := url.Parse(origin)
	if err != nil {
		return "", fmt.Errorf("%w: invalid origin %q", ErrInvalidPolicyHeader, origin)
	}
	if parsed.Scheme == "" || parsed.Host == "" || parsed.User != nil || parsed.Opaque != "" {
		return "", fmt.Errorf("%w: invalid origin %q", ErrInvalidPolicyHeader, origin)
	}
	if parsed.Path != "" && parsed.Path != "/" {
		return "", fmt.Errorf("%w: origin %q must not include a path", ErrInvalidPolicyHeader, origin)
	}
	if parsed.RawQuery != "" || parsed.Fragment != "" {
		return "", fmt.Errorf("%w: origin %q must not include query or fragment", ErrInvalidPolicyHeader, origin)
	}

	scheme := strings.ToLower(parsed.Scheme)
	if !validCSPScheme(scheme) {
		return "", fmt.Errorf("%w: origin scheme %q is malformed", ErrInvalidPolicyHeader, parsed.Scheme)
	}
	host := strings.ToLower(parsed.Host)
	return `"` + scheme + "://" + host + `"`, nil
}

func permissionsPolicyAllowlistRank(value string) int {
	switch value {
	case PermissionsPolicyAll:
		return 0
	case PermissionsPolicySelf:
		return 1
	default:
		return 2
	}
}

func containsPolicyHeaderValue(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func containsPolicyHeaderUnsafeOriginChar(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			return true
		}
		switch r {
		case '"', '\\', ',', ';', '(', ')':
			return true
		}
	}
	return false
}

func clonePolicyHeaderStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	return append([]string(nil), values...)
}
