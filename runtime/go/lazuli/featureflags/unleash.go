package featureflags

import (
	"errors"
	"fmt"
	"net/url"
	"strings"
	"time"
)

const (
	// UnleashProviderName is the descriptor provider identifier.
	UnleashProviderName = "unleash"
	// DefaultUnleashRefreshInterval is used when no poll interval is configured.
	DefaultUnleashRefreshInterval = 15 * time.Second
	// MinUnleashRefreshInterval is the shortest supported descriptor poll interval.
	MinUnleashRefreshInterval = 5 * time.Second
	// MaxUnleashRefreshInterval is the longest supported descriptor poll interval.
	MaxUnleashRefreshInterval = 24 * time.Hour
)

var (
	// ErrUnleashConfigInvalid means Unleash descriptor config is malformed.
	ErrUnleashConfigInvalid = errors.New("unleash: config invalid")
	// ErrUnleashAPIHostMissing means an API host was not configured.
	ErrUnleashAPIHostMissing = errors.New("unleash: api host missing")
	// ErrUnleashAPIHostInvalid means the configured API host is malformed.
	ErrUnleashAPIHostInvalid = errors.New("unleash: api host invalid")
	// ErrUnleashClientTokenMissing means a client token was not configured.
	ErrUnleashClientTokenMissing = errors.New("unleash: client token missing")
	// ErrUnleashClientTokenInvalid means the client token has unsafe characters.
	ErrUnleashClientTokenInvalid = errors.New("unleash: client token invalid")
	// ErrUnleashAppNameInvalid means the app metadata is malformed.
	ErrUnleashAppNameInvalid = errors.New("unleash: app name invalid")
	// ErrUnleashEnvironmentInvalid means the environment metadata is malformed.
	ErrUnleashEnvironmentInvalid = errors.New("unleash: environment invalid")
	// ErrUnleashProjectInvalid means the project metadata is malformed.
	ErrUnleashProjectInvalid = errors.New("unleash: project invalid")
	// ErrUnleashRefreshIntervalInvalid means the refresh interval is outside supported bounds.
	ErrUnleashRefreshIntervalInvalid = errors.New("unleash: refresh interval invalid")
	// ErrUnleashBootstrapInvalid means bootstrap payload metadata is malformed.
	ErrUnleashBootstrapInvalid = errors.New("unleash: bootstrap invalid")
)

// UnleashConfig is metadata for an Unleash-backed feature flag binding.
type UnleashConfig struct {
	APIHost         string
	ClientToken     string
	AppName         string
	Environment     string
	Project         string
	RefreshInterval time.Duration
	Bootstrap       UnleashBootstrap
}

// Validate checks that config can be used by a future Unleash adapter.
func (c UnleashConfig) Validate() error {
	return ValidateUnleashConfig(c)
}

// Normalize trims metadata, applies defaults, and validates descriptor fields.
func (c UnleashConfig) Normalize() (UnleashConfig, error) {
	return NormalizeUnleashConfig(c)
}

// Plan returns redacted, normalized descriptor metadata.
func (c UnleashConfig) Plan() (UnleashPlan, error) {
	return PlanUnleashConfig(c)
}

// UnleashBootstrap describes a bootstrap payload without storing the payload.
type UnleashBootstrap struct {
	Source       string
	Version      string
	ETag         string
	FetchedAt    time.Time
	PayloadBytes int64
	FeatureCount int
}

// Empty reports whether no bootstrap metadata was configured.
func (b UnleashBootstrap) Empty() bool {
	return strings.TrimSpace(b.Source) == "" &&
		strings.TrimSpace(b.Version) == "" &&
		strings.TrimSpace(b.ETag) == "" &&
		b.FetchedAt.IsZero() &&
		b.PayloadBytes == 0 &&
		b.FeatureCount == 0
}

// Normalize trims bootstrap payload metadata.
func (b UnleashBootstrap) Normalize() UnleashBootstrap {
	b.Source = strings.TrimSpace(b.Source)
	b.Version = strings.TrimSpace(b.Version)
	b.ETag = strings.TrimSpace(b.ETag)
	return b
}

// Validate checks that bootstrap metadata is internally consistent.
func (b UnleashBootstrap) Validate() error {
	b = b.Normalize()
	if b.Empty() {
		return nil
	}

	var errs []error
	if b.PayloadBytes < 0 {
		errs = append(errs, unleashBootstrapError(fmt.Errorf("payload bytes must be non-negative")))
	}
	if b.FeatureCount < 0 {
		errs = append(errs, unleashBootstrapError(fmt.Errorf("feature count must be non-negative")))
	}
	if hasControl(b.Source) || hasControl(b.Version) || hasControl(b.ETag) {
		errs = append(errs, unleashBootstrapError(fmt.Errorf("metadata must not contain control characters")))
	}
	if err := errors.Join(errs...); err != nil {
		return err
	}
	return nil
}

// UnleashPlan is redacted descriptor metadata for diagnostics and codegen.
type UnleashPlan struct {
	Provider        string
	APIHost         string
	RedactedAPIHost string
	RedactedToken   string
	AppName         string
	Environment     string
	Project         string
	RefreshInterval time.Duration
	Bootstrap       UnleashBootstrap
	HasBootstrap    bool
	Summary         string
}

// ValidateUnleashConfig checks Unleash descriptor config without network calls.
func ValidateUnleashConfig(config UnleashConfig) error {
	_, err := NormalizeUnleashConfig(config)
	return err
}

