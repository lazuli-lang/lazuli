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
	// PostmarkProviderName is the provider identifier used by email descriptors.
	PostmarkProviderName = "postmark"
	// PostmarkProviderDisplayName is the human-facing provider name.
	PostmarkProviderDisplayName = "Postmark"
	// PostmarkDefaultBaseURL is the default Postmark API host.
	PostmarkDefaultBaseURL = "https://api.postmarkapp.com"
	// PostmarkDefaultMessageStream is Postmark's default outbound stream.
	PostmarkDefaultMessageStream = "outbound"
	// PostmarkServerTokenHeader is the request header used by Postmark adapters.
	PostmarkServerTokenHeader = "X-Postmark-Server-Token"
	// PostmarkIdempotencyMetadataKey is the metadata key Lazuli can attach to messages.
	PostmarkIdempotencyMetadataKey = "lazuli_idempotency_key"
	// PostmarkMaxTagLength bounds a planned Postmark tag.
	PostmarkMaxTagLength = 1000
	// PostmarkMaxMetadataEntries bounds provider-neutral metadata planning.
	PostmarkMaxMetadataEntries = 10
	// PostmarkMaxMetadataKeyLength bounds planned metadata keys.
	PostmarkMaxMetadataKeyLength = 80
	// PostmarkMaxMetadataValueLength bounds planned metadata values.
	PostmarkMaxMetadataValueLength = 1000
	// PostmarkMaxIdempotencyKeyLength bounds retry keys stored in metadata.
	PostmarkMaxIdempotencyKeyLength = PostmarkMaxMetadataValueLength
)

const postmarkHashedIdempotencyKeyPrefix = "email:postmark:sha256:"

var (
	// ErrPostmarkConfigInvalid means Postmark descriptor config is malformed.
	ErrPostmarkConfigInvalid = errors.New("postmark: config invalid")
	// ErrPostmarkServerTokenInvalid means the server token metadata is malformed.
	ErrPostmarkServerTokenInvalid = errors.New("postmark: server token invalid")
	// ErrPostmarkMessageStreamInvalid means the message stream metadata is malformed.
	ErrPostmarkMessageStreamInvalid = errors.New("postmark: message stream invalid")
	// ErrPostmarkBaseURLInvalid means the Postmark API base URL is malformed.
	ErrPostmarkBaseURLInvalid = errors.New("postmark: base url invalid")
	// ErrPostmarkSenderInvalid means the configured sender address is malformed.
	ErrPostmarkSenderInvalid = errors.New("postmark: sender invalid")
	// ErrPostmarkTagInvalid means a message tag is malformed.
	ErrPostmarkTagInvalid = errors.New("postmark: tag invalid")
	// ErrPostmarkMetadataInvalid means planned message metadata is malformed.
	ErrPostmarkMetadataInvalid = errors.New("postmark: metadata invalid")
	// ErrPostmarkIdempotencyKeyMissing means retry metadata cannot be derived.
	ErrPostmarkIdempotencyKeyMissing = errors.New("postmark: idempotency key missing")
	// ErrPostmarkIdempotencyKeyInvalid means retry metadata contains unsafe characters.
	ErrPostmarkIdempotencyKeyInvalid = errors.New("postmark: idempotency key invalid")
)

// PostmarkDescriptor describes Postmark adapter metadata for generated code,
// diagnostics, and deploy adapters. It does not include a client.
type PostmarkDescriptor struct {
	Name                   string
	DisplayName            string
	DefaultBaseURL         string
	DefaultMessageStream   string
	ServerTokenHeader      string
	IdempotencyMetadataKey string
}

// PostmarkProviderDescriptor returns the canonical Postmark provider descriptor.
func PostmarkProviderDescriptor() PostmarkDescriptor {
	return PostmarkDescriptor{
		Name:                   PostmarkProviderName,
		DisplayName:            PostmarkProviderDisplayName,
		DefaultBaseURL:         PostmarkDefaultBaseURL,
		DefaultMessageStream:   PostmarkDefaultMessageStream,
		ServerTokenHeader:      PostmarkServerTokenHeader,
		IdempotencyMetadataKey: PostmarkIdempotencyMetadataKey,
	}
}

