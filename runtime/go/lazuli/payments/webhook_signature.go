package payments

import (
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
)

const (
	// DefaultWebhookSignatureTolerance is the default clock skew allowed for
	// timestamped payment webhook signatures.
	DefaultWebhookSignatureTolerance = 5 * time.Minute
	// MinWebhookSignatureTolerance is the smallest non-zero timestamp tolerance.
	MinWebhookSignatureTolerance = time.Second
	// MaxWebhookSignatureTolerance is the largest accepted timestamp tolerance.
	MaxWebhookSignatureTolerance = 24 * time.Hour
)

var (
	ErrWebhookSignaturePlanInvalid        = errors.New("payments: webhook signature plan invalid")
	ErrWebhookSignatureHeaderInvalid      = errors.New("payments: webhook signature header invalid")
	ErrWebhookSignatureAlgorithmInvalid   = errors.New("payments: webhook signature algorithm invalid")
	ErrWebhookSignatureSecretMissing      = errors.New("payments: webhook signature secret missing")
	ErrWebhookSignatureToleranceInvalid   = errors.New("payments: webhook signature tolerance invalid")
	ErrWebhookSignatureCanonicalPayload   = errors.New("payments: webhook signature canonical payload invalid")
	ErrWebhookSignaturePayloadUnsupported = errors.New("payments: webhook signature payload unsupported")
)

// WebhookSignatureAlgorithm names a provider-neutral signature algorithm.
type WebhookSignatureAlgorithm string

const (
	WebhookSignatureAlgorithmHMACSHA1   WebhookSignatureAlgorithm = "hmac-sha1"
	WebhookSignatureAlgorithmHMACSHA256 WebhookSignatureAlgorithm = "hmac-sha256"
	WebhookSignatureAlgorithmHMACSHA512 WebhookSignatureAlgorithm = "hmac-sha512"
	WebhookSignatureAlgorithmEd25519    WebhookSignatureAlgorithm = "ed25519"
)

// WebhookSignatureAlgorithmMetadata describes algorithm properties without
// implementing provider crypto verification.
type WebhookSignatureAlgorithmMetadata struct {
	Algorithm  WebhookSignatureAlgorithm
	Family     string
	Hash       string
	Keyed      bool
	Asymmetric bool
}

// NormalizeWebhookSignatureAlgorithm trims and canonicalizes an algorithm name.
func NormalizeWebhookSignatureAlgorithm(algorithm string) (WebhookSignatureAlgorithm, error) {
	normalized := WebhookSignatureAlgorithm(strings.ToLower(strings.ReplaceAll(strings.TrimSpace(algorithm), "_", "-")))
	if _, ok := normalized.Metadata(); !ok {
		return "", fmt.Errorf("%w: %q", ErrWebhookSignatureAlgorithmInvalid, algorithm)
	}
	return normalized, nil
}

// Metadata returns stable metadata for a supported algorithm.
func (a WebhookSignatureAlgorithm) Metadata() (WebhookSignatureAlgorithmMetadata, bool) {
	switch a {
	case WebhookSignatureAlgorithmHMACSHA1:
		return WebhookSignatureAlgorithmMetadata{Algorithm: a, Family: "hmac", Hash: "sha1", Keyed: true}, true
	case WebhookSignatureAlgorithmHMACSHA256:
		return WebhookSignatureAlgorithmMetadata{Algorithm: a, Family: "hmac", Hash: "sha256", Keyed: true}, true
	case WebhookSignatureAlgorithmHMACSHA512:
		return WebhookSignatureAlgorithmMetadata{Algorithm: a, Family: "hmac", Hash: "sha512", Keyed: true}, true
	case WebhookSignatureAlgorithmEd25519:
		return WebhookSignatureAlgorithmMetadata{Algorithm: a, Family: "ed25519", Keyed: false, Asymmetric: true}, true
	default:
		return WebhookSignatureAlgorithmMetadata{}, false
	}
}

// Valid reports whether algorithm is supported by the planning helpers.
func (a WebhookSignatureAlgorithm) Valid() bool {
	_, ok := a.Metadata()
	return ok
}

// WebhookSignaturePayloadBody identifies the body material used in a canonical
// provider payload.
type WebhookSignaturePayloadBody string

const (
	// WebhookSignaturePayloadRawBody means the exact raw request body bytes.
	WebhookSignaturePayloadRawBody WebhookSignaturePayloadBody = "raw_body"
)

