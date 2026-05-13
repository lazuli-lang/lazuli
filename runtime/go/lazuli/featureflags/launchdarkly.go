package featureflags

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"time"
)

const (
	// LaunchDarklyProviderName is the descriptor provider identifier.
	LaunchDarklyProviderName = "launchdarkly"
	// DefaultLaunchDarklyMode is used when no evaluation mode is configured.
	DefaultLaunchDarklyMode LaunchDarklyMode = LaunchDarklyModeStream
	// DefaultLaunchDarklyRefreshInterval is used when no poll interval is configured.
	DefaultLaunchDarklyRefreshInterval = time.Minute
	// MinLaunchDarklyRefreshInterval is the shortest supported descriptor poll interval.
	MinLaunchDarklyRefreshInterval = 15 * time.Second
	// MaxLaunchDarklyRefreshInterval is the longest supported descriptor poll interval.
	MaxLaunchDarklyRefreshInterval = 24 * time.Hour
)

const (
	// LaunchDarklyModeStream plans a streaming connection.
	LaunchDarklyModeStream LaunchDarklyMode = "stream"
	// LaunchDarklyModePoll plans periodic polling.
	LaunchDarklyModePoll LaunchDarklyMode = "poll"
	// LaunchDarklyModeOffline plans local/offline evaluation.
	LaunchDarklyModeOffline LaunchDarklyMode = "offline"
)

var (
	// ErrLaunchDarklyConfigInvalid means LaunchDarkly descriptor config is malformed.
	ErrLaunchDarklyConfigInvalid = errors.New("launchdarkly: config invalid")
	// ErrLaunchDarklyKeyMissing means neither SDK nor client key metadata was configured.
	ErrLaunchDarklyKeyMissing = errors.New("launchdarkly: key missing")
	// ErrLaunchDarklySDKKeyInvalid means the SDK key has unsafe characters.
	ErrLaunchDarklySDKKeyInvalid = errors.New("launchdarkly: sdk key invalid")
	// ErrLaunchDarklyClientKeyInvalid means the client key has unsafe characters.
	ErrLaunchDarklyClientKeyInvalid = errors.New("launchdarkly: client key invalid")
	// ErrLaunchDarklyEnvironmentInvalid means the environment metadata is malformed.
	ErrLaunchDarklyEnvironmentInvalid = errors.New("launchdarkly: environment invalid")
	// ErrLaunchDarklyProjectInvalid means the project metadata is malformed.
	ErrLaunchDarklyProjectInvalid = errors.New("launchdarkly: project invalid")
	// ErrLaunchDarklyTagInvalid means a descriptor tag is malformed.
	ErrLaunchDarklyTagInvalid = errors.New("launchdarkly: tag invalid")
	// ErrLaunchDarklyModeInvalid means the evaluation mode is unsupported.
	ErrLaunchDarklyModeInvalid = errors.New("launchdarkly: mode invalid")
	// ErrLaunchDarklyRefreshIntervalInvalid means the refresh interval is outside supported bounds.
	ErrLaunchDarklyRefreshIntervalInvalid = errors.New("launchdarkly: refresh interval invalid")
	// ErrLaunchDarklyServiceURLInvalid means the service URL is malformed.
	ErrLaunchDarklyServiceURLInvalid = errors.New("launchdarkly: service url invalid")
)

// LaunchDarklyMode identifies a provider-neutral connection plan.
type LaunchDarklyMode string

// LaunchDarklyConfig is metadata for a LaunchDarkly-backed feature flag binding.
type LaunchDarklyConfig struct {
	SDKKey          string
	ClientKey       string
	Environment     string
	Project         string
	Tags            []string
	Mode            LaunchDarklyMode
	RefreshInterval time.Duration
	ServiceURL      string
}

// Validate checks that config can be used by a future LaunchDarkly adapter.
func (c LaunchDarklyConfig) Validate() error {
	return ValidateLaunchDarklyConfig(c)
}

// Normalize trims metadata, applies defaults, and validates descriptor fields.
func (c LaunchDarklyConfig) Normalize() (LaunchDarklyConfig, error) {
	return NormalizeLaunchDarklyConfig(c)
}

// Plan returns redacted, normalized descriptor metadata.
func (c LaunchDarklyConfig) Plan() (LaunchDarklyPlan, error) {
	return PlanLaunchDarklyConfig(c)
}

