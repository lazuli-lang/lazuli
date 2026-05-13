package storage

import (
	"errors"
	"fmt"
	"mime"
	"net/http"
	"strings"
	"time"
)

var (
	// ErrSignedURLPolicyInvalid is returned when a signed URL policy or
	// request contains invalid TTL, method, MIME, or response-header values.
	ErrSignedURLPolicyInvalid = errors.New("lazuli/storage: signed_url_policy_invalid")
)

const (
	// SignedURLMethodGET is the canonical method for signed downloads.
	SignedURLMethodGET = http.MethodGet

	// SignedURLMethodHEAD is the canonical method for signed metadata checks.
	SignedURLMethodHEAD = http.MethodHead

	// SignedURLMethodPUT is the canonical method for signed uploads.
	SignedURLMethodPUT = http.MethodPut

	// SignedURLMethodDELETE is the canonical method for signed deletes.
	SignedURLMethodDELETE = http.MethodDelete
)

// SignedURLPolicy is the provider-neutral contract a signer may enforce before
// it asks an adapter to mint a URL. It describes only request shape; concrete
// adapter signing remains outside this helper.
type SignedURLPolicy struct {
	// MaxAge caps a requested signed URL TTL. Zero means no policy cap; the
	// concrete request TTL must still be positive.
	MaxAge time.Duration

	// Methods is the optional allow-list of HTTP methods that may be signed.
	// Empty means the policy does not restrict method.
	Methods []string

	// ContentTypes is the optional allow-list of MIME types for signed upload
	// or response content. Empty means the policy does not restrict MIME type.
	ContentTypes []MimeType

	// ResponseHeaders are fixed response header overrides that should be
	// embedded into the signed URL when the adapter supports them.
	ResponseHeaders SignedURLResponseHeaders
}

// SignedURLRequest is the request shape validated against a SignedURLPolicy
// before an adapter signs it.
type SignedURLRequest struct {
	Method      string
	TTL         time.Duration
	ContentType string
}

// SignedURLResponseHeaders describes common object response header overrides
// supported by S3-compatible signers. Values are rendered as regular HTTP
// response header names; adapters translate them to provider-specific query
// parameters internally.
type SignedURLResponseHeaders struct {
	CacheControl       string
	ContentDisposition string
	ContentEncoding    string
	ContentLanguage    string
	ContentType        string
	Expires            time.Time
}

// SignedURLDownloadPolicy returns a GET-only policy with the provided max age
// and optional response content-type constraints.
func SignedURLDownloadPolicy(maxAge time.Duration, contentTypes ...MimeType) SignedURLPolicy {
	return signedURLPolicy(maxAge, []string{SignedURLMethodGET}, contentTypes)
}

// SignedURLUploadPolicy returns a PUT-only policy with the provided max age and
// optional request content-type constraints.
func SignedURLUploadPolicy(maxAge time.Duration, contentTypes ...MimeType) SignedURLPolicy {
	return signedURLPolicy(maxAge, []string{SignedURLMethodPUT}, contentTypes)
}

// WithResponseHeaders returns a copy of policy with fixed response headers.
func (p SignedURLPolicy) WithResponseHeaders(headers SignedURLResponseHeaders) SignedURLPolicy {
	p.ResponseHeaders = headers
	return p
}

// Validate checks the policy for structural validity.
func (p SignedURLPolicy) Validate() error {
	return ValidateSignedURLPolicy(p)
}

// ValidateRequest checks request against policy.
func (p SignedURLPolicy) ValidateRequest(request SignedURLRequest) error {
	return ValidateSignedURLRequest(p, request)
}

// AllowsMethod reports whether method is permitted by policy.
func (p SignedURLPolicy) AllowsMethod(method string) bool {
	method, ok := normalizeSignedURLMethod(method)
	if !ok {
		return false
	}
	if len(p.Methods) == 0 {
		return true
	}
	for _, candidate := range p.Methods {
		candidate, ok := normalizeSignedURLMethod(candidate)
		if ok && candidate == method {
			return true
		}
	}
	return false
}

// AllowsContentType reports whether contentType is permitted by policy. Empty
// content type is accepted only when the policy has no content-type constraint.
func (p SignedURLPolicy) AllowsContentType(contentType string) bool {
	if len(p.ContentTypes) == 0 {
		return true
	}
	got, err := parseSignedURLContentType(contentType)
	if err != nil {
		return false
	}
	for _, allowed := range p.ContentTypes {
		if signedURLMimeMatches(allowed, got) {
			return true
		}
	}
	return false
}

