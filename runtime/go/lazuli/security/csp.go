package security

import (
	"crypto/rand"
	"encoding/base64"
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
)

const (
	// HeaderContentSecurityPolicy is the enforcing CSP response header.
	HeaderContentSecurityPolicy = "Content-Security-Policy"
	// HeaderContentSecurityPolicyReportOnly is the reporting-only CSP response
	// header.
	HeaderContentSecurityPolicyReportOnly = "Content-Security-Policy-Report-Only"

	// CSPNonceBytes is the number of random bytes used by GenerateCSPNonce.
	CSPNonceBytes = 16

	CSPSourceSelf          = "'self'"
	CSPSourceNone          = "'none'"
	CSPSourceUnsafeInline  = "'unsafe-inline'"
	CSPSourceUnsafeEval    = "'unsafe-eval'"
	CSPSourceStrictDynamic = "'strict-dynamic'"
	CSPSourceReportSample  = "'report-sample'"
	CSPSourceData          = "data:"
	CSPSourceBlob          = "blob:"
)

// ErrInvalidCSPDirective is returned when a CSP directive name or value cannot
// be safely rendered into a Content-Security-Policy header.
var ErrInvalidCSPDirective = errors.New("lazuli/security: invalid_csp_directive")

// CSPDirective is one directive in a Content-Security-Policy header. Sources is
// also used for directive values that are not source lists, such as sandbox
// flags, report endpoints, and trusted-types policy names.
type CSPDirective struct {
	Name    string
	Sources []string
}

// CSPPolicy configures a Content-Security-Policy header. Directives are
// normalized and rendered deterministically by HeaderValue.
type CSPPolicy struct {
	Directives []CSPDirective
	ReportOnly bool
}

// CSPBuilder builds CSPPolicy values with a fluent API.
type CSPBuilder struct {
	directives []CSPDirective
	reportOnly bool
}

// NewCSPBuilder returns an empty Content-Security-Policy builder.
func NewCSPBuilder() CSPBuilder {
	return CSPBuilder{}
}

// Directive appends a CSP directive to the builder. Duplicate directive names
// and sources are merged by Build.
func (b CSPBuilder) Directive(name string, sources ...string) CSPBuilder {
	b.directives = append(b.directives, CSPDirective{
		Name:    name,
		Sources: append([]string(nil), sources...),
	})
	return b
}

// ReportOnly marks the built policy for the Content-Security-Policy-Report-Only
// header.
func (b CSPBuilder) ReportOnly() CSPBuilder {
	b.reportOnly = true
	return b
}

// Build returns a normalized CSPPolicy.
func (b CSPBuilder) Build() (CSPPolicy, error) {
	return normalizeCSPPolicy(CSPPolicy{
		Directives: b.directives,
		ReportOnly: b.reportOnly,
	})
}

// HeaderName returns the CSP header name selected by the builder.
func (b CSPBuilder) HeaderName() string {
	return CSPHeaderName(b.reportOnly)
}

// HeaderValue renders the builder's policy value.
func (b CSPBuilder) HeaderValue() (string, error) {
	policy, err := b.Build()
	if err != nil {
		return "", err
	}
	return policy.HeaderValue()
}

// Header returns the selected CSP header name and rendered value.
func (b CSPBuilder) Header() (string, string, error) {
	value, err := b.HeaderValue()
	return b.HeaderName(), value, err
}

// Validate reports whether policy can be rendered as a CSP header.
func (p CSPPolicy) Validate() error {
	_, err := normalizeCSPPolicy(p)
	return err
}

// HeaderName returns the CSP header name for policy.
func (p CSPPolicy) HeaderName() string {
	return CSPHeaderName(p.ReportOnly)
}

// HeaderValue renders policy as a deterministic CSP header value.
func (p CSPPolicy) HeaderValue() (string, error) {
	return CSPHeaderValue(p)
}

// Header returns policy's selected CSP header name and rendered value.
func (p CSPPolicy) Header() (string, string, error) {
	value, err := p.HeaderValue()
	return p.HeaderName(), value, err
}

// CSPHeaderName returns the enforcing or report-only CSP header name.
func CSPHeaderName(reportOnly bool) string {
	if reportOnly {
		return HeaderContentSecurityPolicyReportOnly
	}
	return HeaderContentSecurityPolicy
}

