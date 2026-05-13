package payments

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"strings"
	"unicode"
)

const (
	// PagarMeProviderName is the provider identifier used by normalized payment records.
	PagarMeProviderName = "pagarme"
	// PagarMeProviderDisplayName is the human-facing provider name.
	PagarMeProviderDisplayName = "Pagar.me"
	// PagarMeDefaultBaseURL is the Pagar.me REST API host and version root.
	PagarMeDefaultBaseURL = "https://api.pagar.me/core/v5"
	// PagarMeHeaderIdempotencyKey is the request header used for idempotency claims.
	PagarMeHeaderIdempotencyKey = "Idempotency-Key"
	// PagarMeMaxIdempotencyKeyLength bounds request idempotency keys.
	PagarMeMaxIdempotencyKeyLength = 255
	// PagarMeMaxInstallments is the conservative provider descriptor limit.
	PagarMeMaxInstallments = 24
)

const pagarMeHashedIdempotencyKeyPrefix = "payments:pagarme:sha256:"

var (
	// ErrPagarMeConfigInvalid means Pagar.me descriptor config is malformed.
	ErrPagarMeConfigInvalid = errors.New("pagarme: config invalid")
	// ErrPagarMeAPIKeyMissing means API request metadata lacks an API key.
	ErrPagarMeAPIKeyMissing = errors.New("pagarme: api key missing")
	// ErrPagarMeAPIKeyInvalid means the configured API key has unsafe characters.
	ErrPagarMeAPIKeyInvalid = errors.New("pagarme: api key invalid")
	// ErrPagarMeAccountInvalid means account metadata has unsafe characters.
	ErrPagarMeAccountInvalid = errors.New("pagarme: account metadata invalid")
	// ErrPagarMeEnvironmentInvalid means the environment token is unknown.
	ErrPagarMeEnvironmentInvalid = errors.New("pagarme: environment invalid")
	// ErrPagarMeBaseURLInvalid means the provider base URL is malformed.
	ErrPagarMeBaseURLInvalid = errors.New("pagarme: base url invalid")
	// ErrPagarMePaymentMetadataInvalid means payment planning metadata is malformed.
	ErrPagarMePaymentMetadataInvalid = errors.New("pagarme: payment metadata invalid")
	// ErrPagarMeIdempotencyKeyMissing means an idempotency key cannot be derived.
	ErrPagarMeIdempotencyKeyMissing = errors.New("pagarme: idempotency key missing")
	// ErrPagarMeIdempotencyKeyInvalid means an idempotency key contains unsafe characters.
	ErrPagarMeIdempotencyKeyInvalid = errors.New("pagarme: idempotency key invalid")
)

// PagarMeEnvironment names a deployment mode. Pagar.me test and live traffic
// use API-key scoping, so both built-in environments share the same base URL.
type PagarMeEnvironment string

const (
	PagarMeEnvironmentProduction PagarMeEnvironment = "production"
	PagarMeEnvironmentSandbox    PagarMeEnvironment = "sandbox"
	PagarMeEnvironmentCustom     PagarMeEnvironment = "custom"
)

// Valid reports whether environment is known.
func (e PagarMeEnvironment) Valid() bool {
	switch e {
	case PagarMeEnvironmentProduction, PagarMeEnvironmentSandbox, PagarMeEnvironmentCustom:
		return true
	default:
		return false
	}
}

// PagarMePaymentMethod names payment methods in provider metadata without
// requiring provider SDK types.
type PagarMePaymentMethod string

const (
	PagarMePaymentMethodCreditCard PagarMePaymentMethod = "credit_card"
	PagarMePaymentMethodDebitCard  PagarMePaymentMethod = "debit_card"
	PagarMePaymentMethodBoleto     PagarMePaymentMethod = "boleto"
	PagarMePaymentMethodPix        PagarMePaymentMethod = "pix"
)

// Valid reports whether method is known.
func (m PagarMePaymentMethod) Valid() bool {
	switch m {
	case PagarMePaymentMethodCreditCard,
		PagarMePaymentMethodDebitCard,
		PagarMePaymentMethodBoleto,
		PagarMePaymentMethodPix:
		return true
	default:
		return false
	}
}

// PagarMeConfig is metadata needed by a future adapter binding. It is
// descriptor-only and never opens sockets.
type PagarMeConfig struct {
	APIKey      string
	AccountID   string
	Environment PagarMeEnvironment
	BaseURL     string
}