// NormalizeUnleashConfig trims config, applies defaults, and validates metadata.
func NormalizeUnleashConfig(config UnleashConfig) (UnleashConfig, error) {
	config.ClientToken = strings.TrimSpace(config.ClientToken)
	config.AppName = strings.TrimSpace(config.AppName)
	config.Environment = strings.TrimSpace(config.Environment)
	config.Project = strings.TrimSpace(config.Project)
	config.Bootstrap = config.Bootstrap.Normalize()
	if config.RefreshInterval == 0 {
		config.RefreshInterval = DefaultUnleashRefreshInterval
	}

	var errs []error
	apiHost, err := NormalizeUnleashAPIHost(config.APIHost)
	if err != nil {
		errs = append(errs, unleashConfigError(err))
	} else {
		config.APIHost = apiHost
	}

	if config.ClientToken == "" {
		errs = append(errs, unleashConfigError(ErrUnleashClientTokenMissing))
	} else if hasSpaceOrControl(config.ClientToken) {
		errs = append(errs, unleashConfigError(ErrUnleashClientTokenInvalid))
	}
	if hasControl(config.AppName) {
		errs = append(errs, unleashConfigError(ErrUnleashAppNameInvalid))
	}
	if hasControl(config.Environment) {
		errs = append(errs, unleashConfigError(ErrUnleashEnvironmentInvalid))
	}
	if hasControl(config.Project) {
		errs = append(errs, unleashConfigError(ErrUnleashProjectInvalid))
	}
	if config.RefreshInterval < MinUnleashRefreshInterval || config.RefreshInterval > MaxUnleashRefreshInterval {
		errs = append(errs, unleashConfigError(ErrUnleashRefreshIntervalInvalid))
	}
	if err := config.Bootstrap.Validate(); err != nil {
		errs = append(errs, unleashConfigError(err))
	}

	if err := errors.Join(errs...); err != nil {
		return UnleashConfig{}, err
	}
	return config, nil
}

// PlanUnleashConfig returns normalized Unleash metadata with redacted secrets.
func PlanUnleashConfig(config UnleashConfig) (UnleashPlan, error) {
	config, err := NormalizeUnleashConfig(config)
	if err != nil {
		return UnleashPlan{}, err
	}
	plan := UnleashPlan{
		Provider:        UnleashProviderName,
		APIHost:         config.APIHost,
		RedactedAPIHost: RedactUnleashAPIHost(config.APIHost),
		RedactedToken:   RedactUnleashClientToken(config.ClientToken),
		AppName:         config.AppName,
		Environment:     config.Environment,
		Project:         config.Project,
		RefreshInterval: config.RefreshInterval,
		Bootstrap:       config.Bootstrap,
		HasBootstrap:    !config.Bootstrap.Empty(),
	}
	plan.Summary = UnleashSafeSummary(plan)
	return plan, nil
}

// ValidateUnleashAPIHost checks whether apiHost is an absolute http(s) URL.
func ValidateUnleashAPIHost(apiHost string) error {
	_, err := NormalizeUnleashAPIHost(apiHost)
	return err
}

// NormalizeUnleashAPIHost trims and validates the Unleash API host.
func NormalizeUnleashAPIHost(apiHost string) (string, error) {
	apiHost = strings.TrimSpace(apiHost)
	if apiHost == "" {
		return "", ErrUnleashAPIHostMissing
	}
	if hasSpaceOrControl(apiHost) {
		return "", ErrUnleashAPIHostInvalid
	}

	parsed, err := url.Parse(apiHost)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrUnleashAPIHostInvalid, err)
	}
	if !validUnleashAPIHostURL(parsed) || parsed.RawQuery != "" {
		return "", ErrUnleashAPIHostInvalid
	}

	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// RedactUnleashClientToken renders a stable diagnostic form of a client token.
func RedactUnleashClientToken(clientToken string) string {
	clientToken = strings.TrimSpace(clientToken)
	if clientToken == "" {
		return ""
	}
	if len(clientToken) <= 8 {
		return "****"
	}
	return clientToken[:4] + "..." + clientToken[len(clientToken)-4:]
}

// RedactUnleashAPIHost removes URL credentials, query, and fragment fields.
func RedactUnleashAPIHost(apiHost string) string {
	apiHost = strings.TrimSpace(apiHost)
	if apiHost == "" {
		return ""
	}
	parsed, err := url.Parse(apiHost)
	if err != nil || parsed.Host == "" {
		return "[redacted-url]"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	parsed.RawPath = ""
	return parsed.String()
}

// UnleashSafeSummary renders plan metadata without unredacted token or URL fields.
func UnleashSafeSummary(plan UnleashPlan) string {
	fields := []string{
		"provider=" + UnleashProviderName,
		"refresh=" + plan.RefreshInterval.String(),
	}
	if plan.AppName != "" {
		fields = append(fields, "app="+plan.AppName)
	}
	if plan.Environment != "" {
		fields = append(fields, "environment="+plan.Environment)
	}
	if plan.Project != "" {
		fields = append(fields, "project="+plan.Project)
	}
	if plan.RedactedToken != "" {
		fields = append(fields, "clientToken="+plan.RedactedToken)
	}
	if plan.RedactedAPIHost != "" {
		fields = append(fields, "apiHost="+plan.RedactedAPIHost)
	}
	fields = append(fields, "bootstrap="+boolSummary(plan.HasBootstrap))
	return strings.Join(fields, " ")
}

func validUnleashAPIHostURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func unleashConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrUnleashConfigInvalid, err)
}

func unleashBootstrapError(err error) error {
	return fmt.Errorf("%w: %w", ErrUnleashBootstrapInvalid, err)
}
