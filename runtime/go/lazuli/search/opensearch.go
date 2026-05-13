package search

import (
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"strings"
)

const (
	// DefaultOpenSearchShardCount is used when descriptor metadata does not
	// request explicit primary shards.
	DefaultOpenSearchShardCount uint16 = 1
	// DefaultOpenSearchBulkMinSize is the smallest accepted bulk batch size.
	DefaultOpenSearchBulkMinSize uint32 = 1
	// DefaultOpenSearchBulkMaxSize is the default largest accepted bulk batch
	// size for dry-run planning.
	DefaultOpenSearchBulkMaxSize uint32 = 1000
	// DefaultOpenSearchBulkSize is the default requested bulk batch size.
	DefaultOpenSearchBulkSize uint32 = 500
)

var (
	// ErrOpenSearchDescriptorInvalid is returned when an OpenSearch descriptor
	// cannot be normalized into deterministic metadata.
	ErrOpenSearchDescriptorInvalid = errors.New("lazuli/search: opensearch_descriptor_invalid")
)

// OpenSearchRefreshPolicy describes the refresh behavior a future adapter may
// request after write operations. It does not perform writes by itself.
type OpenSearchRefreshPolicy string

const (
	OpenSearchRefreshFalse   OpenSearchRefreshPolicy = "false"
	OpenSearchRefreshTrue    OpenSearchRefreshPolicy = "true"
	OpenSearchRefreshWaitFor OpenSearchRefreshPolicy = "wait_for"
)

// OpenSearchAuthMode describes credential shape without binding to a provider
// SDK or transport.
type OpenSearchAuthMode string

const (
	OpenSearchAuthNone   OpenSearchAuthMode = "none"
	OpenSearchAuthBasic  OpenSearchAuthMode = "basic"
	OpenSearchAuthBearer OpenSearchAuthMode = "bearer"
	OpenSearchAuthAPIKey OpenSearchAuthMode = "api-key"
)

// OpenSearchDescriptor describes endpoint, auth, index, refresh, and bulk
// metadata for a future OpenSearch adapter. Helpers in this file do not make
// SDK or HTTP calls.
type OpenSearchDescriptor struct {
	EndpointURL   string
	AuthMode      OpenSearchAuthMode
	Username      string
	Password      string
	BearerToken   string
	APIKey        string
	IndexName     string
	AliasName     string
	ShardCount    uint16
	ReplicaCount  uint16
	RefreshPolicy OpenSearchRefreshPolicy
	BulkSize      uint32
	BulkBounds    OpenSearchBulkBounds
}

// OpenSearchBulkBounds constrains dry-run bulk batch sizes.
type OpenSearchBulkBounds struct {
	Min     uint32
	Default uint32
	Max     uint32
}

// OpenSearchIndexPlan is a deterministic dry-run plan for index binding
// metadata and bulk batching.
type OpenSearchIndexPlan struct {
	EndpointURL        string
	IndexName          string
	AliasName          string
	ShardCount         uint16
	ReplicaCount       uint16
	RefreshPolicy      OpenSearchRefreshPolicy
	BulkSize           uint32
	BulkBounds         OpenSearchBulkBounds
	EstimatedDocuments uint64
	BulkBatchCount     uint64
}

// OpenSearchRedactedSummary is safe to log or expose in diagnostics.
type OpenSearchRedactedSummary struct {
	EndpointURL    string
	AuthMode       OpenSearchAuthMode
	Username       string
	Password       string
	BearerToken    string
	APIKey         string
	HasUsername    bool
	HasPassword    bool
	HasBearerToken bool
	HasAPIKey      bool
	IndexName      string
	AliasName      string
	ShardCount     uint16
	ReplicaCount   uint16
	RefreshPolicy  OpenSearchRefreshPolicy
	BulkSize       uint32
	BulkBounds     OpenSearchBulkBounds
}

