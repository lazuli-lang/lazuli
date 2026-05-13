package storage

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
)

var (
	// ErrGCSPolicyInvalid is returned when a Google Cloud Storage descriptor
	// contains invalid bucket, location, storage class, access, signed URL, or
	// object-key prefix metadata.
	ErrGCSPolicyInvalid = errors.New("lazuli/storage: gcs_policy_invalid")
)

// GCSAccessMode is the provider-neutral public/private access mode requested
// for a bucket descriptor. It does not grant IAM bindings by itself.
type GCSAccessMode string

const (
	GCSAccessModePrivate GCSAccessMode = "private"
	GCSAccessModePublic  GCSAccessMode = "public"
)

// GCSStorageClass is the canonical storage class token used by GCS adapters.
type GCSStorageClass string

const (
	GCSStorageClassStandard GCSStorageClass = "STANDARD"
	GCSStorageClassNearline GCSStorageClass = "NEARLINE"
	GCSStorageClassColdline GCSStorageClass = "COLDLINE"
	GCSStorageClassArchive  GCSStorageClass = "ARCHIVE"
)

// GCSPolicyDescriptor describes the storage-side defaults a future GCS adapter
// may bind. It intentionally contains only normalized data and validation
// metadata; it performs no provider calls.
type GCSPolicyDescriptor struct {
	Bucket       string
	Location     string
	StorageClass GCSStorageClass
	AccessMode   GCSAccessMode
	SignedURL    GCSSignedURLCapability
	ObjectPrefix string
}

// GCSSignedURLCapability records whether a binding can mint signed URLs and
// which constraints should be enforced before calling a signer.
type GCSSignedURLCapability struct {
	Enabled      bool
	Version      string
	MaxAge       time.Duration
	Methods      []string
	CredentialID string
	SignerURL    string
}

// GCSObjectKeyPlan is a dry-run object key plan. Prefix is empty for a bucket
// root plan and otherwise ends with a slash.
type GCSObjectKeyPlan struct {
	Prefix string
	Key    string
	Full   string
}

// GCSRedactedSummary is safe to log or expose in diagnostics.
type GCSRedactedSummary struct {
	Bucket       string
	Location     string
	StorageClass GCSStorageClass
	AccessMode   GCSAccessMode
	SignedURL    GCSSignedURLSummary
	ObjectPrefix string
}

// GCSSignedURLSummary is the redacted signed URL capability metadata.
type GCSSignedURLSummary struct {
	Enabled              bool
	Version              string
	MaxAge               time.Duration
	Methods              []string
	CredentialIDRedacted string
	SignerURLRedacted    string
}

// Normalize returns a descriptor with canonical bucket, location, storage
// class, access mode, signed URL metadata, and object prefix values.
func (d GCSPolicyDescriptor) Normalize() GCSPolicyDescriptor {
	d.Bucket = NormalizeGCSBucket(d.Bucket)
	d.Location = NormalizeGCSLocation(d.Location)
	d.StorageClass = NormalizeGCSStorageClass(d.StorageClass)
	d.AccessMode = NormalizeGCSAccessMode(d.AccessMode)
	d.SignedURL = d.SignedURL.Normalize()
	d.ObjectPrefix = normalizeGCSObjectPrefix(d.ObjectPrefix)
	return d
}

// Validate checks that descriptor metadata can be bound deterministically by an
// adapter.
func (d GCSPolicyDescriptor) Validate() error {
	return ValidateGCSPolicyDescriptor(d)
}

// PlanObjectKey returns a deterministic key plan below descriptor.ObjectPrefix.
func (d GCSPolicyDescriptor) PlanObjectKey(key string, extraPrefixes ...string) (GCSObjectKeyPlan, error) {
	if err := ValidateGCSPolicyDescriptor(d); err != nil {
		return GCSObjectKeyPlan{}, err
	}
	prefixes := append([]string{d.Normalize().ObjectPrefix}, extraPrefixes...)
	return PlanGCSObjectKey(key, prefixes...)
}

// RedactedSummary returns a copy of descriptor with signer details redacted.
func (d GCSPolicyDescriptor) RedactedSummary() GCSRedactedSummary {
	d = d.Normalize()
	return GCSRedactedSummary{
		Bucket:       d.Bucket,
		Location:     d.Location,
		StorageClass: d.StorageClass,
		AccessMode:   d.AccessMode,
		SignedURL:    d.SignedURL.RedactedSummary(),
		ObjectPrefix: d.ObjectPrefix,
	}
}

// Normalize returns canonical signed URL capability metadata.
func (c GCSSignedURLCapability) Normalize() GCSSignedURLCapability {
	c.Version = strings.ToLower(strings.TrimSpace(c.Version))
	c.CredentialID = strings.TrimSpace(c.CredentialID)
	c.SignerURL = strings.TrimSpace(c.SignerURL)
	c.Methods = normalizeGCSSignedURLMethods(c.Methods)
	return c
}

// Validate checks signed URL capability metadata without contacting a signer.
func (c GCSSignedURLCapability) Validate() error {
	return ValidateGCSSignedURLCapability(c)
}

