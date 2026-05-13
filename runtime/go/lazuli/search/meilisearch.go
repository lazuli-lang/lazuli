package search

import (
	"errors"
	"fmt"
	"net/url"
	"slices"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	DefaultMeilisearchBatchSize = 1000
	MinMeilisearchBatchSize     = 1
	MaxMeilisearchBatchSize     = 10000

	meilisearchRedacted = "[redacted]"
)

var ErrMeilisearchDescriptorInvalid = errors.New("lazuli/search: meilisearch descriptor invalid")

// MeilisearchDescriptor describes provider-neutral Meilisearch connection and
// index metadata. Helpers in this file do not create clients or perform HTTP
// calls.
type MeilisearchDescriptor struct {
	Host      string
	APIKey    string
	APIKeyEnv string
	Indexes   []MeilisearchIndexDescriptor
	BatchSize int
}

// MeilisearchIndexDescriptor describes one index and its optional attribute
// settings.
type MeilisearchIndexDescriptor struct {
	UID        string
	PrimaryKey string
	Attributes MeilisearchIndexAttributes
}

// MeilisearchIndexAttributes carries searchable/filterable/sortable metadata
// without binding to a concrete SDK settings shape.
type MeilisearchIndexAttributes struct {
	Filterable []string
	Sortable   []string
	Searchable []string
}

// MeilisearchPlan is the validated, normalized descriptor plan.
type MeilisearchPlan struct {
	Descriptor   MeilisearchDescriptor
	Batch        MeilisearchBatchPlan
	IndexCount   int
	APIKeySource string
}

// MeilisearchBatchPlan describes the effective document batch size.
type MeilisearchBatchPlan struct {
	Size      int
	Defaulted bool
	Min       int
	Max       int
	AtMinimum bool
	AtMaximum bool
}

// MeilisearchSummary is safe for logs and diagnostics.
type MeilisearchSummary struct {
	HostRedacted string
	APIKey       string
	APIKeyEnv    string
	APIKeySource string
	BatchSize    int
	IndexCount   int
	Indexes      []MeilisearchIndexSummary
}

// MeilisearchIndexSummary is safe index metadata for logs and diagnostics.
type MeilisearchIndexSummary struct {
	UID             string
	PrimaryKey      string
	FilterableCount int
	SortableCount   int
	SearchableCount int
}

// Normalize returns a deterministic descriptor copy.
func (d MeilisearchDescriptor) Normalize() (MeilisearchDescriptor, error) {
	return NormalizeMeilisearchDescriptor(d)
}

// Validate reports whether descriptor metadata is structurally usable.
func (d MeilisearchDescriptor) Validate() error {
	return ValidateMeilisearchDescriptor(d)
}

// Plan validates descriptor and returns deterministic adapter metadata.
func (d MeilisearchDescriptor) Plan() (MeilisearchPlan, error) {
	return PlanMeilisearchDescriptor(d)
}

// RedactedSummary returns a diagnostics-safe descriptor summary.
func (d MeilisearchDescriptor) RedactedSummary() MeilisearchSummary {
	summary, _ := RedactMeilisearchDescriptor(d)
	return summary
}

// NormalizeMeilisearchDescriptor trims and deduplicates descriptor metadata.
func NormalizeMeilisearchDescriptor(d MeilisearchDescriptor) (MeilisearchDescriptor, error) {
	d.Host = strings.TrimSpace(d.Host)
	d.APIKey = strings.TrimSpace(d.APIKey)
	d.APIKeyEnv = strings.TrimSpace(d.APIKeyEnv)

	indexes := make([]MeilisearchIndexDescriptor, 0, len(d.Indexes))
	seen := make(map[string]struct{}, len(d.Indexes))
	for _, index := range d.Indexes {
		clean, err := NormalizeMeilisearchIndexDescriptor(index)
		if err != nil {
			return MeilisearchDescriptor{}, err
		}
		if _, ok := seen[clean.UID]; ok {
			return MeilisearchDescriptor{}, fmt.Errorf("%w: duplicate index uid %q", ErrMeilisearchDescriptorInvalid, clean.UID)
		}
		seen[clean.UID] = struct{}{}
		indexes = append(indexes, clean)
	}
	slices.SortFunc(indexes, func(a, b MeilisearchIndexDescriptor) int {
		return strings.Compare(a.UID, b.UID)
	})
	d.Indexes = indexes
	return d, nil
}

