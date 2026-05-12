package deploy

import (
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
	"unicode"
)

const (
	// DefaultHealthGateTimeout is the per-attempt timeout used when a health
	// gate config does not set one.
	DefaultHealthGateTimeout = 2 * time.Second
	// DefaultHealthGateRetryAttempts is the total attempt count per endpoint.
	DefaultHealthGateRetryAttempts = 3
	// DefaultHealthGateRetryInterval is the fixed delay between attempts.
	DefaultHealthGateRetryInterval = time.Second
)

// ErrInvalidHealthGateConfig reports an invalid deploy health gate or smoke
// test plan.
var ErrInvalidHealthGateConfig = errors.New("lazuli/deploy: invalid health gate config")

// HealthGateConfig describes a provider-neutral deploy smoke test plan. It
// only plans checks; executing HTTP requests belongs to deploy adapters.
type HealthGateConfig struct {
	Endpoints []HealthGateEndpoint
	Timeout   time.Duration
	Retry     HealthGateRetry
}

// HealthGateEndpoint is one HTTP endpoint assertion in a deploy health gate.
type HealthGateEndpoint struct {
	Name                  string
	Method                string
	URL                   string
	ExpectedStatus        int
	ExpectedBodySubstring string
}

// HealthGateRetry controls per-endpoint retry behavior. Attempts is the total
// number of attempts, including the first request.
type HealthGateRetry struct {
	Attempts int
	Interval time.Duration
}

// HealthGatePlan is the normalized dry-run plan for deploy health gates.
type HealthGatePlan struct {
	DryRun    bool
	Endpoints []HealthGateEndpoint
	Timeout   time.Duration
	Retry     HealthGateRetry
}

// HealthEndpoint returns a GET endpoint that expects HTTP 200.
func HealthEndpoint(name, endpointURL string) HealthGateEndpoint {
	return HealthGateEndpoint{
		Name:           name,
		Method:         http.MethodGet,
		URL:            endpointURL,
		ExpectedStatus: http.StatusOK,
	}
}

// WithMethod returns a copy of e using method.
func (e HealthGateEndpoint) WithMethod(method string) HealthGateEndpoint {
	e.Method = method
	return e
}

// ExpectStatus returns a copy of e using status as the expected HTTP status.
func (e HealthGateEndpoint) ExpectStatus(status int) HealthGateEndpoint {
	e.ExpectedStatus = status
	return e
}

// ExpectBodySubstring returns a copy of e requiring value in the response body.
func (e HealthGateEndpoint) ExpectBodySubstring(value string) HealthGateEndpoint {
	e.ExpectedBodySubstring = value
	return e
}

// Validate checks that config can be planned after defaults are applied.
func (c HealthGateConfig) Validate() error {
	return ValidateHealthGateConfig(c)
}

// Plan returns the normalized dry-run health gate plan.
func (c HealthGateConfig) Plan() (HealthGatePlan, error) {
	return BuildHealthGatePlan(c)
}

// RenderDryRunPlan renders the normalized plan without executing any checks.
func (c HealthGateConfig) RenderDryRunPlan() (string, error) {
	return RenderHealthGateDryRunPlan(c)
}

// ValidateHealthGateConfig checks whether config can produce a dry-run plan.
func ValidateHealthGateConfig(config HealthGateConfig) error {
	_, err := normalizeHealthGateConfig(config)
	return err
}

// BuildHealthGatePlan returns a normalized dry-run plan. It does not make any
// HTTP requests.
func BuildHealthGatePlan(config HealthGateConfig) (HealthGatePlan, error) {
	return normalizeHealthGateConfig(config)
}

// RenderHealthGateDryRunPlan renders a deterministic YAML-like dry-run plan.
// It does not make any HTTP requests.
func RenderHealthGateDryRunPlan(config HealthGateConfig) (string, error) {
	plan, err := normalizeHealthGateConfig(config)
	if err != nil {
		return "", err
	}
	return renderHealthGatePlan(plan), nil
}

func normalizeHealthGateConfig(config HealthGateConfig) (HealthGatePlan, error) {
	var errs []error
	if len(config.Endpoints) == 0 {
		errs = append(errs, invalidHealthGateConfig("endpoints", "at least one endpoint is required"))
	}

	timeout := config.Timeout
	if timeout == 0 {
		timeout = DefaultHealthGateTimeout
	} else if timeout < 0 {
		errs = append(errs, invalidHealthGateConfig("timeout", "must be positive"))
	}

	retry := config.Retry
	if retry.Attempts == 0 {
		retry.Attempts = DefaultHealthGateRetryAttempts
	} else if retry.Attempts < 0 {
		errs = append(errs, invalidHealthGateConfig("retry.attempts", "must be positive"))
	}
	if retry.Interval == 0 {
		retry.Interval = DefaultHealthGateRetryInterval
	} else if retry.Interval < 0 {
		errs = append(errs, invalidHealthGateConfig("retry.interval", "must be positive"))
	}

	endpoints := make([]HealthGateEndpoint, 0, len(config.Endpoints))
	names := make(map[string]int, len(config.Endpoints))
	for i, endpoint := range config.Endpoints {
		normalized, endpointErrs := normalizeHealthGateEndpoint(endpoint, i)
		errs = append(errs, endpointErrs...)
		if normalized.Name == "" {
			continue
		}
		if first, ok := names[normalized.Name]; ok {
			errs = append(errs, invalidHealthGateConfig(
				fmt.Sprintf("endpoints[%d].name", i),
				fmt.Sprintf("duplicates endpoints[%d].name %q", first, normalized.Name),
			))
			continue
		}
		names[normalized.Name] = i
		endpoints = append(endpoints, normalized)
	}

	if err := errors.Join(errs...); err != nil {
		return HealthGatePlan{}, err
	}
	return HealthGatePlan{
		DryRun:    true,
		Endpoints: endpoints,
		Timeout:   timeout,
		Retry:     retry,
	}, nil
}