// AllowsMethod reports whether method is supported by this capability.
func (c GCSSignedURLCapability) AllowsMethod(method string) bool {
	if !c.Enabled {
		return false
	}
	method, ok := normalizeSignedURLMethod(method)
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

// RedactedSummary returns signed URL capability metadata with signer details
// redacted for diagnostics.
func (c GCSSignedURLCapability) RedactedSummary() GCSSignedURLSummary {
	c = c.Normalize()
	return GCSSignedURLSummary{
		Enabled:              c.Enabled,
		Version:              c.Version,
		MaxAge:               c.MaxAge,
		Methods:              append([]string(nil), c.Methods...),
		CredentialIDRedacted: redactGCSSecret(c.CredentialID),
		SignerURLRedacted:    redactGCSURL(c.SignerURL),
	}
}

// NormalizeGCSBucket returns the canonical bucket token shape.
func NormalizeGCSBucket(bucket string) string {
	return strings.ToLower(strings.TrimSpace(bucket))
}

// NormalizeGCSLocation returns the canonical GCS location token shape.
func NormalizeGCSLocation(location string) string {
	return strings.ToUpper(strings.ReplaceAll(strings.TrimSpace(location), "_", "-"))
}

// NormalizeGCSStorageClass returns the canonical storage class token. Empty
// defaults to STANDARD.
func NormalizeGCSStorageClass(class GCSStorageClass) GCSStorageClass {
	value := strings.ToUpper(strings.ReplaceAll(strings.TrimSpace(string(class)), "-", "_"))
	switch value {
	case "":
		return GCSStorageClassStandard
	case "STANDARD", "REGIONAL", "MULTI_REGIONAL", "DURABLE_REDUCED_AVAILABILITY":
		return GCSStorageClassStandard
	case "NEARLINE":
		return GCSStorageClassNearline
	case "COLDLINE":
		return GCSStorageClassColdline
	case "ARCHIVE":
		return GCSStorageClassArchive
	default:
		return GCSStorageClass(value)
	}
}

// NormalizeGCSAccessMode returns the canonical access mode token.
func NormalizeGCSAccessMode(mode GCSAccessMode) GCSAccessMode {
	value := strings.ToLower(strings.TrimSpace(string(mode)))
	switch value {
	case "", "private", "uniform", "authenticated":
		return GCSAccessModePrivate
	case "public", "public-read", "public_read":
		return GCSAccessModePublic
	default:
		return GCSAccessMode(value)
	}
}

// ValidateGCSPolicyDescriptor checks bucket, location, storage class, access
// mode, signed URL capability, and object prefix metadata.
func ValidateGCSPolicyDescriptor(descriptor GCSPolicyDescriptor) error {
	descriptor = descriptor.Normalize()
	errs := []error{
		ValidateGCSBucket(descriptor.Bucket),
		ValidateGCSLocation(descriptor.Location),
		ValidateGCSStorageClass(descriptor.StorageClass),
		ValidateGCSAccessMode(descriptor.AccessMode),
		ValidateGCSSignedURLCapability(descriptor.SignedURL),
		validateGCSObjectPrefix(descriptor.ObjectPrefix),
	}
	return errors.Join(errs...)
}

// ValidateGCSBucket checks the bucket token shape without checking
// availability or ownership.
func ValidateGCSBucket(bucket string) error {
	bucket = NormalizeGCSBucket(bucket)
	if len(bucket) < 3 || len(bucket) > 63 {
		return fmt.Errorf("%w: bucket must be 3-63 characters", ErrGCSPolicyInvalid)
	}
	if net.ParseIP(bucket) != nil {
		return fmt.Errorf("%w: bucket must not look like an ip address", ErrGCSPolicyInvalid)
	}
	if strings.HasPrefix(bucket, "goog") || strings.Contains(bucket, "google") {
		return fmt.Errorf("%w: bucket uses a reserved name", ErrGCSPolicyInvalid)
	}
	if !isGCSBucketEdge(bucket[0]) || !isGCSBucketEdge(bucket[len(bucket)-1]) {
		return fmt.Errorf("%w: bucket must start and end with a letter or digit", ErrGCSPolicyInvalid)
	}
	for _, r := range bucket {
		if r >= 'a' && r <= 'z' {
			continue
		}
		if r >= '0' && r <= '9' {
			continue
		}
		if r == '-' || r == '_' || r == '.' {
			continue
		}
		return fmt.Errorf("%w: bucket contains invalid character %q", ErrGCSPolicyInvalid, r)
	}
	return nil
}

// ValidateGCSLocation checks the normalized location token.
func ValidateGCSLocation(location string) error {
	location = NormalizeGCSLocation(location)
	if location == "" {
		return fmt.Errorf("%w: location is required", ErrGCSPolicyInvalid)
	}
	for _, r := range location {
		if r >= 'A' && r <= 'Z' {
			continue
		}
		if r >= '0' && r <= '9' {
			continue
		}
		if r == '-' {
			continue
		}
		return fmt.Errorf("%w: location contains invalid character %q", ErrGCSPolicyInvalid, r)
	}
	return nil
}

// ValidateGCSStorageClass checks that class is one of the supported canonical
// provider-neutral storage classes.
func ValidateGCSStorageClass(class GCSStorageClass) error {
	switch NormalizeGCSStorageClass(class) {
	case GCSStorageClassStandard, GCSStorageClassNearline, GCSStorageClassColdline, GCSStorageClassArchive:
		return nil
	default:
		return fmt.Errorf("%w: storage class %q is unknown", ErrGCSPolicyInvalid, class)
	}
}

// ValidateGCSAccessMode checks the public/private bucket access mode.
func ValidateGCSAccessMode(mode GCSAccessMode) error {
	switch NormalizeGCSAccessMode(mode) {
	case GCSAccessModePrivate, GCSAccessModePublic:
		return nil
	default:
		return fmt.Errorf("%w: access mode %q is unknown", ErrGCSPolicyInvalid, mode)
	}
}

// ValidateGCSSignedURLCapability checks local signed URL capability metadata.
func ValidateGCSSignedURLCapability(capability GCSSignedURLCapability) error {
	capability = capability.Normalize()
	if !capability.Enabled {
		if capability.Version != "" || capability.MaxAge != 0 || len(capability.Methods) > 0 || capability.CredentialID != "" || capability.SignerURL != "" {
			return fmt.Errorf("%w: disabled signed url capability must not set signer metadata", ErrGCSPolicyInvalid)
		}
		return nil
	}
	if capability.Version != "v2" && capability.Version != "v4" {
		return fmt.Errorf("%w: signed url version must be v2 or v4", ErrGCSPolicyInvalid)
	}
	if capability.MaxAge <= 0 {
		return fmt.Errorf("%w: signed url max age must be positive", ErrGCSPolicyInvalid)
	}
	if len(capability.Methods) == 0 {
		return fmt.Errorf("%w: signed url methods are required", ErrGCSPolicyInvalid)
	}
	for i, method := range capability.Methods {
		if _, ok := normalizeSignedURLMethod(method); !ok {
			return fmt.Errorf("%w: signed url method %d is invalid", ErrGCSPolicyInvalid, i)
		}
	}
	if capability.CredentialID == "" {
		return fmt.Errorf("%w: signed url credential id is required", ErrGCSPolicyInvalid)
	}
	if capability.SignerURL != "" {
		parsed, err := url.Parse(capability.SignerURL)
		if err != nil || parsed.Scheme == "" || parsed.Host == "" {
			return fmt.Errorf("%w: signed url signer url is invalid", ErrGCSPolicyInvalid)
		}
	}
	return nil
}

// PlanGCSObjectKey normalizes prefix segments and key into a dry-run full
// object key.
func PlanGCSObjectKey(key string, prefixes ...string) (GCSObjectKeyPlan, error) {
	parts := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		prefix = normalizeGCSObjectPrefix(prefix)
		if prefix == "" {
			continue
		}
		if err := validateGCSObjectPrefix(prefix); err != nil {
			return GCSObjectKeyPlan{}, err
		}
		parts = append(parts, prefix)
	}
	key = normalizeGCSObjectKey(key)
	if err := validateGCSObjectKey(key); err != nil {
		return GCSObjectKeyPlan{}, err
	}
	prefix := strings.Join(parts, "/")
	if prefix != "" {
		prefix += "/"
	}
	return GCSObjectKeyPlan{
		Prefix: prefix,
		Key:    key,
		Full:   prefix + key,
	}, nil
}

