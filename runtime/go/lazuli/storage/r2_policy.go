package storage

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"unicode"
)

var (
	// ErrR2PolicyInvalid is returned when a Cloudflare R2 descriptor contains
	// invalid account, bucket, access mode, endpoint, capability, credential, or
	// object-key prefix metadata.
	ErrR2PolicyInvalid = errors.New("lazuli/storage: r2_policy_invalid")
)

// R2AccessMode is the provider-neutral public/private access mode requested
// for a bucket descriptor. It does not grant Cloudflare permissions by itself.
type R2AccessMode string

const (
	R2AccessModePrivate R2AccessMode = "private"
	R2AccessModePublic  R2AccessMode = "public"
)

// R2PolicyDescriptor describes storage-side defaults a future R2 adapter may
// bind. It contains only normalized metadata and performs no provider calls.
type R2PolicyDescriptor struct {
	AccountID    string
	Bucket       string
	AccessMode   R2AccessMode
	EndpointURL  string
	PublicURL    string
	S3           R2S3Compatibility
	ObjectPrefix string
	AccessKeyID  string
	SecretKey    string
}

// R2S3Compatibility records local S3-compatible feature assumptions for a
// future adapter. Disabled capabilities must not set feature flags.
type R2S3Compatibility struct {
	Enabled          bool
	PathStyle        bool
	VirtualHostStyle bool
	PresignedURLs    bool
	MultipartUpload  bool
	Methods          []string
}

// R2EndpointPlan is a deterministic endpoint derivation result.
type R2EndpointPlan struct {
	AccountID  string
	Bucket     string
	S3Endpoint string
	BucketHost string
	BucketURL  string
	PublicURL  string
	AccessMode R2AccessMode
}

// R2ObjectKeyPlan is a dry-run object key plan. Prefix is empty for a bucket
// root plan and otherwise ends with a slash.
type R2ObjectKeyPlan struct {
	Prefix string
	Key    string
	Full   string
}

// R2PolicySummary is safe to log or expose in diagnostics.
type R2PolicySummary struct {
	AccountIDRedacted string
	Bucket            string
	AccessMode        R2AccessMode
	EndpointURL       string
	PublicURL         string
	S3                R2S3Compatibility
	ObjectPrefix      string
	AccessKeyID       string
	SecretKey         string
	HasAccessKeyID    bool
	HasSecretKey      bool
}

// Normalize returns a descriptor with canonical account, bucket, access mode,
// endpoint, capability, credential, and object prefix values.
func (d R2PolicyDescriptor) Normalize() R2PolicyDescriptor {
	d.AccountID = NormalizeR2AccountID(d.AccountID)
	d.Bucket = NormalizeR2Bucket(d.Bucket)
	d.AccessMode = NormalizeR2AccessMode(d.AccessMode)
	d.EndpointURL = normalizeR2URL(d.EndpointURL)
	d.PublicURL = normalizeR2URL(d.PublicURL)
	d.S3 = d.S3.Normalize()
	d.ObjectPrefix = normalizeR2ObjectPrefix(d.ObjectPrefix)
	d.AccessKeyID = strings.TrimSpace(d.AccessKeyID)
	d.SecretKey = strings.TrimSpace(d.SecretKey)
	if d.EndpointURL == "" && d.AccountID != "" {
		d.EndpointURL = BuildR2S3Endpoint(d.AccountID)
	}
	return d
}

// Validate checks the descriptor for structural validity.
func (d R2PolicyDescriptor) Validate() error {
	return ValidateR2PolicyDescriptor(d)
}

// EndpointPlan returns deterministic S3 and public endpoint metadata.
func (d R2PolicyDescriptor) EndpointPlan() (R2EndpointPlan, error) {
	if err := ValidateR2PolicyDescriptor(d); err != nil {
		return R2EndpointPlan{}, err
	}
	d = d.Normalize()
	return DeriveR2EndpointPlan(d.AccountID, d.Bucket, d.AccessMode, d.PublicURL)
}

// PlanObjectKey returns a deterministic key plan below descriptor.ObjectPrefix.
func (d R2PolicyDescriptor) PlanObjectKey(key string, extraPrefixes ...string) (R2ObjectKeyPlan, error) {
	if err := ValidateR2PolicyDescriptor(d); err != nil {
		return R2ObjectKeyPlan{}, err
	}
	prefixes := append([]string{d.Normalize().ObjectPrefix}, extraPrefixes...)
	return PlanR2ObjectKey(key, prefixes...)
}

