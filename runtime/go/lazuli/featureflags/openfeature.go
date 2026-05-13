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
	// DefaultOpenFeatureProviderName is used when no provider name is configured.
	DefaultOpenFeatureProviderName = "openfeature"
	// DefaultOpenFeatureMode is used when no evaluation mode is configured.
	DefaultOpenFeatureMode OpenFeatureMode = OpenFeatureModeDefault
	// DefaultOpenFeatureRefreshInterval is used when no refresh interval is configured.
	DefaultOpenFeatureRefreshInterval = time.Minute
	// MinOpenFeatureRefreshInterval is the shortest supported descriptor refresh interval.
	MinOpenFeatureRefreshInterval = 15 * time.Second
	// MaxOpenFeatureRefreshInterval is the longest supported descriptor refresh interval.
	MaxOpenFeatureRefreshInterval = 24 * time.Hour
)

const (
	// OpenFeatureModeDefault plans a provider's normal evaluation behavior.
	OpenFeatureModeDefault OpenFeatureMode = "default"
	// OpenFeatureModeOffline plans local/default evaluation without provider network access.
	OpenFeatureModeOffline OpenFeatureMode = "offline"
)

var (
	// ErrOpenFeatureConfigInvalid means OpenFeature descriptor config is malformed.
	ErrOpenFeatureConfigInvalid = errors.New("openfeature: config invalid")
	// ErrOpenFeatureProviderNameInvalid means the provider name metadata is malformed.
	ErrOpenFeatureProviderNameInvalid = errors.New("openfeature: provider name invalid")
	// ErrOpenFeatureProviderVersionInvalid means the provider version metadata is malformed.
	ErrOpenFeatureProviderVersionInvalid = errors.New("openfeature: provider version invalid")
	// ErrOpenFeatureEvaluationContextInvalid means evaluation context metadata is malformed.
	ErrOpenFeatureEvaluationContextInvalid = errors.New("openfeature: evaluation context invalid")
	// ErrOpenFeatureHookNameInvalid means a hook name is malformed.
	ErrOpenFeatureHookNameInvalid = errors.New("openfeature: hook name invalid")
	// ErrOpenFeatureModeInvalid means the evaluation mode is unsupported.
	ErrOpenFeatureModeInvalid = errors.New("openfeature: mode invalid")
	// ErrOpenFeatureRefreshIntervalInvalid means the refresh interval is outside supported bounds.
	ErrOpenFeatureRefreshIntervalInvalid = errors.New("openfeature: refresh interval invalid")
	// ErrOpenFeatureEndpointURLInvalid means the endpoint URL is malformed.
	ErrOpenFeatureEndpointURLInvalid = errors.New("openfeature: endpoint url invalid")
)

// OpenFeatureMode identifies provider-neutral OpenFeature evaluation planning.
type OpenFeatureMode string

// OpenFeatureConfig is provider-neutral metadata for a future OpenFeature binding.
type OpenFeatureConfig struct {
	ProviderName        string
	ProviderVersion     string
	EvaluationContext   map[string]string
	HookNames           []string
	Mode                OpenFeatureMode
	RefreshInterval     time.Duration
	EndpointURL         string
	DefaultFlagFallback bool
}

// Validate checks that config can be used by a future OpenFeature adapter.
func (c OpenFeatureConfig) Validate() error {
	return ValidateOpenFeatureConfig(c)
}

// Normalize trims metadata, applies defaults, and validates descriptor fields.
func (c OpenFeatureConfig) Normalize() (OpenFeatureConfig, error) {
	return NormalizeOpenFeatureConfig(c)
}

// Plan returns redacted, normalized descriptor metadata.
func (c OpenFeatureConfig) Plan() (OpenFeaturePlan, error) {
	return PlanOpenFeatureConfig(c)
}

// OpenFeaturePlan is redacted descriptor metadata for diagnostics and codegen.
type OpenFeaturePlan struct {
	ProviderName              string
	ProviderVersion           string
	EvaluationContext         map[string]string
	RedactedEvaluationContext map[string]string
	HookNames                 []string
	Mode                      OpenFeatureMode
	Offline                   bool
	RequiresNetwork           bool
	RefreshInterval           time.Duration
	EndpointURL               string
	RedactedEndpointURL       string
	DefaultFlagFallback       bool
	Summary                   string
}

// ValidateOpenFeatureConfig checks OpenFeature descriptor config without SDK calls.
func ValidateOpenFeatureConfig(config OpenFeatureConfig) error {
	_, err := NormalizeOpenFeatureConfig(config)
	return err
}

