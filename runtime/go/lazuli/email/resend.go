package email

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	netmail "net/mail"
	"net/url"
	"sort"
	"strings"
	"unicode"
)

const (
	// ResendProviderName is the provider identifier used by email descriptors.
	ResendProviderName = "resend"
	// ResendProviderDisplayName is the human-facing provider name.
	ResendProviderDisplayName = "Resend"
	// ResendDefaultBaseURL is the default Resend API host.
	ResendDefaultBaseURL = "https://api.resend.com"
	// ResendAPIKeyEnv is the conventional environment variable for API keys.
	ResendAPIKeyEnv = "RESEND_API_KEY"
	// ResendIdempotencyHeader is the retry metadata header Lazuli can attach to messages.
	ResendIdempotencyHeader = "Idempotency-Key"
	// ResendMaxIdempotencyKeyLength bounds retry keys that may be mirrored in headers.
	ResendMaxIdempotencyKeyLength = 256
	// ResendMaxTagNameLength bounds message tag names.
	ResendMaxTagNameLength = 256
	// ResendMaxTagValueLength bounds message tag values.
	ResendMaxTagValueLength = 256
)

const resendHashedIdempotencyKeyPrefix = "email:resend:sha256:"

var (
	// ErrResendConfigInvalid means Resend descriptor config is malformed.
	ErrResendConfigInvalid = errors.New("resend: config invalid")
	// ErrResendBaseURLInvalid means the Resend API base URL is malformed.
	ErrResendBaseURLInvalid = errors.New("resend: base url invalid")
	// ErrResendAPIKeyInvalid means the API key has unsafe characters.
	ErrResendAPIKeyInvalid = errors.New("resend: api key invalid")
	// ErrResendSenderInvalid means the configured sender address is malformed.
	ErrResendSenderInvalid = errors.New("resend: sender invalid")
	// ErrResendAudienceInvalid means an audience identifier is malformed.
	ErrResendAudienceInvalid = errors.New("resend: audience invalid")
	// ErrResendTagInvalid means a message tag is malformed.
	ErrResendTagInvalid = errors.New("resend: tag invalid")
	// ErrResendIdempotencyKeyMissing means retry metadata cannot be derived.
	ErrResendIdempotencyKeyMissing = errors.New("resend: idempotency key missing")
	// ErrResendIdempotencyKeyInvalid means retry metadata contains unsafe characters.
	ErrResendIdempotencyKeyInvalid = errors.New("resend: idempotency key invalid")
)

// ResendDescriptor describes Resend adapter metadata for generated code,
// diagnostics, and deploy adapters. It does not include a client.
type ResendDescriptor struct {
	Name              string
	DisplayName       string
	DefaultBaseURL    string
	APIKeyEnv         string
	IdempotencyHeader string
}

// ResendProviderDescriptor returns the canonical Resend provider descriptor.
func ResendProviderDescriptor() ResendDescriptor {
	return ResendDescriptor{
		Name:              ResendProviderName,
		DisplayName:       ResendProviderDisplayName,
		DefaultBaseURL:    ResendDefaultBaseURL,
		APIKeyEnv:         ResendAPIKeyEnv,
		IdempotencyHeader: ResendIdempotencyHeader,
	}
}

// ResendConfig is metadata for a Resend adapter binding.
type ResendConfig struct {
	APIKey     string
	APIBaseURL string
	Sender     string
}

// Validate checks whether config can be used by a future Resend adapter.
func (c ResendConfig) Validate() error {
	_, err := NormalizeResendConfig(c)
	return err
}

// NormalizeResendConfig trims config, applies defaults, and validates metadata.
// It does not contact Resend.
func NormalizeResendConfig(config ResendConfig) (ResendConfig, error) {
	var errs []error

	apiKey, err := NormalizeResendAPIKey(config.APIKey)
	if err != nil {
		errs = append(errs, resendConfigError(err))
	} else {
		config.APIKey = apiKey
	}

	baseURL, err := NormalizeResendBaseURL(config.APIBaseURL)
	if err != nil {
		errs = append(errs, resendConfigError(err))
	} else {
		config.APIBaseURL = baseURL
	}

	sender, err := NormalizeResendSender(config.Sender)
	if err != nil {
		errs = append(errs, resendConfigError(err))
	} else {
		config.Sender = sender
	}

	if err := errors.Join(errs...); err != nil {
		return ResendConfig{}, err
	}
	return config, nil
}

// ResendRedactedSummary is safe to log or expose in diagnostics.
type ResendRedactedSummary struct {
	Provider   string
	APIBaseURL string
	APIKey     string
	Sender     string
}