// ResponseHeaderValues returns a copy of the policy's response headers using
// canonical HTTP header names.
func (p SignedURLPolicy) ResponseHeaderValues() map[string]string {
	return p.ResponseHeaders.Values()
}

// Validate checks response header override values for valid HTTP header shape.
func (h SignedURLResponseHeaders) Validate() error {
	for name, value := range h.stringValues() {
		if strings.TrimSpace(value) == "" {
			return fmt.Errorf("%w: response header %s is empty", ErrSignedURLPolicyInvalid, name)
		}
		if !isValidSignedURLHeaderValue(value) {
			return fmt.Errorf("%w: response header %s contains invalid control characters", ErrSignedURLPolicyInvalid, name)
		}
	}
	if h.ContentType != "" {
		if _, _, err := mime.ParseMediaType(strings.TrimSpace(h.ContentType)); err != nil {
			return fmt.Errorf("%w: response header Content-Type is invalid: %v", ErrSignedURLPolicyInvalid, err)
		}
	}
	return nil
}

// Values returns a map with canonical HTTP response header names. The map is
// newly allocated and can be mutated by callers.
func (h SignedURLResponseHeaders) Values() map[string]string {
	values := h.stringValues()
	if !h.Expires.IsZero() {
		values["Expires"] = h.Expires.UTC().Format(http.TimeFormat)
	}
	return values
}

// Empty reports whether no response header override is set.
func (h SignedURLResponseHeaders) Empty() bool {
	return h.CacheControl == "" &&
		h.ContentDisposition == "" &&
		h.ContentEncoding == "" &&
		h.ContentLanguage == "" &&
		h.ContentType == "" &&
		h.Expires.IsZero()
}

// ValidateSignedURLPolicy checks that a policy's restrictions are coherent.
func ValidateSignedURLPolicy(policy SignedURLPolicy) error {
	if policy.MaxAge < 0 {
		return fmt.Errorf("%w: max age must be non-negative", ErrSignedURLPolicyInvalid)
	}
	if err := validateSignedURLMethods(policy.Methods); err != nil {
		return err
	}
	if err := validateSignedURLMimeTypes(policy.ContentTypes); err != nil {
		return err
	}
	if err := policy.ResponseHeaders.Validate(); err != nil {
		return err
	}
	return nil
}

// ValidateSignedURLRequest checks request against the policy max age, method
// allow-list, and content-type constraints.
func ValidateSignedURLRequest(policy SignedURLPolicy, request SignedURLRequest) error {
	if err := ValidateSignedURLPolicy(policy); err != nil {
		return err
	}
	if err := ValidateSignedURLTTL(request.TTL, policy.MaxAge); err != nil {
		return err
	}
	if !policy.AllowsMethod(request.Method) {
		return fmt.Errorf("%w: method %q is not allowed", ErrSignedURLPolicyInvalid, request.Method)
	}
	if request.ContentType != "" {
		if _, err := parseSignedURLContentType(request.ContentType); err != nil {
			return fmt.Errorf("%w: content type %q is invalid: %v", ErrSignedURLPolicyInvalid, request.ContentType, err)
		}
	}
	if !policy.AllowsContentType(request.ContentType) {
		return fmt.Errorf("%w: content type %q is not allowed", ErrFileMimeRejected, request.ContentType)
	}
	return nil
}

// ValidateSignedURLTTL checks that ttl is positive and does not exceed maxAge.
// A zero maxAge means no policy cap.
func ValidateSignedURLTTL(ttl, maxAge time.Duration) error {
	if maxAge < 0 {
		return fmt.Errorf("%w: max age must be non-negative", ErrSignedURLPolicyInvalid)
	}
	if ttl <= 0 {
		return fmt.Errorf("%w: ttl must be positive", ErrSignedURLPolicyInvalid)
	}
	if maxAge > 0 && ttl > maxAge {
		return fmt.Errorf("%w: ttl %s exceeds max age %s", ErrSignedURLPolicyInvalid, ttl, maxAge)
	}
	return nil
}