// Validate checks whether config can be used by a Pagar.me adapter.
func (c PagarMeConfig) Validate() error {
	_, err := NormalizePagarMeConfig(c)
	return err
}

// NormalizePagarMeConfig trims config, applies defaults, and validates URL and
// secret metadata. It does not contact Pagar.me.
func NormalizePagarMeConfig(config PagarMeConfig) (PagarMeConfig, error) {
	config.APIKey = strings.TrimSpace(config.APIKey)
	config.AccountID = strings.TrimSpace(config.AccountID)

	environment, err := NormalizePagarMeEnvironment(string(config.Environment))
	if err != nil {
		return PagarMeConfig{}, fmt.Errorf("%w: %w", ErrPagarMeConfigInvalid, err)
	}
	config.Environment = environment

	var errs []error
	if config.APIKey == "" {
		errs = append(errs, pagarMeConfigError(ErrPagarMeAPIKeyMissing))
	} else if pagarMeHasSpaceOrControl(config.APIKey) {
		errs = append(errs, pagarMeConfigError(ErrPagarMeAPIKeyInvalid))
	}
	if config.AccountID != "" && pagarMeHasSpaceOrControl(config.AccountID) {
		errs = append(errs, pagarMeConfigError(ErrPagarMeAccountInvalid))
	}

	baseURL, err := NormalizePagarMeBaseURL(config.BaseURL)
	if err != nil {
		errs = append(errs, pagarMeConfigError(err))
	} else {
		config.BaseURL = baseURL
	}

	if err := errors.Join(errs...); err != nil {
		return PagarMeConfig{}, err
	}
	return config, nil
}

// PagarMeProviderDescriptor describes stable provider metadata for code generation,
// diagnostics, and adapter registration.
type PagarMeProviderDescriptor struct {
	Name                 string
	DisplayName          string
	DefaultBaseURL       string
	IdempotencyHeader    string
	MaxInstallments      int
	MaxIdempotencyLength int
}

// Descriptor returns the canonical Pagar.me descriptor.
func (c PagarMeConfig) Descriptor() PagarMeProviderDescriptor {
	return PagarMeDescriptor()
}

// PagarMeDescriptor returns the canonical Pagar.me descriptor.
func PagarMeDescriptor() PagarMeProviderDescriptor {
	return PagarMeProviderDescriptor{
		Name:                 PagarMeProviderName,
		DisplayName:          PagarMeProviderDisplayName,
		DefaultBaseURL:       PagarMeDefaultBaseURL,
		IdempotencyHeader:    PagarMeHeaderIdempotencyKey,
		MaxInstallments:      PagarMeMaxInstallments,
		MaxIdempotencyLength: PagarMeMaxIdempotencyKeyLength,
	}
}

// NormalizePagarMeEnvironment trims and canonicalizes a Pagar.me environment.
// Empty input resolves to production.
func NormalizePagarMeEnvironment(environment string) (PagarMeEnvironment, error) {
	switch strings.ToLower(strings.TrimSpace(environment)) {
	case "":
		return PagarMeEnvironmentProduction, nil
	case string(PagarMeEnvironmentProduction), "prod", "live":
		return PagarMeEnvironmentProduction, nil
	case string(PagarMeEnvironmentSandbox), "test":
		return PagarMeEnvironmentSandbox, nil
	case string(PagarMeEnvironmentCustom):
		return PagarMeEnvironmentCustom, nil
	default:
		return "", ErrPagarMeEnvironmentInvalid
	}
}

// ValidatePagarMeBaseURL checks whether baseURL is an absolute http(s) provider
// URL. Empty input resolves to PagarMeDefaultBaseURL.
func ValidatePagarMeBaseURL(baseURL string) error {
	_, err := NormalizePagarMeBaseURL(baseURL)
	return err
}