// RedactedSummary returns a deterministic, secret-safe config summary.
func (c ResendConfig) RedactedSummary() ResendRedactedSummary {
	normalized, err := NormalizeResendConfig(c)
	if err == nil {
		c = normalized
	} else {
		c.APIBaseURL = RedactResendURL(c.APIBaseURL)
		c.APIKey = strings.TrimSpace(c.APIKey)
		c.Sender = strings.TrimSpace(c.Sender)
	}
	return ResendRedactedSummary{
		Provider:   ResendProviderName,
		APIBaseURL: RedactResendURL(c.APIBaseURL),
		APIKey:     RedactResendSecret(c.APIKey),
		Sender:     c.Sender,
	}
}

// RedactResendSecret hides a configured secret while preserving empty state.
func RedactResendSecret(secret string) string {
	if strings.TrimSpace(secret) == "" {
		return ""
	}
	return "redacted"
}

// RedactResendURL removes URL credentials when present.
func RedactResendURL(rawURL string) string {
	rawURL = strings.TrimSpace(rawURL)
	parsed, err := url.Parse(rawURL)
	if err != nil || parsed.User == nil {
		return rawURL
	}
	parsed.User = url.User("redacted")
	return parsed.String()
}

// ValidateResendAPIKey checks whether apiKey has no unsafe characters.
func ValidateResendAPIKey(apiKey string) error {
	_, err := NormalizeResendAPIKey(apiKey)
	return err
}

// NormalizeResendAPIKey trims a Resend API key. Empty keys are allowed so
// adapters can bind credentials from environment metadata.
func NormalizeResendAPIKey(apiKey string) (string, error) {
	apiKey = strings.TrimSpace(apiKey)
	if apiKey != "" && hasResendSpaceOrControl(apiKey) {
		return "", ErrResendAPIKeyInvalid
	}
	return apiKey, nil
}

// ValidateResendBaseURL checks whether baseURL is an absolute http(s) API URL.
func ValidateResendBaseURL(baseURL string) error {
	_, err := NormalizeResendBaseURL(baseURL)
	return err
}

// NormalizeResendBaseURL trims and validates a Resend API base URL. Empty
// baseURL resolves to the default API host.
func NormalizeResendBaseURL(baseURL string) (string, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		baseURL = ResendDefaultBaseURL
	}
	if hasResendSpaceOrControl(baseURL) {
		return "", ErrResendBaseURLInvalid
	}
	parsed, err := url.Parse(baseURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrResendBaseURLInvalid, err)
	}
	if !validResendAbsoluteURL(parsed) || parsed.RawQuery != "" {
		return "", ErrResendBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// ValidateResendSender checks whether sender is a single RFC 5322 mailbox.
func ValidateResendSender(sender string) error {
	_, err := NormalizeResendSender(sender)
	return err
}

// NormalizeResendSender trims and canonicalizes a sender mailbox using net/mail.
func NormalizeResendSender(sender string) (string, error) {
	sender = strings.TrimSpace(sender)
	if sender == "" || hasResendControl(sender) {
		return "", ErrResendSenderInvalid
	}
	address, err := netmail.ParseAddress(sender)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrResendSenderInvalid, err)
	}
	if address.Address == "" || strings.Contains(address.Address, " ") {
		return "", ErrResendSenderInvalid
	}
	return address.String(), nil
}

// ResendMetadataInput describes provider-neutral message metadata before it is
// mapped to Resend request fields.
type ResendMetadataInput struct {
	AudienceID     string
	Tags           []ResendTag
	IdempotencyKey string
}

// ResendTag is provider metadata attached to a planned Resend send.
type ResendTag struct {
	Name  string
	Value string
}

// ResendMetadataPlan is the deterministic Resend metadata plan for a message.
type ResendMetadataPlan struct {
	AudienceID     string
	Tags           []ResendTag
	IdempotencyKey string
}

// RequestFields renders metadata as provider-neutral field names without bodies
// or transport details.
func (p ResendMetadataPlan) RequestFields() map[string][]string {
	fields := make(map[string][]string, len(p.Tags)+2)
	if p.AudienceID != "" {
		fields["audience_id"] = []string{p.AudienceID}
	}
	if p.IdempotencyKey != "" {
		fields[ResendIdempotencyHeader] = []string{p.IdempotencyKey}
	}
	for _, tag := range p.Tags {
		fields["tags."+tag.Name] = append(fields["tags."+tag.Name], tag.Value)
	}
	return fields
}