// Normalize returns descriptor with canonical endpoint, auth mode, index,
// refresh, and bulk metadata.
func (d OpenSearchDescriptor) Normalize() (OpenSearchDescriptor, error) {
	var err error
	d.EndpointURL, err = NormalizeOpenSearchEndpointURL(d.EndpointURL)
	if err != nil {
		return OpenSearchDescriptor{}, err
	}
	d.AuthMode = NormalizeOpenSearchAuthMode(d.AuthMode)
	d.Username = strings.TrimSpace(d.Username)
	d.Password = strings.TrimSpace(d.Password)
	d.BearerToken = strings.TrimSpace(d.BearerToken)
	d.APIKey = strings.TrimSpace(d.APIKey)
	d.IndexName, err = NormalizeOpenSearchIndexName(d.IndexName)
	if err != nil {
		return OpenSearchDescriptor{}, err
	}
	d.AliasName, err = NormalizeOpenSearchOptionalIndexName(d.AliasName)
	if err != nil {
		return OpenSearchDescriptor{}, err
	}
	if d.ShardCount == 0 {
		d.ShardCount = DefaultOpenSearchShardCount
	}
	d.RefreshPolicy, err = NormalizeOpenSearchRefreshPolicy(d.RefreshPolicy)
	if err != nil {
		return OpenSearchDescriptor{}, err
	}
	d.BulkBounds, err = NormalizeOpenSearchBulkBounds(d.BulkBounds)
	if err != nil {
		return OpenSearchDescriptor{}, err
	}
	d.BulkSize, err = NormalizeOpenSearchBulkSize(d.BulkSize, d.BulkBounds)
	if err != nil {
		return OpenSearchDescriptor{}, err
	}
	return d, nil
}

// Validate checks descriptor metadata without making network calls.
func (d OpenSearchDescriptor) Validate() error {
	return ValidateOpenSearchDescriptor(d)
}

// PlanIndex returns deterministic index and bulk metadata for expected
// document count.
func (d OpenSearchDescriptor) PlanIndex(estimatedDocuments uint64) (OpenSearchIndexPlan, error) {
	normalized, err := d.Normalize()
	if err != nil {
		return OpenSearchIndexPlan{}, err
	}
	if err := ValidateOpenSearchDescriptor(normalized); err != nil {
		return OpenSearchIndexPlan{}, err
	}
	return PlanOpenSearchIndex(normalized, estimatedDocuments)
}

// RedactedSummary returns normalized descriptor metadata with secret-bearing
// values removed.
func (d OpenSearchDescriptor) RedactedSummary() OpenSearchRedactedSummary {
	normalized, err := d.Normalize()
	if err == nil {
		d = normalized
	} else {
		d.EndpointURL = RedactOpenSearchEndpointURL(d.EndpointURL)
		d.AuthMode = NormalizeOpenSearchAuthMode(d.AuthMode)
		d.Username = strings.TrimSpace(d.Username)
		d.Password = strings.TrimSpace(d.Password)
		d.BearerToken = strings.TrimSpace(d.BearerToken)
		d.APIKey = strings.TrimSpace(d.APIKey)
		if index, indexErr := NormalizeOpenSearchOptionalIndexName(d.IndexName); indexErr == nil {
			d.IndexName = index
		}
		if alias, aliasErr := NormalizeOpenSearchOptionalIndexName(d.AliasName); aliasErr == nil {
			d.AliasName = alias
		}
		if d.ShardCount == 0 {
			d.ShardCount = DefaultOpenSearchShardCount
		}
		if refresh, refreshErr := NormalizeOpenSearchRefreshPolicy(d.RefreshPolicy); refreshErr == nil {
			d.RefreshPolicy = refresh
		}
		if bounds, boundsErr := NormalizeOpenSearchBulkBounds(d.BulkBounds); boundsErr == nil {
			d.BulkBounds = bounds
			if bulk, bulkErr := NormalizeOpenSearchBulkSize(d.BulkSize, bounds); bulkErr == nil {
				d.BulkSize = bulk
			}
		}
	}
	return OpenSearchRedactedSummary{
		EndpointURL:    RedactOpenSearchEndpointURL(d.EndpointURL),
		AuthMode:       d.AuthMode,
		Username:       redactOpenSearchSecret(d.Username),
		Password:       redactOpenSearchSecret(d.Password),
		BearerToken:    redactOpenSearchSecret(d.BearerToken),
		APIKey:         redactOpenSearchSecret(d.APIKey),
		HasUsername:    strings.TrimSpace(d.Username) != "",
		HasPassword:    strings.TrimSpace(d.Password) != "",
		HasBearerToken: strings.TrimSpace(d.BearerToken) != "",
		HasAPIKey:      strings.TrimSpace(d.APIKey) != "",
		IndexName:      d.IndexName,
		AliasName:      d.AliasName,
		ShardCount:     d.ShardCount,
		ReplicaCount:   d.ReplicaCount,
		RefreshPolicy:  d.RefreshPolicy,
		BulkSize:       d.BulkSize,
		BulkBounds:     d.BulkBounds,
	}
}

