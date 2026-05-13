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
	// MailgunProviderName is the provider identifier used by email descriptors.
	MailgunProviderName = "mailgun"
	// MailgunProviderDisplayName is the human-facing provider name.
	MailgunProviderDisplayName = "Mailgun"
	// MailgunDefaultUSBaseURL is the default Mailgun US API host.
	MailgunDefaultUSBaseURL = "https://api.mailgun.net"
	// MailgunDefaultEUBaseURL is the default Mailgun EU API host.
	MailgunDefaultEUBaseURL = "https://api.eu.mailgun.net"
	// MailgunIdempotencyHeader is the retry metadata header Lazuli can attach to messages.
	MailgunIdempotencyHeader = "X-Lazuli-Idempotency-Key"
	// MailgunMaxIdempotencyKeyLength bounds retry keys that may be mirrored in headers.
	MailgunMaxIdempotencyKeyLength = 255
	// MailgunMaxTagLength bounds individual message tags.
	MailgunMaxTagLength = 128
)

const mailgunHashedIdempotencyKeyPrefix = "email:mailgun:sha256:"

var (
	// ErrMailgunConfigInvalid means Mailgun descriptor config is malformed.
	ErrMailgunConfigInvalid = errors.New("mailgun: config invalid")
	// ErrMailgunDomainInvalid means the sending domain is malformed.
	ErrMailgunDomainInvalid = errors.New("mailgun: domain invalid")
	// ErrMailgunRegionInvalid means the Mailgun API region is unknown.
	ErrMailgunRegionInvalid = errors.New("mailgun: region invalid")
	// ErrMailgunBaseURLInvalid means the Mailgun API base URL is malformed.
	ErrMailgunBaseURLInvalid = errors.New("mailgun: base url invalid")
	// ErrMailgunSenderInvalid means the configured sender address is malformed.
	ErrMailgunSenderInvalid = errors.New("mailgun: sender invalid")
	// ErrMailgunAPIKeyInvalid means the API key has unsafe characters.
	ErrMailgunAPIKeyInvalid = errors.New("mailgun: api key invalid")
	// ErrMailgunTagInvalid means a message tag is malformed.
	ErrMailgunTagInvalid = errors.New("mailgun: tag invalid")
	// ErrMailgunHeaderInvalid means a planned message header is malformed.
	ErrMailgunHeaderInvalid = errors.New("mailgun: header invalid")
	// ErrMailgunIdempotencyKeyMissing means retry metadata cannot be derived.
	ErrMailgunIdempotencyKeyMissing = errors.New("mailgun: idempotency key missing")
	// ErrMailgunIdempotencyKeyInvalid means retry metadata contains unsafe characters.
	ErrMailgunIdempotencyKeyInvalid = errors.New("mailgun: idempotency key invalid")
)

// MailgunRegion is the Mailgun API region for a sending domain.
type MailgunRegion string

const (
	MailgunRegionUS MailgunRegion = "us"
	MailgunRegionEU MailgunRegion = "eu"
)

// MailgunDescriptor describes Mailgun adapter metadata for generated code,
// diagnostics, and deploy adapters. It does not include a client.
type MailgunDescriptor struct {
	Name              string
	DisplayName       string
	DefaultRegion     MailgunRegion
	DefaultUSBaseURL  string
	DefaultEUBaseURL  string
	IdempotencyHeader string
}

// MailgunProviderDescriptor returns the canonical Mailgun provider descriptor.
func MailgunProviderDescriptor() MailgunDescriptor {
	return MailgunDescriptor{
		Name:              MailgunProviderName,
		DisplayName:       MailgunProviderDisplayName,
		DefaultRegion:     MailgunRegionUS,
		DefaultUSBaseURL:  MailgunDefaultUSBaseURL,
		DefaultEUBaseURL:  MailgunDefaultEUBaseURL,
		IdempotencyHeader: MailgunIdempotencyHeader,
	}
}

