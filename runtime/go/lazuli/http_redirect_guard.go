package lazuli

import (
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"unicode"
)

// ErrRedirectRejected is returned when a redirect target fails RedirectGuard
// validation. Use errors.Is to classify wrapped rejection reasons.
var ErrRedirectRejected = errors.New("lazuli: redirect rejected")

// RedirectGuard controls which redirect targets SafeRedirectURL accepts.
// Relative URLs are allowed by default. Absolute URLs are accepted only when
// both their scheme and host match the configured allow lists.
type RedirectGuard struct {
	// DenyRelative rejects relative redirect targets. When false, local paths
	// such as "/dashboard" and "settings/profile" are accepted.
	DenyRelative bool

	// AllowedSchemes lists schemes accepted for absolute URLs, typically
	// "https" and, for local development, "http".
	AllowedSchemes []string

	// AllowedHosts lists hosts accepted for absolute URLs. Hosts are matched
	// case-insensitively against URL.Host; entries without a port only match
	// redirect targets that also omit a port.
	AllowedHosts []string
}

// SafeRedirectURL validates raw as a redirect target and returns the canonical
// URL string that may be written to a Location header.
func SafeRedirectURL(raw string, config RedirectGuard) (string, error) {
	if err := validateRedirectRaw(raw); err != nil {
		return "", err
	}

	u, err := url.Parse(raw)
	if err != nil {
		return "", redirectReject("invalid URL", err)
	}
	if strings.HasPrefix(raw, "//") {
		return "", redirectReject("scheme-relative URL", nil)
	}
	if u.User != nil {
		return "", redirectReject("userinfo is not allowed", nil)
	}
	if err := validateRedirectPath(u.Path); err != nil {
		return "", err
	}

	if u.IsAbs() {
		if u.Host == "" {
			return "", redirectReject("absolute URL missing host", nil)
		}
		if !redirectSchemeAllowed(u.Scheme, config.AllowedSchemes) {
			return "", redirectReject("scheme is not allowed", nil)
		}
		if !redirectHostAllowed(u, config.AllowedHosts) {
			return "", redirectReject("host is not allowed", nil)
		}
		return u.String(), nil
	}

	if u.Host != "" {
		return "", redirectReject("host is not allowed on relative URL", nil)
	}
	if strings.HasPrefix(u.Path, "//") {
		return "", redirectReject("scheme-relative URL", nil)
	}
	if config.DenyRelative {
		return "", redirectReject("relative URL is not allowed", nil)
	}
	return u.String(), nil
}

// Redirect validates target with SafeRedirectURL, then delegates to
// http.Redirect. It returns an error without writing a response when the target
// is rejected.
func Redirect(w http.ResponseWriter, r *http.Request, target string, status int, config RedirectGuard) error {
	safe, err := SafeRedirectURL(target, config)
	if err != nil {
		return err
	}
	http.Redirect(w, r, safe, status)
	return nil
}

func validateRedirectRaw(raw string) error {
	if raw == "" {
		return redirectReject("empty URL", nil)
	}
	if strings.TrimSpace(raw) != raw {
		return redirectReject("leading or trailing whitespace", nil)
	}
	for _, r := range raw {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return redirectReject("whitespace or control character", nil)
		}
		if r == '\\' {
			return redirectReject("backslash is not allowed", nil)
		}
	}
	if containsEscapedRedirectControl(raw) {
		return redirectReject("escaped control character", nil)
	}
	return nil
}

func validateRedirectPath(path string) error {
	if strings.Contains(path, `\`) {
		return redirectReject("backslash is not allowed", nil)
	}
	for _, segment := range strings.Split(path, "/") {
		if segment == ".." {
			return redirectReject("path traversal segment", nil)
		}
	}
	return nil
}

func redirectSchemeAllowed(scheme string, allowed []string) bool {
	scheme = strings.ToLower(scheme)
	for _, candidate := range allowed {
		if strings.ToLower(strings.TrimSpace(candidate)) == scheme {
			return true
		}
	}
	return false
}

func redirectHostAllowed(u *url.URL, allowed []string) bool {
	host := strings.ToLower(u.Host)
	hostname := strings.ToLower(u.Hostname())
	hasPort := u.Port() != ""
	for _, candidate := range allowed {
		candidate = strings.ToLower(strings.TrimSpace(candidate))
		if candidate == "" {
			continue
		}
		if candidate == host {
			return true
		}
		if !hasPort && candidate == hostname {
			return true
		}
	}
	return false
}

func containsEscapedRedirectControl(s string) bool {
	for i := 0; i+2 < len(s); i++ {
		if s[i] != '%' || !isRedirectHex(s[i+1]) || !isRedirectHex(s[i+2]) {
			continue
		}
		b := redirectHexValue(s[i+1])<<4 | redirectHexValue(s[i+2])
		if b < 0x20 || b == 0x7f {
			return true
		}
	}
	return false
}

func isRedirectHex(b byte) bool {
	return ('0' <= b && b <= '9') || ('a' <= b && b <= 'f') || ('A' <= b && b <= 'F')
}

func redirectHexValue(b byte) byte {
	switch {
	case '0' <= b && b <= '9':
		return b - '0'
	case 'a' <= b && b <= 'f':
		return b - 'a' + 10
	default:
		return b - 'A' + 10
	}
}

func redirectReject(reason string, err error) error {
	if err != nil {
		return fmt.Errorf("%w: %s: %v", ErrRedirectRejected, reason, err)
	}
	return fmt.Errorf("%w: %s", ErrRedirectRejected, reason)
}