// PlanResendMetadata normalizes audience, tags, and retry metadata for a future
// Resend send call.
func PlanResendMetadata(input ResendMetadataInput) (ResendMetadataPlan, error) {
	var errs []error

	audienceID, err := NormalizeResendAudienceID(input.AudienceID)
	if err != nil && strings.TrimSpace(input.AudienceID) != "" {
		errs = append(errs, err)
	} else if err == nil {
		audienceID = strings.TrimSpace(audienceID)
	}

	tags := make([]ResendTag, 0, len(input.Tags))
	seenTags := make(map[string]int, len(input.Tags))
	for _, raw := range input.Tags {
		tag, err := NormalizeResendTag(raw)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		key := tag.Name + "\x00" + tag.Value
		if _, ok := seenTags[key]; ok {
			continue
		}
		seenTags[key] = len(tags)
		tags = append(tags, tag)
	}
	sort.SliceStable(tags, func(i, j int) bool {
		if tags[i].Name == tags[j].Name {
			return tags[i].Value < tags[j].Value
		}
		return tags[i].Name < tags[j].Name
	})

	idempotencyKey := strings.TrimSpace(input.IdempotencyKey)
	if idempotencyKey != "" {
		normalized, err := NormalizeResendIdempotencyKey(idempotencyKey)
		if err != nil {
			errs = append(errs, err)
		} else {
			idempotencyKey = normalized
		}
	}

	if err := errors.Join(errs...); err != nil {
		return ResendMetadataPlan{}, err
	}
	return ResendMetadataPlan{
		AudienceID:     audienceID,
		Tags:           tags,
		IdempotencyKey: idempotencyKey,
	}, nil
}

// NormalizeResendAudienceID trims a Resend audience identifier.
func NormalizeResendAudienceID(audienceID string) (string, error) {
	audienceID = strings.TrimSpace(audienceID)
	if audienceID == "" {
		return "", nil
	}
	if hasResendSpaceOrControl(audienceID) {
		return "", ErrResendAudienceInvalid
	}
	return audienceID, nil
}

// ValidateResendAudienceID checks whether audienceID can be used as metadata.
func ValidateResendAudienceID(audienceID string) error {
	_, err := NormalizeResendAudienceID(audienceID)
	return err
}

// Normalize returns a canonical tag copy.
func (t ResendTag) Normalize() ResendTag {
	normalized, err := NormalizeResendTag(t)
	if err != nil {
		t.Name = strings.TrimSpace(t.Name)
		t.Value = strings.TrimSpace(t.Value)
		return t
	}
	return normalized
}

// NormalizeResendTag trims and validates a Resend tag.
func NormalizeResendTag(tag ResendTag) (ResendTag, error) {
	tag.Name = strings.TrimSpace(tag.Name)
	tag.Value = strings.TrimSpace(tag.Value)
	if tag.Name == "" || len(tag.Name) > ResendMaxTagNameLength || hasResendControl(tag.Name) {
		return ResendTag{}, ErrResendTagInvalid
	}
	if tag.Value == "" || len(tag.Value) > ResendMaxTagValueLength || hasResendControl(tag.Value) {
		return ResendTag{}, ErrResendTagInvalid
	}
	return tag, nil
}

// ValidateResendTag checks whether tag can be used as message metadata.
func ValidateResendTag(tag ResendTag) error {
	_, err := NormalizeResendTag(tag)
	return err
}

// ResendIdempotencyMetadata describes retry-safe message identity metadata.
type ResendIdempotencyMetadata struct {
	Provider string
	Key      string
	Header   string
}

// PlanResendIdempotencyMetadata normalizes a retry key for logging, planning,
// and optional message header attachment.
func PlanResendIdempotencyMetadata(key string) (ResendIdempotencyMetadata, error) {
	normalized, err := NormalizeResendIdempotencyKey(key)
	if err != nil {
		return ResendIdempotencyMetadata{}, err
	}
	return ResendIdempotencyMetadata{
		Provider: ResendProviderName,
		Key:      normalized,
		Header:   ResendIdempotencyHeader,
	}, nil
}

// NormalizeResendIdempotencyKey trims and bounds retry metadata. Oversized keys
// are replaced with a stable SHA-256 key.
func NormalizeResendIdempotencyKey(key string) (string, error) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", ErrResendIdempotencyKeyMissing
	}
	if hasResendControl(key) {
		return "", ErrResendIdempotencyKeyInvalid
	}
	if len(key) <= ResendMaxIdempotencyKeyLength {
		return key, nil
	}
	sum := sha256.Sum256([]byte(key))
	return resendHashedIdempotencyKeyPrefix + hex.EncodeToString(sum[:]), nil
}

func validResendAbsoluteURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func hasResendSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasResendControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func resendConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrResendConfigInvalid, err)
}