// NormalizePagarMeBaseURL trims and validates a Pagar.me API base URL.
func NormalizePagarMeBaseURL(baseURL string) (string, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		baseURL = PagarMeDefaultBaseURL
	}
	if pagarMeHasSpaceOrControl(baseURL) {
		return "", ErrPagarMeBaseURLInvalid
	}

	parsed, err := url.Parse(baseURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrPagarMeBaseURLInvalid, err)
	}
	if !pagarMeValidAbsoluteURL(parsed) || parsed.RawQuery != "" {
		return "", ErrPagarMeBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// PagarMePaymentMetadata is the normalized metadata for a payment request plan.
type PagarMePaymentMetadata struct {
	Method       PagarMePaymentMethod
	CaptureMode  CaptureMode
	Installments int
}

// Validate checks whether metadata is structurally usable.
func (m PagarMePaymentMetadata) Validate() error {
	_, err := NormalizePagarMePaymentMetadata(m)
	return err
}

// NormalizePagarMePaymentMetadata trims defaults and validates method, capture,
// and installment metadata.
func NormalizePagarMePaymentMetadata(metadata PagarMePaymentMetadata) (PagarMePaymentMetadata, error) {
	metadata.Method = PagarMePaymentMethod(strings.TrimSpace(string(metadata.Method)))
	metadata.CaptureMode = CaptureMode(strings.TrimSpace(string(metadata.CaptureMode)))
	if metadata.CaptureMode == "" {
		metadata.CaptureMode = CaptureModeAutomatic
	}
	if metadata.Installments == 0 {
		metadata.Installments = 1
	}

	if !metadata.Method.Valid() {
		return PagarMePaymentMetadata{}, fmt.Errorf("%w: method %q is unknown", ErrPagarMePaymentMetadataInvalid, metadata.Method)
	}
	if metadata.CaptureMode != CaptureModeAutomatic && metadata.CaptureMode != CaptureModeManual {
		return PagarMePaymentMetadata{}, fmt.Errorf("%w: capture mode %q is unknown", ErrPagarMePaymentMetadataInvalid, metadata.CaptureMode)
	}
	if metadata.Installments < 1 || metadata.Installments > PagarMeMaxInstallments {
		return PagarMePaymentMetadata{}, fmt.Errorf("%w: installments must be between 1 and %d", ErrPagarMePaymentMetadataInvalid, PagarMeMaxInstallments)
	}
	if metadata.Installments > 1 && metadata.Method != PagarMePaymentMethodCreditCard {
		return PagarMePaymentMetadata{}, fmt.Errorf("%w: installments require credit_card", ErrPagarMePaymentMetadataInvalid)
	}
	if metadata.CaptureMode == CaptureModeManual && metadata.Method != PagarMePaymentMethodCreditCard {
		return PagarMePaymentMetadata{}, fmt.Errorf("%w: manual capture requires credit_card", ErrPagarMePaymentMetadataInvalid)
	}
	return metadata, nil
}

// PagarMeIdempotencyMetadata describes request idempotency without performing a
// provider request.
type PagarMeIdempotencyMetadata struct {
	Header string
	Key    string
}

// Validate checks whether metadata is structurally usable.
func (m PagarMeIdempotencyMetadata) Validate() error {
	_, err := NormalizePagarMeIdempotencyMetadata(m.Key)
	return err
}

// NormalizePagarMeIdempotencyMetadata trims, validates, and bounds a Pagar.me
// idempotency key for future request headers.
func NormalizePagarMeIdempotencyMetadata(key string) (PagarMeIdempotencyMetadata, error) {
	normalized, err := NormalizePagarMeIdempotencyKey(key)
	if err != nil {
		return PagarMeIdempotencyMetadata{}, err
	}
	return PagarMeIdempotencyMetadata{
		Header: PagarMeHeaderIdempotencyKey,
		Key:    normalized,
	}, nil
}

// FormatPagarMeIdempotencyKey renders a provider-neutral idempotency key for
// use with Pagar.me request metadata.
func FormatPagarMeIdempotencyKey(key IdempotencyKey) (string, error) {
	key.Operation = Operation(strings.TrimSpace(string(key.Operation)))
	key.Provider = PagarMeProviderName
	key.Tenant = strings.TrimSpace(key.Tenant)
	key.TransactionID = strings.TrimSpace(key.TransactionID)
	key.Subject = strings.TrimSpace(key.Subject)
	if key.Operation == "" || key.Subject == "" {
		return "", ErrPagarMeIdempotencyKeyMissing
	}
	return NormalizePagarMeIdempotencyKey(key.String())
}

// NormalizePagarMeIdempotencyKey trims and bounds a Pagar.me idempotency key.
// Oversized keys are replaced with a stable SHA-256 key.
func NormalizePagarMeIdempotencyKey(key string) (string, error) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", ErrPagarMeIdempotencyKeyMissing
	}
	if pagarMeHasControl(key) {
		return "", ErrPagarMeIdempotencyKeyInvalid
	}
	if len(key) <= PagarMeMaxIdempotencyKeyLength {
		return key, nil
	}

	sum := sha256.Sum256([]byte(key))
	return pagarMeHashedIdempotencyKeyPrefix + hex.EncodeToString(sum[:]), nil
}

