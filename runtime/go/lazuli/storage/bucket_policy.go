package storage

import (
	"errors"
	"fmt"
	"strings"
	"time"
	"unicode"
)

var (
	// ErrBucketPolicyInvalid is returned when a provider-neutral bucket policy
	// has an invalid visibility/TTL combination, namespace, MIME class, or
	// lifecycle label.
	ErrBucketPolicyInvalid = errors.New("lazuli/storage: bucket_policy_invalid")
)

// BucketPolicy describes provider-neutral defaults for a logical storage
// bucket. It intentionally stops at validation and key-prefix derivation;
// adapters remain responsible for binding the policy to concrete provider
// settings.
type BucketPolicy struct {
	Name               string
	Visibility         FileVisibility
	SignedTTL          time.Duration
	Namespace          BucketNamespace
	AllowedMimeClasses []MimeClass
	LifecycleLabels    []LifecycleLabel
}

// Validate checks the policy for structural validity.
func (p BucketPolicy) Validate() error {
	return ValidateBucketPolicy(p)
}

// ObjectPrefix resolves the namespace prefix for tenant. Tenant is required
// only for tenant-scoped namespaces.
func (p BucketPolicy) ObjectPrefix(tenant string) (string, error) {
	if err := ValidateBucketPolicy(p); err != nil {
		return "", err
	}
	return p.Namespace.PrefixForTenant(tenant)
}

// AllowsMimeType reports whether mime is allowed by the bucket-level MIME
// classes. An empty class list means the bucket policy does not add a class
// restriction beyond the per-field FileContract.Accept list.
func (p BucketPolicy) AllowsMimeType(mime MimeType) bool {
	if len(p.AllowedMimeClasses) == 0 {
		return true
	}
	for _, class := range p.AllowedMimeClasses {
		if class.Matches(mime) {
			return true
		}
	}
	return false
}

// AllowsContentType parses a Content-Type header and applies AllowsMimeType.
func (p BucketPolicy) AllowsContentType(contentType string) bool {
	return p.AllowsMimeType(parseMime(strings.TrimSpace(contentType)))
}

// BucketPolicyOption mutates a BucketPolicy during construction.
type BucketPolicyOption func(*BucketPolicy)

// PublicBucket returns a public bucket policy.
func PublicBucket(name string, opts ...BucketPolicyOption) BucketPolicy {
	return bucketPolicy(name, VisibilityPublic, 0, opts...)
}

// PrivateBucket returns a private bucket policy.
func PrivateBucket(name string, opts ...BucketPolicyOption) BucketPolicy {
	return bucketPolicy(name, VisibilityPrivate, 0, opts...)
}

// SignedBucket returns a signed bucket policy with a required positive TTL.
func SignedBucket(name string, ttl time.Duration, opts ...BucketPolicyOption) BucketPolicy {
	return bucketPolicy(name, VisibilitySigned, ttl, opts...)
}

// BucketPrefix sets the fixed object-key prefix for a policy namespace.
func BucketPrefix(prefix string) BucketPolicyOption {
	return func(p *BucketPolicy) {
		p.Namespace.Prefix = normalizeBucketPath(prefix)
	}
}

// TenantPrefix marks the policy namespace as tenant-scoped and sets the path
// segment placed before the tenant identifier.
func TenantPrefix(prefix string) BucketPolicyOption {
	return func(p *BucketPolicy) {
		p.Namespace.TenantScoped = true
		p.Namespace.TenantPrefix = normalizeBucketPath(prefix)
	}
}

// AllowedMimeClasses sets the bucket-level MIME class allow-list.
func AllowedMimeClasses(classes ...MimeClass) BucketPolicyOption {
	return func(p *BucketPolicy) {
		p.AllowedMimeClasses = append([]MimeClass(nil), classes...)
	}
}

// LifecycleLabels sets provider-neutral lifecycle labels for objects written
// under the bucket. Labels are normalized to lowercase trimmed tokens.
func LifecycleLabels(labels ...string) BucketPolicyOption {
	return func(p *BucketPolicy) {
		p.LifecycleLabels = make([]LifecycleLabel, 0, len(labels))
		for _, label := range labels {
			p.LifecycleLabels = append(p.LifecycleLabels, LifecycleLabel(normalizeLifecycleLabel(label)))
		}
	}
}

