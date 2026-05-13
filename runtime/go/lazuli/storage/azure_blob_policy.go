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
	// ErrAzureBlobPolicyInvalid is returned when an Azure Blob container
	// descriptor contains an invalid account/container name, access tier,
	// public access mode, key prefix, or SAS capability.
	ErrAzureBlobPolicyInvalid = errors.New("lazuli/storage: azure_blob_policy_invalid")
)

// AzureBlobContainerPolicy describes provider-neutral Azure Blob container
// settings for future adapter bindings. It performs no SDK calls and does not
// imply that any container exists.
type AzureBlobContainerPolicy struct {
	AccountName   string
	ContainerName string
	AccessTier    AzureBlobAccessTier
	PublicMode    AzureBlobPublicMode
	KeyPrefix     AzureBlobKeyPrefix
	SAS           AzureBlobSASCapabilities

	EndpointURL      string
	AccountKey       string
	ConnectionString string
}

// Validate checks the descriptor for structural validity.
func (p AzureBlobContainerPolicy) Validate() error {
	return ValidateAzureBlobContainerPolicy(p)
}

// Normalize returns a copy with canonical account/container names, access tier,
// public access mode, key-prefix fields, and SAS permissions.
func (p AzureBlobContainerPolicy) Normalize() (AzureBlobContainerPolicy, error) {
	return NormalizeAzureBlobContainerPolicy(p)
}

// BlobPrefix resolves the blob key prefix for tenant. Tenant is required only
// when KeyPrefix is tenant-scoped.
func (p AzureBlobContainerPolicy) BlobPrefix(tenant string) (string, error) {
	normalized, err := NormalizeAzureBlobContainerPolicy(p)
	if err != nil {
		return "", err
	}
	return normalized.KeyPrefix.PrefixForTenant(tenant)
}

// RedactedSummary returns stable, log-safe descriptor metadata. Endpoint URLs
// are reduced to host/path shape and credential-bearing values are redacted.
func (p AzureBlobContainerPolicy) RedactedSummary() AzureBlobContainerPolicySummary {
	normalized, err := NormalizeAzureBlobContainerPolicy(p)
	if err == nil {
		p = normalized
	}
	return AzureBlobContainerPolicySummary{
		AccountName:         strings.TrimSpace(strings.ToLower(p.AccountName)),
		ContainerName:       strings.TrimSpace(strings.ToLower(p.ContainerName)),
		AccessTier:          p.AccessTier.String(),
		PublicMode:          p.PublicMode.String(),
		KeyPrefix:           p.KeyPrefix.Normalize(),
		SAS:                 p.SAS.Normalize(),
		EndpointURL:         redactAzureBlobURL(p.EndpointURL),
		AccountKey:          redactAzureBlobSecret(p.AccountKey),
		ConnectionString:    redactAzureBlobSecret(p.ConnectionString),
		HasAccountKey:       strings.TrimSpace(p.AccountKey) != "",
		HasConnectionString: strings.TrimSpace(p.ConnectionString) != "",
	}
}

// AzureBlobContainerPolicySummary is a log-safe Azure Blob policy view.
type AzureBlobContainerPolicySummary struct {
	AccountName         string
	ContainerName       string
	AccessTier          string
	PublicMode          string
	KeyPrefix           AzureBlobKeyPrefix
	SAS                 AzureBlobSASCapabilities
	EndpointURL         string
	AccountKey          string
	ConnectionString    string
	HasAccountKey       bool
	HasConnectionString bool
}

// AzureBlobAccessTier is a provider-neutral Azure Blob access tier token.
type AzureBlobAccessTier string

const (
	AzureBlobAccessTierHot     AzureBlobAccessTier = "hot"
	AzureBlobAccessTierCool    AzureBlobAccessTier = "cool"
	AzureBlobAccessTierCold    AzureBlobAccessTier = "cold"
	AzureBlobAccessTierArchive AzureBlobAccessTier = "archive"
)

// String renders the tier as a stable lowercase token.
func (t AzureBlobAccessTier) String() string {
	t = normalizeAzureBlobAccessTier(t)
	if isKnownAzureBlobAccessTier(t) {
		return string(t)
	}
	return "unknown"
}

// Validate checks the access tier token.
func (t AzureBlobAccessTier) Validate() error {
	return ValidateAzureBlobAccessTier(t)
}

