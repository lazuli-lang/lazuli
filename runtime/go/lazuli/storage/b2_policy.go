package storage

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
)

var (
	// ErrB2PolicyInvalid is returned when a Backblaze B2 descriptor contains
	// invalid bucket, application key, region, endpoint, S3-compatible mode, or
	// object-key prefix metadata.
	ErrB2PolicyInvalid = errors.New("lazuli/storage: b2_policy_invalid")
)

// B2PolicyDescriptor describes provider-neutral Backblaze B2 bucket metadata
// for future adapter bindings. It performs no SDK calls and does not imply
// that any bucket or application key exists.
type B2PolicyDescriptor struct {
	Bucket         string
	Region         string
	EndpointURL    string
	ApplicationKey B2ApplicationKeyMetadata
	S3Compatible   B2S3CompatibleMode
	ObjectPrefix   string
}

// B2ApplicationKeyMetadata records local application-key metadata. The
// ApplicationKey value is secret material and is redacted from summaries.
type B2ApplicationKeyMetadata struct {
	KeyID          string
	KeyName        string
	ApplicationKey string
	Capabilities   []string
	ExpiresAt      time.Time
}

// B2S3AddressingStyle is the provider-neutral addressing style requested for
// S3-compatible B2 access.
type B2S3AddressingStyle string

const (
	B2S3AddressingStyleVirtualHost B2S3AddressingStyle = "virtual_host"
	B2S3AddressingStylePath        B2S3AddressingStyle = "path"
)

// B2S3CompatibleMode records whether a future adapter may use Backblaze B2's
// S3-compatible API and which local endpoint metadata should be bound.
type B2S3CompatibleMode struct {
	Enabled         bool
	Region          string
	EndpointURL     string
	AddressingStyle B2S3AddressingStyle
}

// B2ObjectKeyPlan is a dry-run object key plan. Prefix is empty for a bucket
// root plan and otherwise ends with a slash.
type B2ObjectKeyPlan struct {
	Prefix string
	Key    string
	Full   string
}

// B2RedactedSummary is safe to log or expose in diagnostics.
type B2RedactedSummary struct {
	Bucket         string
	Region         string
	EndpointURL    string
	ApplicationKey B2ApplicationKeySummary
	S3Compatible   B2S3CompatibleSummary
	ObjectPrefix   string
}

// B2ApplicationKeySummary is redacted application-key metadata.
type B2ApplicationKeySummary struct {
	KeyIDRedacted          string
	KeyName                string
	ApplicationKeyRedacted string
	Capabilities           []string
	ExpiresAt              time.Time
	HasApplicationKey      bool
}

// B2S3CompatibleSummary is redacted S3-compatible mode metadata.
type B2S3CompatibleSummary struct {
	Enabled         bool
	Region          string
	EndpointURL     string
	AddressingStyle B2S3AddressingStyle
}

// Normalize returns a descriptor with canonical bucket, region, endpoint,
// application-key metadata, S3-compatible mode, and object prefix values.
func (d B2PolicyDescriptor) Normalize() B2PolicyDescriptor {
	d.Bucket = NormalizeB2Bucket(d.Bucket)
	d.Region = NormalizeB2Region(d.Region)
	d.EndpointURL = NormalizeB2EndpointURL(d.EndpointURL)
	d.ApplicationKey = d.ApplicationKey.Normalize()
	d.S3Compatible = d.S3Compatible.Normalize()
	d.ObjectPrefix = normalizeB2ObjectPrefix(d.ObjectPrefix)
	return d
}

// Validate checks that descriptor metadata can be bound deterministically by an
// adapter.
func (d B2PolicyDescriptor) Validate() error {
	return ValidateB2PolicyDescriptor(d)
}

// PlanObjectKey returns a deterministic key plan below descriptor.ObjectPrefix.
func (d B2PolicyDescriptor) PlanObjectKey(key string, extraPrefixes ...string) (B2ObjectKeyPlan, error) {
	if err := ValidateB2PolicyDescriptor(d); err != nil {
		return B2ObjectKeyPlan{}, err
	}
	prefixes := append([]string{d.Normalize().ObjectPrefix}, extraPrefixes...)
	return PlanB2ObjectKey(key, prefixes...)
}