// MailgunConfig is metadata for a Mailgun adapter binding.
type MailgunConfig struct {
	Domain     string
	Region     MailgunRegion
	APIBaseURL string
	APIKey     string
	Sender     string
}

// Validate checks whether config can be used by a future Mailgun adapter.
func (c MailgunConfig) Validate() error {
	_, err := NormalizeMailgunConfig(c)
	return err
}

// NormalizeMailgunConfig trims config, applies defaults, and validates metadata.
// It does not contact Mailgun.
func NormalizeMailgunConfig(config MailgunConfig) (MailgunConfig, error) {
	var errs []error

	domain, err := NormalizeMailgunDomain(config.Domain)
	if err != nil {
		errs = append(errs, mailgunConfigError(err))
	} else {
		config.Domain = domain
	}

	region, err := NormalizeMailgunRegion(string(config.Region))
	if err != nil {
		errs = append(errs, mailgunConfigError(err))
	} else {
		config.Region = region
	}

	baseURL, err := NormalizeMailgunBaseURL(config.APIBaseURL, config.Region)
	if err != nil {
		errs = append(errs, mailgunConfigError(err))
	} else {
		config.APIBaseURL = baseURL
	}

	config.APIKey = strings.TrimSpace(config.APIKey)
	if config.APIKey != "" && hasMailgunSpaceOrControl(config.APIKey) {
		errs = append(errs, mailgunConfigError(ErrMailgunAPIKeyInvalid))
	}

	sender, err := NormalizeMailgunSender(config.Sender)
	if err != nil {
		errs = append(errs, mailgunConfigError(err))
	} else {
		config.Sender = sender
	}

	if err := errors.Join(errs...); err != nil {
		return MailgunConfig{}, err
	}
	return config, nil
}

// MailgunRedactedSummary is safe to log or expose in diagnostics.
type MailgunRedactedSummary struct {
	Provider   string
	Domain     string
	Region     MailgunRegion
	APIBaseURL string
	APIKey     string
	Sender     string
}

// RedactedSummary returns a deterministic, secret-safe config summary.
func (c MailgunConfig) RedactedSummary() MailgunRedactedSummary {
	normalized, err := NormalizeMailgunConfig(c)
	if err == nil {
		c = normalized
	} else {
		c.Domain = strings.TrimSpace(c.Domain)
		c.APIBaseURL = RedactMailgunURL(c.APIBaseURL)
		c.APIKey = strings.TrimSpace(c.APIKey)
		c.Sender = strings.TrimSpace(c.Sender)
	}
	return MailgunRedactedSummary{
		Provider:   MailgunProviderName,
		Domain:     c.Domain,
		Region:     c.Region,
		APIBaseURL: RedactMailgunURL(c.APIBaseURL),
		APIKey:     RedactMailgunSecret(c.APIKey),
		Sender:     c.Sender,
	}
}

// RedactMailgunSecret hides a configured secret while preserving empty state.
func RedactMailgunSecret(secret string) string {
	if strings.TrimSpace(secret) == "" {
		return ""
	}
	return "redacted"
}

// RedactMailgunURL removes URL credentials when present.
func RedactMailgunURL(rawURL string) string {
	rawURL = strings.TrimSpace(rawURL)
	parsed, err := url.Parse(rawURL)
	if err != nil || parsed.User == nil {
		return rawURL
	}
	parsed.User = url.User("redacted")
	return parsed.String()
}

// ValidateMailgunDomain checks whether domain is a usable provider domain.
func ValidateMailgunDomain(domain string) error {
	_, err := NormalizeMailgunDomain(domain)
	return err
}

// NormalizeMailgunDomain trims and lowercases a DNS sending domain.
func NormalizeMailgunDomain(domain string) (string, error) {
	domain = strings.TrimSuffix(strings.TrimSpace(domain), ".")
	domain = strings.ToLower(domain)
	if domain == "" || len(domain) > 253 || strings.Contains(domain, "..") {
		return "", ErrMailgunDomainInvalid
	}
	labels := strings.Split(domain, ".")
	if len(labels) < 2 {
		return "", ErrMailgunDomainInvalid
	}
	for _, label := range labels {
		if !validMailgunDomainLabel(label) {
			return "", ErrMailgunDomainInvalid
		}
	}
	return domain, nil
}

