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
	// ErrSpacesPolicyInvalid is returned when a DigitalOcean Spaces descriptor
	// contains invalid region, space, endpoint, CDN, access, capability, or
	// object-key prefix metadata.
	ErrSpacesPolicyInvalid = errors.New("lazuli/storage: spaces_policy_invalid")
)

// SpacesAccessMode is the provider-neutral public/private access mode
// requested for a Space descriptor. It does not update bucket policy by itself.
type SpacesAccessMode string

const (
	SpacesAccessModePrivate    SpacesAccessMode = "private"
	SpacesAccessModePublicRead SpacesAccessMode = "public-read"
)

// SpacesPolicyDescriptor describes the storage-side defaults a future
// DigitalOcean Spaces adapter may bind. It performs no SDK or network calls.
type SpacesPolicyDescriptor struct {
	Region       string
	Space        string
	EndpointURL  string
	CDNEndpoint  string
	AccessMode   SpacesAccessMode
	Capabilities SpacesS3Capabilities
	ObjectPrefix string

	AccessKeyID     string
	SecretAccessKey string
}

// SpacesS3Capabilities records S3-compatible features an adapter may rely on.
type SpacesS3Capabilities struct {
	VirtualHostedStyle bool
	PathStyle          bool
	PresignedURLs      bool
	MultipartUpload    bool
	ObjectACLs         bool
	ObjectTags         bool
	Methods            []string
	MaxSignedURLAge    time.Duration
}

// SpacesObjectKeyPlan is a dry-run object key plan. Prefix is empty for a
// Space root plan and otherwise ends with a slash.
type SpacesObjectKeyPlan struct {
	Prefix string
	Key    string
	Full   string
}

// SpacesRedactedSummary is safe to log or expose in diagnostics.
type SpacesRedactedSummary struct {
	Region              string
	Space               string
	EndpointURL         string
	CDNEndpoint         string
	AccessMode          SpacesAccessMode
	Capabilities        SpacesS3Capabilities
	ObjectPrefix        string
	AccessKeyIDRedacted string
	SecretAccessKey     string
	HasAccessKeyID      bool
	HasSecretAccessKey  bool
}

// Normalize returns a descriptor with canonical region, space, endpoint, CDN,
// access mode, capability, and object prefix values.
func (d SpacesPolicyDescriptor) Normalize() (SpacesPolicyDescriptor, error) {
	var err error
	d.Region = NormalizeSpacesRegion(d.Region)
	d.Space = NormalizeSpacesName(d.Space)
	d.EndpointURL, err = NormalizeSpacesEndpointURL(d.Region, d.EndpointURL)
	if err != nil {
		return SpacesPolicyDescriptor{}, err
	}
	d.CDNEndpoint, err = NormalizeSpacesCDNEndpointURL(d.CDNEndpoint)
	if err != nil {
		return SpacesPolicyDescriptor{}, err
	}
	d.AccessMode = NormalizeSpacesAccessMode(d.AccessMode)
	d.Capabilities = d.Capabilities.Normalize()
	d.ObjectPrefix = normalizeSpacesObjectPrefix(d.ObjectPrefix)
	d.AccessKeyID = strings.TrimSpace(d.AccessKeyID)
	d.SecretAccessKey = strings.TrimSpace(d.SecretAccessKey)
	return d, nil
}

// Validate checks that descriptor metadata can be bound deterministically by an
// adapter.
func (d SpacesPolicyDescriptor) Validate() error {
	return ValidateSpacesPolicyDescriptor(d)
}

// PlanObjectKey returns a deterministic key plan below descriptor.ObjectPrefix.
func (d SpacesPolicyDescriptor) PlanObjectKey(key string, extraPrefixes ...string) (SpacesObjectKeyPlan, error) {
	normalized, err := NormalizeSpacesPolicyDescriptor(d)
	if err != nil {
		return SpacesObjectKeyPlan{}, err
	}
	prefixes := append([]string{normalized.ObjectPrefix}, extraPrefixes...)
	return PlanSpacesObjectKey(key, prefixes...)
}

