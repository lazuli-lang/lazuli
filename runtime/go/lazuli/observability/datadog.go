package observability

import (
	"errors"
	"fmt"
	"net/url"
	"strings"
	"time"
	"unicode"
)

var (
	// ErrDatadogDescriptorInvalid is returned when a Datadog descriptor cannot
	// be normalized or bound deterministically by a future adapter.
	ErrDatadogDescriptorInvalid = errors.New("lazuli/observability: datadog_descriptor_invalid")
)

const (
	DatadogDefaultSite          = "https://datadoghq.com"
	DatadogDefaultFlushInterval = 5 * time.Second
	DatadogMinFlushInterval     = 1 * time.Second
	DatadogMaxFlushInterval     = 5 * time.Minute
)

// DatadogDescriptor describes Datadog observability metadata a future adapter
// may bind. It performs no SDK, HTTP, or environment calls.
type DatadogDescriptor struct {
	Site string

	APIKey string
	AppKey string

	Service string
	Env     string
	Version string

	LogsEnabled    bool
	TracesEnabled  bool
	MetricsEnabled bool
	FlushInterval  time.Duration
}

// DatadogPlan is a dry-run binding plan with normalized descriptor metadata.
type DatadogPlan struct {
	Site string

	Service string
	Env     string
	Version string

	LogsEnabled    bool
	TracesEnabled  bool
	MetricsEnabled bool
	FlushInterval  time.Duration

	HasAPIKey bool
	HasAppKey bool
	Summary   DatadogRedactedSummary
}

// DatadogRedactedSummary is safe to log or expose in diagnostics.
type DatadogRedactedSummary struct {
	Site string

	APIKey string
	AppKey string

	Service string
	Env     string
	Version string

	LogsEnabled    bool
	TracesEnabled  bool
	MetricsEnabled bool
	FlushInterval  time.Duration

	HasAPIKey bool
	HasAppKey bool
}

// Normalize returns a descriptor with canonical site, metadata, and flush
// interval values.
func (d DatadogDescriptor) Normalize() (DatadogDescriptor, error) {
	var err error
	d.Site, err = NormalizeDatadogSite(d.Site)
	if err != nil {
		return DatadogDescriptor{}, err
	}
	d.APIKey = strings.TrimSpace(d.APIKey)
	d.AppKey = strings.TrimSpace(d.AppKey)
	d.Service = NormalizeDatadogService(d.Service)
	d.Env = NormalizeDatadogEnv(d.Env)
	d.Version = NormalizeDatadogVersion(d.Version)
	d.FlushInterval = NormalizeDatadogFlushInterval(d.FlushInterval)
	return d, nil
}

// Validate checks that descriptor metadata can be bound deterministically by an
// adapter.
func (d DatadogDescriptor) Validate() error {
	return ValidateDatadogDescriptor(d)
}

// Plan returns a deterministic dry-run plan for a future Datadog adapter.
func (d DatadogDescriptor) Plan() (DatadogPlan, error) {
	return PlanDatadogDescriptor(d)
}

// RedactedSummary returns descriptor metadata with secret-bearing values
// redacted for diagnostics.
func (d DatadogDescriptor) RedactedSummary() DatadogRedactedSummary {
	normalized, err := d.Normalize()
	if err == nil {
		d = normalized
	} else {
		d.APIKey = strings.TrimSpace(d.APIKey)
		d.AppKey = strings.TrimSpace(d.AppKey)
		d.Service = NormalizeDatadogService(d.Service)
		d.Env = NormalizeDatadogEnv(d.Env)
		d.Version = NormalizeDatadogVersion(d.Version)
		d.FlushInterval = NormalizeDatadogFlushInterval(d.FlushInterval)
	}
	return DatadogRedactedSummary{
		Site:           RedactDatadogURL(d.Site),
		APIKey:         RedactDatadogSecret(d.APIKey),
		AppKey:         RedactDatadogSecret(d.AppKey),
		Service:        d.Service,
		Env:            d.Env,
		Version:        d.Version,
		LogsEnabled:    d.LogsEnabled,
		TracesEnabled:  d.TracesEnabled,
		MetricsEnabled: d.MetricsEnabled,
		FlushInterval:  d.FlushInterval,
		HasAPIKey:      strings.TrimSpace(d.APIKey) != "",
		HasAppKey:      strings.TrimSpace(d.AppKey) != "",
	}
}

// NormalizeDatadogDescriptor returns a canonical descriptor copy.
func NormalizeDatadogDescriptor(descriptor DatadogDescriptor) (DatadogDescriptor, error) {
	return descriptor.Normalize()
}