// AzureBlobPublicMode describes anonymous read exposure for a container.
type AzureBlobPublicMode string

const (
	AzureBlobPublicModePrivate   AzureBlobPublicMode = "private"
	AzureBlobPublicModeBlob      AzureBlobPublicMode = "blob"
	AzureBlobPublicModeContainer AzureBlobPublicMode = "container"
)

// String renders the public mode as a stable lowercase token.
func (m AzureBlobPublicMode) String() string {
	m = normalizeAzureBlobPublicMode(m)
	if isKnownAzureBlobPublicMode(m) {
		return string(m)
	}
	return "unknown"
}

// Validate checks the public access mode token.
func (m AzureBlobPublicMode) Validate() error {
	return ValidateAzureBlobPublicMode(m)
}

// AzureBlobKeyPrefix describes provider-neutral blob key layout.
type AzureBlobKeyPrefix struct {
	Prefix       string
	TenantPrefix string
	TenantScoped bool
}

// Normalize trims path separators and whitespace from prefix fields.
func (p AzureBlobKeyPrefix) Normalize() AzureBlobKeyPrefix {
	normalized := AzureBlobKeyPrefix{
		Prefix:       normalizeAzureBlobKeyPath(p.Prefix),
		TenantPrefix: normalizeAzureBlobKeyPath(p.TenantPrefix),
		TenantScoped: p.TenantScoped,
	}
	if normalized.TenantPrefix != "" {
		normalized.TenantScoped = true
	}
	return normalized
}

// Validate checks that prefix fields are safe blob key path segments.
func (p AzureBlobKeyPrefix) Validate() error {
	return validateAzureBlobKeyPrefix(p)
}

// PrefixForTenant resolves the blob key prefix. The returned value is empty
// for the container root and otherwise ends with a slash.
func (p AzureBlobKeyPrefix) PrefixForTenant(tenant string) (string, error) {
	p = p.Normalize()
	if err := validateAzureBlobKeyPrefix(p); err != nil {
		return "", err
	}

	parts := make([]string, 0, 3)
	if p.Prefix != "" {
		parts = append(parts, p.Prefix)
	}
	if p.TenantScoped {
		tenant = strings.TrimSpace(tenant)
		if !isAzureBlobKeySegment(tenant) {
			return "", fmt.Errorf("%w: invalid tenant %q", ErrAzureBlobPolicyInvalid, tenant)
		}
		parts = append(parts, p.TenantPrefix, tenant)
	}
	if len(parts) == 0 {
		return "", nil
	}
	return strings.Join(parts, "/") + "/", nil
}

// AzureBlobSASCapabilities records whether a future adapter may mint signed
// URLs/SAS tokens and which permissions it may request.
type AzureBlobSASCapabilities struct {
	Enabled     bool
	MaxAge      time.Duration
	Permissions []string
}

// Normalize returns a copy with sorted, deduplicated lowercase permission
// tokens. Disabled capabilities drop max age and permissions.
func (c AzureBlobSASCapabilities) Normalize() AzureBlobSASCapabilities {
	if !c.Enabled {
		return AzureBlobSASCapabilities{}
	}
	seen := make(map[string]struct{}, len(c.Permissions))
	permissions := make([]string, 0, len(c.Permissions))
	for _, permission := range c.Permissions {
		permission = normalizeAzureBlobSASPermission(permission)
		if permission == "" {
			continue
		}
		if _, ok := seen[permission]; ok {
			continue
		}
		seen[permission] = struct{}{}
		permissions = append(permissions, permission)
	}
	sort.Strings(permissions)
	return AzureBlobSASCapabilities{
		Enabled:     true,
		MaxAge:      c.MaxAge,
		Permissions: permissions,
	}
}

// Validate checks SAS capability metadata.
func (c AzureBlobSASCapabilities) Validate() error {
	return validateAzureBlobSASCapabilities(c)
}

// AllowsPermission reports whether permission is allowed by the capability
// metadata. Disabled capabilities allow no permissions.
func (c AzureBlobSASCapabilities) AllowsPermission(permission string) bool {
	c = c.Normalize()
	if !c.Enabled {
		return false
	}
	permission = normalizeAzureBlobSASPermission(permission)
	for _, candidate := range c.Permissions {
		if candidate == permission {
			return true
		}
	}
	return false
}