// CSPHeaderValue renders policy as a deterministic CSP header value.
func CSPHeaderValue(policy CSPPolicy) (string, error) {
	normalized, err := normalizeCSPPolicy(policy)
	if err != nil {
		return "", err
	}

	parts := make([]string, 0, len(normalized.Directives))
	for _, directive := range normalized.Directives {
		if len(directive.Sources) == 0 {
			parts = append(parts, directive.Name)
			continue
		}
		parts = append(parts, directive.Name+" "+strings.Join(directive.Sources, " "))
	}
	return strings.Join(parts, "; "), nil
}

// ValidateCSPDirective reports whether directive can be rendered safely.
func ValidateCSPDirective(directive CSPDirective) error {
	_, _, err := normalizeCSPDirective(directive)
	return err
}

// ValidateCSPDirectiveName reports whether name follows CSP directive token
// syntax.
func ValidateCSPDirectiveName(name string) error {
	_, err := normalizeCSPDirectiveName(name)
	return err
}

// ValidateCSPSource reports whether source can be rendered as one CSP directive
// value token.
func ValidateCSPSource(source string) error {
	_, err := normalizeCSPSource(source)
	return err
}

// GenerateCSPNonce returns a random base64 nonce suitable for CSPNonceSource.
func GenerateCSPNonce() (string, error) {
	nonce := make([]byte, CSPNonceBytes)
	if _, err := rand.Read(nonce); err != nil {
		return "", err
	}
	return base64.RawStdEncoding.EncodeToString(nonce), nil
}

// CSPNonceSource returns a CSP nonce source expression for a nonce value.
func CSPNonceSource(nonce string) string {
	return "'nonce-" + strings.TrimSpace(nonce) + "'"
}

// CSPHashSource returns a CSP hash source expression for sha256, sha384, or
// sha512 digests.
func CSPHashSource(algorithm, digest string) string {
	return "'" + strings.ToLower(strings.TrimSpace(algorithm)) + "-" + strings.TrimSpace(digest) + "'"
}

// CSPSchemeSource returns a normalized scheme source expression such as
// "https:".
func CSPSchemeSource(scheme string) string {
	scheme = strings.ToLower(strings.TrimSpace(scheme))
	scheme = strings.TrimSuffix(scheme, ":")
	return scheme + ":"
}

func normalizeCSPPolicy(policy CSPPolicy) (CSPPolicy, error) {
	if len(policy.Directives) == 0 {
		return CSPPolicy{}, fmt.Errorf("%w: at least one directive is required", ErrInvalidCSPDirective)
	}

	byName := make(map[string]map[string]struct{}, len(policy.Directives))
	for i, directive := range policy.Directives {
		name, sources, err := normalizeCSPDirective(directive)
		if err != nil {
			return CSPPolicy{}, fmt.Errorf("%w: directive[%d]: %v", ErrInvalidCSPDirective, i, err)
		}
		if _, ok := byName[name]; !ok {
			byName[name] = make(map[string]struct{}, len(sources))
		}
		for _, source := range sources {
			byName[name][source] = struct{}{}
		}
	}

	names := make([]string, 0, len(byName))
	for name := range byName {
		names = append(names, name)
	}
	sort.Strings(names)

	directives := make([]CSPDirective, 0, len(names))
	for _, name := range names {
		sources := make([]string, 0, len(byName[name]))
		for source := range byName[name] {
			sources = append(sources, source)
		}
		sort.Strings(sources)
		if len(sources) > 1 && containsCSPSource(sources, CSPSourceNone) {
			return CSPPolicy{}, fmt.Errorf("%w: %s mixes 'none' with other sources", ErrInvalidCSPDirective, name)
		}
		directives = append(directives, CSPDirective{Name: name, Sources: sources})
	}

	return CSPPolicy{
		Directives: directives,
		ReportOnly: policy.ReportOnly,
	}, nil
}

func normalizeCSPDirective(directive CSPDirective) (string, []string, error) {
	name, err := normalizeCSPDirectiveName(directive.Name)
	if err != nil {
		return "", nil, err
	}

	sources := make([]string, 0, len(directive.Sources))
	for i, source := range directive.Sources {
		normalized, err := normalizeCSPSource(source)
		if err != nil {
			return "", nil, fmt.Errorf("%s source[%d]: %w", name, i, err)
		}
		sources = append(sources, normalized)
	}
	if len(sources) > 1 && containsCSPSource(sources, CSPSourceNone) {
		return "", nil, fmt.Errorf("%w: %s mixes 'none' with other sources", ErrInvalidCSPDirective, name)
	}
	return name, sources, nil
}