// NormalizeOpenFeatureConfig trims config, applies defaults, and validates metadata.
func NormalizeOpenFeatureConfig(config OpenFeatureConfig) (OpenFeatureConfig, error) {
	config.ProviderName = strings.TrimSpace(config.ProviderName)
	config.ProviderVersion = strings.TrimSpace(config.ProviderVersion)
	config.EvaluationContext = NormalizeOpenFeatureEvaluationContext(config.EvaluationContext)
	config.HookNames = NormalizeOpenFeatureHookNames(config.HookNames)
	config.Mode = NormalizeOpenFeatureMode(config.Mode)
	if config.ProviderName == "" {
		config.ProviderName = DefaultOpenFeatureProviderName
	}
	if config.RefreshInterval == 0 {
		config.RefreshInterval = DefaultOpenFeatureRefreshInterval
	}

	var errs []error
	if invalidOpenFeatureToken(config.ProviderName) {
		errs = append(errs, openFeatureConfigError(ErrOpenFeatureProviderNameInvalid))
	}
	if hasControl(config.ProviderVersion) {
		errs = append(errs, openFeatureConfigError(ErrOpenFeatureProviderVersionInvalid))
	}
	for name, value := range config.EvaluationContext {
		if invalidOpenFeatureToken(name) || hasControl(value) {
			errs = append(errs, openFeatureConfigError(ErrOpenFeatureEvaluationContextInvalid))
			break
		}
	}
	for _, hook := range config.HookNames {
		if invalidOpenFeatureToken(hook) {
			errs = append(errs, openFeatureConfigError(ErrOpenFeatureHookNameInvalid))
			break
		}
	}
	if !validOpenFeatureMode(config.Mode) {
		errs = append(errs, openFeatureConfigError(ErrOpenFeatureModeInvalid))
	}
	if config.RefreshInterval < MinOpenFeatureRefreshInterval || config.RefreshInterval > MaxOpenFeatureRefreshInterval {
		errs = append(errs, openFeatureConfigError(ErrOpenFeatureRefreshIntervalInvalid))
	}
	if config.EndpointURL != "" {
		endpointURL, err := NormalizeOpenFeatureEndpointURL(config.EndpointURL)
		if err != nil {
			errs = append(errs, openFeatureConfigError(err))
		} else {
			config.EndpointURL = endpointURL
		}
	}

	if err := errors.Join(errs...); err != nil {
		return OpenFeatureConfig{}, err
	}
	return config, nil
}

// PlanOpenFeatureConfig returns normalized OpenFeature metadata with redacted diagnostics.
func PlanOpenFeatureConfig(config OpenFeatureConfig) (OpenFeaturePlan, error) {
	config, err := NormalizeOpenFeatureConfig(config)
	if err != nil {
		return OpenFeaturePlan{}, err
	}
	plan := OpenFeaturePlan{
		ProviderName:              config.ProviderName,
		ProviderVersion:           config.ProviderVersion,
		EvaluationContext:         copyOpenFeatureStringMap(config.EvaluationContext),
		RedactedEvaluationContext: RedactOpenFeatureEvaluationContext(config.EvaluationContext),
		HookNames:                 append([]string(nil), config.HookNames...),
		Mode:                      config.Mode,
		Offline:                   config.Mode == OpenFeatureModeOffline,
		RequiresNetwork:           config.Mode != OpenFeatureModeOffline,
		RefreshInterval:           config.RefreshInterval,
		EndpointURL:               config.EndpointURL,
		RedactedEndpointURL:       RedactOpenFeatureEndpointURL(config.EndpointURL),
		DefaultFlagFallback:       config.DefaultFlagFallback,
	}
	plan.Summary = OpenFeatureSafeSummary(plan)
	return plan, nil
}

// NormalizeOpenFeatureMode trims and lowercases an evaluation mode.
func NormalizeOpenFeatureMode(mode OpenFeatureMode) OpenFeatureMode {
	normalized := strings.ToLower(strings.TrimSpace(string(mode)))
	if normalized == "" {
		return DefaultOpenFeatureMode
	}
	return OpenFeatureMode(normalized)
}

// NormalizeOpenFeatureHookNames trims, deduplicates, and sorts hook names.
func NormalizeOpenFeatureHookNames(hooks []string) []string {
	seen := make(map[string]struct{}, len(hooks))
	normalized := make([]string, 0, len(hooks))
	for _, hook := range hooks {
		hook = strings.TrimSpace(hook)
		if hook == "" {
			continue
		}
		if _, ok := seen[hook]; ok {
			continue
		}
		seen[hook] = struct{}{}
		normalized = append(normalized, hook)
	}
	sort.Strings(normalized)
	return normalized
}