// RedactedSummary returns a copy of descriptor with secret-bearing values
// redacted for diagnostics.
func (d SpacesPolicyDescriptor) RedactedSummary() SpacesRedactedSummary {
	normalized, err := d.Normalize()
	if err == nil {
		d = normalized
	} else {
		d.Region = NormalizeSpacesRegion(d.Region)
		d.Space = NormalizeSpacesName(d.Space)
		d.AccessMode = NormalizeSpacesAccessMode(d.AccessMode)
		d.Capabilities = d.Capabilities.Normalize()
		d.ObjectPrefix = normalizeSpacesObjectPrefix(d.ObjectPrefix)
	}
	return SpacesRedactedSummary{
		Region:              d.Region,
		Space:               d.Space,
		EndpointURL:         redactSpacesURL(d.EndpointURL),
		CDNEndpoint:         redactSpacesURL(d.CDNEndpoint),
		AccessMode:          d.AccessMode,
		Capabilities:        d.Capabilities,
		ObjectPrefix:        d.ObjectPrefix,
		AccessKeyIDRedacted: redactSpacesSecret(d.AccessKeyID),
		SecretAccessKey:     redactSpacesSecret(d.SecretAccessKey),
		HasAccessKeyID:      strings.TrimSpace(d.AccessKeyID) != "",
		HasSecretAccessKey:  strings.TrimSpace(d.SecretAccessKey) != "",
	}
}

// Normalize returns canonical S3-compatible capability metadata.
func (c SpacesS3Capabilities) Normalize() SpacesS3Capabilities {
	c.Methods = normalizeSpacesCapabilityMethods(c.Methods)
	return c
}

// Validate checks S3-compatible capability metadata.
func (c SpacesS3Capabilities) Validate() error {
	return ValidateSpacesS3Capabilities(c)
}

// AllowsMethod reports whether method is supported by this capability.
func (c SpacesS3Capabilities) AllowsMethod(method string) bool {
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

// NormalizeSpacesPolicyDescriptor returns a canonical descriptor copy.
func NormalizeSpacesPolicyDescriptor(descriptor SpacesPolicyDescriptor) (SpacesPolicyDescriptor, error) {
	return descriptor.Normalize()
}

// ValidateSpacesPolicyDescriptor checks region, space, endpoint, CDN, access
// mode, capability, and object prefix metadata.
func ValidateSpacesPolicyDescriptor(descriptor SpacesPolicyDescriptor) error {
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return err
	}
	errs := []error{
		ValidateSpacesRegion(descriptor.Region),
		ValidateSpacesName(descriptor.Space),
		ValidateSpacesEndpointURL(descriptor.EndpointURL),
		ValidateSpacesCDNEndpointURL(descriptor.CDNEndpoint),
		ValidateSpacesAccessMode(descriptor.AccessMode),
		ValidateSpacesS3Capabilities(descriptor.Capabilities),
		validateSpacesObjectPrefix(descriptor.ObjectPrefix),
	}
	return errors.Join(errs...)
}

// NormalizeSpacesRegion returns the canonical DigitalOcean region token.
func NormalizeSpacesRegion(region string) string {
	return strings.ToLower(strings.ReplaceAll(strings.TrimSpace(region), "_", "-"))
}

// NormalizeSpacesName returns the canonical Space name token.
func NormalizeSpacesName(space string) string {
	return strings.ToLower(strings.TrimSpace(space))
}

// NormalizeSpacesEndpointURL returns a canonical endpoint URL. Empty endpoint
// values derive the standard Spaces origin endpoint from region.
func NormalizeSpacesEndpointURL(region, endpoint string) (string, error) {
	endpoint = strings.TrimSpace(endpoint)
	if endpoint == "" {
		region = NormalizeSpacesRegion(region)
		if err := ValidateSpacesRegion(region); err != nil {
			return "", err
		}
		return "https://" + region + ".digitaloceanspaces.com", nil
	}
	return normalizeSpacesURL(endpoint, false)
}