func normalizeCSPDirectiveName(name string) (string, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if name == "" {
		return "", fmt.Errorf("%w: directive name is required", ErrInvalidCSPDirective)
	}
	for i := 0; i < len(name); i++ {
		c := name[i]
		if (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-' {
			continue
		}
		return "", fmt.Errorf("%w: directive name %q contains invalid character %q", ErrInvalidCSPDirective, name, c)
	}
	return name, nil
}

func normalizeCSPSource(source string) (string, error) {
	source = strings.TrimSpace(source)
	if source == "" {
		return "", fmt.Errorf("%w: source is required", ErrInvalidCSPDirective)
	}
	if containsCSPValueSeparator(source) {
		return "", fmt.Errorf("%w: source %q contains whitespace, control characters, or semicolons", ErrInvalidCSPDirective, source)
	}

	quoteCount := strings.Count(source, "'")
	if quoteCount > 0 {
		if quoteCount != 2 || !strings.HasPrefix(source, "'") || !strings.HasSuffix(source, "'") {
			return "", fmt.Errorf("%w: quoted source %q is malformed", ErrInvalidCSPDirective, source)
		}
		inner := source[1 : len(source)-1]
		if inner == "" {
			return "", fmt.Errorf("%w: quoted source is empty", ErrInvalidCSPDirective)
		}
		switch {
		case strings.HasPrefix(inner, "nonce-"):
			if !validCSPBase64Value(strings.TrimPrefix(inner, "nonce-")) {
				return "", fmt.Errorf("%w: nonce source is malformed", ErrInvalidCSPDirective)
			}
		case strings.HasPrefix(inner, "sha256-"):
			if !validCSPBase64Value(strings.TrimPrefix(inner, "sha256-")) {
				return "", fmt.Errorf("%w: sha256 source is malformed", ErrInvalidCSPDirective)
			}
		case strings.HasPrefix(inner, "sha384-"):
			if !validCSPBase64Value(strings.TrimPrefix(inner, "sha384-")) {
				return "", fmt.Errorf("%w: sha384 source is malformed", ErrInvalidCSPDirective)
			}
		case strings.HasPrefix(inner, "sha512-"):
			if !validCSPBase64Value(strings.TrimPrefix(inner, "sha512-")) {
				return "", fmt.Errorf("%w: sha512 source is malformed", ErrInvalidCSPDirective)
			}
		}
		return source, nil
	}

	if strings.Contains(source, `"`) {
		return "", fmt.Errorf("%w: source %q contains a double quote", ErrInvalidCSPDirective, source)
	}
	if strings.HasSuffix(source, ":") {
		scheme := strings.ToLower(strings.TrimSuffix(source, ":"))
		if !validCSPScheme(scheme) {
			return "", fmt.Errorf("%w: scheme source %q is malformed", ErrInvalidCSPDirective, source)
		}
		return scheme + ":", nil
	}
	return source, nil
}

func containsCSPValueSeparator(value string) bool {
	for _, r := range value {
		if r == ';' || unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func containsCSPSource(sources []string, want string) bool {
	for _, source := range sources {
		if source == want {
			return true
		}
	}
	return false
}

func validCSPScheme(scheme string) bool {
	if scheme == "" {
		return false
	}
	for i := 0; i < len(scheme); i++ {
		c := scheme[i]
		if c >= 'a' && c <= 'z' {
			continue
		}
		if i > 0 && ((c >= '0' && c <= '9') || c == '+' || c == '-' || c == '.') {
			continue
		}
		return false
	}
	return true
}

func validCSPBase64Value(value string) bool {
	if value == "" {
		return false
	}

	data := 0
	padding := 0
	for i := 0; i < len(value); i++ {
		c := value[i]
		if c == '=' {
			padding++
			if padding > 2 {
				return false
			}
			continue
		}
		if padding > 0 {
			return false
		}
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '+' || c == '/' || c == '-' || c == '_' {
			data++
			continue
		}
		return false
	}
	return data > 0
}