// NormalizeOpenSearchDescriptor returns a canonical descriptor copy.
func NormalizeOpenSearchDescriptor(descriptor OpenSearchDescriptor) (OpenSearchDescriptor, error) {
	return descriptor.Normalize()
}

// ValidateOpenSearchDescriptor checks endpoint, auth, index, refresh, and bulk
// metadata.
func ValidateOpenSearchDescriptor(descriptor OpenSearchDescriptor) error {
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return err
	}
	errs := []error{
		ValidateOpenSearchEndpointURL(descriptor.EndpointURL),
		ValidateOpenSearchAuth(descriptor),
		ValidateOpenSearchIndexName(descriptor.IndexName),
		ValidateOpenSearchOptionalIndexName(descriptor.AliasName),
		ValidateOpenSearchRefreshPolicy(descriptor.RefreshPolicy),
		ValidateOpenSearchBulkBounds(descriptor.BulkBounds),
	}
	if descriptor.ShardCount == 0 {
		errs = append(errs, fmt.Errorf("%w: shard count is required", ErrOpenSearchDescriptorInvalid))
	}
	return errors.Join(errs...)
}

// NormalizeOpenSearchEndpointURL returns endpoint without userinfo, query, or
// fragment metadata.
func NormalizeOpenSearchEndpointURL(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", fmt.Errorf("%w: endpoint url is required", ErrOpenSearchDescriptorInvalid)
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "", fmt.Errorf("%w: endpoint url is invalid", ErrOpenSearchDescriptorInvalid)
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	if parsed.Scheme != "https" && parsed.Scheme != "http" {
		return "", fmt.Errorf("%w: endpoint url scheme must be http or https", ErrOpenSearchDescriptorInvalid)
	}
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
}

// ValidateOpenSearchEndpointURL checks endpoint shape without connecting to it.
func ValidateOpenSearchEndpointURL(endpoint string) error {
	_, err := NormalizeOpenSearchEndpointURL(endpoint)
	return err
}

// RedactOpenSearchEndpointURL returns a safe endpoint string for diagnostics.
func RedactOpenSearchEndpointURL(raw string) string {
	endpoint, err := NormalizeOpenSearchEndpointURL(raw)
	if err != nil {
		return "[redacted]"
	}
	return endpoint
}

// NormalizeOpenSearchAuthMode returns a canonical auth mode. Empty defaults to
// none.
func NormalizeOpenSearchAuthMode(mode OpenSearchAuthMode) OpenSearchAuthMode {
	value := strings.ToLower(strings.TrimSpace(string(mode)))
	switch value {
	case "", "none", "anonymous":
		return OpenSearchAuthNone
	case "basic", "password":
		return OpenSearchAuthBasic
	case "bearer", "token":
		return OpenSearchAuthBearer
	case "api_key", "api-key", "apikey":
		return OpenSearchAuthAPIKey
	default:
		return OpenSearchAuthMode(value)
	}
}