// BucketNamespace describes the object-key namespace attached to a bucket.
// Prefix is an optional fixed prefix. When TenantScoped is true, TenantPrefix
// and the caller-provided tenant identifier are appended below Prefix.
type BucketNamespace struct {
	Prefix       string
	TenantPrefix string
	TenantScoped bool
}

// Normalize trims path separators and whitespace from namespace prefixes.
func (n BucketNamespace) Normalize() BucketNamespace {
	normalized := BucketNamespace{
		Prefix:       normalizeBucketPath(n.Prefix),
		TenantPrefix: normalizeBucketPath(n.TenantPrefix),
		TenantScoped: n.TenantScoped,
	}
	if normalized.TenantPrefix != "" {
		normalized.TenantScoped = true
	}
	return normalized
}

// Validate checks that namespace prefixes are safe object-key path segments.
func (n BucketNamespace) Validate() error {
	return validateBucketNamespace(n)
}

// PrefixForTenant resolves the namespace prefix. The returned prefix is empty
// for the global root and otherwise ends with a slash so callers can append an
// object key segment directly.
func (n BucketNamespace) PrefixForTenant(tenant string) (string, error) {
	n = n.Normalize()
	if err := validateBucketNamespace(n); err != nil {
		return "", err
	}

	parts := make([]string, 0, 3)
	if n.Prefix != "" {
		parts = append(parts, n.Prefix)
	}
	if n.TenantScoped {
		tenant = strings.TrimSpace(tenant)
		if !isBucketPathSegment(tenant) {
			return "", fmt.Errorf("%w: invalid tenant %q", ErrBucketPolicyInvalid, tenant)
		}
		parts = append(parts, n.TenantPrefix, tenant)
	}
	if len(parts) == 0 {
		return "", nil
	}
	return strings.Join(parts, "/") + "/", nil
}

// MimeClass is a bucket-level MIME family allow-list entry.
type MimeClass string

const (
	MimeClassApplication MimeClass = "application"
	MimeClassAudio       MimeClass = "audio"
	MimeClassFont        MimeClass = "font"
	MimeClassImage       MimeClass = "image"
	MimeClassText        MimeClass = "text"
	MimeClassVideo       MimeClass = "video"
)

// String renders the MIME class as its stable lowercase token.
func (c MimeClass) String() string {
	c = normalizeMimeClass(c)
	if isKnownMimeClass(c) {
		return string(c)
	}
	return "unknown"
}

// Matches reports whether the class permits the provided MIME type.
func (c MimeClass) Matches(mime MimeType) bool {
	c = normalizeMimeClass(c)
	if !isKnownMimeClass(c) {
		return false
	}
	family := strings.ToLower(strings.TrimSpace(mime.Family))
	return family == "*" || family == string(c)
}

// LifecycleLabel is a provider-neutral storage lifecycle tag. Adapters may map
// it to object tags, metadata, or provider lifecycle selectors.
type LifecycleLabel string

// String renders the lifecycle label as a normalized token.
func (l LifecycleLabel) String() string {
	return normalizeLifecycleLabel(string(l))
}

// ValidateBucketPolicy checks that the policy can be safely interpreted by
// generated code and adapter bindings.
func ValidateBucketPolicy(policy BucketPolicy) error {
	if err := validateBucketName(policy.Name); err != nil {
		return err
	}
	if !isKnownFileVisibility(policy.Visibility) {
		return fmt.Errorf("%w: unknown visibility %q", ErrBucketPolicyInvalid, policy.Visibility)
	}
	switch policy.Visibility {
	case VisibilitySigned:
		if policy.SignedTTL <= 0 {
			return fmt.Errorf("%w: signed visibility requires a positive ttl", ErrBucketPolicyInvalid)
		}
	case VisibilityPrivate, VisibilityPublic:
		if policy.SignedTTL != 0 {
			return fmt.Errorf("%w: %s visibility must not set a signed ttl", ErrBucketPolicyInvalid, policy.Visibility)
		}
	}

	if err := validateBucketNamespace(policy.Namespace); err != nil {
		return err
	}
	if err := validateMimeClasses(policy.AllowedMimeClasses); err != nil {
		return err
	}
	if err := validateLifecycleLabels(policy.LifecycleLabels); err != nil {
		return err
	}
	return nil
}

func bucketPolicy(name string, visibility FileVisibility, ttl time.Duration, opts ...BucketPolicyOption) BucketPolicy {
	policy := BucketPolicy{
		Name:       strings.TrimSpace(name),
		Visibility: visibility,
		SignedTTL:  ttl,
	}
	for _, opt := range opts {
		if opt != nil {
			opt(&policy)
		}
	}
	return policy
}