// RedactedSummary returns a stable, log-safe descriptor view.
func (d R2PolicyDescriptor) RedactedSummary() R2PolicySummary {
	normalized := d.Normalize()
	return R2PolicySummary{
		AccountIDRedacted: redactR2AccountID(normalized.AccountID),
		Bucket:            normalized.Bucket,
		AccessMode:        normalized.AccessMode,
		EndpointURL:       redactR2URL(normalized.EndpointURL),
		PublicURL:         redactR2URL(normalized.PublicURL),
		S3:                normalized.S3,
		ObjectPrefix:      normalized.ObjectPrefix,
		AccessKeyID:       redactR2Secret(normalized.AccessKeyID),
		SecretKey:         redactR2Secret(normalized.SecretKey),
		HasAccessKeyID:    normalized.AccessKeyID != "",
		HasSecretKey:      normalized.SecretKey != "",
	}
}

// Normalize returns canonical S3-compatible capability metadata.
func (c R2S3Compatibility) Normalize() R2S3Compatibility {
	if !c.Enabled {
		return R2S3Compatibility{}
	}
	methods := normalizeR2Methods(c.Methods)
	if len(methods) == 0 {
		methods = []string{"DELETE", "GET", "HEAD", "POST", "PUT"}
	}
	return R2S3Compatibility{
		Enabled:          true,
		PathStyle:        c.PathStyle,
		VirtualHostStyle: c.VirtualHostStyle,
		PresignedURLs:    c.PresignedURLs,
		MultipartUpload:  c.MultipartUpload,
		Methods:          methods,
	}
}

// Validate checks S3-compatible capability metadata.
func (c R2S3Compatibility) Validate() error {
	return ValidateR2S3Compatibility(c)
}

// AllowsMethod reports whether method is allowed by this capability.
func (c R2S3Compatibility) AllowsMethod(method string) bool {
	if !c.Enabled {
		return false
	}
	method, ok := normalizeR2Method(method)
	if !ok {
		return false
	}
	for _, candidate := range c.Normalize().Methods {
		if candidate == method {
			return true
		}
	}
	return false
}

// NormalizeR2AccountID returns the canonical R2 account identifier.
func NormalizeR2AccountID(accountID string) string {
	return strings.ToLower(strings.TrimSpace(accountID))
}

// NormalizeR2Bucket returns the canonical R2 bucket token shape.
func NormalizeR2Bucket(bucket string) string {
	return strings.ToLower(strings.TrimSpace(bucket))
}

// NormalizeR2AccessMode returns the canonical public/private access mode.
func NormalizeR2AccessMode(mode R2AccessMode) R2AccessMode {
	value := strings.ToLower(strings.TrimSpace(string(mode)))
	switch value {
	case "", "private", "none", "disabled":
		return R2AccessModePrivate
	case "public", "public-read", "public_read", "anonymous":
		return R2AccessModePublic
	default:
		return R2AccessMode(value)
	}
}

// BuildR2S3Endpoint returns the account-scoped R2 S3-compatible endpoint URL.
func BuildR2S3Endpoint(accountID string) string {
	accountID = NormalizeR2AccountID(accountID)
	if accountID == "" {
		return ""
	}
	return "https://" + accountID + ".r2.cloudflarestorage.com"
}

// DeriveR2EndpointPlan derives deterministic account, bucket, and optional
// public endpoint metadata. It does not check whether endpoints exist.
func DeriveR2EndpointPlan(accountID, bucket string, mode R2AccessMode, publicURL string) (R2EndpointPlan, error) {
	accountID = NormalizeR2AccountID(accountID)
	bucket = NormalizeR2Bucket(bucket)
	mode = NormalizeR2AccessMode(mode)
	publicURL = normalizeR2URL(publicURL)

	errs := []error{
		ValidateR2AccountID(accountID),
		ValidateR2Bucket(bucket),
		ValidateR2AccessMode(mode),
	}
	if mode == R2AccessModePrivate && publicURL != "" {
		errs = append(errs, fmt.Errorf("%w: private access mode must not set public url", ErrR2PolicyInvalid))
	}
	if publicURL != "" {
		errs = append(errs, validateR2URL(publicURL, "public url"))
	}
	if err := errors.Join(errs...); err != nil {
		return R2EndpointPlan{}, err
	}

	host := bucket + "." + accountID + ".r2.cloudflarestorage.com"
	return R2EndpointPlan{
		AccountID:  accountID,
		Bucket:     bucket,
		S3Endpoint: BuildR2S3Endpoint(accountID),
		BucketHost: host,
		BucketURL:  "https://" + host,
		PublicURL:  publicURL,
		AccessMode: mode,
	}, nil
}