func normalizeHealthGateEndpoint(endpoint HealthGateEndpoint, index int) (HealthGateEndpoint, []error) {
	var errs []error
	field := fmt.Sprintf("endpoints[%d]", index)

	endpoint.Name = strings.TrimSpace(endpoint.Name)
	if endpoint.Name == "" {
		endpoint.Name = fmt.Sprintf("endpoint-%d", index+1)
	} else if !validComposeName(endpoint.Name) {
		errs = append(errs, invalidHealthGateConfig(field+".name", fmt.Sprintf("invalid name %q", endpoint.Name)))
	}

	endpoint.Method = strings.ToUpper(strings.TrimSpace(endpoint.Method))
	if endpoint.Method == "" {
		endpoint.Method = http.MethodGet
	} else if !validHealthGateMethod(endpoint.Method) {
		errs = append(errs, invalidHealthGateConfig(field+".method", fmt.Sprintf("invalid method %q", endpoint.Method)))
	}

	endpoint.URL = strings.TrimSpace(endpoint.URL)
	if !validHealthGateURL(endpoint.URL) {
		errs = append(errs, invalidHealthGateConfig(field+".url", "must be an absolute http or https URL without credentials, fragments, whitespace, or control characters"))
	}

	if endpoint.ExpectedStatus == 0 {
		endpoint.ExpectedStatus = http.StatusOK
	} else if endpoint.ExpectedStatus < 100 || endpoint.ExpectedStatus > 599 {
		errs = append(errs, invalidHealthGateConfig(field+".expected_status", "must be between 100 and 599"))
	}

	return endpoint, errs
}

func renderHealthGatePlan(plan HealthGatePlan) string {
	var b strings.Builder
	b.WriteString("health_gate:\n")
	b.WriteString("  dry_run: ")
	b.WriteString(strconv.FormatBool(plan.DryRun))
	b.WriteByte('\n')
	writeHealthGateDuration(&b, 2, "timeout", plan.Timeout)
	b.WriteString("  retry:\n")
	b.WriteString("    attempts: ")
	b.WriteString(strconv.Itoa(plan.Retry.Attempts))
	b.WriteByte('\n')
	writeHealthGateDuration(&b, 4, "interval", plan.Retry.Interval)
	b.WriteString("  endpoints:\n")
	for _, endpoint := range plan.Endpoints {
		b.WriteString("    - name: ")
		b.WriteString(quoteYAML(endpoint.Name))
		b.WriteByte('\n')
		writeHealthGateString(&b, 6, "method", endpoint.Method)
		writeHealthGateString(&b, 6, "url", endpoint.URL)
		b.WriteString("      expect:\n")
		b.WriteString("        status: ")
		b.WriteString(strconv.Itoa(endpoint.ExpectedStatus))
		b.WriteByte('\n')
		writeHealthGateString(&b, 8, "body_substring", endpoint.ExpectedBodySubstring)
	}
	return b.String()
}

func writeHealthGateString(b *strings.Builder, indent int, key, value string) {
	b.WriteString(strings.Repeat(" ", indent))
	b.WriteString(key)
	b.WriteString(": ")
	b.WriteString(quoteYAML(value))
	b.WriteByte('\n')
}

func writeHealthGateDuration(b *strings.Builder, indent int, key string, value time.Duration) {
	writeHealthGateString(b, indent, key, value.String())
}

func validHealthGateURL(raw string) bool {
	if raw == "" || healthGateHasSpaceOrControl(raw) {
		return false
	}
	parsed, err := url.Parse(raw)
	if err != nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func validHealthGateMethod(method string) bool {
	if method == "" {
		return false
	}
	for _, r := range method {
		if r > unicode.MaxASCII || !healthGateTokenRune(byte(r)) {
			return false
		}
	}
	return true
}

func healthGateTokenRune(r byte) bool {
	switch {
	case r >= 'a' && r <= 'z':
		return true
	case r >= 'A' && r <= 'Z':
		return true
	case r >= '0' && r <= '9':
		return true
	}
	switch r {
	case '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~':
		return true
	default:
		return false
	}
}

func healthGateHasSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func invalidHealthGateConfig(field, detail string) error {
	return fmt.Errorf("%w: %s: %s", ErrInvalidHealthGateConfig, field, detail)
}