// ValidateOpenSearchAuth checks credential presence for the descriptor auth
// mode.
func ValidateOpenSearchAuth(descriptor OpenSearchDescriptor) error {
	descriptor.AuthMode = NormalizeOpenSearchAuthMode(descriptor.AuthMode)
	descriptor.Username = strings.TrimSpace(descriptor.Username)
	descriptor.Password = strings.TrimSpace(descriptor.Password)
	descriptor.BearerToken = strings.TrimSpace(descriptor.BearerToken)
	descriptor.APIKey = strings.TrimSpace(descriptor.APIKey)
	switch descriptor.AuthMode {
	case OpenSearchAuthNone:
		if descriptor.Username != "" || descriptor.Password != "" || descriptor.BearerToken != "" || descriptor.APIKey != "" {
			return fmt.Errorf("%w: auth mode none must not set credentials", ErrOpenSearchDescriptorInvalid)
		}
	case OpenSearchAuthBasic:
		if descriptor.Username == "" || descriptor.Password == "" {
			return fmt.Errorf("%w: basic auth requires username and password", ErrOpenSearchDescriptorInvalid)
		}
		if descriptor.BearerToken != "" || descriptor.APIKey != "" {
			return fmt.Errorf("%w: basic auth must not set token credentials", ErrOpenSearchDescriptorInvalid)
		}
	case OpenSearchAuthBearer:
		if descriptor.BearerToken == "" {
			return fmt.Errorf("%w: bearer auth requires token", ErrOpenSearchDescriptorInvalid)
		}
		if descriptor.Username != "" || descriptor.Password != "" || descriptor.APIKey != "" {
			return fmt.Errorf("%w: bearer auth must not set other credentials", ErrOpenSearchDescriptorInvalid)
		}
	case OpenSearchAuthAPIKey:
		if descriptor.APIKey == "" {
			return fmt.Errorf("%w: api-key auth requires api key", ErrOpenSearchDescriptorInvalid)
		}
		if descriptor.Username != "" || descriptor.Password != "" || descriptor.BearerToken != "" {
			return fmt.Errorf("%w: api-key auth must not set other credentials", ErrOpenSearchDescriptorInvalid)
		}
	default:
		return fmt.Errorf("%w: auth mode %q is unknown", ErrOpenSearchDescriptorInvalid, descriptor.AuthMode)
	}
	return nil
}

// NormalizeOpenSearchIndexName returns a canonical OpenSearch index name.
func NormalizeOpenSearchIndexName(name string) (string, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if err := ValidateOpenSearchIndexName(name); err != nil {
		return "", err
	}
	return name, nil
}

// NormalizeOpenSearchOptionalIndexName normalizes an optional index-like name.
func NormalizeOpenSearchOptionalIndexName(name string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", nil
	}
	return NormalizeOpenSearchIndexName(name)
}

// ValidateOpenSearchIndexName checks OpenSearch index-name metadata.
func ValidateOpenSearchIndexName(name string) error {
	name = strings.TrimSpace(name)
	if name == "" {
		return fmt.Errorf("%w: index name is required", ErrOpenSearchDescriptorInvalid)
	}
	if len(name) > 255 {
		return fmt.Errorf("%w: index name must be 255 bytes or fewer", ErrOpenSearchDescriptorInvalid)
	}
	if name == "." || name == ".." {
		return fmt.Errorf("%w: index name must not be . or ..", ErrOpenSearchDescriptorInvalid)
	}
	if strings.HasPrefix(name, "-") || strings.HasPrefix(name, "_") || strings.HasPrefix(name, "+") {
		return fmt.Errorf("%w: index name must not start with -, _, or +", ErrOpenSearchDescriptorInvalid)
	}
	for _, r := range name {
		if r >= 'A' && r <= 'Z' {
			return fmt.Errorf("%w: index name must be lowercase", ErrOpenSearchDescriptorInvalid)
		}
		if r <= ' ' || strings.ContainsRune(`\/*?"<>|,#`, r) {
			return fmt.Errorf("%w: index name contains invalid character %q", ErrOpenSearchDescriptorInvalid, r)
		}
	}
	return nil
}

// ValidateOpenSearchOptionalIndexName checks optional alias/index metadata.
func ValidateOpenSearchOptionalIndexName(name string) error {
	if strings.TrimSpace(name) == "" {
		return nil
	}
	return ValidateOpenSearchIndexName(name)
}

// NormalizeOpenSearchRefreshPolicy returns a supported refresh policy. Empty
// defaults to false.
func NormalizeOpenSearchRefreshPolicy(policy OpenSearchRefreshPolicy) (OpenSearchRefreshPolicy, error) {
	value := strings.ToLower(strings.TrimSpace(string(policy)))
	switch value {
	case "", "false", "none", "disabled":
		return OpenSearchRefreshFalse, nil
	case "true", "immediate":
		return OpenSearchRefreshTrue, nil
	case "wait_for", "wait-for", "wait":
		return OpenSearchRefreshWaitFor, nil
	default:
		return "", fmt.Errorf("%w: refresh policy %q is unknown", ErrOpenSearchDescriptorInvalid, policy)
	}
}

