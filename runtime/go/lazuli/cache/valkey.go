package cache

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"time"
)

const (
	valkeyDefaultAddress = "127.0.0.1:6379"
	valkeyDefaultTTL     = time.Minute
	valkeyKeySeparator   = ":"
)

var ErrInvalidValkeyDescriptor = errors.New("lazuli/cache: invalid valkey descriptor")

// ValkeyDescriptor describes provider-neutral Valkey cache connection and key
// policy metadata. It is side-effect free and does not imply a client library.
type ValkeyDescriptor struct {
	Address   string
	DB        int
	TLS       ValkeyTLSMetadata
	Auth      ValkeyAuthMetadata
	KeyPrefix string
	TTL       ValkeyTTLPolicy
	Cluster   ValkeyClusterMetadata
	Sentinel  ValkeySentinelMetadata
}

// ValkeyTLSMetadata carries TLS metadata without binding to a concrete client.
type ValkeyTLSMetadata struct {
	Enabled            bool
	ServerName         string
	CAPath             string
	InsecureSkipVerify bool
}

// ValkeyAuthMetadata carries authentication metadata. Password and credential
// URLs are always redacted from summaries.
type ValkeyAuthMetadata struct {
	Username      string
	Password      string
	PasswordEnv   string
	CredentialURL string
}

// ValkeyTTLPolicy describes deterministic cache expiration defaults.
type ValkeyTTLPolicy struct {
	DefaultTTL time.Duration
	MinimumTTL time.Duration
	MaximumTTL time.Duration
}

// ValkeyClusterMetadata describes Valkey cluster topology metadata.
type ValkeyClusterMetadata struct {
	Enabled   bool
	Addresses []string
}

// ValkeySentinelMetadata describes Valkey Sentinel topology metadata.
type ValkeySentinelMetadata struct {
	MasterName string
	Addresses  []string
}

// ValkeyKeyPrefixPlan is the normalized key prefix shape adapters can use.
type ValkeyKeyPrefixPlan struct {
	Prefix       string
	EntryPrefix  string
	TagPrefix    string
	LockPrefix   string
	QueryPattern string
}

// ValkeyCachePlan is the validated, normalized descriptor plan.
type ValkeyCachePlan struct {
	Descriptor ValkeyDescriptor
	Mode       string
	Prefixes   ValkeyKeyPrefixPlan
}

// ValkeyDescriptorSummary is safe for logs and diagnostics.
type ValkeyDescriptorSummary struct {
	Mode          string
	Address       string
	Addresses     []string
	DB            int
	TLS           bool
	TLSServerName string
	Auth          string
	KeyPrefix     string
	DefaultTTL    time.Duration
	MinimumTTL    time.Duration
	MaximumTTL    time.Duration
	Sentinel      string
}

// Normalize returns a copy with trimmed strings and deterministic topology
// metadata ordering.
func (d ValkeyDescriptor) Normalize() ValkeyDescriptor {
	return NormalizeValkeyDescriptor(d)
}

// Validate reports whether descriptor metadata is structurally usable.
func (d ValkeyDescriptor) Validate() error {
	return ValidateValkeyDescriptor(d)
}

// Plan validates descriptor and returns deterministic adapter metadata.
func (d ValkeyDescriptor) Plan() (ValkeyCachePlan, error) {
	return PlanValkeyCache(d)
}

// RedactedSummary returns a diagnostics-safe descriptor summary.
func (d ValkeyDescriptor) RedactedSummary() ValkeyDescriptorSummary {
	return RedactValkeyDescriptor(d)
}

// NormalizeValkeyDescriptor returns a deterministic descriptor copy.
func NormalizeValkeyDescriptor(d ValkeyDescriptor) ValkeyDescriptor {
	d.Address = strings.TrimSpace(d.Address)
	d.DB = d.DB
	d.TLS.ServerName = strings.TrimSpace(d.TLS.ServerName)
	d.TLS.CAPath = strings.TrimSpace(d.TLS.CAPath)
	d.Auth.Username = strings.TrimSpace(d.Auth.Username)
	d.Auth.PasswordEnv = strings.TrimSpace(d.Auth.PasswordEnv)
	d.Auth.CredentialURL = strings.TrimSpace(d.Auth.CredentialURL)
	d.KeyPrefix = NormalizeNamespace(d.KeyPrefix)
	d.Cluster.Addresses = normalizeValkeyAddresses(d.Cluster.Addresses)
	d.Sentinel.MasterName = strings.TrimSpace(d.Sentinel.MasterName)
	d.Sentinel.Addresses = normalizeValkeyAddresses(d.Sentinel.Addresses)
	return d
}