// ValidateMailgunRegion checks whether region is known.
func ValidateMailgunRegion(region string) error {
	_, err := NormalizeMailgunRegion(region)
	return err
}

// NormalizeMailgunRegion trims and lowercases a Mailgun region. Empty defaults to US.
func NormalizeMailgunRegion(region string) (MailgunRegion, error) {
	switch MailgunRegion(strings.ToLower(strings.TrimSpace(region))) {
	case "", MailgunRegionUS:
		return MailgunRegionUS, nil
	case MailgunRegionEU:
		return MailgunRegionEU, nil
	default:
		return "", ErrMailgunRegionInvalid
	}
}

// ValidateMailgunBaseURL checks whether baseURL is an absolute http(s) API URL.
func ValidateMailgunBaseURL(baseURL string, region MailgunRegion) error {
	_, err := NormalizeMailgunBaseURL(baseURL, region)
	return err
}

// NormalizeMailgunBaseURL trims and validates a Mailgun API base URL. Empty
// baseURL resolves from the normalized region.
func NormalizeMailgunBaseURL(baseURL string, region MailgunRegion) (string, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		normalizedRegion, err := NormalizeMailgunRegion(string(region))
		if err != nil {
			return "", err
		}
		if normalizedRegion == MailgunRegionEU {
			baseURL = MailgunDefaultEUBaseURL
		} else {
			baseURL = MailgunDefaultUSBaseURL
		}
	}
	if hasMailgunSpaceOrControl(baseURL) {
		return "", ErrMailgunBaseURLInvalid
	}
	parsed, err := url.Parse(baseURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrMailgunBaseURLInvalid, err)
	}
	if !validMailgunAbsoluteURL(parsed) || parsed.RawQuery != "" {
		return "", ErrMailgunBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// ValidateMailgunSender checks whether sender is a single RFC 5322 mailbox.
func ValidateMailgunSender(sender string) error {
	_, err := NormalizeMailgunSender(sender)
	return err
}

// NormalizeMailgunSender trims and canonicalizes a sender mailbox using net/mail.
func NormalizeMailgunSender(sender string) (string, error) {
	sender = strings.TrimSpace(sender)
	if sender == "" || hasMailgunControl(sender) {
		return "", ErrMailgunSenderInvalid
	}
	address, err := netmail.ParseAddress(sender)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrMailgunSenderInvalid, err)
	}
	if address.Address == "" || strings.Contains(address.Address, " ") {
		return "", ErrMailgunSenderInvalid
	}
	return address.String(), nil
}

// MailgunMetadataInput describes provider-neutral message metadata before it is
// mapped to Mailgun request fields.
type MailgunMetadataInput struct {
	Tags           []string
	Headers        map[string]string
	IdempotencyKey string
}

// MailgunMetadataPlan is the deterministic Mailgun metadata plan for a message.
type MailgunMetadataPlan struct {
	Tags           []string
	Headers        map[string]string
	IdempotencyKey string
}

// RequestFields renders metadata as Mailgun form field names without bodies or
// transport details.
func (p MailgunMetadataPlan) RequestFields() map[string][]string {
	fields := make(map[string][]string, len(p.Tags)+len(p.Headers))
	for _, tag := range p.Tags {
		fields["o:tag"] = append(fields["o:tag"], tag)
	}
	headerNames := make([]string, 0, len(p.Headers))
	for name := range p.Headers {
		headerNames = append(headerNames, name)
	}
	sort.Strings(headerNames)
	for _, name := range headerNames {
		fields["h:"+name] = []string{p.Headers[name]}
	}
	return fields
}

