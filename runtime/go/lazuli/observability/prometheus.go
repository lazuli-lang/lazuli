package observability

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
)

const (
	DefaultPrometheusScrapePath     = "/metrics"
	DefaultPrometheusScrapeScheme   = "http"
	DefaultPrometheusScrapeInterval = 30 * time.Second
	DefaultPrometheusScrapeTimeout  = 10 * time.Second

	MinPrometheusScrapeInterval = time.Second
	MaxPrometheusScrapeInterval = 24 * time.Hour
	MinPrometheusScrapeTimeout  = time.Second
	MaxPrometheusScrapeTimeout  = 5 * time.Minute
)

// Typed errors.
var (
	// ErrPrometheusScrapeInvalid is returned when a scrape descriptor cannot be normalized.
	ErrPrometheusScrapeInvalid = errors.New("lazuli/observability: prometheus_scrape_invalid")
)

// PrometheusScrapeDescriptor describes provider-neutral scrape metadata.
//
// It intentionally does not register handlers or assume a service-discovery
// provider. The descriptor is suitable for adapters that need a stable scrape
// path, port, scheme, metadata, interval, timeout, and histogram toggle.
type PrometheusScrapeDescriptor struct {
	Path              string
	Port              int
	Scheme            string
	Labels            map[string]string
	Annotations       map[string]string
	Interval          time.Duration
	Timeout           time.Duration
	HistogramsEnabled bool
}

// PrometheusScrapePlan is the normalized form of a scrape descriptor.
type PrometheusScrapePlan struct {
	Path              string
	Port              int
	Scheme            string
	Labels            map[string]string
	Annotations       map[string]string
	Interval          time.Duration
	Timeout           time.Duration
	HistogramsEnabled bool
}

// PrometheusScrapeSummary is a redacted, stable view of a scrape plan.
type PrometheusScrapeSummary struct {
	Path              string            `json:"path"`
	Port              int               `json:"port"`
	Scheme            string            `json:"scheme"`
	Labels            map[string]string `json:"labels,omitempty"`
	Annotations       map[string]string `json:"annotations,omitempty"`
	Interval          string            `json:"interval"`
	Timeout           string            `json:"timeout"`
	HistogramsEnabled bool              `json:"histograms_enabled"`
}

// NormalizePrometheusScrapeDescriptor returns a deterministic copy of descriptor
// with defaults applied.
func NormalizePrometheusScrapeDescriptor(descriptor PrometheusScrapeDescriptor) PrometheusScrapeDescriptor {
	normalized := PrometheusScrapeDescriptor{
		Path:              strings.TrimSpace(descriptor.Path),
		Port:              descriptor.Port,
		Scheme:            strings.ToLower(strings.TrimSpace(descriptor.Scheme)),
		Labels:            normalizePrometheusMetadata(descriptor.Labels, true),
		Annotations:       normalizePrometheusMetadata(descriptor.Annotations, false),
		Interval:          descriptor.Interval,
		Timeout:           descriptor.Timeout,
		HistogramsEnabled: descriptor.HistogramsEnabled,
	}
	if normalized.Path == "" {
		normalized.Path = DefaultPrometheusScrapePath
	}
	if normalized.Scheme == "" {
		normalized.Scheme = DefaultPrometheusScrapeScheme
	}
	if normalized.Interval == 0 {
		normalized.Interval = DefaultPrometheusScrapeInterval
	}
	if normalized.Timeout == 0 {
		normalized.Timeout = DefaultPrometheusScrapeTimeout
	}
	return normalized
}

// ValidatePrometheusScrapeDescriptor checks descriptor without mutating it.
func ValidatePrometheusScrapeDescriptor(descriptor PrometheusScrapeDescriptor) error {
	_, err := PlanPrometheusScrape(descriptor)
	return err
}

// PlanPrometheusScrape returns a normalized provider-neutral scrape plan.
func PlanPrometheusScrape(descriptor PrometheusScrapeDescriptor) (PrometheusScrapePlan, error) {
	normalized := NormalizePrometheusScrapeDescriptor(descriptor)
	if err := validatePrometheusScrapePath(normalized.Path); err != nil {
		return PrometheusScrapePlan{}, err
	}
	if normalized.Port < 1 || normalized.Port > 65535 {
		return PrometheusScrapePlan{}, fmt.Errorf("%w: port must be between 1 and 65535", ErrPrometheusScrapeInvalid)
	}
	if normalized.Scheme != "http" && normalized.Scheme != "https" {
		return PrometheusScrapePlan{}, fmt.Errorf("%w: scheme must be http or https", ErrPrometheusScrapeInvalid)
	}
	if err := validatePrometheusMetadata(normalized.Labels, true); err != nil {
		return PrometheusScrapePlan{}, err
	}
	if err := validatePrometheusMetadata(normalized.Annotations, false); err != nil {
		return PrometheusScrapePlan{}, err
	}
	if normalized.Interval < MinPrometheusScrapeInterval || normalized.Interval > MaxPrometheusScrapeInterval {
		return PrometheusScrapePlan{}, fmt.Errorf("%w: interval must be between %s and %s",
			ErrPrometheusScrapeInvalid, MinPrometheusScrapeInterval, MaxPrometheusScrapeInterval)
	}
	if normalized.Timeout < MinPrometheusScrapeTimeout || normalized.Timeout > MaxPrometheusScrapeTimeout {
		return PrometheusScrapePlan{}, fmt.Errorf("%w: timeout must be between %s and %s",
			ErrPrometheusScrapeInvalid, MinPrometheusScrapeTimeout, MaxPrometheusScrapeTimeout)
	}
	if normalized.Timeout > normalized.Interval {
		return PrometheusScrapePlan{}, fmt.Errorf("%w: timeout must not exceed interval", ErrPrometheusScrapeInvalid)
	}
	return PrometheusScrapePlan{
		Path:              normalized.Path,
		Port:              normalized.Port,
		Scheme:            normalized.Scheme,
		Labels:            clonePrometheusMetadata(normalized.Labels),
		Annotations:       clonePrometheusMetadata(normalized.Annotations),
		Interval:          normalized.Interval,
		Timeout:           normalized.Timeout,
		HistogramsEnabled: normalized.HistogramsEnabled,
	}, nil
}