// RedactedSummary returns a copy of descriptor with credentials and endpoint
// query details redacted.
func (d B2PolicyDescriptor) RedactedSummary() B2RedactedSummary {
	d = d.Normalize()
	return B2RedactedSummary{
		Bucket:         d.Bucket,
		Region:         d.Region,
		EndpointURL:    redactB2URL(d.EndpointURL),
		ApplicationKey: d.ApplicationKey.RedactedSummary(),
		S3Compatible:   d.S3Compatible.RedactedSummary(),
		ObjectPrefix:   d.ObjectPrefix,
	}
}

// Normalize returns canonical application-key metadata.
func (m B2ApplicationKeyMetadata) Normalize() B2ApplicationKeyMetadata {
	m.KeyID = strings.TrimSpace(m.KeyID)
	m.KeyName = strings.TrimSpace(m.KeyName)
	m.ApplicationKey = strings.TrimSpace(m.ApplicationKey)
	m.Capabilities = normalizeB2Capabilities(m.Capabilities)
	return m
}

// Validate checks application-key metadata without contacting Backblaze B2.
func (m B2ApplicationKeyMetadata) Validate() error {
	return ValidateB2ApplicationKeyMetadata(m)
}

// AllowsCapability reports whether capability is listed in the metadata.
func (m B2ApplicationKeyMetadata) AllowsCapability(capability string) bool {
	capability = normalizeB2Capability(capability)
	for _, candidate := range m.Normalize().Capabilities {
		if candidate == capability {
			return true
		}
	}
	return false
}

// RedactedSummary returns application-key metadata with secrets redacted.
func (m B2ApplicationKeyMetadata) RedactedSummary() B2ApplicationKeySummary {
	m = m.Normalize()
	return B2ApplicationKeySummary{
		KeyIDRedacted:          redactB2Secret(m.KeyID),
		KeyName:                m.KeyName,
		ApplicationKeyRedacted: redactB2Secret(m.ApplicationKey),
		Capabilities:           append([]string(nil), m.Capabilities...),
		ExpiresAt:              m.ExpiresAt,
		HasApplicationKey:      m.ApplicationKey != "",
	}
}

// Normalize returns canonical S3-compatible mode metadata.
func (m B2S3CompatibleMode) Normalize() B2S3CompatibleMode {
	m.Region = NormalizeB2Region(m.Region)
	m.EndpointURL = NormalizeB2EndpointURL(m.EndpointURL)
	m.AddressingStyle = NormalizeB2S3AddressingStyle(m.AddressingStyle)
	if !m.Enabled {
		return B2S3CompatibleMode{}
	}
	return m
}

// Validate checks S3-compatible mode metadata without network calls.
func (m B2S3CompatibleMode) Validate() error {
	return ValidateB2S3CompatibleMode(m)
}

// RedactedSummary returns S3-compatible mode metadata with endpoint query
// details redacted.
func (m B2S3CompatibleMode) RedactedSummary() B2S3CompatibleSummary {
	m = m.Normalize()
	return B2S3CompatibleSummary{
		Enabled:         m.Enabled,
		Region:          m.Region,
		EndpointURL:     redactB2URL(m.EndpointURL),
		AddressingStyle: m.AddressingStyle,
	}
}

// NormalizeB2Bucket returns the canonical bucket token shape.
func NormalizeB2Bucket(bucket string) string {
	return strings.ToLower(strings.TrimSpace(bucket))
}

// NormalizeB2Region returns the canonical B2 region token shape.
func NormalizeB2Region(region string) string {
	return strings.ToLower(strings.ReplaceAll(strings.TrimSpace(region), "_", "-"))
}

// NormalizeB2EndpointURL returns endpoint with whitespace removed and no
// trailing slash. Empty endpoints stay empty.
func NormalizeB2EndpointURL(endpoint string) string {
	endpoint = strings.TrimSpace(endpoint)
	if endpoint == "" {
		return ""
	}
	return strings.TrimRight(endpoint, "/")
}

// NormalizeB2S3AddressingStyle returns the canonical S3 addressing style.
// Empty defaults to virtual-host addressing.
func NormalizeB2S3AddressingStyle(style B2S3AddressingStyle) B2S3AddressingStyle {
	value := strings.ToLower(strings.ReplaceAll(strings.TrimSpace(string(style)), "-", "_"))
	switch value {
	case "", "virtual", "virtual_host", "virtualhost":
		return B2S3AddressingStyleVirtualHost
	case "path", "path_style", "pathstyle":
		return B2S3AddressingStylePath
	default:
		return B2S3AddressingStyle(value)
	}
}

