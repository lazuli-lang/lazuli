// Package stripe contains Stripe-specific payment provider descriptors.
//
// The package is intentionally metadata-only: it validates provider config,
// names supported webhook events, and formats idempotency keys without linking
// the Stripe SDK or making network calls.
package stripe

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/url"
	"strings"
	"unicode"

	"lazuli.dev/runtime/lazuli/payments"
)

const (
	// ProviderName is the provider identifier used by normalized payment records.
	ProviderName = "stripe"
	// ProviderDisplayName is the human-facing provider name.
	ProviderDisplayName = "Stripe"
	// DefaultBaseURL is the Stripe REST API host.
	DefaultBaseURL = "https://api.stripe.com"
	// DefaultWebhookEndpoint is the conventional Lazuli route for Stripe events.
	DefaultWebhookEndpoint = "/webhooks/stripe"
	// HeaderIdempotencyKey is the Stripe idempotency header.
	HeaderIdempotencyKey = "Idempotency-Key"
	// HeaderSignature is the Stripe webhook signature header.
	HeaderSignature = "Stripe-Signature"
	// MaxIdempotencyKeyLength is Stripe's documented idempotency key limit.
	MaxIdempotencyKeyLength = 255
)

const hashedIdempotencyKeyPrefix = "payments:stripe:sha256:"

var (
	// ErrConfigInvalid means the Stripe descriptor config is malformed.
	ErrConfigInvalid = errors.New("stripe: config invalid")
	// ErrSecretKeyMissing means API request capabilities were enabled without a secret key.
	ErrSecretKeyMissing = errors.New("stripe: secret key missing")
	// ErrSecretKeyInvalid means the configured API secret key has unsafe characters.
	ErrSecretKeyInvalid = errors.New("stripe: secret key invalid")
	// ErrWebhookSecretMissing means webhook parsing was enabled without a webhook secret.
	ErrWebhookSecretMissing = errors.New("stripe: webhook secret missing")
	// ErrWebhookSecretInvalid means the configured webhook secret has unsafe characters.
	ErrWebhookSecretInvalid = errors.New("stripe: webhook secret invalid")
	// ErrBaseURLInvalid means the configured provider base URL is malformed.
	ErrBaseURLInvalid = errors.New("stripe: base url invalid")
	// ErrEndpointInvalid means a provider API endpoint path is malformed.
	ErrEndpointInvalid = errors.New("stripe: endpoint invalid")
	// ErrWebhookEndpointInvalid means a configured webhook endpoint is malformed.
	ErrWebhookEndpointInvalid = errors.New("stripe: webhook endpoint invalid")
	// ErrCapabilityUnsupported means the config requested an unknown capability bit.
	ErrCapabilityUnsupported = errors.New("stripe: capability unsupported")
	// ErrIdempotencyKeyMissing means a Stripe idempotency key cannot be derived.
	ErrIdempotencyKeyMissing = errors.New("stripe: idempotency key missing")
	// ErrIdempotencyKeyInvalid means an idempotency key contains unsafe characters.
	ErrIdempotencyKeyInvalid = errors.New("stripe: idempotency key invalid")
)

// Capability is a bitmask of Stripe adapter capabilities.
type Capability uint64

const (
	// CapabilityCreatePaymentIntent supports creating provider payment intents.
	CapabilityCreatePaymentIntent Capability = 1 << iota
	// CapabilityConfirmPayment supports confirming a provider payment intent.
	CapabilityConfirmPayment
	// CapabilityCapturePayment supports capturing an authorized payment intent.
	CapabilityCapturePayment
	// CapabilityRefundPayment supports full or partial provider refunds.
	CapabilityRefundPayment
	// CapabilityParseWebhookEvent supports verifying and normalizing Stripe events.
	CapabilityParseWebhookEvent
	// CapabilityHostedCheckout supports Stripe-hosted checkout flows.
	CapabilityHostedCheckout
	// CapabilityClientSecret supports returning client_secret values to clients.
	CapabilityClientSecret
	// CapabilityManualCapture supports authorize-now, capture-later flows.
	CapabilityManualCapture
	// CapabilityIdempotencyKeys supports Stripe Idempotency-Key request headers.
	CapabilityIdempotencyKeys
)