// NormalizeSpacesCDNEndpointURL returns a canonical CDN endpoint URL. Empty
// CDN endpoint values stay empty because CDN enablement is optional metadata.
func NormalizeSpacesCDNEndpointURL(endpoint string) (string, error) {
	endpoint = strings.TrimSpace(endpoint)
	if endpoint == "" {
		return "", nil
	}
	return normalizeSpacesURL(endpoint, false)
}

// NormalizeSpacesAccessMode returns the canonical access mode token. Empty
// defaults to private.
func NormalizeSpacesAccessMode(mode SpacesAccessMode) SpacesAccessMode {
	value := strings.ToLower(strings.TrimSpace(string(mode)))
	switch value {
	case "", "private":
		return SpacesAccessModePrivate
	case "public", "public_read", "public-read":
		return SpacesAccessModePublicRead
	default:
		return SpacesAccessMode(value)
	}
}

// ValidateSpacesRegion checks a normalized region token.
func ValidateSpacesRegion(region string) error {
	region = NormalizeSpacesRegion(region)
	if region == "" {
		return fmt.Errorf("%w: region is required", ErrSpacesPolicyInvalid)
	}
	if strings.HasPrefix(region, "-") || strings.HasSuffix(region, "-") {
		return fmt.Errorf("%w: region must not start or end with hyphen", ErrSpacesPolicyInvalid)
	}
	for _, r := range region {
		if r >= 'a' && r <= 'z' {
			continue
		}
		if r >= '0' && r <= '9' {
			continue
		}
		if r == '-' {
			continue
		}
		return fmt.Errorf("%w: region contains invalid character %q", ErrSpacesPolicyInvalid, r)
	}
	return nil
}

// ValidateSpacesName checks the Space name token shape without checking
// availability or ownership.
func ValidateSpacesName(space string) error {
	space = NormalizeSpacesName(space)
	if len(space) < 3 || len(space) > 63 {
		return fmt.Errorf("%w: space name must be 3-63 characters", ErrSpacesPolicyInvalid)
	}
	if net.ParseIP(space) != nil {
		return fmt.Errorf("%w: space name must not look like an ip address", ErrSpacesPolicyInvalid)
	}
	if !isSpacesNameEdge(space[0]) || !isSpacesNameEdge(space[len(space)-1]) {
		return fmt.Errorf("%w: space name must start and end with a letter or digit", ErrSpacesPolicyInvalid)
	}
	for _, r := range space {
		if r >= 'a' && r <= 'z' {
			continue
		}
		if r >= '0' && r <= '9' {
			continue
		}
		if r == '-' || r == '.' {
			continue
		}
		return fmt.Errorf("%w: space name contains invalid character %q", ErrSpacesPolicyInvalid, r)
	}
	if strings.Contains(space, "..") || strings.Contains(space, ".-") || strings.Contains(space, "-.") {
		return fmt.Errorf("%w: space name has invalid separator placement", ErrSpacesPolicyInvalid)
	}
	return nil
}

// ValidateSpacesEndpointURL checks an origin endpoint URL.
func ValidateSpacesEndpointURL(endpoint string) error {
	if strings.TrimSpace(endpoint) == "" {
		return fmt.Errorf("%w: endpoint url is required", ErrSpacesPolicyInvalid)
	}
	_, err := normalizeSpacesURL(endpoint, false)
	return err
}

// ValidateSpacesCDNEndpointURL checks optional CDN endpoint URL metadata.
func ValidateSpacesCDNEndpointURL(endpoint string) error {
	if strings.TrimSpace(endpoint) == "" {
		return nil
	}
	_, err := normalizeSpacesURL(endpoint, false)
	return err
}

// ValidateSpacesAccessMode checks the public/private access mode.
func ValidateSpacesAccessMode(mode SpacesAccessMode) error {
	switch NormalizeSpacesAccessMode(mode) {
	case SpacesAccessModePrivate, SpacesAccessModePublicRead:
		return nil
	default:
		return fmt.Errorf("%w: access mode %q is unknown", ErrSpacesPolicyInvalid, mode)
	}
}