// NormalizeOpenFeatureEvaluationContext trims and sorts evaluation context metadata.
func NormalizeOpenFeatureEvaluationContext(attributes map[string]string) map[string]string {
	if len(attributes) == 0 {
		return nil
	}
	normalized := make(map[string]string, len(attributes))
	keys := sortedOpenFeatureKeys(attributes)
	for _, name := range keys {
		value := attributes[name]
		name = strings.TrimSpace(name)
		if name == "" {
			continue
		}
		normalized[name] = strings.TrimSpace(value)
	}
	if len(normalized) == 0 {
		return nil
	}
	return normalized
}

// ValidateOpenFeatureEndpointURL checks whether endpointURL is an absolute http(s) URL.
func ValidateOpenFeatureEndpointURL(endpointURL string) error {
	_, err := NormalizeOpenFeatureEndpointURL(endpointURL)
	return err
}

// NormalizeOpenFeatureEndpointURL trims and validates an optional endpoint URL.
func NormalizeOpenFeatureEndpointURL(endpointURL string) (string, error) {
	endpointURL = strings.TrimSpace(endpointURL)
	if endpointURL == "" {
		return "", nil
	}
	if hasSpaceOrControl(endpointURL) {
		return "", ErrOpenFeatureEndpointURLInvalid
	}

	parsed, err := url.Parse(endpointURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrOpenFeatureEndpointURLInvalid, err)
	}
	if !validOpenFeatureEndpointURL(parsed) || parsed.RawQuery != "" {
		return "", ErrOpenFeatureEndpointURLInvalid
	}

	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// RedactOpenFeatureEndpointURL removes URL credentials, query, and fragment fields.
func RedactOpenFeatureEndpointURL(endpointURL string) string {
	endpointURL = strings.TrimSpace(endpointURL)
	if endpointURL == "" {
		return ""
	}
	parsed, err := url.Parse(endpointURL)
	if err != nil || parsed.Host == "" {
		return "[redacted-url]"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	parsed.RawPath = ""
	return parsed.String()
}

// RedactOpenFeatureEvaluationContext renders attribute values safe for diagnostics.
func RedactOpenFeatureEvaluationContext(attributes map[string]string) map[string]string {
	if len(attributes) == 0 {
		return nil
	}
	redacted := make(map[string]string, len(attributes))
	for name, value := range NormalizeOpenFeatureEvaluationContext(attributes) {
		if sensitiveOpenFeatureAttribute(name) {
			redacted[name] = "[redacted]"
			continue
		}
		if looksLikeURL(value) {
			redacted[name] = RedactOpenFeatureEndpointURL(value)
			continue
		}
		redacted[name] = value
	}
	if len(redacted) == 0 {
		return nil
	}
	return redacted
}

// OpenFeatureSafeSummary renders plan metadata without raw context values.
func OpenFeatureSafeSummary(plan OpenFeaturePlan) string {
	fields := []string{
		"provider=" + plan.ProviderName,
		"mode=" + string(plan.Mode),
		"network=" + boolSummary(plan.RequiresNetwork),
	}
	if plan.ProviderVersion != "" {
		fields = append(fields, "version="+plan.ProviderVersion)
	}
	if len(plan.HookNames) > 0 {
		fields = append(fields, "hooks="+strings.Join(plan.HookNames, ","))
	}
	if len(plan.RedactedEvaluationContext) > 0 {
		fields = append(fields, "context="+strings.Join(sortedOpenFeatureKeys(plan.RedactedEvaluationContext), ","))
	}
	if plan.RefreshInterval != 0 {
		fields = append(fields, "refresh="+plan.RefreshInterval.String())
	}
	if plan.RedactedEndpointURL != "" {
		fields = append(fields, "endpointURL="+plan.RedactedEndpointURL)
	}
	if plan.DefaultFlagFallback {
		fields = append(fields, "defaultFallback=true")
	}
	return strings.Join(fields, " ")
}

func validOpenFeatureMode(mode OpenFeatureMode) bool {
	return mode == OpenFeatureModeDefault || mode == OpenFeatureModeOffline
}

func validOpenFeatureEndpointURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func invalidOpenFeatureToken(value string) bool {
	value = strings.TrimSpace(value)
	return value == "" || hasSpaceOrControl(value)
}

func sensitiveOpenFeatureAttribute(name string) bool {
	name = strings.ToLower(strings.TrimSpace(name))
	return strings.Contains(name, "secret") ||
		strings.Contains(name, "token") ||
		strings.Contains(name, "password") ||
		strings.Contains(name, "credential") ||
		strings.Contains(name, "key")
}

func looksLikeURL(value string) bool {
	parsed, err := url.Parse(strings.TrimSpace(value))
	if err != nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") && parsed.Host != ""
}

func sortedOpenFeatureKeys(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func copyOpenFeatureStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	copied := make(map[string]string, len(values))
	for key, value := range values {
		copied[key] = value
	}
	return copied
}

func openFeatureConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrOpenFeatureConfigInvalid, err)
}