// ValidateValkeyDescriptor checks address, db, TLS, auth, TTL, cluster, and
// sentinel metadata without opening sockets.
func ValidateValkeyDescriptor(d ValkeyDescriptor) error {
	d = d.Normalize()

	var errs []error
	if d.DB < 0 {
		errs = append(errs, fmt.Errorf("%w: db must not be negative", ErrInvalidValkeyDescriptor))
	}
	if err := validateValkeyAddress("address", d.Address, true); err != nil {
		errs = append(errs, err)
	}
	if err := validateValkeyTLS(d.TLS); err != nil {
		errs = append(errs, err)
	}
	if err := validateValkeyAuth(d.Auth); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateValkeyTTLPolicy(d.TTL); err != nil {
		errs = append(errs, err)
	}
	if err := validateValkeyCluster(d.Cluster); err != nil {
		errs = append(errs, err)
	}
	if err := validateValkeySentinel(d.Sentinel); err != nil {
		errs = append(errs, err)
	}
	if d.Cluster.Enabled && d.Sentinel.MasterName != "" {
		errs = append(errs, fmt.Errorf("%w: cluster and sentinel modes are mutually exclusive", ErrInvalidValkeyDescriptor))
	}
	return errors.Join(errs...)
}

// ValidateValkeyTTLPolicy checks deterministic TTL bounds.
func ValidateValkeyTTLPolicy(policy ValkeyTTLPolicy) error {
	var errs []error
	if policy.DefaultTTL < 0 {
		errs = append(errs, fmt.Errorf("%w: default TTL must not be negative", ErrInvalidValkeyDescriptor))
	}
	if policy.MinimumTTL < 0 {
		errs = append(errs, fmt.Errorf("%w: minimum TTL must not be negative", ErrInvalidValkeyDescriptor))
	}
	if policy.MaximumTTL < 0 {
		errs = append(errs, fmt.Errorf("%w: maximum TTL must not be negative", ErrInvalidValkeyDescriptor))
	}
	if policy.MinimumTTL > 0 && policy.MaximumTTL > 0 && policy.MinimumTTL > policy.MaximumTTL {
		errs = append(errs, fmt.Errorf("%w: minimum TTL must not exceed maximum TTL", ErrInvalidValkeyDescriptor))
	}
	if policy.DefaultTTL > 0 && policy.MinimumTTL > 0 && policy.DefaultTTL < policy.MinimumTTL {
		errs = append(errs, fmt.Errorf("%w: default TTL must not be below minimum TTL", ErrInvalidValkeyDescriptor))
	}
	if policy.DefaultTTL > 0 && policy.MaximumTTL > 0 && policy.DefaultTTL > policy.MaximumTTL {
		errs = append(errs, fmt.Errorf("%w: default TTL must not exceed maximum TTL", ErrInvalidValkeyDescriptor))
	}
	return errors.Join(errs...)
}

// PlanValkeyCache validates descriptor and returns normalized metadata.
func PlanValkeyCache(d ValkeyDescriptor) (ValkeyCachePlan, error) {
	d = d.Normalize()
	if err := ValidateValkeyDescriptor(d); err != nil {
		return ValkeyCachePlan{}, err
	}
	return ValkeyCachePlan{
		Descriptor: d,
		Mode:       valkeyMode(d),
		Prefixes:   PlanValkeyKeyPrefixes(d.KeyPrefix),
	}, nil
}

// PlanValkeyKeyPrefixes builds deterministic entry, tag, and lock prefixes.
func PlanValkeyKeyPrefixes(prefix string) ValkeyKeyPrefixPlan {
	prefix = NormalizeNamespace(prefix)
	base := prefix
	if base != "" {
		base += valkeyKeySeparator
	}
	return ValkeyKeyPrefixPlan{
		Prefix:       prefix,
		EntryPrefix:  base + "entry" + valkeyKeySeparator,
		TagPrefix:    base + "tag" + valkeyKeySeparator,
		LockPrefix:   base + "lock" + valkeyKeySeparator,
		QueryPattern: base + "entry" + valkeyKeySeparator + "%s" + valkeyKeySeparator + "*",
	}
}