// LaunchDarklyPlan is redacted descriptor metadata for diagnostics and codegen.
type LaunchDarklyPlan struct {
	Provider                 string
	Mode                     LaunchDarklyMode
	Streaming                bool
	Polling                  bool
	Offline                  bool
	RefreshInterval          time.Duration
	ServiceURL               string
	RedactedServiceURL       string
	RedactedSDKKey           string
	RedactedClientKey        string
	Environment              string
	Project                  string
	Tags                     []string
	RequiresNetwork          bool
	UsesServerKeyMetadata    bool
	UsesClientSideIDMetadata bool
	Summary                  string
}

// ValidateLaunchDarklyConfig checks LaunchDarkly descriptor config without network calls.
func ValidateLaunchDarklyConfig(config LaunchDarklyConfig) error {
	_, err := NormalizeLaunchDarklyConfig(config)
	return err
}

// NormalizeLaunchDarklyConfig trims config, applies defaults, and validates metadata.
func NormalizeLaunchDarklyConfig(config LaunchDarklyConfig) (LaunchDarklyConfig, error) {
	config.SDKKey = strings.TrimSpace(config.SDKKey)
	config.ClientKey = strings.TrimSpace(config.ClientKey)
	config.Environment = strings.TrimSpace(config.Environment)
	config.Project = strings.TrimSpace(config.Project)
	config.Tags = NormalizeLaunchDarklyTags(config.Tags)
	config.Mode = NormalizeLaunchDarklyMode(config.Mode)
	if config.RefreshInterval == 0 {
		config.RefreshInterval = DefaultLaunchDarklyRefreshInterval
	}

	var errs []error
	if config.ServiceURL != "" {
		serviceURL, err := NormalizeLaunchDarklyServiceURL(config.ServiceURL)
		if err != nil {
			errs = append(errs, launchDarklyConfigError(err))
		} else {
			config.ServiceURL = serviceURL
		}
	}

	if config.Mode == "" {
		config.Mode = DefaultLaunchDarklyMode
	}
	if !validLaunchDarklyMode(config.Mode) {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyModeInvalid))
	}
	if config.Mode != LaunchDarklyModeOffline && config.SDKKey == "" && config.ClientKey == "" {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyKeyMissing))
	}
	if config.SDKKey != "" && hasSpaceOrControl(config.SDKKey) {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklySDKKeyInvalid))
	}
	if config.ClientKey != "" && hasSpaceOrControl(config.ClientKey) {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyClientKeyInvalid))
	}
	if hasControl(config.Environment) {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyEnvironmentInvalid))
	}
	if hasControl(config.Project) {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyProjectInvalid))
	}
	for _, tag := range config.Tags {
		if tag == "" || hasControl(tag) {
			errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyTagInvalid))
			break
		}
	}
	if config.Mode == LaunchDarklyModePoll && (config.RefreshInterval < MinLaunchDarklyRefreshInterval || config.RefreshInterval > MaxLaunchDarklyRefreshInterval) {
		errs = append(errs, launchDarklyConfigError(ErrLaunchDarklyRefreshIntervalInvalid))
	}

	if err := errors.Join(errs...); err != nil {
		return LaunchDarklyConfig{}, err
	}
	return config, nil
}

// PlanLaunchDarklyConfig returns normalized LaunchDarkly metadata with redacted secrets.
func PlanLaunchDarklyConfig(config LaunchDarklyConfig) (LaunchDarklyPlan, error) {
	config, err := NormalizeLaunchDarklyConfig(config)
	if err != nil {
		return LaunchDarklyPlan{}, err
	}
	plan := LaunchDarklyPlan{
		Provider:                 LaunchDarklyProviderName,
		Mode:                     config.Mode,
		Streaming:                config.Mode == LaunchDarklyModeStream,
		Polling:                  config.Mode == LaunchDarklyModePoll,
		Offline:                  config.Mode == LaunchDarklyModeOffline,
		RefreshInterval:          config.RefreshInterval,
		ServiceURL:               config.ServiceURL,
		RedactedServiceURL:       RedactLaunchDarklyServiceURL(config.ServiceURL),
		RedactedSDKKey:           RedactLaunchDarklySDKKey(config.SDKKey),
		RedactedClientKey:        RedactLaunchDarklyClientKey(config.ClientKey),
		Environment:              config.Environment,
		Project:                  config.Project,
		Tags:                     append([]string(nil), config.Tags...),
		RequiresNetwork:          config.Mode != LaunchDarklyModeOffline,
		UsesServerKeyMetadata:    config.SDKKey != "",
		UsesClientSideIDMetadata: config.ClientKey != "",
	}
	plan.Summary = LaunchDarklySafeSummary(plan)
	return plan, nil
}