// SafeSummary returns a redacted scrape summary suitable for diagnostics.
func (p PrometheusScrapePlan) SafeSummary() PrometheusScrapeSummary {
	return PrometheusScrapeSummary{
		Path:              p.Path,
		Port:              p.Port,
		Scheme:            p.Scheme,
		Labels:            redactPrometheusMetadata(p.Labels),
		Annotations:       redactPrometheusMetadata(p.Annotations),
		Interval:          p.Interval.String(),
		Timeout:           p.Timeout.String(),
		HistogramsEnabled: p.HistogramsEnabled,
	}
}

func validatePrometheusScrapePath(path string) error {
	if path == "" || !strings.HasPrefix(path, "/") || strings.HasPrefix(path, "//") {
		return fmt.Errorf("%w: path must be an absolute URL path", ErrPrometheusScrapeInvalid)
	}
	if strings.ContainsAny(path, "?#") {
		return fmt.Errorf("%w: path must not include query or fragment", ErrPrometheusScrapeInvalid)
	}
	for _, r := range path {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return fmt.Errorf("%w: path must not contain whitespace or control characters", ErrPrometheusScrapeInvalid)
		}
	}
	parsed, err := url.ParseRequestURI(path)
	if err != nil || parsed.Path != path {
		return fmt.Errorf("%w: invalid path", ErrPrometheusScrapeInvalid)
	}
	return nil
}

func normalizePrometheusMetadata(metadata map[string]string, labels bool) map[string]string {
	if len(metadata) == 0 {
		return nil
	}
	keys := make([]string, 0, len(metadata))
	for key := range metadata {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	normalized := make(map[string]string, len(metadata))
	for _, key := range keys {
		normalizedKey := strings.TrimSpace(key)
		normalizedValue := strings.TrimSpace(metadata[key])
		if normalizedKey == "" || normalizedValue == "" {
			continue
		}
		if labels {
			normalizedKey = strings.ReplaceAll(normalizedKey, "-", "_")
		}
		normalized[normalizedKey] = normalizedValue
	}
	if len(normalized) == 0 {
		return nil
	}
	return normalized
}

func validatePrometheusMetadata(metadata map[string]string, labels bool) error {
	for key := range metadata {
		if labels {
			if !validPrometheusLabelName(key) {
				return fmt.Errorf("%w: invalid label %q", ErrPrometheusScrapeInvalid, key)
			}
			continue
		}
		if !validPrometheusAnnotationKey(key) {
			return fmt.Errorf("%w: invalid annotation %q", ErrPrometheusScrapeInvalid, key)
		}
	}
	return nil
}

func validPrometheusLabelName(name string) bool {
	if name == "" {
		return false
	}
	for i, r := range name {
		if i == 0 {
			if !isPrometheusLabelLetter(r) && r != '_' {
				return false
			}
			continue
		}
		if !isPrometheusLabelLetter(r) && !isPrometheusLabelDigit(r) && r != '_' {
			return false
		}
	}
	return true
}

func validPrometheusAnnotationKey(key string) bool {
	if key == "" || len(key) > 253 {
		return false
	}
	for _, r := range key {
		if isPrometheusLabelLetter(r) || isPrometheusLabelDigit(r) {
			continue
		}
		switch r {
		case '.', '/', '_', '-':
			continue
		default:
			return false
		}
	}
	return true
}

func isPrometheusLabelLetter(r rune) bool {
	return r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z'
}

func isPrometheusLabelDigit(r rune) bool {
	return r >= '0' && r <= '9'
}

func clonePrometheusMetadata(metadata map[string]string) map[string]string {
	if len(metadata) == 0 {
		return nil
	}
	out := make(map[string]string, len(metadata))
	for key, value := range metadata {
		out[key] = value
	}
	return out
}

func redactPrometheusMetadata(metadata map[string]string) map[string]string {
	if len(metadata) == 0 {
		return nil
	}
	out := make(map[string]string, len(metadata))
	for key, value := range metadata {
		if shouldRedactPrometheusMetadata(key, value) {
			out[key] = "[redacted]"
			continue
		}
		out[key] = value
	}
	return out
}

func shouldRedactPrometheusMetadata(key, value string) bool {
	key = strings.ToLower(strings.TrimSpace(key))
	for _, marker := range []string{"secret", "token", "password", "credential", "apikey", "api_key", "private_key", "dsn", "url"} {
		if strings.Contains(key, marker) {
			return true
		}
	}
	value = strings.TrimSpace(value)
	if strings.Contains(value, "://") {
		parsed, err := url.Parse(value)
		return err == nil && parsed.Scheme != "" && parsed.Host != ""
	}
	return false
}