// PlanMailgunMetadata normalizes tags, headers, and retry metadata for a future
// Mailgun send call.
func PlanMailgunMetadata(input MailgunMetadataInput) (MailgunMetadataPlan, error) {
	var errs []error
	tags := make([]string, 0, len(input.Tags))
	seenTags := make(map[string]struct{}, len(input.Tags))
	for _, raw := range input.Tags {
		tag, err := normalizeMailgunTag(raw)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if _, ok := seenTags[tag]; ok {
			continue
		}
		seenTags[tag] = struct{}{}
		tags = append(tags, tag)
	}

	headers := make(map[string]string, len(input.Headers)+1)
	for rawName, rawValue := range input.Headers {
		name, value, err := normalizeMailgunHeader(rawName, rawValue)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		headers[name] = value
	}

	idempotencyKey := strings.TrimSpace(input.IdempotencyKey)
	if idempotencyKey != "" {
		normalized, err := NormalizeMailgunIdempotencyKey(idempotencyKey)
		if err != nil {
			errs = append(errs, err)
		} else {
			idempotencyKey = normalized
			headers[MailgunIdempotencyHeader] = idempotencyKey
		}
	}

	if err := errors.Join(errs...); err != nil {
		return MailgunMetadataPlan{}, err
	}
	return MailgunMetadataPlan{
		Tags:           tags,
		Headers:        headers,
		IdempotencyKey: idempotencyKey,
	}, nil
}

// MailgunIdempotencyMetadata describes retry-safe message identity metadata.
type MailgunIdempotencyMetadata struct {
	Provider string
	Key      string
	Header   string
}

// PlanMailgunIdempotencyMetadata normalizes a retry key for logging, planning,
// and optional message header attachment.
func PlanMailgunIdempotencyMetadata(key string) (MailgunIdempotencyMetadata, error) {
	normalized, err := NormalizeMailgunIdempotencyKey(key)
	if err != nil {
		return MailgunIdempotencyMetadata{}, err
	}
	return MailgunIdempotencyMetadata{
		Provider: MailgunProviderName,
		Key:      normalized,
		Header:   MailgunIdempotencyHeader,
	}, nil
}

// NormalizeMailgunIdempotencyKey trims and bounds retry metadata. Oversized keys
// are replaced with a stable SHA-256 key.
func NormalizeMailgunIdempotencyKey(key string) (string, error) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", ErrMailgunIdempotencyKeyMissing
	}
	if hasMailgunControl(key) {
		return "", ErrMailgunIdempotencyKeyInvalid
	}
	if len(key) <= MailgunMaxIdempotencyKeyLength {
		return key, nil
	}
	sum := sha256.Sum256([]byte(key))
	return mailgunHashedIdempotencyKeyPrefix + hex.EncodeToString(sum[:]), nil
}

func normalizeMailgunTag(tag string) (string, error) {
	tag = strings.TrimSpace(tag)
	if tag == "" || len(tag) > MailgunMaxTagLength || hasMailgunControl(tag) {
		return "", ErrMailgunTagInvalid
	}
	return tag, nil
}

func normalizeMailgunHeader(name, value string) (string, string, error) {
	name = strings.TrimSpace(name)
	value = strings.TrimSpace(value)
	if !validMailgunHeaderName(name) || hasMailgunControl(value) {
		return "", "", ErrMailgunHeaderInvalid
	}
	return name, value, nil
}

func validMailgunDomainLabel(label string) bool {
	if label == "" || len(label) > 63 || strings.HasPrefix(label, "-") || strings.HasSuffix(label, "-") {
		return false
	}
	for _, r := range label {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') || r == '-' {
			continue
		}
		return false
	}
	return true
}

func validMailgunAbsoluteURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func validMailgunHeaderName(name string) bool {
	if name == "" {
		return false
	}
	for _, r := range name {
		if r > 127 || unicode.IsControl(r) || unicode.IsSpace(r) {
			return false
		}
		switch r {
		case '(', ')', '<', '>', '@', ',', ';', ':', '\\', '"', '/', '[', ']', '?', '=', '{', '}':
			return false
		}
	}
	return true
}

func hasMailgunSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasMailgunControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func mailgunConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrMailgunConfigInvalid, err)
}