// NormalizeLaunchDarklyMode trims and lowercases an evaluation mode.
func NormalizeLaunchDarklyMode(mode LaunchDarklyMode) LaunchDarklyMode {
	normalized := strings.ToLower(strings.TrimSpace(string(mode)))
	if normalized == "" {
		return DefaultLaunchDarklyMode
	}
	return LaunchDarklyMode(normalized)
}

// NormalizeLaunchDarklyTags trims, deduplicates, and sorts descriptor tags.
func NormalizeLaunchDarklyTags(tags []string) []string {
	seen := make(map[string]struct{}, len(tags))
	normalized := make([]string, 0, len(tags))
	for _, tag := range tags {
		tag = strings.TrimSpace(tag)
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		normalized = append(normalized, tag)
	}
	sort.Strings(normalized)
	return normalized
}

// ValidateLaunchDarklyServiceURL checks whether serviceURL is an absolute http(s) URL.
func ValidateLaunchDarklyServiceURL(serviceURL string) error {
	_, err := NormalizeLaunchDarklyServiceURL(serviceURL)
	return err
}

// NormalizeLaunchDarklyServiceURL trims and validates an optional service URL.
func NormalizeLaunchDarklyServiceURL(serviceURL string) (string, error) {
	serviceURL = strings.TrimSpace(serviceURL)
	if serviceURL == "" {
		return "", nil
	}
	if hasSpaceOrControl(serviceURL) {
		return "", ErrLaunchDarklyServiceURLInvalid
	}

	parsed, err := url.Parse(serviceURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrLaunchDarklyServiceURLInvalid, err)
	}
	if !validLaunchDarklyServiceURL(parsed) || parsed.RawQuery != "" {
		return "", ErrLaunchDarklyServiceURLInvalid
	}

	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// RedactLaunchDarklySDKKey renders a stable diagnostic form of an SDK key.
func RedactLaunchDarklySDKKey(sdkKey string) string {
	return redactLaunchDarklyKey(sdkKey)
}

// RedactLaunchDarklyClientKey renders a stable diagnostic form of a client-side ID.
func RedactLaunchDarklyClientKey(clientKey string) string {
	return redactLaunchDarklyKey(clientKey)
}

// RedactLaunchDarklyServiceURL removes URL credentials, query, and fragment fields.
func RedactLaunchDarklyServiceURL(serviceURL string) string {
	serviceURL = strings.TrimSpace(serviceURL)
	if serviceURL == "" {
		return ""
	}
	parsed, err := url.Parse(serviceURL)
	if err != nil || parsed.Host == "" {
		return "[redacted-url]"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	parsed.RawPath = ""
	return parsed.String()
}

// LaunchDarklySafeSummary renders plan metadata without secret key or URL fields.
func LaunchDarklySafeSummary(plan LaunchDarklyPlan) string {
	fields := []string{
		"provider=" + LaunchDarklyProviderName,
		"mode=" + string(plan.Mode),
		"network=" + boolSummary(plan.RequiresNetwork),
	}
	if plan.Environment != "" {
		fields = append(fields, "environment="+plan.Environment)
	}
	if plan.Project != "" {
		fields = append(fields, "project="+plan.Project)
	}
	if len(plan.Tags) > 0 {
		fields = append(fields, "tags="+strings.Join(plan.Tags, ","))
	}
	if plan.Polling {
		fields = append(fields, "refresh="+plan.RefreshInterval.String())
	}
	if plan.UsesServerKeyMetadata {
		fields = append(fields, "sdkKey="+plan.RedactedSDKKey)
	}
	if plan.UsesClientSideIDMetadata {
		fields = append(fields, "clientKey="+plan.RedactedClientKey)
	}
	if plan.RedactedServiceURL != "" {
		fields = append(fields, "serviceURL="+plan.RedactedServiceURL)
	}
	return strings.Join(fields, " ")
}

func validLaunchDarklyMode(mode LaunchDarklyMode) bool {
	return mode == LaunchDarklyModeStream || mode == LaunchDarklyModePoll || mode == LaunchDarklyModeOffline
}

func validLaunchDarklyServiceURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func redactLaunchDarklyKey(key string) string {
	key = strings.TrimSpace(key)
	if key == "" {
		return ""
	}
	if len(key) <= 8 {
		return "****"
	}
	return key[:4] + "..." + key[len(key)-4:]
}

func boolSummary(value bool) string {
	if value {
		return "true"
	}
	return "false"
}

func launchDarklyConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrLaunchDarklyConfigInvalid, err)
}
