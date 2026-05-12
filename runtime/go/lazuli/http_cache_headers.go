package lazuli

import (
	"net/http"
	"strings"
)

// CacheHeaders configures common HTTP cache response headers.
type CacheHeaders struct {
	// CacheControl is written as the Cache-Control header when non-empty.
	//
	// Example values include "no-store" and "public, max-age=60".
	CacheControl string

	// ETag is written as the ETag header when non-empty. Raw values are
	// normalized with ETag before being written.
	ETag string

	// Vary appends Vary header values when non-empty.
	Vary []string
}

// Apply writes the configured cache headers to w.
func (h CacheHeaders) Apply(w http.ResponseWriter) {
	header := w.Header()
	if h.CacheControl != "" {
		header.Set("Cache-Control", h.CacheControl)
	}
	if h.ETag != "" {
		header.Set("ETag", ETag(h.ETag))
	}
	for _, vary := range h.Vary {
		if vary = strings.TrimSpace(vary); vary != "" {
			header.Add("Vary", vary)
		}
	}
}

// CacheHeadersMiddleware applies cache headers and handles matching
// If-None-Match conditional GET and HEAD requests.
func CacheHeadersMiddleware(config CacheHeaders) Middleware {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			config.Apply(w)
			if WriteNotModifiedIfMatch(w, r, config.ETag) {
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// ETag returns value as a strong HTTP entity tag. Values already formatted as
// strong or weak entity tags are returned unchanged.
func ETag(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return ""
	}
	if isEntityTag(value) {
		return value
	}
	return `"` + sanitizeETagValue(value) + `"`
}

// WriteNotModifiedIfMatch writes a 304 Not Modified response and returns true
// when r is a GET or HEAD request with an If-None-Match header matching etag.
// It returns false without writing a response for other methods or misses.
func WriteNotModifiedIfMatch(w http.ResponseWriter, r *http.Request, etag string) bool {
	etag = ETag(etag)
	if etag == "" || (r.Method != http.MethodGet && r.Method != http.MethodHead) {
		return false
	}
	if !ifNoneMatch(r.Header.Get("If-None-Match"), etag) {
		return false
	}

	header := w.Header()
	header.Set("ETag", etag)
	header.Del("Content-Length")
	header.Del("Content-Type")
	w.WriteHeader(http.StatusNotModified)
	return true
}

func sanitizeETagValue(value string) string {
	var b strings.Builder
	const hex = "0123456789abcdef"
	for _, r := range value {
		if isETagChar(r) {
			b.WriteRune(r)
			continue
		}
		b.WriteString(`\x`)
		b.WriteByte(hex[byte(r)>>4])
		b.WriteByte(hex[byte(r)&0x0f])
	}
	return b.String()
}

func isEntityTag(value string) bool {
	_, ok := entityTagOpaqueValue(value)
	return ok
}

func ifNoneMatch(header, etag string) bool {
	for header = strings.TrimSpace(header); header != ""; header = strings.TrimSpace(header) {
		if header[0] == ',' {
			header = header[1:]
			continue
		}
		if strings.HasPrefix(header, "*") {
			rest := strings.TrimSpace(header[1:])
			if rest == "" || rest[0] == ',' {
				return true
			}
			return false
		}

		tag, rest, ok := nextEntityTag(header)
		if !ok {
			return false
		}
		if weakEntityTagMatch(tag, etag) {
			return true
		}
		header = rest
	}
	return false
}

func nextEntityTag(header string) (tag, rest string, ok bool) {
	start := 0
	if strings.HasPrefix(header, "W/") {
		start = 2
	}
	if start >= len(header) || header[start] != '"' {
		return "", "", false
	}

	end := start + 1
	for end < len(header) && header[end] != '"' {
		end++
	}
	if end >= len(header) {
		return "", "", false
	}

	tag = header[:end+1]
	rest = strings.TrimSpace(header[end+1:])
	if rest == "" {
		return tag, "", true
	}
	if rest[0] != ',' {
		return "", "", false
	}
	return tag, rest[1:], true
}

func weakEntityTagMatch(a, b string) bool {
	av, aok := entityTagOpaqueValue(a)
	bv, bok := entityTagOpaqueValue(b)
	return aok && bok && av == bv
}

func entityTagOpaqueValue(value string) (string, bool) {
	value = strings.TrimSpace(value)
	if strings.HasPrefix(value, "W/") {
		value = value[2:]
	}
	if len(value) < 2 || value[0] != '"' || value[len(value)-1] != '"' {
		return "", false
	}
	for _, r := range value[1 : len(value)-1] {
		if !isETagChar(r) {
			return "", false
		}
	}
	return value[1 : len(value)-1], true
}

func isETagChar(r rune) bool {
	return r == 0x21 || (r >= 0x23 && r <= 0x7e) || r >= 0x80
}