// PlanValkeyTTL returns the effective TTL for a cache write. Zero requests use
// the descriptor default, and an all-zero policy falls back to Valkey defaults.
func PlanValkeyTTL(policy ValkeyTTLPolicy, requested time.Duration) (time.Duration, error) {
	if err := ValidateValkeyTTLPolicy(policy); err != nil {
		return 0, err
	}
	if requested < 0 {
		return 0, fmt.Errorf("%w: requested TTL must not be negative", ErrInvalidValkeyDescriptor)
	}
	ttl := requested
	if ttl == 0 {
		ttl = policy.DefaultTTL
	}
	if ttl == 0 {
		ttl = valkeyDefaultTTL
	}
	if policy.MinimumTTL > 0 && ttl < policy.MinimumTTL {
		ttl = policy.MinimumTTL
	}
	if policy.MaximumTTL > 0 && ttl > policy.MaximumTTL {
		ttl = policy.MaximumTTL
	}
	return ttl, nil
}

// RedactValkeyDescriptor returns a stable, secret-free summary.
func RedactValkeyDescriptor(d ValkeyDescriptor) ValkeyDescriptorSummary {
	d = d.Normalize()
	return ValkeyDescriptorSummary{
		Mode:          valkeyMode(d),
		Address:       redactValkeyAddress(defaultValkeyAddress(d.Address)),
		Addresses:     redactValkeyAddresses(valkeyTopologyAddresses(d)),
		DB:            d.DB,
		TLS:           d.TLS.Enabled,
		TLSServerName: d.TLS.ServerName,
		Auth:          valkeyAuthSummary(d.Auth),
		KeyPrefix:     d.KeyPrefix,
		DefaultTTL:    d.TTL.DefaultTTL,
		MinimumTTL:    d.TTL.MinimumTTL,
		MaximumTTL:    d.TTL.MaximumTTL,
		Sentinel:      d.Sentinel.MasterName,
	}
}

func validateValkeyTLS(tls ValkeyTLSMetadata) error {
	if !tls.Enabled && (tls.ServerName != "" || tls.CAPath != "" || tls.InsecureSkipVerify) {
		return fmt.Errorf("%w: tls metadata requires TLS to be enabled", ErrInvalidValkeyDescriptor)
	}
	return nil
}

func validateValkeyAuth(auth ValkeyAuthMetadata) error {
	var errs []error
	if strings.ContainsAny(auth.Username, " \t\r\n") {
		errs = append(errs, fmt.Errorf("%w: auth username must not contain whitespace", ErrInvalidValkeyDescriptor))
	}
	if auth.Password != "" && auth.PasswordEnv != "" {
		errs = append(errs, fmt.Errorf("%w: auth password and password env are mutually exclusive", ErrInvalidValkeyDescriptor))
	}
	if auth.CredentialURL != "" {
		u, err := url.Parse(auth.CredentialURL)
		if err != nil || u.Scheme == "" || u.Host == "" {
			errs = append(errs, fmt.Errorf("%w: credential URL must be absolute", ErrInvalidValkeyDescriptor))
		}
	}
	return errors.Join(errs...)
}

func validateValkeyCluster(cluster ValkeyClusterMetadata) error {
	if !cluster.Enabled {
		if len(cluster.Addresses) > 0 {
			return fmt.Errorf("%w: cluster addresses require cluster mode", ErrInvalidValkeyDescriptor)
		}
		return nil
	}
	if len(cluster.Addresses) == 0 {
		return fmt.Errorf("%w: cluster mode requires at least one address", ErrInvalidValkeyDescriptor)
	}
	return validateValkeyAddresses("cluster address", cluster.Addresses)
}