// ValidateOpenSearchRefreshPolicy checks refresh policy metadata.
func ValidateOpenSearchRefreshPolicy(policy OpenSearchRefreshPolicy) error {
	_, err := NormalizeOpenSearchRefreshPolicy(policy)
	return err
}

// NormalizeOpenSearchBulkBounds returns usable bulk bounds.
func NormalizeOpenSearchBulkBounds(bounds OpenSearchBulkBounds) (OpenSearchBulkBounds, error) {
	if bounds.Min == 0 {
		bounds.Min = DefaultOpenSearchBulkMinSize
	}
	if bounds.Max == 0 {
		bounds.Max = DefaultOpenSearchBulkMaxSize
	}
	if bounds.Min > bounds.Max {
		return OpenSearchBulkBounds{}, fmt.Errorf("%w: bulk min must be less than or equal to max", ErrOpenSearchDescriptorInvalid)
	}
	if bounds.Default == 0 {
		bounds.Default = DefaultOpenSearchBulkSize
		if bounds.Default < bounds.Min {
			bounds.Default = bounds.Min
		}
		if bounds.Default > bounds.Max {
			bounds.Default = bounds.Max
		}
	}
	if bounds.Default < bounds.Min || bounds.Default > bounds.Max {
		return OpenSearchBulkBounds{}, fmt.Errorf("%w: bulk default must be within bounds", ErrOpenSearchDescriptorInvalid)
	}
	return bounds, nil
}

// ValidateOpenSearchBulkBounds checks bulk size bounds.
func ValidateOpenSearchBulkBounds(bounds OpenSearchBulkBounds) error {
	_, err := NormalizeOpenSearchBulkBounds(bounds)
	return err
}

// NormalizeOpenSearchBulkSize returns requested size or bounds default when
// size is zero.
func NormalizeOpenSearchBulkSize(size uint32, bounds OpenSearchBulkBounds) (uint32, error) {
	bounds, err := NormalizeOpenSearchBulkBounds(bounds)
	if err != nil {
		return 0, err
	}
	if size == 0 {
		return bounds.Default, nil
	}
	if size < bounds.Min || size > bounds.Max {
		return 0, fmt.Errorf("%w: bulk size %d is outside %d-%d", ErrOpenSearchDescriptorInvalid, size, bounds.Min, bounds.Max)
	}
	return size, nil
}

// PlanOpenSearchIndex returns deterministic dry-run metadata for descriptor.
func PlanOpenSearchIndex(descriptor OpenSearchDescriptor, estimatedDocuments uint64) (OpenSearchIndexPlan, error) {
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return OpenSearchIndexPlan{}, err
	}
	if err := ValidateOpenSearchDescriptor(descriptor); err != nil {
		return OpenSearchIndexPlan{}, err
	}
	return OpenSearchIndexPlan{
		EndpointURL:        descriptor.EndpointURL,
		IndexName:          descriptor.IndexName,
		AliasName:          descriptor.AliasName,
		ShardCount:         descriptor.ShardCount,
		ReplicaCount:       descriptor.ReplicaCount,
		RefreshPolicy:      descriptor.RefreshPolicy,
		BulkSize:           descriptor.BulkSize,
		BulkBounds:         descriptor.BulkBounds,
		EstimatedDocuments: estimatedDocuments,
		BulkBatchCount:     OpenSearchBulkBatchCount(estimatedDocuments, descriptor.BulkSize),
	}, nil
}

// OpenSearchBulkBatchCount returns the number of bulk batches needed for total.
func OpenSearchBulkBatchCount(total uint64, bulkSize uint32) uint64 {
	if total == 0 || bulkSize == 0 {
		return 0
	}
	size := uint64(bulkSize)
	return (total + size - 1) / size
}

func redactOpenSearchSecret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

func (p OpenSearchRefreshPolicy) String() string {
	return string(p)
}

func (m OpenSearchAuthMode) String() string {
	return string(m)
}

func (b OpenSearchBulkBounds) String() string {
	normalized, err := NormalizeOpenSearchBulkBounds(b)
	if err != nil {
		return ""
	}
	return strconv.FormatUint(uint64(normalized.Min), 10) + "-" +
		strconv.FormatUint(uint64(normalized.Default), 10) + "-" +
		strconv.FormatUint(uint64(normalized.Max), 10)
}