// PostmarkConfig is metadata for a Postmark adapter binding.
type PostmarkConfig struct {
	ServerToken   string
	MessageStream string
	APIBaseURL    string
	Sender        string
}

// Validate checks whether config can be used by a future Postmark adapter.
func (c PostmarkConfig) Validate() error {
	_, err := NormalizePostmarkConfig(c)
	return err
}

// NormalizePostmarkConfig trims config, applies defaults, and validates metadata.
// It does not contact Postmark.
func NormalizePostmarkConfig(config PostmarkConfig) (PostmarkConfig, error) {
	var errs []error

	token, err := NormalizePostmarkServerToken(config.ServerToken)
	if err != nil {
		errs = append(errs, postmarkConfigError(err))
	} else {
		config.ServerToken = token
	}

	stream, err := NormalizePostmarkMessageStream(config.MessageStream)
	if err != nil {
		errs = append(errs, postmarkConfigError(err))
	} else {
		config.MessageStream = stream
	}

	baseURL, err := NormalizePostmarkBaseURL(config.APIBaseURL)
	if err != nil {
		errs = append(errs, postmarkConfigError(err))
	} else {
		config.APIBaseURL = baseURL
	}

	sender, err := NormalizePostmarkSender(config.Sender)
	if err != nil {
		errs = append(errs, postmarkConfigError(err))
	} else {
		config.Sender = sender
	}

	if err := errors.Join(errs...); err != nil {
		return PostmarkConfig{}, err
	}
	return config, nil
}

// PostmarkRedactedSummary is safe to log or expose in diagnostics.
type PostmarkRedactedSummary struct {
	Provider      string
	MessageStream string
	APIBaseURL    string
	ServerToken   string
	Sender        string
}

// RedactedSummary returns a deterministic, secret-safe config summary.
func (c PostmarkConfig) RedactedSummary() PostmarkRedactedSummary {
	normalized, err := NormalizePostmarkConfig(c)
	if err == nil {
		c = normalized
	} else {
		c.ServerToken = strings.TrimSpace(c.ServerToken)
		c.MessageStream = strings.TrimSpace(c.MessageStream)
		c.APIBaseURL = RedactPostmarkURL(c.APIBaseURL)
		c.Sender = strings.TrimSpace(c.Sender)
	}
	return PostmarkRedactedSummary{
		Provider:      PostmarkProviderName,
		MessageStream: c.MessageStream,
		APIBaseURL:    RedactPostmarkURL(c.APIBaseURL),
		ServerToken:   RedactPostmarkSecret(c.ServerToken),
		Sender:        redactPostmarkSender(c.Sender),
	}
}

// RedactPostmarkSecret hides a configured secret while preserving empty state.
func RedactPostmarkSecret(secret string) string {
	if strings.TrimSpace(secret) == "" {
		return ""
	}
	return "redacted"
}

// RedactPostmarkURL removes URL credentials when present.
func RedactPostmarkURL(rawURL string) string {
	rawURL = strings.TrimSpace(rawURL)
	parsed, err := url.Parse(rawURL)
	if err != nil || parsed.User == nil {
		return rawURL
	}
	parsed.User = url.User("redacted")
	return parsed.String()
}

// ValidatePostmarkServerToken checks whether token is safe to carry as metadata.
func ValidatePostmarkServerToken(token string) error {
	_, err := NormalizePostmarkServerToken(token)
	return err
}

// NormalizePostmarkServerToken trims and validates a server token.
func NormalizePostmarkServerToken(token string) (string, error) {
	token = strings.TrimSpace(token)
	if token == "" || hasPostmarkSpaceOrControl(token) {
		return "", ErrPostmarkServerTokenInvalid
	}
	return token, nil
}