const (
	apiRequestCapabilities = CapabilityCreatePaymentIntent |
		CapabilityConfirmPayment |
		CapabilityCapturePayment |
		CapabilityRefundPayment |
		CapabilityHostedCheckout

	// DefaultCapabilities is the Stripe descriptor's closed supported set.
	DefaultCapabilities = apiRequestCapabilities |
		CapabilityParseWebhookEvent |
		CapabilityClientSecret |
		CapabilityManualCapture |
		CapabilityIdempotencyKeys
)

// Has reports whether every flag in required is present.
func (c Capability) Has(required Capability) bool {
	return required != 0 && c&required == required
}

// HasAny reports whether any flag in candidates is present.
func (c Capability) HasAny(candidates Capability) bool {
	return candidates != 0 && c&candidates != 0
}

// Validate reports unknown capability bits.
func (c Capability) Validate() error {
	if unsupported := c &^ DefaultCapabilities; unsupported != 0 {
		return fmt.Errorf("%w: 0x%x", ErrCapabilityUnsupported, uint64(unsupported))
	}
	return nil
}

// ProviderDescriptor describes Stripe adapter metadata for generated code,
// diagnostics, and deploy adapters.
type ProviderDescriptor struct {
	Name                   string
	DisplayName            string
	DefaultBaseURL         string
	DefaultWebhookEndpoint string
	IdempotencyHeader      string
	WebhookSignatureHeader string
	Capabilities           Capability
	WebhookEvents          []WebhookEventName
}

// Descriptor returns the canonical Stripe provider descriptor.
func Descriptor() ProviderDescriptor {
	return ProviderDescriptor{
		Name:                   ProviderName,
		DisplayName:            ProviderDisplayName,
		DefaultBaseURL:         DefaultBaseURL,
		DefaultWebhookEndpoint: DefaultWebhookEndpoint,
		IdempotencyHeader:      HeaderIdempotencyKey,
		WebhookSignatureHeader: HeaderSignature,
		Capabilities:           DefaultCapabilities,
		WebhookEvents:          SupportedWebhookEvents(),
	}
}

// Supports reports whether descriptor includes every requested capability.
func (d ProviderDescriptor) Supports(capability Capability) bool {
	return d.Capabilities.Has(capability)
}

// SupportsWebhookEvent reports whether descriptor lists event.
func (d ProviderDescriptor) SupportsWebhookEvent(event WebhookEventName) bool {
	event = WebhookEventName(strings.TrimSpace(string(event)))
	for _, candidate := range d.WebhookEvents {
		if candidate == event {
			return true
		}
	}
	return false
}

// WebhookEventName is a Stripe webhook event type string.
type WebhookEventName string