func normalizeGCSSignedURLMethods(methods []string) []string {
	if len(methods) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(methods))
	normalized := make([]string, 0, len(methods))
	for _, method := range methods {
		method, ok := normalizeSignedURLMethod(method)
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

func normalizeGCSObjectPrefix(prefix string) string {
	return strings.Trim(strings.TrimSpace(prefix), "/")
}

func normalizeGCSObjectKey(key string) string {
	return strings.Trim(strings.TrimSpace(key), "/")
}

func validateGCSObjectPrefix(prefix string) error {
	prefix = normalizeGCSObjectPrefix(prefix)
	if prefix == "" {
		return nil
	}
	for _, segment := range strings.Split(prefix, "/") {
		if !isGCSObjectPathSegment(segment) {
			return fmt.Errorf("%w: object prefix contains invalid segment %q", ErrGCSPolicyInvalid, segment)
		}
	}
	return nil
}

func validateGCSObjectKey(key string) error {
	if key == "" {
		return fmt.Errorf("%w: object key is required", ErrGCSPolicyInvalid)
	}
	for _, segment := range strings.Split(key, "/") {
		if !isGCSObjectPathSegment(segment) {
			return fmt.Errorf("%w: object key contains invalid segment %q", ErrGCSPolicyInvalid, segment)
		}
	}
	return nil
}

func isGCSObjectPathSegment(segment string) bool {
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

func isGCSBucketEdge(b byte) bool {
	return (b >= 'a' && b <= 'z') || (b >= '0' && b <= '9')
}

func redactGCSSecret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

func redactGCSURL(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return ""
	}
	parsed, err := url.Parse(value)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "[redacted]"
	}
	return parsed.Scheme + "://" + parsed.Host + "/[redacted]"
}