// ValidateR2PolicyDescriptor checks account, bucket, access mode, endpoint,
// S3 compatibility, credentials, and object prefix metadata.
func ValidateR2PolicyDescriptor(descriptor R2PolicyDescriptor) error {
	normalized := descriptor.Normalize()
	errs := []error{
		ValidateR2AccountID(normalized.AccountID),
		ValidateR2Bucket(normalized.Bucket),
		ValidateR2AccessMode(normalized.AccessMode),
		validateR2URL(normalized.EndpointURL, "endpoint url"),
		ValidateR2S3Compatibility(descriptor.S3),
		validateR2ObjectPrefix(normalized.ObjectPrefix),
	}
	if normalized.AccessMode == R2AccessModePrivate && normalized.PublicURL != "" {
		errs = append(errs, fmt.Errorf("%w: private access mode must not set public url", ErrR2PolicyInvalid))
	}
	if normalized.PublicURL != "" {
		errs = append(errs, validateR2URL(normalized.PublicURL, "public url"))
	}
	if normalized.SecretKey != "" && normalized.AccessKeyID == "" {
		errs = append(errs, fmt.Errorf("%w: access key id is required when secret key is set", ErrR2PolicyInvalid))
	}
	return errors.Join(errs...)
}

// ValidateR2AccountID checks the account identifier shape without checking
// ownership or existence.
func ValidateR2AccountID(accountID string) error {
	accountID = NormalizeR2AccountID(accountID)
	if len(accountID) != 32 {
		return fmt.Errorf("%w: account id must be 32 lowercase hex characters", ErrR2PolicyInvalid)
	}
	for _, r := range accountID {
		if r >= '0' && r <= '9' {
			continue
		}
		if r >= 'a' && r <= 'f' {
			continue
		}
		return fmt.Errorf("%w: account id contains invalid character %q", ErrR2PolicyInvalid, r)
	}
	return nil
}

// ValidateR2Bucket checks the bucket token shape without checking availability
// or ownership.
func ValidateR2Bucket(bucket string) error {
	bucket = NormalizeR2Bucket(bucket)
	if len(bucket) < 3 || len(bucket) > 63 {
		return fmt.Errorf("%w: bucket must be 3-63 characters", ErrR2PolicyInvalid)
	}
	if !isR2BucketEdge(bucket[0]) || !isR2BucketEdge(bucket[len(bucket)-1]) {
		return fmt.Errorf("%w: bucket must start and end with a letter or digit", ErrR2PolicyInvalid)
	}
	for _, r := range bucket {
		if r >= 'a' && r <= 'z' {
			continue
		}
		if r >= '0' && r <= '9' {
			continue
		}
		if r == '-' {
			continue
		}
		return fmt.Errorf("%w: bucket contains invalid character %q", ErrR2PolicyInvalid, r)
	}
	return nil
}

// ValidateR2AccessMode checks the public/private bucket access mode.
func ValidateR2AccessMode(mode R2AccessMode) error {
	switch NormalizeR2AccessMode(mode) {
	case R2AccessModePrivate, R2AccessModePublic:
		return nil
	default:
		return fmt.Errorf("%w: access mode %q is unknown", ErrR2PolicyInvalid, mode)
	}
}

// ValidateR2S3Compatibility checks local S3-compatible capability metadata.
func ValidateR2S3Compatibility(capability R2S3Compatibility) error {
	normalized := capability.Normalize()
	if !capability.Enabled {
		if capability.PathStyle || capability.VirtualHostStyle || capability.PresignedURLs || capability.MultipartUpload || len(capability.Methods) > 0 {
			return fmt.Errorf("%w: disabled s3 compatibility must not set feature metadata", ErrR2PolicyInvalid)
		}
		return nil
	}
	if !normalized.PathStyle && !normalized.VirtualHostStyle {
		return fmt.Errorf("%w: s3 compatibility requires at least one addressing style", ErrR2PolicyInvalid)
	}
	for i, method := range normalized.Methods {
		if _, ok := normalizeR2Method(method); !ok {
			return fmt.Errorf("%w: s3 method %d is invalid", ErrR2PolicyInvalid, i)
		}
	}
	return nil
}