const (
	// WebhookEventCheckoutSessionCompleted is emitted when Checkout completes.
	WebhookEventCheckoutSessionCompleted WebhookEventName = "checkout.session.completed"
	// WebhookEventCheckoutSessionExpired is emitted when Checkout expires.
	WebhookEventCheckoutSessionExpired WebhookEventName = "checkout.session.expired"
	// WebhookEventPaymentIntentCreated is emitted when a PaymentIntent is created.
	WebhookEventPaymentIntentCreated WebhookEventName = "payment_intent.created"
	// WebhookEventPaymentIntentRequiresAction is emitted when a PaymentIntent requires customer action.
	WebhookEventPaymentIntentRequiresAction WebhookEventName = "payment_intent.requires_action"
	// WebhookEventPaymentIntentSucceeded is emitted when a PaymentIntent succeeds.
	WebhookEventPaymentIntentSucceeded WebhookEventName = "payment_intent.succeeded"
	// WebhookEventPaymentIntentFailed is emitted when a PaymentIntent payment fails.
	WebhookEventPaymentIntentFailed WebhookEventName = "payment_intent.payment_failed"
	// WebhookEventPaymentIntentCanceled is emitted when a PaymentIntent is canceled.
	WebhookEventPaymentIntentCanceled WebhookEventName = "payment_intent.canceled"
	// WebhookEventChargeCaptured is emitted when a charge capture completes.
	WebhookEventChargeCaptured WebhookEventName = "charge.captured"
	// WebhookEventChargeRefunded is emitted when a charge is refunded.
	WebhookEventChargeRefunded WebhookEventName = "charge.refunded"
	// WebhookEventRefundCreated is emitted when a refund is created.
	WebhookEventRefundCreated WebhookEventName = "refund.created"
	// WebhookEventRefundUpdated is emitted when a refund changes state.
	WebhookEventRefundUpdated WebhookEventName = "refund.updated"
	// WebhookEventRefundFailed is emitted when a refund fails.
	WebhookEventRefundFailed WebhookEventName = "refund.failed"
)

var defaultWebhookEvents = []WebhookEventName{
	WebhookEventCheckoutSessionCompleted,
	WebhookEventCheckoutSessionExpired,
	WebhookEventPaymentIntentCreated,
	WebhookEventPaymentIntentRequiresAction,
	WebhookEventPaymentIntentSucceeded,
	WebhookEventPaymentIntentFailed,
	WebhookEventPaymentIntentCanceled,
	WebhookEventChargeCaptured,
	WebhookEventChargeRefunded,
	WebhookEventRefundCreated,
	WebhookEventRefundUpdated,
	WebhookEventRefundFailed,
}

// SupportedWebhookEvents returns the closed Stripe event set known to Lazuli.
func SupportedWebhookEvents() []WebhookEventName {
	return append([]WebhookEventName(nil), defaultWebhookEvents...)
}

// Valid reports whether event is in the descriptor's supported event set.
func (e WebhookEventName) Valid() bool {
	_, ok := e.NormalizedType()
	return ok
}

// NormalizedType maps a Stripe event name to the provider-neutral payment event
// category used by lazuli/payments.
func (e WebhookEventName) NormalizedType() (payments.WebhookEventType, bool) {
	switch WebhookEventName(strings.TrimSpace(string(e))) {
	case WebhookEventPaymentIntentCreated:
		return payments.WebhookEventPaymentCreated, true
	case WebhookEventPaymentIntentFailed:
		return payments.WebhookEventPaymentFailed, true
	case WebhookEventCheckoutSessionExpired:
		return payments.WebhookEventPaymentExpired, true
	case WebhookEventRefundCreated:
		return payments.WebhookEventRefundCreated, true
	case WebhookEventChargeRefunded, WebhookEventRefundUpdated, WebhookEventRefundFailed:
		return payments.WebhookEventRefundUpdated, true
	case WebhookEventCheckoutSessionCompleted,
		WebhookEventPaymentIntentRequiresAction,
		WebhookEventPaymentIntentSucceeded,
		WebhookEventPaymentIntentCanceled,
		WebhookEventChargeCaptured:
		return payments.WebhookEventPaymentUpdated, true
	default:
		return "", false
	}
}

// Config is provider config metadata for a Stripe adapter binding.
type Config struct {
	SecretKey       string
	WebhookSecret   string
	BaseURL         string
	WebhookEndpoint string
	Capabilities    Capability
}

// Validate checks that config can support its enabled capabilities.
func (c Config) Validate() error {
	return ValidateConfig(c)
}

// ValidateConfig checks that config can support its enabled capabilities.
func ValidateConfig(config Config) error {
	_, err := NormalizeConfig(config)
	return err
}