// NormalizeAzureBlobAccountName canonicalizes an Azure Blob account name.
func NormalizeAzureBlobAccountName(name string) (string, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if err := validateAzureBlobAccountName(name); err != nil {
		return "", err
	}
	return name, nil
}

// NormalizeAzureBlobContainerName canonicalizes an Azure Blob container name.
func NormalizeAzureBlobContainerName(name string) (string, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if err := validateAzureBlobContainerName(name); err != nil {
		return "", err
	}
	return name, nil
}

// ValidateAzureBlobAccessTier checks an Azure Blob access tier token.
func ValidateAzureBlobAccessTier(tier AzureBlobAccessTier) error {
	if !isKnownAzureBlobAccessTier(normalizeAzureBlobAccessTier(tier)) {
		return fmt.Errorf("%w: unknown access tier %q", ErrAzureBlobPolicyInvalid, tier)
	}
	return nil
}

// ValidateAzureBlobPublicMode checks an Azure Blob anonymous access mode token.
func ValidateAzureBlobPublicMode(mode AzureBlobPublicMode) error {
	if !isKnownAzureBlobPublicMode(normalizeAzureBlobPublicMode(mode)) {
		return fmt.Errorf("%w: unknown public mode %q", ErrAzureBlobPolicyInvalid, mode)
	}
	return nil
}

// PlanAzureBlobKeyPrefix resolves a blob key prefix from prefix metadata.
func PlanAzureBlobKeyPrefix(prefix AzureBlobKeyPrefix, tenant string) (string, error) {
	return prefix.PrefixForTenant(tenant)
}

// NormalizeAzureBlobContainerPolicy returns a canonical descriptor copy.
func NormalizeAzureBlobContainerPolicy(policy AzureBlobContainerPolicy) (AzureBlobContainerPolicy, error) {
	var err error
	policy.AccountName, err = NormalizeAzureBlobAccountName(policy.AccountName)
	if err != nil {
		return AzureBlobContainerPolicy{}, err
	}
	policy.ContainerName, err = NormalizeAzureBlobContainerName(policy.ContainerName)
	if err != nil {
		return AzureBlobContainerPolicy{}, err
	}
	policy.AccessTier = normalizeAzureBlobAccessTier(policy.AccessTier)
	if err := ValidateAzureBlobAccessTier(policy.AccessTier); err != nil {
		return AzureBlobContainerPolicy{}, err
	}
	policy.PublicMode = normalizeAzureBlobPublicMode(policy.PublicMode)
	if err := ValidateAzureBlobPublicMode(policy.PublicMode); err != nil {
		return AzureBlobContainerPolicy{}, err
	}
	policy.KeyPrefix = policy.KeyPrefix.Normalize()
	policy.SAS = policy.SAS.Normalize()
	if err := validateAzureBlobKeyPrefix(policy.KeyPrefix); err != nil {
		return AzureBlobContainerPolicy{}, err
	}
	if err := validateAzureBlobSASCapabilities(policy.SAS); err != nil {
		return AzureBlobContainerPolicy{}, err
	}
	return policy, nil
}

// ValidateAzureBlobContainerPolicy checks descriptor shape without mutating it.
func ValidateAzureBlobContainerPolicy(policy AzureBlobContainerPolicy) error {
	_, err := NormalizeAzureBlobContainerPolicy(policy)
	return err
}

func validateAzureBlobAccountName(name string) error {
	if name == "" {
		return fmt.Errorf("%w: account name is required", ErrAzureBlobPolicyInvalid)
	}
	if len(name) < 3 || len(name) > 24 {
		return fmt.Errorf("%w: account name must be 3-24 characters", ErrAzureBlobPolicyInvalid)
	}
	for _, r := range name {
		if r < 'a' || r > 'z' {
			if r < '0' || r > '9' {
				return fmt.Errorf("%w: account name contains invalid character %q", ErrAzureBlobPolicyInvalid, r)
			}
		}
	}
	return nil
}