func validateBucketName(name string) error {
	if strings.TrimSpace(name) == "" {
		return fmt.Errorf("%w: bucket name is required", ErrBucketPolicyInvalid)
	}
	if strings.TrimSpace(name) != name {
		return fmt.Errorf("%w: bucket name must be trimmed", ErrBucketPolicyInvalid)
	}
	for _, r := range name {
		if r == '/' || r == '\\' || unicode.IsControl(r) {
			return fmt.Errorf("%w: bucket name contains invalid character %q", ErrBucketPolicyInvalid, r)
		}
		if unicode.IsSpace(r) {
			return fmt.Errorf("%w: bucket name contains whitespace", ErrBucketPolicyInvalid)
		}
	}
	return nil
}

func validateBucketNamespace(namespace BucketNamespace) error {
	namespace = namespace.Normalize()
	if err := validateBucketPath("prefix", namespace.Prefix, true); err != nil {
		return err
	}
	if namespace.TenantScoped && namespace.TenantPrefix == "" {
		return fmt.Errorf("%w: tenant prefix is required for tenant-scoped buckets", ErrBucketPolicyInvalid)
	}
	if !namespace.TenantScoped && namespace.TenantPrefix != "" {
		return fmt.Errorf("%w: tenant prefix requires tenant scope", ErrBucketPolicyInvalid)
	}
	return validateBucketPath("tenant prefix", namespace.TenantPrefix, !namespace.TenantScoped)
}

func validateBucketPath(name, value string, allowEmpty bool) error {
	if value == "" {
		if allowEmpty {
			return nil
		}
		return fmt.Errorf("%w: %s is required", ErrBucketPolicyInvalid, name)
	}
	for _, segment := range strings.Split(value, "/") {
		if !isBucketPathSegment(segment) {
			return fmt.Errorf("%w: invalid %s segment %q", ErrBucketPolicyInvalid, name, segment)
		}
	}
	return nil
}

func normalizeBucketPath(value string) string {
	return strings.Trim(strings.TrimSpace(value), "/")
}

func isBucketPathSegment(segment string) bool {
	if segment == "" || segment == "." || segment == ".." {
		return false
	}
	for _, r := range segment {
		if r == '/' || r == '\\' || unicode.IsControl(r) || unicode.IsSpace(r) {
			return false
		}
	}
	return true
}

func validateMimeClasses(classes []MimeClass) error {
	seen := make(map[MimeClass]int, len(classes))
	for i, class := range classes {
		class = normalizeMimeClass(class)
		if !isKnownMimeClass(class) {
			return fmt.Errorf("%w: class %d is unknown", ErrBucketPolicyInvalid, i)
		}
		if previous, ok := seen[class]; ok {
			return fmt.Errorf("%w: class %d duplicates class %d", ErrBucketPolicyInvalid, i, previous)
		}
		seen[class] = i
	}
	return nil
}

func normalizeMimeClass(class MimeClass) MimeClass {
	return MimeClass(strings.ToLower(strings.TrimSpace(string(class))))
}

func isKnownMimeClass(class MimeClass) bool {
	switch class {
	case MimeClassApplication, MimeClassAudio, MimeClassFont, MimeClassImage, MimeClassText, MimeClassVideo:
		return true
	default:
		return false
	}
}

func validateLifecycleLabels(labels []LifecycleLabel) error {
	seen := make(map[string]int, len(labels))
	for i, label := range labels {
		value := normalizeLifecycleLabel(string(label))
		if !isLifecycleLabel(value) {
			return fmt.Errorf("%w: lifecycle label %d is invalid", ErrBucketPolicyInvalid, i)
		}
		if previous, ok := seen[value]; ok {
			return fmt.Errorf("%w: lifecycle label %d duplicates label %d", ErrBucketPolicyInvalid, i, previous)
		}
		seen[value] = i
	}
	return nil
}

func normalizeLifecycleLabel(label string) string {
	return strings.ToLower(strings.TrimSpace(label))
}

func isLifecycleLabel(label string) bool {
	if label == "" {
		return false
	}
	for _, r := range label {
		if r >= 'a' && r <= 'z' {
			continue
		}
		if r >= '0' && r <= '9' {
			continue
		}
		if r == '-' || r == '_' {
			continue
		}
		return false
	}
	return true
}