// NormalizeConfig trims config, applies defaults, and validates endpoint
// metadata. It does not contact Stripe.
func NormalizeConfig(config Config) (Config, error) {
	config.SecretKey = strings.TrimSpace(config.SecretKey)
	config.WebhookSecret = strings.TrimSpace(config.WebhookSecret)
	config.WebhookEndpoint = strings.TrimSpace(config.WebhookEndpoint)
	if config.Capabilities == 0 {
		config.Capabilities = DefaultCapabilities
	}

	var errs []error
	if err := config.Capabilities.Validate(); err != nil {
		errs = append(errs, configError(err))
	}

	if config.Capabilities.HasAny(apiRequestCapabilities) {
		if config.SecretKey == "" {
			errs = append(errs, configError(ErrSecretKeyMissing))
		} else if hasSpaceOrControl(config.SecretKey) {
			errs = append(errs, configError(ErrSecretKeyInvalid))
		}
	}
	if config.Capabilities.Has(CapabilityParseWebhookEvent) {
		if config.WebhookSecret == "" {
			errs = append(errs, configError(ErrWebhookSecretMissing))
		} else if hasSpaceOrControl(config.WebhookSecret) {
			errs = append(errs, configError(ErrWebhookSecretInvalid))
		}
	}

	baseURL, err := NormalizeBaseURL(config.BaseURL)
	if err != nil {
		errs = append(errs, configError(err))
	} else {
		config.BaseURL = baseURL
	}

	webhookEndpoint, err := NormalizeWebhookEndpoint(config.WebhookEndpoint)
	if err != nil {
		errs = append(errs, configError(err))
	} else {
		config.WebhookEndpoint = webhookEndpoint
	}

	if err := errors.Join(errs...); err != nil {
		return Config{}, err
	}
	return config, nil
}

// ValidateBaseURL checks whether baseURL is an absolute http(s) provider URL.
// An empty baseURL is valid and resolves to DefaultBaseURL.
func ValidateBaseURL(baseURL string) error {
	_, err := NormalizeBaseURL(baseURL)
	return err
}

// NormalizeBaseURL trims and validates a Stripe API base URL.
func NormalizeBaseURL(baseURL string) (string, error) {
	parsed, err := parseBaseURL(baseURL)
	if err != nil {
		return "", err
	}
	return parsed.String(), nil
}

// APIEndpoint joins endpointPath to baseURL after validating both values.
func APIEndpoint(baseURL, endpointPath string) (string, error) {
	parsed, err := parseBaseURL(baseURL)
	if err != nil {
		return "", err
	}

	endpointPath = strings.TrimSpace(endpointPath)
	if endpointPath == "" {
		return parsed.String(), nil
	}
	if hasSpaceOrControl(endpointPath) {
		return "", ErrEndpointInvalid
	}

	relative, err := url.Parse(endpointPath)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrEndpointInvalid, err)
	}
	if relative.IsAbs() || relative.Host != "" || relative.User != nil || relative.Fragment != "" || relative.Path == "" {
		return "", ErrEndpointInvalid
	}

	parsed.Path = joinURLPath(parsed.Path, relative.Path)
	parsed.RawQuery = relative.RawQuery
	return parsed.String(), nil
}

// ValidateWebhookEndpoint checks whether endpoint is an absolute http(s) URL
// or an absolute route path. Empty endpoints are valid.
func ValidateWebhookEndpoint(endpoint string) error {
	_, err := NormalizeWebhookEndpoint(endpoint)
	return err
}

// NormalizeWebhookEndpoint trims and validates a webhook endpoint URL or route
// path. Empty endpoints stay empty.
func NormalizeWebhookEndpoint(endpoint string) (string, error) {
	endpoint = strings.TrimSpace(endpoint)
	if endpoint == "" {
		return "", nil
	}
	if hasSpaceOrControl(endpoint) {
		return "", ErrWebhookEndpointInvalid
	}

	parsed, err := url.Parse(endpoint)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrWebhookEndpointInvalid, err)
	}
	if parsed.IsAbs() || parsed.Host != "" {
		if !validAbsoluteEndpointURL(parsed) || parsed.RawQuery != "" {
			return "", ErrWebhookEndpointInvalid
		}
		parsed.Scheme = strings.ToLower(parsed.Scheme)
		return parsed.String(), nil
	}
	if !strings.HasPrefix(endpoint, "/") ||
		parsed.Path == "" ||
		parsed.RawQuery != "" ||
		parsed.Fragment != "" ||
		parsed.User != nil {
		return "", ErrWebhookEndpointInvalid
	}
	return parsed.Path, nil
}