// PagarMeCreatePaymentIntentIdempotencyKey builds a Pagar.me key for creating
// a payment intent.
func PagarMeCreatePaymentIntentIdempotencyKey(tenant, transactionID string) (string, error) {
	return FormatPagarMeIdempotencyKey(CreateIntentKey(tenant, transactionID))
}

// PagarMeCapturePaymentIdempotencyKey builds a Pagar.me key for capturing a payment.
func PagarMeCapturePaymentIdempotencyKey(tenant, transactionID, paymentID string) (string, error) {
	return FormatPagarMeIdempotencyKey(CaptureKey(tenant, transactionID, paymentID))
}

// PagarMeRefundPaymentIdempotencyKey builds a Pagar.me key for refunding a payment.
func PagarMeRefundPaymentIdempotencyKey(tenant, transactionID, refundID string) (string, error) {
	return FormatPagarMeIdempotencyKey(RefundKey(tenant, transactionID, refundID))
}

// PagarMeSafeSummary is redacted metadata suitable for logs and diagnostics.
type PagarMeSafeSummary struct {
	Provider       string
	Environment    PagarMeEnvironment
	BaseURL        string
	APIKey         string
	AccountID      string
	PaymentMethod  PagarMePaymentMethod
	CaptureMode    CaptureMode
	Installments   int
	IdempotencyKey string
}

// BuildPagarMeSafeSummary normalizes config, payment, and idempotency metadata
// and returns a deterministic redacted summary.
func BuildPagarMeSafeSummary(
	config PagarMeConfig,
	payment PagarMePaymentMetadata,
	idempotency PagarMeIdempotencyMetadata,
) (PagarMeSafeSummary, error) {
	config, err := NormalizePagarMeConfig(config)
	if err != nil {
		return PagarMeSafeSummary{}, err
	}
	payment, err = NormalizePagarMePaymentMetadata(payment)
	if err != nil {
		return PagarMeSafeSummary{}, err
	}
	idempotency, err = NormalizePagarMeIdempotencyMetadata(idempotency.Key)
	if err != nil {
		return PagarMeSafeSummary{}, err
	}

	return PagarMeSafeSummary{
		Provider:       PagarMeProviderName,
		Environment:    config.Environment,
		BaseURL:        RedactPagarMeURL(config.BaseURL),
		APIKey:         RedactPagarMeAPIKey(config.APIKey),
		AccountID:      RedactPagarMeAccountID(config.AccountID),
		PaymentMethod:  payment.Method,
		CaptureMode:    payment.CaptureMode,
		Installments:   payment.Installments,
		IdempotencyKey: RedactPagarMeIdempotencyKey(idempotency.Key),
	}, nil
}

// RedactPagarMeAPIKey redacts API key metadata while keeping a small stable
// prefix and suffix for operator diagnostics.
func RedactPagarMeAPIKey(apiKey string) string {
	return pagarMeRedactToken(strings.TrimSpace(apiKey), 4, 4)
}

// RedactPagarMeAccountID redacts account metadata while preserving enough shape
// to identify which account was configured.
func RedactPagarMeAccountID(accountID string) string {
	return pagarMeRedactToken(strings.TrimSpace(accountID), 3, 3)
}

// RedactPagarMeIdempotencyKey redacts idempotency metadata for summaries.
func RedactPagarMeIdempotencyKey(key string) string {
	return pagarMeRedactToken(strings.TrimSpace(key), 12, 8)
}

// RedactPagarMeURL removes userinfo and query/fragment data from URLs before
// they are written to logs. Invalid URLs return a redacted marker.
func RedactPagarMeURL(rawURL string) string {
	normalized, err := NormalizePagarMeBaseURL(rawURL)
	if err != nil {
		return "redacted"
	}
	parsed, err := url.Parse(normalized)
	if err != nil {
		return "redacted"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String()
}

func pagarMeValidAbsoluteURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func pagarMeHasSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func pagarMeHasControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func pagarMeRedactToken(token string, prefix, suffix int) string {
	if token == "" {
		return ""
	}
	if len(token) <= prefix+suffix {
		return "redacted:" + strconv.Itoa(len(token))
	}
	return token[:prefix] + "..." + token[len(token)-suffix:]
}

func pagarMeConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrPagarMeConfigInvalid, err)
}