// ValidatePostmarkMessageStream checks whether stream is safe to carry as metadata.
func ValidatePostmarkMessageStream(stream string) error {
	_, err := NormalizePostmarkMessageStream(stream)
	return err
}

// NormalizePostmarkMessageStream trims stream metadata. Empty defaults to outbound.
func NormalizePostmarkMessageStream(stream string) (string, error) {
	stream = strings.TrimSpace(stream)
	if stream == "" {
		return PostmarkDefaultMessageStream, nil
	}
	if hasPostmarkSpaceOrControl(stream) {
		return "", ErrPostmarkMessageStreamInvalid
	}
	for _, r := range stream {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '-' || r == '_' {
			continue
		}
		return "", ErrPostmarkMessageStreamInvalid
	}
	return stream, nil
}

// ValidatePostmarkBaseURL checks whether baseURL is an absolute http(s) API URL.
func ValidatePostmarkBaseURL(baseURL string) error {
	_, err := NormalizePostmarkBaseURL(baseURL)
	return err
}

// NormalizePostmarkBaseURL trims and validates a Postmark API base URL. Empty
// baseURL resolves to the default host.
func NormalizePostmarkBaseURL(baseURL string) (string, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		baseURL = PostmarkDefaultBaseURL
	}
	if hasPostmarkSpaceOrControl(baseURL) {
		return "", ErrPostmarkBaseURLInvalid
	}
	parsed, err := url.Parse(baseURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrPostmarkBaseURLInvalid, err)
	}
	if !validPostmarkAbsoluteURL(parsed) || parsed.RawQuery != "" {
		return "", ErrPostmarkBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// ValidatePostmarkSender checks whether sender is a single RFC 5322 mailbox.
func ValidatePostmarkSender(sender string) error {
	_, err := NormalizePostmarkSender(sender)
	return err
}

// NormalizePostmarkSender trims and canonicalizes a sender mailbox using net/mail.
func NormalizePostmarkSender(sender string) (string, error) {
	sender = strings.TrimSpace(sender)
	if sender == "" || hasPostmarkControl(sender) {
		return "", ErrPostmarkSenderInvalid
	}
	address, err := netmail.ParseAddress(sender)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrPostmarkSenderInvalid, err)
	}
	if address.Address == "" || strings.Contains(address.Address, " ") {
		return "", ErrPostmarkSenderInvalid
	}
	return address.String(), nil
}

// PostmarkMetadataInput describes provider-neutral message metadata before it is
// mapped to Postmark request fields.
type PostmarkMetadataInput struct {
	Tag            string
	Metadata       map[string]string
	IdempotencyKey string
}

// PostmarkMetadataPlan is the deterministic Postmark metadata plan for a message.
type PostmarkMetadataPlan struct {
	Tag            string
	Metadata       map[string]string
	IdempotencyKey string
}

// RequestFields renders metadata as Postmark request field names without bodies
// or transport details.
func (p PostmarkMetadataPlan) RequestFields() map[string]any {
	fields := make(map[string]any, 2)
	if p.Tag != "" {
		fields["Tag"] = p.Tag
	}
	if len(p.Metadata) > 0 {
		metadata := make(map[string]string, len(p.Metadata))
		names := make([]string, 0, len(p.Metadata))
		for name := range p.Metadata {
			names = append(names, name)
		}
		sort.Strings(names)
		for _, name := range names {
			metadata[name] = p.Metadata[name]
		}
		fields["Metadata"] = metadata
	}
	return fields
}

