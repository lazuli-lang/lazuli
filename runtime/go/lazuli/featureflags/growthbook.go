package featureflags

import (
	"errors"
	"fmt"
	"net/url"
	"strings"
	"time"
	"unicode"
)

const (
	// GrowthBookProviderName is the descriptor provider identifier.
	GrowthBookProviderName = "growthbook"
	// DefaultGrowthBookAPIHost is the conventional GrowthBook feature payload host.
	DefaultGrowthBookAPIHost = "https://cdn.growthbook.io"
	// DefaultGrowthBookRefreshInterval is used when no poll interval is configured.
	DefaultGrowthBookRefreshInterval = time.Minute
	// MinGrowthBookRefreshInterval is the shortest supported descriptor poll interval.
	MinGrowthBookRefreshInterval = 15 * time.Second
	// MaxGrowthBookRefreshInterval is the longest supported descriptor poll interval.
	MaxGrowthBookRefreshInterval = 24 * time.Hour
)

var (
	// ErrGrowthBookConfigInvalid means GrowthBook descriptor config is malformed.
	ErrGrowthBookConfigInvalid = errors.New("growthbook: config invalid")
	// ErrGrowthBookAPIHostInvalid means the configured API host is malformed.
	ErrGrowthBookAPIHostInvalid = errors.New("growthbook: api host invalid")
	// ErrGrowthBookClientKeyMissing means a client key was not configured.
	ErrGrowthBookClientKeyMissing = errors.New("growthbook: client key missing")
	// ErrGrowthBookClientKeyInvalid means the client key has unsafe characters.
	ErrGrowthBookClientKeyInvalid = errors.New("growthbook: client key invalid")
	// ErrGrowthBookEnvironmentInvalid means the environment metadata is malformed.
	ErrGrowthBookEnvironmentInvalid = errors.New("growthbook: environment invalid")
	// ErrGrowthBookProjectInvalid means the project metadata is malformed.
	ErrGrowthBookProjectInvalid = errors.New("growthbook: project invalid")
	// ErrGrowthBookRefreshIntervalInvalid means the refresh interval is outside supported bounds.
	ErrGrowthBookRefreshIntervalInvalid = errors.New("growthbook: refresh interval invalid")
	// ErrGrowthBookBootstrapInvalid means bootstrap payload metadata is malformed.
	ErrGrowthBookBootstrapInvalid = errors.New("growthbook: bootstrap invalid")
)

// GrowthBookConfig is metadata for a GrowthBook-backed feature flag binding.
type GrowthBookConfig struct {
	APIHost         string
	ClientKey       string
	Environment     string
	Project         string
	RefreshInterval time.Duration
	LocalEvaluation bool
	Bootstrap       GrowthBookBootstrap
}

// Validate checks that config can be used by a future GrowthBook adapter.
func (c GrowthBookConfig) Validate() error {
	return ValidateGrowthBookConfig(c)
}

// Normalize trims metadata, applies defaults, and validates descriptor fields.
func (c GrowthBookConfig) Normalize() (GrowthBookConfig, error) {
	return NormalizeGrowthBookConfig(c)
}

// Plan returns redacted, normalized descriptor metadata.
func (c GrowthBookConfig) Plan() (GrowthBookPlan, error) {
	return PlanGrowthBookConfig(c)
}

// GrowthBookBootstrap describes a bootstrap payload without storing the payload.
type GrowthBookBootstrap struct {
	Source       string
	Version      string
	ETag         string
	FetchedAt    time.Time
	PayloadBytes int64
	FeatureCount int
}

// Empty reports whether no bootstrap metadata was configured.
func (b GrowthBookBootstrap) Empty() bool {
	return strings.TrimSpace(b.Source) == "" &&
		strings.TrimSpace(b.Version) == "" &&
		strings.TrimSpace(b.ETag) == "" &&
		b.FetchedAt.IsZero() &&
		b.PayloadBytes == 0 &&
		b.FeatureCount == 0
}

// Normalize trims bootstrap payload metadata.
func (b GrowthBookBootstrap) Normalize() GrowthBookBootstrap {
	b.Source = strings.TrimSpace(b.Source)
	b.Version = strings.TrimSpace(b.Version)
	b.ETag = strings.TrimSpace(b.ETag)
	return b
}

// Validate checks that bootstrap metadata is internally consistent.
func (b GrowthBookBootstrap) Validate() error {
	b = b.Normalize()
	if b.Empty() {
		return nil
	}

	var errs []error
	if b.PayloadBytes < 0 {
		errs = append(errs, bootstrapError(fmt.Errorf("payload bytes must be non-negative")))
	}
	if b.FeatureCount < 0 {
		errs = append(errs, bootstrapError(fmt.Errorf("feature count must be non-negative")))
	}
	if hasControl(b.Source) || hasControl(b.Version) || hasControl(b.ETag) {
		errs = append(errs, bootstrapError(fmt.Errorf("metadata must not contain control characters")))
	}
	if err := errors.Join(errs...); err != nil {
		return err
	}
	return nil
}

