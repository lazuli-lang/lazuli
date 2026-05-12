package security

import (
	"errors"
	"fmt"
	"html"
	"net/url"
	"strings"
	"unicode"
)

// SafeString marks a string that is already safe to emit into generated HTML.
// Use this only for trusted, framework-produced fragments; untrusted input
// should pass through the context-specific escape helpers below.
type SafeString string

// String returns the marked safe string.
func (s SafeString) String() string {
	return string(s)
}

// ErrHTMLURLRejected is returned when a URL is unsafe for an HTML URL context.
// Use errors.Is to classify wrapped rejection reasons.
var ErrHTMLURLRejected = errors.New("lazuli/security: html_url_rejected")

// EscapeHTMLText escapes value for an HTML text node context. SafeString values
// are returned unchanged so generated templates can compose trusted fragments
// without double-escaping.
func EscapeHTMLText(value any) SafeString {
	if safe, ok := value.(SafeString); ok {
		return safe
	}
	return SafeString(html.EscapeString(htmlValueString(value)))
}

// EscapeHTMLAttribute escapes value for a quoted HTML attribute context.
// Generated templates should still quote attribute values.
func EscapeHTMLAttribute(value any) SafeString {
	if safe, ok := value.(SafeString); ok {
		return safe
	}
	return SafeString(html.EscapeString(htmlValueString(value)))
}

// EscapeHTMLURL validates and escapes value for an HTML URL attribute context,
// such as href or src. javascript: and data: URLs are rejected before the URL is
// HTML-escaped.
func EscapeHTMLURL(value any) (SafeString, error) {
	safe, err := normalizeHTMLURL(htmlValueString(value))
	if err != nil {
		return "", err
	}
	return SafeString(html.EscapeString(safe)), nil
}

// ValidateHTMLURL rejects URLs that are unsafe for generated HTML URL
// attributes. It accepts relative URLs and non-javascript, non-data absolute
// schemes.
func ValidateHTMLURL(raw string) error {
	_, err := normalizeHTMLURL(raw)
	return err
}

func normalizeHTMLURL(raw string) (string, error) {
	if raw == "" {
		return "", htmlURLReject("empty URL", nil)
	}
	if strings.TrimSpace(raw) != raw {
		return "", htmlURLReject("leading or trailing whitespace", nil)
	}
	for _, r := range raw {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return "", htmlURLReject("whitespace or control character", nil)
		}
	}

	parsed, err := url.Parse(raw)
	if err != nil {
		return "", htmlURLReject("invalid URL", err)
	}
	switch strings.ToLower(parsed.Scheme) {
	case "javascript", "data":
		return "", htmlURLReject("scheme is not allowed", nil)
	}
	return parsed.String(), nil
}

func htmlValueString(value any) string {
	if value == nil {
		return ""
	}
	return fmt.Sprint(value)
}

func htmlURLReject(reason string, err error) error {
	if err != nil {
		return fmt.Errorf("%w: %s: %v", ErrHTMLURLRejected, reason, err)
	}
	return fmt.Errorf("%w: %s", ErrHTMLURLRejected, reason)
}