// WebhookSignatureCanonicalPayload describes the inputs a future verifier will
// use to construct provider signed bytes. It is metadata only.
type WebhookSignatureCanonicalPayload struct {
	Body               WebhookSignaturePayloadBody
	TimestampHeader    string
	TimestampSeparator string
	SignedHeaders      []string
}

// WebhookSignatureValidationOptions declares a provider-neutral signature
// validation plan.
type WebhookSignatureValidationOptions struct {
	Provider          string
	SignatureHeader   string
	TimestampHeader   string
	Algorithm         WebhookSignatureAlgorithm
	Tolerance         time.Duration
	Secret            string
	CanonicalPayload  WebhookSignatureCanonicalPayload
	EndpointURL       string
	ProviderEventHint string
}

// WebhookSignatureValidationPlan is the normalized, log-safe plan returned by
// PlanWebhookSignatureValidation.
type WebhookSignatureValidationPlan struct {
	Provider          string
	SignatureHeader   string
	TimestampHeader   string
	Algorithm         WebhookSignatureAlgorithm
	AlgorithmMetadata WebhookSignatureAlgorithmMetadata
	Tolerance         time.Duration
	Secret            string
	EndpointURL       string
	CanonicalPayload  WebhookSignatureCanonicalPayload
	ProviderEventHint string
}

// NormalizeWebhookSignatureHeaderName trims and canonicalizes an HTTP header
// field name used by webhook signature metadata.
func NormalizeWebhookSignatureHeaderName(header string) (string, error) {
	header = strings.TrimSpace(header)
	if header == "" || !validHTTPHeaderName(header) {
		return "", fmt.Errorf("%w: %q", ErrWebhookSignatureHeaderInvalid, header)
	}
	return http.CanonicalHeaderKey(strings.ToLower(header)), nil
}

// NormalizeWebhookSignatureTolerance applies the default tolerance and enforces
// deterministic timestamp skew bounds.
func NormalizeWebhookSignatureTolerance(tolerance time.Duration) (time.Duration, error) {
	if tolerance == 0 {
		return DefaultWebhookSignatureTolerance, nil
	}
	if tolerance < MinWebhookSignatureTolerance || tolerance > MaxWebhookSignatureTolerance {
		return 0, fmt.Errorf("%w: %s", ErrWebhookSignatureToleranceInvalid, tolerance)
	}
	return tolerance, nil
}

// RedactWebhookSecret returns a stable log-safe representation of a webhook
// secret. Empty input stays empty so missing values remain visible.
func RedactWebhookSecret(secret string) string {
	secret = strings.TrimSpace(secret)
	if secret == "" {
		return ""
	}
	if len(secret) <= 8 {
		return "[redacted]"
	}
	return secret[:4] + "...[redacted]..." + secret[len(secret)-4:]
}

// RedactWebhookURL removes userinfo and sensitive query values from a webhook
// URL or route used in planning diagnostics.
func RedactWebhookURL(rawURL string) string {
	rawURL = strings.TrimSpace(rawURL)
	if rawURL == "" {
		return ""
	}

	parsed, err := url.Parse(rawURL)
	if err != nil {
		return "[redacted]"
	}
	parsed.User = nil

	query := parsed.Query()
	for key, values := range query {
		if sensitiveWebhookURLKey(key) {
			for i := range values {
				values[i] = "[redacted]"
			}
			query[key] = values
		}
	}
	parsed.RawQuery = query.Encode()
	return parsed.String()
}

// ValidateWebhookSignaturePlan validates options without returning normalized
// metadata.
func ValidateWebhookSignaturePlan(options WebhookSignatureValidationOptions) error {
	_, err := PlanWebhookSignatureValidation(options)
	return err
}