// PlanPostmarkMetadata normalizes tag, metadata, and retry metadata for a
// future Postmark send call.
func PlanPostmarkMetadata(input PostmarkMetadataInput) (PostmarkMetadataPlan, error) {
	var errs []error

	tag, err := normalizePostmarkTag(input.Tag)
	if err != nil {
		errs = append(errs, err)
	}

	metadata := make(map[string]string, len(input.Metadata)+1)
	for rawName, rawValue := range input.Metadata {
		name, value, err := normalizePostmarkMetadata(rawName, rawValue)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		metadata[name] = value
	}

	idempotencyKey := strings.TrimSpace(input.IdempotencyKey)
	if idempotencyKey != "" {
		normalized, err := NormalizePostmarkIdempotencyKey(idempotencyKey)
		if err != nil {
			errs = append(errs, err)
		} else {
			idempotencyKey = normalized
			metadata[PostmarkIdempotencyMetadataKey] = idempotencyKey
		}
	}
	if len(metadata) > PostmarkMaxMetadataEntries {
		errs = append(errs, ErrPostmarkMetadataInvalid)
	}

	if err := errors.Join(errs...); err != nil {
		return PostmarkMetadataPlan{}, err
	}
	return PostmarkMetadataPlan{
		Tag:            tag,
		Metadata:       metadata,
		IdempotencyKey: idempotencyKey,
	}, nil
}

// PostmarkIdempotencyMetadata describes retry-safe message identity metadata.
type PostmarkIdempotencyMetadata struct {
	Provider    string
	Key         string
	MetadataKey string
}

// PlanPostmarkIdempotencyMetadata normalizes a retry key for logging, planning,
// and optional message metadata attachment.
func PlanPostmarkIdempotencyMetadata(key string) (PostmarkIdempotencyMetadata, error) {
	normalized, err := NormalizePostmarkIdempotencyKey(key)
	if err != nil {
		return PostmarkIdempotencyMetadata{}, err
	}
	return PostmarkIdempotencyMetadata{
		Provider:    PostmarkProviderName,
		Key:         normalized,
		MetadataKey: PostmarkIdempotencyMetadataKey,
	}, nil
}

// NormalizePostmarkIdempotencyKey trims and bounds retry metadata. Oversized keys
// are replaced with a stable SHA-256 key.
func NormalizePostmarkIdempotencyKey(key string) (string, error) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", ErrPostmarkIdempotencyKeyMissing
	}
	if hasPostmarkControl(key) {
		return "", ErrPostmarkIdempotencyKeyInvalid
	}
	if len(key) <= PostmarkMaxIdempotencyKeyLength {
		return key, nil
	}
	sum := sha256.Sum256([]byte(key))
	return postmarkHashedIdempotencyKeyPrefix + hex.EncodeToString(sum[:]), nil
}

func normalizePostmarkTag(tag string) (string, error) {
	tag = strings.TrimSpace(tag)
	if tag == "" {
		return "", nil
	}
	if len(tag) > PostmarkMaxTagLength || hasPostmarkControl(tag) {
		return "", ErrPostmarkTagInvalid
	}
	return tag, nil
}

func normalizePostmarkMetadata(name, value string) (string, string, error) {
	name = strings.TrimSpace(name)
	value = strings.TrimSpace(value)
	if name == "" || len(name) > PostmarkMaxMetadataKeyLength || hasPostmarkControl(name) || strings.Contains(name, ".") {
		return "", "", ErrPostmarkMetadataInvalid
	}
	if len(value) > PostmarkMaxMetadataValueLength || hasPostmarkControl(value) {
		return "", "", ErrPostmarkMetadataInvalid
	}
	return name, value, nil
}

func validPostmarkAbsoluteURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func redactPostmarkSender(sender string) string {
	parsed, err := netmail.ParseAddress(sender)
	if err != nil {
		return sender
	}
	at := strings.LastIndex(parsed.Address, "@")
	if at <= 0 {
		return sender
	}
	redactedAddress := "***@" + strings.ToLower(parsed.Address[at+1:])
	if parsed.Name == "" {
		return redactedAddress
	}
	return (&netmail.Address{Name: parsed.Name, Address: redactedAddress}).String()
}

func hasPostmarkSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasPostmarkControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func postmarkConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrPostmarkConfigInvalid, err)
}