// GrowthBookPlan is redacted descriptor metadata for diagnostics and codegen.
type GrowthBookPlan struct {
	Provider        string
	APIHost         string
	RedactedAPIHost string
	RedactedKey     string
	Environment     string
	Project         string
	RefreshInterval time.Duration
	LocalEvaluation bool
	Bootstrap       GrowthBookBootstrap
	HasBootstrap    bool
}

// ValidateGrowthBookConfig checks GrowthBook descriptor config without network calls.
func ValidateGrowthBookConfig(config GrowthBookConfig) error {
	_, err := NormalizeGrowthBookConfig(config)
	return err
}

// NormalizeGrowthBookConfig trims config, applies defaults, and validates metadata.
func NormalizeGrowthBookConfig(config GrowthBookConfig) (GrowthBookConfig, error) {
	config.ClientKey = strings.TrimSpace(config.ClientKey)
	config.Environment = strings.TrimSpace(config.Environment)
	config.Project = strings.TrimSpace(config.Project)
	config.Bootstrap = config.Bootstrap.Normalize()
	if config.RefreshInterval == 0 {
		config.RefreshInterval = DefaultGrowthBookRefreshInterval
	}

	var errs []error
	apiHost, err := NormalizeGrowthBookAPIHost(config.APIHost)
	if err != nil {
		errs = append(errs, configError(err))
	} else {
		config.APIHost = apiHost
	}

	if config.ClientKey == "" {
		errs = append(errs, configError(ErrGrowthBookClientKeyMissing))
	} else if hasSpaceOrControl(config.ClientKey) {
		errs = append(errs, configError(ErrGrowthBookClientKeyInvalid))
	}
	if hasControl(config.Environment) {
		errs = append(errs, configError(ErrGrowthBookEnvironmentInvalid))
	}
	if hasControl(config.Project) {
		errs = append(errs, configError(ErrGrowthBookProjectInvalid))
	}
	if config.RefreshInterval < MinGrowthBookRefreshInterval || config.RefreshInterval > MaxGrowthBookRefreshInterval {
		errs = append(errs, configError(ErrGrowthBookRefreshIntervalInvalid))
	}
	if err := config.Bootstrap.Validate(); err != nil {
		errs = append(errs, configError(err))
	}

	if err := errors.Join(errs...); err != nil {
		return GrowthBookConfig{}, err
	}
	return config, nil
}

// PlanGrowthBookConfig returns normalized GrowthBook metadata with redacted secrets.
func PlanGrowthBookConfig(config GrowthBookConfig) (GrowthBookPlan, error) {
	config, err := NormalizeGrowthBookConfig(config)
	if err != nil {
		return GrowthBookPlan{}, err
	}
	return GrowthBookPlan{
		Provider:        GrowthBookProviderName,
		APIHost:         config.APIHost,
		RedactedAPIHost: RedactGrowthBookAPIHost(config.APIHost),
		RedactedKey:     RedactGrowthBookClientKey(config.ClientKey),
		Environment:     config.Environment,
		Project:         config.Project,
		RefreshInterval: config.RefreshInterval,
		LocalEvaluation: config.LocalEvaluation,
		Bootstrap:       config.Bootstrap,
		HasBootstrap:    !config.Bootstrap.Empty(),
	}, nil
}

// ValidateGrowthBookAPIHost checks whether apiHost is an absolute http(s) URL.
func ValidateGrowthBookAPIHost(apiHost string) error {
	_, err := NormalizeGrowthBookAPIHost(apiHost)
	return err
}

// NormalizeGrowthBookAPIHost trims and validates the GrowthBook API host.
func NormalizeGrowthBookAPIHost(apiHost string) (string, error) {
	apiHost = strings.TrimSpace(apiHost)
	if apiHost == "" {
		apiHost = DefaultGrowthBookAPIHost
	}
	if hasSpaceOrControl(apiHost) {
		return "", ErrGrowthBookAPIHostInvalid
	}

	parsed, err := url.Parse(apiHost)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrGrowthBookAPIHostInvalid, err)
	}
	if !validAPIHostURL(parsed) || parsed.RawQuery != "" {
		return "", ErrGrowthBookAPIHostInvalid
	}

	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// RedactGrowthBookClientKey renders a stable diagnostic form of a client key.
func RedactGrowthBookClientKey(clientKey string) string {
	clientKey = strings.TrimSpace(clientKey)
	if clientKey == "" {
		return ""
	}
	if len(clientKey) <= 8 {
		return "****"
	}
	return clientKey[:4] + "..." + clientKey[len(clientKey)-4:]
}

// RedactGrowthBookAPIHost removes URL credentials, query, and fragment fields.
func RedactGrowthBookAPIHost(apiHost string) string {
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

func validAPIHostURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func hasSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func configError(err error) error {
	return fmt.Errorf("%w: %w", ErrGrowthBookConfigInvalid, err)
}

func bootstrapError(err error) error {
	return fmt.Errorf("%w: %w", ErrGrowthBookBootstrapInvalid, err)
}