// PlanWebhookSignatureValidation normalizes provider-neutral signature metadata
// for a future verifier. It does not perform crypto verification.
func PlanWebhookSignatureValidation(options WebhookSignatureValidationOptions) (WebhookSignatureValidationPlan, error) {
	provider := strings.TrimSpace(options.Provider)
	signatureHeader, signatureErr := NormalizeWebhookSignatureHeaderName(options.SignatureHeader)
	timestampHeader, timestampErr := normalizeOptionalWebhookHeaderName(options.TimestampHeader)

	algorithm, algorithmErr := NormalizeWebhookSignatureAlgorithm(string(options.Algorithm))
	algorithmMetadata, algorithmOK := algorithm.Metadata()

	tolerance, toleranceErr := NormalizeWebhookSignatureTolerance(options.Tolerance)
	canonicalPayload, canonicalErr := normalizeWebhookCanonicalPayload(options.CanonicalPayload, timestampHeader)
	secret := strings.TrimSpace(options.Secret)

	var errs []error
	if signatureErr != nil {
		errs = append(errs, planError(signatureErr))
	}
	if timestampErr != nil {
		errs = append(errs, planError(timestampErr))
	}
	if algorithmErr != nil {
		errs = append(errs, planError(algorithmErr))
	} else if !algorithmOK {
		errs = append(errs, planError(fmt.Errorf("%w: %q", ErrWebhookSignatureAlgorithmInvalid, algorithm)))
	}
	if toleranceErr != nil {
		errs = append(errs, planError(toleranceErr))
	}
	if canonicalErr != nil {
		errs = append(errs, planError(canonicalErr))
	}
	if secret == "" {
		errs = append(errs, planError(ErrWebhookSignatureSecretMissing))
	}
	if err := errors.Join(errs...); err != nil {
		return WebhookSignatureValidationPlan{}, err
	}

	return WebhookSignatureValidationPlan{
		Provider:          provider,
		SignatureHeader:   signatureHeader,
		TimestampHeader:   timestampHeader,
		Algorithm:         algorithm,
		AlgorithmMetadata: algorithmMetadata,
		Tolerance:         tolerance,
		Secret:            RedactWebhookSecret(secret),
		EndpointURL:       RedactWebhookURL(options.EndpointURL),
		CanonicalPayload:  canonicalPayload,
		ProviderEventHint: strings.TrimSpace(options.ProviderEventHint),
	}, nil
}

func normalizeWebhookCanonicalPayload(payload WebhookSignatureCanonicalPayload, defaultTimestampHeader string) (WebhookSignatureCanonicalPayload, error) {
	if payload.Body == "" {
		payload.Body = WebhookSignaturePayloadRawBody
	}
	if payload.Body != WebhookSignaturePayloadRawBody {
		return WebhookSignatureCanonicalPayload{}, fmt.Errorf("%w: %w: %q", ErrWebhookSignatureCanonicalPayload, ErrWebhookSignaturePayloadUnsupported, payload.Body)
	}

	var err error
	payload.TimestampHeader, err = normalizeOptionalWebhookHeaderName(webhookFirstNonEmpty(payload.TimestampHeader, defaultTimestampHeader))
	if err != nil {
		return WebhookSignatureCanonicalPayload{}, fmt.Errorf("%w: %w", ErrWebhookSignatureCanonicalPayload, err)
	}
	payload.TimestampSeparator = strings.TrimSpace(payload.TimestampSeparator)

	headers := make([]string, 0, len(payload.SignedHeaders))
	seen := make(map[string]struct{}, len(payload.SignedHeaders))
	for _, header := range payload.SignedHeaders {
		normalized, err := NormalizeWebhookSignatureHeaderName(header)
		if err != nil {
			return WebhookSignatureCanonicalPayload{}, fmt.Errorf("%w: %w", ErrWebhookSignatureCanonicalPayload, err)
		}
		key := strings.ToLower(normalized)
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		headers = append(headers, normalized)
	}
	sort.Strings(headers)
	payload.SignedHeaders = headers
	return payload, nil
}

func normalizeOptionalWebhookHeaderName(header string) (string, error) {
	header = strings.TrimSpace(header)
	if header == "" {
		return "", nil
	}
	return NormalizeWebhookSignatureHeaderName(header)
}

func validHTTPHeaderName(header string) bool {
	for _, r := range header {
		if r > unicode.MaxASCII || !validHTTPHeaderTokenRune(byte(r)) {
			return false
		}
	}
	return true
}

func validHTTPHeaderTokenRune(b byte) bool {
	switch {
	case b >= 'a' && b <= 'z':
		return true
	case b >= 'A' && b <= 'Z':
		return true
	case b >= '0' && b <= '9':
		return true
	default:
		return strings.ContainsRune("!#$%&'*+-.^_`|~", rune(b))
	}
}

func sensitiveWebhookURLKey(key string) bool {
	key = strings.ToLower(key)
	for _, part := range []string{"secret", "signature", "token", "key", "password", "credential", "auth"} {
		if strings.Contains(key, part) {
			return true
		}
	}
	return false
}

func webhookFirstNonEmpty(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}

func planError(err error) error {
	return fmt.Errorf("%w: %w", ErrWebhookSignaturePlanInvalid, err)
}