// PlanR2ObjectKey normalizes prefix segments and key into a dry-run full object
// key.
func PlanR2ObjectKey(key string, prefixes ...string) (R2ObjectKeyPlan, error) {
	parts := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		prefix = normalizeR2ObjectPrefix(prefix)
		if prefix == "" {
			continue
		}
		if err := validateR2ObjectPrefix(prefix); err != nil {
			return R2ObjectKeyPlan{}, err
		}
		parts = append(parts, prefix)
	}
	key = normalizeR2ObjectKey(key)
	if err := validateR2ObjectKey(key); err != nil {
		return R2ObjectKeyPlan{}, err
	}
	prefix := strings.Join(parts, "/")
	if prefix != "" {
		prefix += "/"
	}
	return R2ObjectKeyPlan{
		Prefix: prefix,
		Key:    key,
		Full:   prefix + key,
	}, nil
}

func normalizeR2Methods(methods []string) []string {
	if len(methods) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(methods))
	normalized := make([]string, 0, len(methods))
	for _, method := range methods {
		method, ok := normalizeR2Method(method)
		if !ok {
			normalized = append(normalized, strings.ToUpper(strings.TrimSpace(method)))
			continue
		}
		if _, ok := seen[method]; ok {
			continue
		}
		seen[method] = struct{}{}
		normalized = append(normalized, method)
	}
	sort.Strings(normalized)
	return normalized
}

func normalizeR2Method(method string) (string, bool) {
	method = strings.ToUpper(strings.TrimSpace(method))
	switch method {
	case "DELETE", "GET", "HEAD", "POST", "PUT":
		return method, true
	default:
		return method, false
	}
}

func normalizeR2URL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	parsed, err := url.Parse(raw)
	if err != nil {
		return raw
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String()
}

func validateR2URL(raw, label string) error {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return fmt.Errorf("%w: %s is required", ErrR2PolicyInvalid, label)
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return fmt.Errorf("%w: %s is invalid", ErrR2PolicyInvalid, label)
	}
	if parsed.Scheme != "https" {
		return fmt.Errorf("%w: %s must use https", ErrR2PolicyInvalid, label)
	}
	return nil
}

func normalizeR2ObjectPrefix(prefix string) string {
	return strings.Trim(strings.TrimSpace(prefix), "/")
}

func normalizeR2ObjectKey(key string) string {
	return strings.Trim(strings.TrimSpace(key), "/")
}

func validateR2ObjectPrefix(prefix string) error {
	prefix = normalizeR2ObjectPrefix(prefix)
	if prefix == "" {
		return nil
	}
	for _, segment := range strings.Split(prefix, "/") {
		if !isR2ObjectPathSegment(segment) {
			return fmt.Errorf("%w: object prefix contains invalid segment %q", ErrR2PolicyInvalid, segment)
		}
	}
	return nil
}

func validateR2ObjectKey(key string) error {
	if key == "" {
		return fmt.Errorf("%w: object key is required", ErrR2PolicyInvalid)
	}
	for _, segment := range strings.Split(key, "/") {
		if !isR2ObjectPathSegment(segment) {
			return fmt.Errorf("%w: object key contains invalid segment %q", ErrR2PolicyInvalid, segment)
		}
	}
	return nil
}

func isR2ObjectPathSegment(segment string) bool {
	if segment == "" || segment == "." || segment == ".." {
		return false
	}
	for _, r := range segment {
		if r == '\\' || unicode.IsControl(r) {
			return false
		}
	}
	return true
}

func isR2BucketEdge(b byte) bool {
	return (b >= 'a' && b <= 'z') || (b >= '0' && b <= '9')
}

func redactR2AccountID(accountID string) string {
	accountID = strings.TrimSpace(accountID)
	if accountID == "" {
		return ""
	}
	if len(accountID) <= 8 {
		return "[redacted]"
	}
	return accountID[:4] + "..." + accountID[len(accountID)-4:]
}

func redactR2Secret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

func redactR2URL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "[redacted]"
	}
	if parsed.Path == "" {
		return parsed.Scheme + "://" + parsed.Host
	}
	return parsed.Scheme + "://" + parsed.Host + parsed.EscapedPath()
}