// ValidateSpacesS3Capabilities checks local S3-compatible capability metadata.
func ValidateSpacesS3Capabilities(capability SpacesS3Capabilities) error {
	capability = capability.Normalize()
	if capability.MaxSignedURLAge < 0 {
		return fmt.Errorf("%w: max signed url age must be non-negative", ErrSpacesPolicyInvalid)
	}
	if capability.PresignedURLs && len(capability.Methods) == 0 {
		return fmt.Errorf("%w: signed url methods are required when presigned urls are enabled", ErrSpacesPolicyInvalid)
	}
	if !capability.PresignedURLs && (len(capability.Methods) > 0 || capability.MaxSignedURLAge > 0) {
		return fmt.Errorf("%w: disabled presigned urls must not set signer metadata", ErrSpacesPolicyInvalid)
	}
	for i, method := range capability.Methods {
		if _, ok := normalizeSignedURLMethod(method); !ok {
			return fmt.Errorf("%w: signed url method %d is invalid", ErrSpacesPolicyInvalid, i)
		}
	}
	return nil
}

// PlanSpacesObjectKey normalizes prefix segments and key into a dry-run full
// object key.
func PlanSpacesObjectKey(key string, prefixes ...string) (SpacesObjectKeyPlan, error) {
	parts := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		prefix = normalizeSpacesObjectPrefix(prefix)
		if prefix == "" {
			continue
		}
		if err := validateSpacesObjectPrefix(prefix); err != nil {
			return SpacesObjectKeyPlan{}, err
		}
		parts = append(parts, prefix)
	}
	key = normalizeSpacesObjectKey(key)
	if err := validateSpacesObjectKey(key); err != nil {
		return SpacesObjectKeyPlan{}, err
	}
	prefix := strings.Join(parts, "/")
	if prefix != "" {
		prefix += "/"
	}
	return SpacesObjectKeyPlan{
		Prefix: prefix,
		Key:    key,
		Full:   prefix + key,
	}, nil
}

func normalizeSpacesCapabilityMethods(methods []string) []string {
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

func normalizeSpacesURL(raw string, allowEmpty bool) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		if allowEmpty {
			return "", nil
		}
		return "", fmt.Errorf("%w: endpoint url is required", ErrSpacesPolicyInvalid)
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "", fmt.Errorf("%w: endpoint url is invalid", ErrSpacesPolicyInvalid)
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	if parsed.Scheme != "https" && parsed.Scheme != "http" {
		return "", fmt.Errorf("%w: endpoint url scheme must be http or https", ErrSpacesPolicyInvalid)
	}
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
}

func normalizeSpacesObjectPrefix(prefix string) string {
	return strings.Trim(strings.TrimSpace(prefix), "/")
}

func normalizeSpacesObjectKey(key string) string {
	return strings.Trim(strings.TrimSpace(key), "/")
}

func validateSpacesObjectPrefix(prefix string) error {
	prefix = normalizeSpacesObjectPrefix(prefix)
	if prefix == "" {
		return nil
	}
	for _, segment := range strings.Split(prefix, "/") {
		if !isSpacesObjectPathSegment(segment) {
			return fmt.Errorf("%w: object prefix contains invalid segment %q", ErrSpacesPolicyInvalid, segment)
		}
	}
	return nil
}

func validateSpacesObjectKey(key string) error {
	if key == "" {
		return fmt.Errorf("%w: object key is required", ErrSpacesPolicyInvalid)
	}
	for _, segment := range strings.Split(key, "/") {
		if !isSpacesObjectPathSegment(segment) {
			return fmt.Errorf("%w: object key contains invalid segment %q", ErrSpacesPolicyInvalid, segment)
		}
	}
	return nil
}

func isSpacesObjectPathSegment(segment string) bool {
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

func isSpacesNameEdge(b byte) bool {
	return (b >= 'a' && b <= 'z') || (b >= '0' && b <= '9')
}

func redactSpacesSecret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

func redactSpacesURL(raw string) string {
	normalized, err := normalizeSpacesURL(raw, true)
	if err != nil {
		return "[redacted]"
	}
	return normalized
}