// ValidateB2PolicyDescriptor checks bucket, region, endpoint, application key,
// S3-compatible mode, and object prefix metadata.
func ValidateB2PolicyDescriptor(descriptor B2PolicyDescriptor) error {
	descriptor = descriptor.Normalize()
	errs := []error{
		ValidateB2Bucket(descriptor.Bucket),
		ValidateB2Region(descriptor.Region),
		ValidateB2EndpointURL(descriptor.EndpointURL),
		ValidateB2ApplicationKeyMetadata(descriptor.ApplicationKey),
		ValidateB2S3CompatibleMode(descriptor.S3Compatible),
		validateB2ObjectPrefix(descriptor.ObjectPrefix),
	}
	return errors.Join(errs...)
}

// ValidateB2Bucket checks the bucket token shape without checking availability
// or ownership.
func ValidateB2Bucket(bucket string) error {
	bucket = NormalizeB2Bucket(bucket)
	if len(bucket) < 6 || len(bucket) > 50 {
		return fmt.Errorf("%w: bucket must be 6-50 characters", ErrB2PolicyInvalid)
	}
	if !isB2BucketEdge(bucket[0]) || !isB2BucketEdge(bucket[len(bucket)-1]) {
		return fmt.Errorf("%w: bucket must start and end with a letter or digit", ErrB2PolicyInvalid)
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
		return fmt.Errorf("%w: bucket contains invalid character %q", ErrB2PolicyInvalid, r)
	}
	return nil
}

// ValidateB2Region checks the normalized region token.
func ValidateB2Region(region string) error {
	region = NormalizeB2Region(region)
	if region == "" {
		return fmt.Errorf("%w: region is required", ErrB2PolicyInvalid)
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
		return fmt.Errorf("%w: region contains invalid character %q", ErrB2PolicyInvalid, r)
	}
	return nil
}

// ValidateB2EndpointURL checks endpoint shape without checking reachability.
func ValidateB2EndpointURL(endpoint string) error {
	endpoint = NormalizeB2EndpointURL(endpoint)
	if endpoint == "" {
		return nil
	}
	parsed, err := url.Parse(endpoint)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return fmt.Errorf("%w: endpoint url is invalid", ErrB2PolicyInvalid)
	}
	if parsed.Scheme != "https" {
		return fmt.Errorf("%w: endpoint url must use https", ErrB2PolicyInvalid)
	}
	return nil
}

// ValidateB2ApplicationKeyMetadata checks local application-key metadata.
func ValidateB2ApplicationKeyMetadata(metadata B2ApplicationKeyMetadata) error {
	metadata = metadata.Normalize()
	if metadata.KeyID == "" {
		return fmt.Errorf("%w: application key id is required", ErrB2PolicyInvalid)
	}
	if !isB2Token(metadata.KeyID) {
		return fmt.Errorf("%w: application key id contains invalid characters", ErrB2PolicyInvalid)
	}
	if metadata.KeyName != "" && !isB2KeyName(metadata.KeyName) {
		return fmt.Errorf("%w: application key name contains invalid characters", ErrB2PolicyInvalid)
	}
	if metadata.ApplicationKey == "" {
		return fmt.Errorf("%w: application key secret is required", ErrB2PolicyInvalid)
	}
	if !isB2Token(metadata.ApplicationKey) {
		return fmt.Errorf("%w: application key secret contains invalid characters", ErrB2PolicyInvalid)
	}
	if len(metadata.Capabilities) == 0 {
		return fmt.Errorf("%w: application key capabilities are required", ErrB2PolicyInvalid)
	}
	for i, capability := range metadata.Capabilities {
		if !isKnownB2Capability(capability) {
			return fmt.Errorf("%w: application key capability %d is unknown", ErrB2PolicyInvalid, i)
		}
	}
	return nil
}