func validateAzureBlobContainerName(name string) error {
	if name == "" {
		return fmt.Errorf("%w: container name is required", ErrAzureBlobPolicyInvalid)
	}
	if len(name) < 3 || len(name) > 63 {
		return fmt.Errorf("%w: container name must be 3-63 characters", ErrAzureBlobPolicyInvalid)
	}
	var previousHyphen bool
	for i, r := range name {
		if r >= 'a' && r <= 'z' || r >= '0' && r <= '9' {
			previousHyphen = false
			continue
		}
		if r == '-' {
			if i == 0 || i == len(name)-1 || previousHyphen {
				return fmt.Errorf("%w: container name has invalid hyphen placement", ErrAzureBlobPolicyInvalid)
			}
			previousHyphen = true
			continue
		}
		return fmt.Errorf("%w: container name contains invalid character %q", ErrAzureBlobPolicyInvalid, r)
	}
	return nil
}

func validateAzureBlobKeyPrefix(prefix AzureBlobKeyPrefix) error {
	prefix = prefix.Normalize()
	if err := validateAzureBlobKeyPath("prefix", prefix.Prefix, true); err != nil {
		return err
	}
	if prefix.TenantScoped && prefix.TenantPrefix == "" {
		return fmt.Errorf("%w: tenant prefix is required for tenant-scoped containers", ErrAzureBlobPolicyInvalid)
	}
	if !prefix.TenantScoped && prefix.TenantPrefix != "" {
		return fmt.Errorf("%w: tenant prefix requires tenant scope", ErrAzureBlobPolicyInvalid)
	}
	return validateAzureBlobKeyPath("tenant prefix", prefix.TenantPrefix, !prefix.TenantScoped)
}

func validateAzureBlobKeyPath(name, value string, allowEmpty bool) error {
	if value == "" {
		if allowEmpty {
			return nil
		}
		return fmt.Errorf("%w: %s is required", ErrAzureBlobPolicyInvalid, name)
	}
	for _, segment := range strings.Split(value, "/") {
		if !isAzureBlobKeySegment(segment) {
			return fmt.Errorf("%w: invalid %s segment %q", ErrAzureBlobPolicyInvalid, name, segment)
		}
	}
	return nil
}

func validateAzureBlobSASCapabilities(c AzureBlobSASCapabilities) error {
	c = c.Normalize()
	if !c.Enabled {
		return nil
	}
	if c.MaxAge < 0 {
		return fmt.Errorf("%w: sas max age must be non-negative", ErrAzureBlobPolicyInvalid)
	}
	if len(c.Permissions) == 0 {
		return fmt.Errorf("%w: sas permissions are required when enabled", ErrAzureBlobPolicyInvalid)
	}
	for i, permission := range c.Permissions {
		if !isKnownAzureBlobSASPermission(permission) {
			return fmt.Errorf("%w: sas permission %d is unknown", ErrAzureBlobPolicyInvalid, i)
		}
	}
	return nil
}

func normalizeAzureBlobAccessTier(tier AzureBlobAccessTier) AzureBlobAccessTier {
	return AzureBlobAccessTier(strings.ToLower(strings.TrimSpace(string(tier))))
}

func isKnownAzureBlobAccessTier(tier AzureBlobAccessTier) bool {
	switch tier {
	case AzureBlobAccessTierHot, AzureBlobAccessTierCool, AzureBlobAccessTierCold, AzureBlobAccessTierArchive:
		return true
	default:
		return false
	}
}

func normalizeAzureBlobPublicMode(mode AzureBlobPublicMode) AzureBlobPublicMode {
	return AzureBlobPublicMode(strings.ToLower(strings.TrimSpace(string(mode))))
}

func isKnownAzureBlobPublicMode(mode AzureBlobPublicMode) bool {
	switch mode {
	case AzureBlobPublicModePrivate, AzureBlobPublicModeBlob, AzureBlobPublicModeContainer:
		return true
	default:
		return false
	}
}

func normalizeAzureBlobKeyPath(value string) string {
	return strings.Trim(strings.TrimSpace(value), "/")
}

func isAzureBlobKeySegment(segment string) bool {
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

func normalizeAzureBlobSASPermission(permission string) string {
	return strings.ToLower(strings.TrimSpace(permission))
}

func isKnownAzureBlobSASPermission(permission string) bool {
	switch permission {
	case "read", "add", "create", "write", "delete", "list":
		return true
	default:
		return false
	}
}

func redactAzureBlobSecret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "redacted"
}

func redactAzureBlobURL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	u, err := url.Parse(raw)
	if err != nil || u.Host == "" {
		return "redacted"
	}
	u.User = nil
	u.RawQuery = ""
	u.Fragment = ""
	return u.String()
}