// ValidateDatadogDescriptor checks site, key presence, service metadata, and
// flush interval bounds.
func ValidateDatadogDescriptor(descriptor DatadogDescriptor) error {
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return err
	}
	errs := []error{
		ValidateDatadogSite(descriptor.Site),
		validateDatadogLabel("service", descriptor.Service),
		validateDatadogLabel("env", descriptor.Env),
		validateDatadogLabel("version", descriptor.Version),
		ValidateDatadogFlushInterval(descriptor.FlushInterval),
	}
	if descriptor.LogsEnabled || descriptor.TracesEnabled || descriptor.MetricsEnabled {
		if descriptor.APIKey == "" {
			errs = append(errs, fmt.Errorf("%w: api key is required when telemetry is enabled", ErrDatadogDescriptorInvalid))
		}
	}
	return errors.Join(errs...)
}

// PlanDatadogDescriptor returns a deterministic dry-run binding plan.
func PlanDatadogDescriptor(descriptor DatadogDescriptor) (DatadogPlan, error) {
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return DatadogPlan{}, err
	}
	if err := ValidateDatadogDescriptor(descriptor); err != nil {
		return DatadogPlan{}, err
	}
	return DatadogPlan{
		Site:           descriptor.Site,
		Service:        descriptor.Service,
		Env:            descriptor.Env,
		Version:        descriptor.Version,
		LogsEnabled:    descriptor.LogsEnabled,
		TracesEnabled:  descriptor.TracesEnabled,
		MetricsEnabled: descriptor.MetricsEnabled,
		FlushInterval:  descriptor.FlushInterval,
		HasAPIKey:      descriptor.APIKey != "",
		HasAppKey:      descriptor.AppKey != "",
		Summary:        descriptor.RedactedSummary(),
	}, nil
}

// NormalizeDatadogSite returns a canonical HTTPS site URL. Empty values default
// to DatadogDefaultSite. Userinfo, query strings, and fragments are stripped.
func NormalizeDatadogSite(site string) (string, error) {
	site = strings.TrimSpace(site)
	if site == "" {
		return DatadogDefaultSite, nil
	}
	if !strings.Contains(site, "://") {
		site = "https://" + site
	}
	parsed, err := url.Parse(site)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "", fmt.Errorf("%w: site url is invalid", ErrDatadogDescriptorInvalid)
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	if parsed.Scheme != "https" && parsed.Scheme != "http" {
		return "", fmt.Errorf("%w: site url scheme must be http or https", ErrDatadogDescriptorInvalid)
	}
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
}

// ValidateDatadogSite checks site URL metadata without making a network call.
func ValidateDatadogSite(site string) error {
	_, err := NormalizeDatadogSite(site)
	return err
}

func NormalizeDatadogService(service string) string {
	return normalizeDatadogLabel(service)
}

func NormalizeDatadogEnv(env string) string {
	return normalizeDatadogLabel(env)
}

func NormalizeDatadogVersion(version string) string {
	return strings.TrimSpace(version)
}

// NormalizeDatadogFlushInterval returns the default flush interval for zero
// values. Non-zero values are left unchanged so validation can report bounds.
func NormalizeDatadogFlushInterval(interval time.Duration) time.Duration {
	if interval == 0 {
		return DatadogDefaultFlushInterval
	}
	return interval
}

// ValidateDatadogFlushInterval checks flush interval bounds.
func ValidateDatadogFlushInterval(interval time.Duration) error {
	interval = NormalizeDatadogFlushInterval(interval)
	if interval < DatadogMinFlushInterval || interval > DatadogMaxFlushInterval {
		return fmt.Errorf("%w: flush interval must be between %s and %s", ErrDatadogDescriptorInvalid, DatadogMinFlushInterval, DatadogMaxFlushInterval)
	}
	return nil
}

// RedactDatadogSecret masks non-empty API and app key values.
func RedactDatadogSecret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

// RedactDatadogURL strips credentials, query strings, and fragments from URLs.
// Unparseable non-empty values are replaced with a redaction marker.
func RedactDatadogURL(raw string) string {
	normalized, err := NormalizeDatadogSite(raw)
	if err != nil {
		if strings.TrimSpace(raw) == "" {
			return ""
		}
		return "[redacted]"
	}
	return normalized
}

func normalizeDatadogLabel(value string) string {
	value = strings.TrimSpace(value)
	value = strings.ReplaceAll(value, "_", "-")
	value = strings.ToLower(value)
	return value
}

func validateDatadogLabel(name, value string) error {
	if value == "" {
		return nil
	}
	if len(value) > 200 {
		return fmt.Errorf("%w: %s must be at most 200 characters", ErrDatadogDescriptorInvalid, name)
	}
	for _, r := range value {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			continue
		}
		switch r {
		case '-', '.', ':', '/', '@':
			continue
		default:
			return fmt.Errorf("%w: %s contains invalid character %q", ErrDatadogDescriptorInvalid, name, r)
		}
	}
	return nil
}