// ValidateB2S3CompatibleMode checks S3-compatible mode metadata.
func ValidateB2S3CompatibleMode(mode B2S3CompatibleMode) error {
	mode = mode.Normalize()
	if !mode.Enabled {
		return nil
	}
	if err := ValidateB2Region(mode.Region); err != nil {
		return err
	}
	if err := ValidateB2EndpointURL(mode.EndpointURL); err != nil {
		return err
	}
	if mode.EndpointURL == "" {
		return fmt.Errorf("%w: s3-compatible endpoint url is required when enabled", ErrB2PolicyInvalid)
	}
	switch mode.AddressingStyle {
	case B2S3AddressingStyleVirtualHost, B2S3AddressingStylePath:
		return nil
	default:
		return fmt.Errorf("%w: s3-compatible addressing style %q is unknown", ErrB2PolicyInvalid, mode.AddressingStyle)
	}
}

// PlanB2ObjectKey normalizes prefix segments and key into a dry-run full
// object key.
func PlanB2ObjectKey(key string, prefixes ...string) (B2ObjectKeyPlan, error) {
	parts := make([]string, 0, len(prefixes))
	for _, prefix := range prefixes {
		prefix = normalizeB2ObjectPrefix(prefix)
		if prefix == "" {
			continue
		}
		if err := validateB2ObjectPrefix(prefix); err != nil {
			return B2ObjectKeyPlan{}, err
		}
		parts = append(parts, prefix)
	}
	key = normalizeB2ObjectKey(key)
	if err := validateB2ObjectKey(key); err != nil {
		return B2ObjectKeyPlan{}, err
	}
	prefix := strings.Join(parts, "/")
	if prefix != "" {
		prefix += "/"
	}
	return B2ObjectKeyPlan{
		Prefix: prefix,
		Key:    key,
		Full:   prefix + key,
	}, nil
}

func normalizeB2Capabilities(capabilities []string) []string {
	if len(capabilities) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(capabilities))
	normalized := make([]string, 0, len(capabilities))
	for _, capability := range capabilities {
		capability = normalizeB2Capability(capability)
		if capability == "" {
			continue
		}
		if _, ok := seen[capability]; ok {
			continue
		}
		seen[capability] = struct{}{}
		normalized = append(normalized, capability)
	}
	sort.Strings(normalized)
	return normalized
}

func normalizeB2Capability(capability string) string {
	return strings.TrimSpace(capability)
}

func isKnownB2Capability(capability string) bool {
	switch capability {
	case "listKeys", "writeKeys", "deleteKeys", "listBuckets", "writeBuckets", "deleteBuckets",
		"listFiles", "readFiles", "shareFiles", "writeFiles", "deleteFiles", "bypassGovernance",
		"readBucketEncryption", "writeBucketEncryption", "readBucketRetentions", "writeBucketRetentions",
		"readFileRetentions", "writeFileRetentions", "readFileLegalHolds", "writeFileLegalHolds":
		return true
	default:
		return false
	}
}

func normalizeB2ObjectPrefix(prefix string) string {
	return strings.Trim(strings.TrimSpace(prefix), "/")
}

func normalizeB2ObjectKey(key string) string {
	return strings.Trim(strings.TrimSpace(key), "/")
}

func validateB2ObjectPrefix(prefix string) error {
	prefix = normalizeB2ObjectPrefix(prefix)
	if prefix == "" {
		return nil
	}
	for _, segment := range strings.Split(prefix, "/") {
		if !isB2ObjectPathSegment(segment) {
			return fmt.Errorf("%w: object prefix contains invalid segment %q", ErrB2PolicyInvalid, segment)
		}
	}
	return nil
}

func validateB2ObjectKey(key string) error {
	if key == "" {
		return fmt.Errorf("%w: object key is required", ErrB2PolicyInvalid)
	}
	for _, segment := range strings.Split(key, "/") {
		if !isB2ObjectPathSegment(segment) {
			return fmt.Errorf("%w: object key contains invalid segment %q", ErrB2PolicyInvalid, segment)
		}
	}
	return nil
}

func isB2ObjectPathSegment(segment string) bool {
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

func isB2BucketEdge(b byte) bool {
	return (b >= 'a' && b <= 'z') || (b >= '0' && b <= '9')
}

func isB2KeyName(name string) bool {
	for _, r := range name {
		if unicode.IsControl(r) || r == '/' || r == '\\' {
			return false
		}
	}
	return true
}

func isB2Token(token string) bool {
	for _, r := range token {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			return false
		}
	}
	return true
}

func redactB2Secret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

func redactB2URL(value string) string {
	value = NormalizeB2EndpointURL(value)
	if value == "" {
		return ""
	}
	parsed, err := url.Parse(value)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "[redacted]"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String()
}