// ValidateMeilisearchDescriptor checks host, credentials, indexes, attributes,
// and batch bounds without I/O.
func ValidateMeilisearchDescriptor(d MeilisearchDescriptor) error {
	normalized, err := NormalizeMeilisearchDescriptor(d)
	if err != nil {
		return err
	}

	var errs []error
	if err := ValidateMeilisearchHost(normalized.Host); err != nil {
		errs = append(errs, err)
	}
	if normalized.APIKey != "" && normalized.APIKeyEnv != "" {
		errs = append(errs, fmt.Errorf("%w: api key and api key env are mutually exclusive", ErrMeilisearchDescriptorInvalid))
	}
	if err := validateMeilisearchText("api key env", normalized.APIKeyEnv, true); err != nil {
		errs = append(errs, err)
	}
	if strings.ContainsAny(normalized.APIKeyEnv, " \t\r\n") {
		errs = append(errs, fmt.Errorf("%w: api key env must not contain whitespace", ErrMeilisearchDescriptorInvalid))
	}
	if len(normalized.Indexes) == 0 {
		errs = append(errs, fmt.Errorf("%w: at least one index is required", ErrMeilisearchDescriptorInvalid))
	}
	for _, index := range normalized.Indexes {
		if err := ValidateMeilisearchIndexDescriptor(index); err != nil {
			errs = append(errs, err)
		}
	}
	if _, err := PlanMeilisearchBatchSize(normalized.BatchSize); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// PlanMeilisearchDescriptor validates descriptor and returns normalized metadata.
func PlanMeilisearchDescriptor(d MeilisearchDescriptor) (MeilisearchPlan, error) {
	normalized, err := NormalizeMeilisearchDescriptor(d)
	if err != nil {
		return MeilisearchPlan{}, err
	}
	if err := ValidateMeilisearchDescriptor(normalized); err != nil {
		return MeilisearchPlan{}, err
	}
	batch, err := PlanMeilisearchBatchSize(normalized.BatchSize)
	if err != nil {
		return MeilisearchPlan{}, err
	}
	return MeilisearchPlan{
		Descriptor:   normalized,
		Batch:        batch,
		IndexCount:   len(normalized.Indexes),
		APIKeySource: meilisearchAPIKeySource(normalized),
	}, nil
}

// NormalizeMeilisearchIndexDescriptor returns a deterministic index metadata copy.
func NormalizeMeilisearchIndexDescriptor(index MeilisearchIndexDescriptor) (MeilisearchIndexDescriptor, error) {
	uid, err := NormalizeMeilisearchIndexUID(index.UID)
	if err != nil {
		return MeilisearchIndexDescriptor{}, err
	}
	primaryKey, err := normalizeMeilisearchAttribute("primary key", index.PrimaryKey, true)
	if err != nil {
		return MeilisearchIndexDescriptor{}, err
	}
	attributes, err := NormalizeMeilisearchIndexAttributes(index.Attributes)
	if err != nil {
		return MeilisearchIndexDescriptor{}, err
	}
	return MeilisearchIndexDescriptor{
		UID:        uid,
		PrimaryKey: primaryKey,
		Attributes: attributes,
	}, nil
}

// ValidateMeilisearchIndexDescriptor checks index UID, primary key, and settings
// metadata.
func ValidateMeilisearchIndexDescriptor(index MeilisearchIndexDescriptor) error {
	_, err := NormalizeMeilisearchIndexDescriptor(index)
	return err
}

// NormalizeMeilisearchIndexAttributes trims, deduplicates, and sorts attributes.
func NormalizeMeilisearchIndexAttributes(attributes MeilisearchIndexAttributes) (MeilisearchIndexAttributes, error) {
	filterable, err := normalizeMeilisearchAttributes("filterable attribute", attributes.Filterable)
	if err != nil {
		return MeilisearchIndexAttributes{}, err
	}
	sortable, err := normalizeMeilisearchAttributes("sortable attribute", attributes.Sortable)
	if err != nil {
		return MeilisearchIndexAttributes{}, err
	}
	searchable, err := normalizeMeilisearchAttributes("searchable attribute", attributes.Searchable)
	if err != nil {
		return MeilisearchIndexAttributes{}, err
	}
	return MeilisearchIndexAttributes{
		Filterable: filterable,
		Sortable:   sortable,
		Searchable: searchable,
	}, nil
}

// NormalizeMeilisearchIndexUID trims and validates an index UID.
func NormalizeMeilisearchIndexUID(uid string) (string, error) {
	uid = strings.TrimSpace(uid)
	if uid == "" {
		return "", fmt.Errorf("%w: index uid is required", ErrMeilisearchDescriptorInvalid)
	}
	if !utf8.ValidString(uid) {
		return "", fmt.Errorf("%w: index uid must be valid utf-8", ErrMeilisearchDescriptorInvalid)
	}
	for _, r := range uid {
		if !(r == '_' || r == '-' || (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')) {
			return "", fmt.Errorf("%w: index uid %q contains unsupported character", ErrMeilisearchDescriptorInvalid, uid)
		}
	}
	return uid, nil
}

// ValidateMeilisearchIndexUID checks whether uid is structurally usable.
func ValidateMeilisearchIndexUID(uid string) error {
	_, err := NormalizeMeilisearchIndexUID(uid)
	return err
}

// ValidateMeilisearchHost checks that host is an HTTP(S) URL without I/O.
func ValidateMeilisearchHost(host string) error {
	host = strings.TrimSpace(host)
	if host == "" {
		return fmt.Errorf("%w: host is required", ErrMeilisearchDescriptorInvalid)
	}
	u, err := url.Parse(host)
	if err != nil || u.Scheme == "" || u.Host == "" {
		return fmt.Errorf("%w: host must be an absolute URL", ErrMeilisearchDescriptorInvalid)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return fmt.Errorf("%w: host scheme %q is unsupported", ErrMeilisearchDescriptorInvalid, u.Scheme)
	}
	return validateMeilisearchText("host", host, false)
}

// PlanMeilisearchBatchSize returns the effective batch size and boundary flags.
func PlanMeilisearchBatchSize(size int) (MeilisearchBatchPlan, error) {
	defaulted := size == 0
	if defaulted {
		size = DefaultMeilisearchBatchSize
	}
	if size < MinMeilisearchBatchSize || size > MaxMeilisearchBatchSize {
		return MeilisearchBatchPlan{}, fmt.Errorf("%w: batch size must be between %d and %d", ErrMeilisearchDescriptorInvalid, MinMeilisearchBatchSize, MaxMeilisearchBatchSize)
	}
	return MeilisearchBatchPlan{
		Size:      size,
		Defaulted: defaulted,
		Min:       MinMeilisearchBatchSize,
		Max:       MaxMeilisearchBatchSize,
		AtMinimum: size == MinMeilisearchBatchSize,
		AtMaximum: size == MaxMeilisearchBatchSize,
	}, nil
}

// RedactMeilisearchHost removes user info, query strings, and fragments.
func RedactMeilisearchHost(host string) string {
	host = strings.TrimSpace(host)
	u, err := url.Parse(host)
	if err != nil || u.Scheme == "" || u.Host == "" {
		return host
	}
	if u.User != nil {
		u.User = url.User("redacted")
	}
	u.RawQuery = ""
	u.Fragment = ""
	return u.String()
}

// RedactMeilisearchAPIKey returns a stable secret-free API key marker.
func RedactMeilisearchAPIKey(apiKey string) string {
	if strings.TrimSpace(apiKey) == "" {
		return ""
	}
	return meilisearchRedacted
}

// RedactMeilisearchDescriptor returns a stable, secret-free summary.
func RedactMeilisearchDescriptor(d MeilisearchDescriptor) (MeilisearchSummary, error) {
	normalized, err := NormalizeMeilisearchDescriptor(d)
	if err != nil {
		return MeilisearchSummary{}, err
	}
	batch, err := PlanMeilisearchBatchSize(normalized.BatchSize)
	if err != nil {
		return MeilisearchSummary{}, err
	}
	indexes := make([]MeilisearchIndexSummary, 0, len(normalized.Indexes))
	for _, index := range normalized.Indexes {
		indexes = append(indexes, MeilisearchIndexSummary{
			UID:             index.UID,
			PrimaryKey:      index.PrimaryKey,
			FilterableCount: len(index.Attributes.Filterable),
			SortableCount:   len(index.Attributes.Sortable),
			SearchableCount: len(index.Attributes.Searchable),
		})
	}
	return MeilisearchSummary{
		HostRedacted: RedactMeilisearchHost(normalized.Host),
		APIKey:       RedactMeilisearchAPIKey(normalized.APIKey),
		APIKeyEnv:    normalized.APIKeyEnv,
		APIKeySource: meilisearchAPIKeySource(normalized),
		BatchSize:    batch.Size,
		IndexCount:   len(indexes),
		Indexes:      indexes,
	}, nil
}

// RedactedSummary returns a diagnostics-safe plan summary.
func (p MeilisearchPlan) RedactedSummary() MeilisearchSummary {
	summary, _ := RedactMeilisearchDescriptor(p.Descriptor)
	return summary
}

func normalizeMeilisearchAttributes(label string, values []string) ([]string, error) {
	if len(values) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{}, len(values))
	out := make([]string, 0, len(values))
	for _, value := range values {
		clean, err := normalizeMeilisearchAttribute(label, value, false)
		if err != nil {
			return nil, err
		}
		if _, ok := seen[clean]; ok {
			continue
		}
		seen[clean] = struct{}{}
		out = append(out, clean)
	}
	slices.Sort(out)
	return out, nil
}

func normalizeMeilisearchAttribute(label, value string, allowEmpty bool) (string, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		if allowEmpty {
			return "", nil
		}
		return "", fmt.Errorf("%w: %s must be non-empty", ErrMeilisearchDescriptorInvalid, label)
	}
	if err := validateMeilisearchText(label, value, false); err != nil {
		return "", err
	}
	return value, nil
}

func validateMeilisearchText(label, value string, allowEmpty bool) error {
	if value == "" {
		if allowEmpty {
			return nil
		}
		return fmt.Errorf("%w: %s must be non-empty", ErrMeilisearchDescriptorInvalid, label)
	}
	if !utf8.ValidString(value) {
		return fmt.Errorf("%w: %s must be valid utf-8", ErrMeilisearchDescriptorInvalid, label)
	}
	for _, r := range value {
		if unicode.IsControl(r) {
			return fmt.Errorf("%w: %s contains control character", ErrMeilisearchDescriptorInvalid, label)
		}
	}
	return nil
}

func meilisearchAPIKeySource(d MeilisearchDescriptor) string {
	switch {
	case d.APIKeyEnv != "":
		return "env:" + d.APIKeyEnv
	case d.APIKey != "":
		return "inline"
	default:
		return "none"
	}
}