func signedURLPolicy(maxAge time.Duration, methods []string, contentTypes []MimeType) SignedURLPolicy {
	return SignedURLPolicy{
		MaxAge:       maxAge,
		Methods:      append([]string(nil), methods...),
		ContentTypes: append([]MimeType(nil), contentTypes...),
	}
}

func validateSignedURLMethods(methods []string) error {
	seen := make(map[string]int, len(methods))
	for i, method := range methods {
		normalized, ok := normalizeSignedURLMethod(method)
		if !ok {
			return fmt.Errorf("%w: method %d is invalid", ErrSignedURLPolicyInvalid, i)
		}
		if previous, ok := seen[normalized]; ok {
			return fmt.Errorf("%w: method %d duplicates method %d", ErrSignedURLPolicyInvalid, i, previous)
		}
		seen[normalized] = i
	}
	return nil
}

func validateSignedURLMimeTypes(contentTypes []MimeType) error {
	seen := make(map[MimeType]int, len(contentTypes))
	for i, contentType := range contentTypes {
		normalized, err := normalizeSignedURLMimeType(contentType)
		if err != nil {
			return fmt.Errorf("%w: content type %d is invalid: %v", ErrSignedURLPolicyInvalid, i, err)
		}
		if previous, ok := seen[normalized]; ok {
			return fmt.Errorf("%w: content type %d duplicates content type %d", ErrSignedURLPolicyInvalid, i, previous)
		}
		seen[normalized] = i
	}
	return nil
}

func normalizeSignedURLMethod(method string) (string, bool) {
	method = strings.ToUpper(strings.TrimSpace(method))
	return method, method != "" && isSignedURLToken(method)
}

func normalizeSignedURLMimeType(contentType MimeType) (MimeType, error) {
	family := strings.ToLower(strings.TrimSpace(contentType.Family))
	subtype := strings.ToLower(strings.TrimSpace(contentType.Subtype))
	if family == "" || subtype == "" {
		return MimeType{}, fmt.Errorf("family and subtype are required")
	}
	if !isSignedURLToken(family) || !isSignedURLToken(subtype) {
		return MimeType{}, fmt.Errorf("family and subtype must be MIME tokens")
	}
	return MimeType{Family: family, Subtype: subtype}, nil
}

func parseSignedURLContentType(raw string) (MimeType, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return MimeType{}, fmt.Errorf("content type is required")
	}
	mediaType, _, err := mime.ParseMediaType(raw)
	if err != nil {
		return MimeType{}, err
	}
	parts := strings.Split(mediaType, "/")
	if len(parts) != 2 {
		return MimeType{}, fmt.Errorf("content type must contain family/subtype")
	}
	return normalizeSignedURLMimeType(MimeType{Family: parts[0], Subtype: parts[1]})
}

func signedURLMimeMatches(allowed, got MimeType) bool {
	allowed, err := normalizeSignedURLMimeType(allowed)
	if err != nil {
		return false
	}
	got, err = normalizeSignedURLMimeType(got)
	if err != nil {
		return false
	}
	return allowed.Matches(got)
}

func (h SignedURLResponseHeaders) stringValues() map[string]string {
	values := make(map[string]string)
	if h.CacheControl != "" {
		values["Cache-Control"] = h.CacheControl
	}
	if h.ContentDisposition != "" {
		values["Content-Disposition"] = h.ContentDisposition
	}
	if h.ContentEncoding != "" {
		values["Content-Encoding"] = h.ContentEncoding
	}
	if h.ContentLanguage != "" {
		values["Content-Language"] = h.ContentLanguage
	}
	if h.ContentType != "" {
		values["Content-Type"] = h.ContentType
	}
	return values
}

func isValidSignedURLHeaderValue(value string) bool {
	for _, r := range value {
		if r == '\t' {
			continue
		}
		if r < 0x20 || r == 0x7f {
			return false
		}
	}
	return true
}

func isSignedURLToken(value string) bool {
	for _, r := range value {
		if r > 0x7f || !isSignedURLTokenRune(r) {
			return false
		}
	}
	return value != ""
}

func isSignedURLTokenRune(r rune) bool {
	if r >= 'a' && r <= 'z' {
		return true
	}
	if r >= 'A' && r <= 'Z' {
		return true
	}
	if r >= '0' && r <= '9' {
		return true
	}
	switch r {
	case '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~':
		return true
	default:
		return false
	}
}