// FormatIdempotencyKey renders a provider-neutral payment idempotency key for
// use with Stripe's Idempotency-Key header.
func FormatIdempotencyKey(key payments.IdempotencyKey) (string, error) {
	key.Operation = payments.Operation(strings.TrimSpace(string(key.Operation)))
	key.Provider = ProviderName
	key.Tenant = strings.TrimSpace(key.Tenant)
	key.TransactionID = strings.TrimSpace(key.TransactionID)
	key.Subject = strings.TrimSpace(key.Subject)
	if key.Operation == "" || key.Subject == "" {
		return "", ErrIdempotencyKeyMissing
	}
	return NormalizeIdempotencyKey(key.String())
}

// NormalizeIdempotencyKey trims and bounds a Stripe idempotency key. Oversized
// keys are replaced with a stable SHA-256 key to satisfy Stripe's length limit.
func NormalizeIdempotencyKey(key string) (string, error) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", ErrIdempotencyKeyMissing
	}
	if hasControl(key) {
		return "", ErrIdempotencyKeyInvalid
	}
	if len(key) <= MaxIdempotencyKeyLength {
		return key, nil
	}

	sum := sha256.Sum256([]byte(key))
	return hashedIdempotencyKeyPrefix + hex.EncodeToString(sum[:]), nil
}

// CreatePaymentIntentIdempotencyKey builds a Stripe key for creating a payment intent.
func CreatePaymentIntentIdempotencyKey(tenant, transactionID string) (string, error) {
	return FormatIdempotencyKey(payments.CreateIntentKey(tenant, transactionID))
}

// ConfirmPaymentIdempotencyKey builds a Stripe key for confirming a payment intent.
func ConfirmPaymentIdempotencyKey(tenant, transactionID, paymentID string) (string, error) {
	return FormatIdempotencyKey(payments.ConfirmKey(tenant, transactionID, paymentID))
}

// CapturePaymentIdempotencyKey builds a Stripe key for capturing a payment intent.
func CapturePaymentIdempotencyKey(tenant, transactionID, paymentID string) (string, error) {
	return FormatIdempotencyKey(payments.CaptureKey(tenant, transactionID, paymentID))
}

// RefundPaymentIdempotencyKey builds a Stripe key for refunding a payment.
func RefundPaymentIdempotencyKey(tenant, transactionID, refundID string) (string, error) {
	return FormatIdempotencyKey(payments.RefundKey(tenant, transactionID, refundID))
}

// WebhookIdempotencyKey builds a Stripe key for deduping a webhook event.
func WebhookIdempotencyKey(eventID string) (string, error) {
	return FormatIdempotencyKey(payments.WebhookKey(ProviderName, eventID))
}

func parseBaseURL(baseURL string) (*url.URL, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		baseURL = DefaultBaseURL
	}
	if hasSpaceOrControl(baseURL) {
		return nil, ErrBaseURLInvalid
	}

	parsed, err := url.Parse(baseURL)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrBaseURLInvalid, err)
	}
	if !validAbsoluteEndpointURL(parsed) || parsed.RawQuery != "" {
		return nil, ErrBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed, nil
}

func validAbsoluteEndpointURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func joinURLPath(basePath, endpointPath string) string {
	basePath = strings.TrimRight(basePath, "/")
	endpointPath = strings.TrimLeft(endpointPath, "/")
	if endpointPath == "" {
		return basePath
	}
	return basePath + "/" + endpointPath
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
	return fmt.Errorf("%w: %w", ErrConfigInvalid, err)
}