func validateValkeySentinel(sentinel ValkeySentinelMetadata) error {
	if sentinel.MasterName == "" && len(sentinel.Addresses) == 0 {
		return nil
	}
	var errs []error
	if sentinel.MasterName == "" {
		errs = append(errs, fmt.Errorf("%w: sentinel master name is required", ErrInvalidValkeyDescriptor))
	}
	if len(sentinel.Addresses) == 0 {
		errs = append(errs, fmt.Errorf("%w: sentinel addresses are required", ErrInvalidValkeyDescriptor))
	} else if err := validateValkeyAddresses("sentinel address", sentinel.Addresses); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

func validateValkeyAddresses(label string, addresses []string) error {
	var errs []error
	for _, address := range addresses {
		if err := validateValkeyAddress(label, address, false); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}

func validateValkeyAddress(label, address string, allowEmpty bool) error {
	address = strings.TrimSpace(address)
	if address == "" {
		if allowEmpty {
			return nil
		}
		return fmt.Errorf("%w: %s must be non-empty", ErrInvalidValkeyDescriptor, label)
	}
	if u, err := url.Parse(address); err == nil && u.Scheme != "" && strings.Contains(address, "://") {
		switch u.Scheme {
		case "valkey", "valkeys", "redis", "rediss":
		default:
			return fmt.Errorf("%w: %s scheme %q is unsupported", ErrInvalidValkeyDescriptor, label, u.Scheme)
		}
		if u.Host == "" {
			return fmt.Errorf("%w: %s host is required", ErrInvalidValkeyDescriptor, label)
		}
		return validateValkeyHostPort(label, u.Host)
	}
	return validateValkeyHostPort(label, address)
}

func validateValkeyHostPort(label, hostport string) error {
	host, port, err := net.SplitHostPort(hostport)
	if err != nil {
		return fmt.Errorf("%w: %s must include host and port", ErrInvalidValkeyDescriptor, label)
	}
	if strings.TrimSpace(host) == "" {
		return fmt.Errorf("%w: %s host is required", ErrInvalidValkeyDescriptor, label)
	}
	portNumber, err := strconv.Atoi(port)
	if err != nil || portNumber < 1 || portNumber > 65535 {
		return fmt.Errorf("%w: %s port is invalid", ErrInvalidValkeyDescriptor, label)
	}
	return nil
}

func defaultValkeyAddress(address string) string {
	if strings.TrimSpace(address) == "" {
		return valkeyDefaultAddress
	}
	return strings.TrimSpace(address)
}

func normalizeValkeyAddresses(addresses []string) []string {
	seen := make(map[string]struct{}, len(addresses))
	out := make([]string, 0, len(addresses))
	for _, address := range addresses {
		address = strings.TrimSpace(address)
		if address == "" {
			continue
		}
		if _, ok := seen[address]; ok {
			continue
		}
		seen[address] = struct{}{}
		out = append(out, address)
	}
	sort.Strings(out)
	return out
}

func valkeyMode(d ValkeyDescriptor) string {
	switch {
	case d.Cluster.Enabled:
		return "cluster"
	case d.Sentinel.MasterName != "":
		return "sentinel"
	default:
		return "standalone"
	}
}

func valkeyTopologyAddresses(d ValkeyDescriptor) []string {
	switch valkeyMode(d) {
	case "cluster":
		return d.Cluster.Addresses
	case "sentinel":
		return d.Sentinel.Addresses
	default:
		return []string{defaultValkeyAddress(d.Address)}
	}
}

func redactValkeyAddresses(addresses []string) []string {
	redacted := make([]string, len(addresses))
	for i, address := range addresses {
		redacted[i] = redactValkeyAddress(address)
	}
	return redacted
}

func redactValkeyAddress(address string) string {
	u, err := url.Parse(address)
	if err != nil || u.Scheme == "" {
		return address
	}
	if u.User != nil {
		username := u.User.Username()
		if username == "" {
			u.User = url.UserPassword("redacted", "redacted")
		} else {
			u.User = url.UserPassword(username, "redacted")
		}
	}
	if u.RawQuery != "" {
		u.RawQuery = "redacted"
	}
	return u.String()
}

func valkeyAuthSummary(auth ValkeyAuthMetadata) string {
	switch {
	case auth.CredentialURL != "":
		return "credential_url"
	case auth.PasswordEnv != "":
		return "password_env:" + auth.PasswordEnv
	case auth.Password != "":
		return "password"
	case auth.Username != "":
		return "username"
	default:
		return "none"
	}
}
