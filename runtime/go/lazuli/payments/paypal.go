package payments

import (
	"errors"
	"fmt"
	"net/url"
	"strings"
)

const (
	PayPalEnvironmentSandbox PayPalEnvironment = "sandbox"
	PayPalEnvironmentLive    PayPalEnvironment = "live"

	PayPalSandboxBaseURL = "https://api-m.sandbox.paypal.com"
	PayPalLiveBaseURL    = "https://api-m.paypal.com"

	payPalProvider = "paypal"
)

var ErrInvalidPayPalDescriptor = errors.New("payments: invalid paypal descriptor")

// PayPalEnvironment names the PayPal API environment without binding callers
// to an SDK or HTTP client.
type PayPalEnvironment string

// PayPalCredentials describes the non-transport metadata needed by a future
// adapter to select a PayPal API host and credentials.
type PayPalCredentials struct {
	ClientID     string
	ClientSecret string
	Environment  PayPalEnvironment
	BaseURL      string
}

// PayPalIntentDescriptor records the provider-neutral intent metadata for a
// PayPal order. CaptureMode controls the normalized PayPal intent.
type PayPalIntentDescriptor struct {
	CaptureMode     CaptureMode
	TransactionID   string
	PaymentIntentID string
	OrderID         string
}

// PayPalCaptureDescriptor records capture metadata without performing a
// capture request.
type PayPalCaptureDescriptor struct {
	PaymentID       string
	OrderID         string
	AuthorizationID string
	CaptureID       string
	FinalCapture    bool
}

// PayPalWebhookDescriptor records identifiers used to verify and de-duplicate
// PayPal webhook delivery.
type PayPalWebhookDescriptor struct {
	WebhookID      string
	TransmissionID string
	EventID        string
	EventType      WebhookEventType
}

// PayPalIdempotencyDescriptor records the PayPal-Request-Id metadata a future
// adapter may attach to an outbound operation.
type PayPalIdempotencyDescriptor struct {
	Operation     Operation
	RequestID     string
	TransactionID string
	Subject       string
}

// PayPalDescriptorPlan is a deterministic, safe-to-log description of PayPal
// payment metadata. It never opens sockets or calls PayPal APIs.
type PayPalDescriptorPlan struct {
	Credentials PayPalCredentials
	Intent      PayPalIntentDescriptor
	Capture     PayPalCaptureDescriptor
	Webhook     PayPalWebhookDescriptor
	Idempotency PayPalIdempotencyDescriptor
	Summary     PayPalDescriptorSummary
}

// PayPalDescriptorSummary contains redacted PayPal metadata for logs,
// diagnostics, and generated adapter dry-runs.
type PayPalDescriptorSummary struct {
	Provider             string
	Environment          PayPalEnvironment
	BaseURL              string
	ClientID             string
	ClientSecret         string
	Intent               string
	PaymentIntentID      string
	OrderID              string
	PaymentID            string
	AuthorizationID      string
	CaptureID            string
	WebhookID            string
	TransmissionID       string
	EventID              string
	EventType            WebhookEventType
	IdempotencyOperation Operation
	IdempotencyRequestID string
	IdempotencyKey       string
}

// NormalizePayPalCredentials trims credential metadata, normalizes the
// environment, and resolves a default base URL when none is provided.
func NormalizePayPalCredentials(credentials PayPalCredentials) PayPalCredentials {
	environment := NormalizePayPalEnvironment(credentials.Environment)
	baseURL := NormalizePayPalBaseURL(environment, credentials.BaseURL)
	return PayPalCredentials{
		ClientID:     strings.TrimSpace(credentials.ClientID),
		ClientSecret: strings.TrimSpace(credentials.ClientSecret),
		Environment:  environment,
		BaseURL:      baseURL,
	}
}

// ValidatePayPalCredentials checks credential and endpoint metadata without
// using the credentials.
func ValidatePayPalCredentials(credentials PayPalCredentials) error {
	credentials = NormalizePayPalCredentials(credentials)
	var errs []error
	if credentials.ClientID == "" {
		errs = append(errs, fmt.Errorf("%w: client id required", ErrInvalidPayPalDescriptor))
	}
	if credentials.ClientSecret == "" {
		errs = append(errs, fmt.Errorf("%w: client secret required", ErrInvalidPayPalDescriptor))
	}
	switch credentials.Environment {
	case PayPalEnvironmentSandbox, PayPalEnvironmentLive:
	default:
		errs = append(errs, fmt.Errorf("%w: unsupported environment %q", ErrInvalidPayPalDescriptor, credentials.Environment))
	}
	if !isPayPalBaseURL(credentials.BaseURL) {
		errs = append(errs, fmt.Errorf("%w: base url must be https without userinfo, query, or fragment", ErrInvalidPayPalDescriptor))
	}
	return errors.Join(errs...)
}

// NormalizePayPalEnvironment maps common environment labels to PayPal's stable
// sandbox/live vocabulary. Empty values default to sandbox.
func NormalizePayPalEnvironment(environment PayPalEnvironment) PayPalEnvironment {
	switch strings.ToLower(strings.TrimSpace(string(environment))) {
	case "", "sandbox", "test", "testing":
		return PayPalEnvironmentSandbox
	case "live", "prod", "production":
		return PayPalEnvironmentLive
	default:
		return PayPalEnvironment(strings.ToLower(strings.TrimSpace(string(environment))))
	}
}

// NormalizePayPalBaseURL trims and canonicalizes a PayPal API base URL. Empty
// input resolves to the environment default.
func NormalizePayPalBaseURL(environment PayPalEnvironment, raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		switch NormalizePayPalEnvironment(environment) {
		case PayPalEnvironmentLive:
			return PayPalLiveBaseURL
		default:
			return PayPalSandboxBaseURL
		}
	}
	u, err := url.Parse(raw)
	if err != nil || u == nil {
		return raw
	}
	u.Scheme = strings.ToLower(u.Scheme)
	u.Host = strings.ToLower(u.Host)
	u.Path = strings.TrimRight(u.Path, "/")
	u.RawQuery = ""
	u.Fragment = ""
	return u.String()
}

// RedactPayPalClientID keeps a short non-secret fingerprint for diagnostics.
func RedactPayPalClientID(clientID string) string {
	return redactPayPalValue(clientID)
}

// RedactPayPalClientSecret removes secret material from logs.
func RedactPayPalClientSecret(string) string {
	return "[redacted]"
}

// RedactPayPalBaseURL removes URL credentials, query strings, and fragments.
func RedactPayPalBaseURL(raw string) string {
	raw = strings.TrimSpace(raw)
	u, err := url.Parse(raw)
	if err != nil || u == nil || u.Scheme == "" || u.Host == "" {
		return "[redacted]"
	}
	u.User = nil
	u.RawQuery = ""
	u.Fragment = ""
	u.Path = strings.TrimRight(u.Path, "/")
	return u.String()
}

// NormalizePayPalIntentDescriptor trims identifiers and applies automatic
// capture as the default capture mode.
func NormalizePayPalIntentDescriptor(desc PayPalIntentDescriptor) PayPalIntentDescriptor {
	mode := desc.CaptureMode
	if mode == "" {
		mode = CaptureModeAutomatic
	}
	return PayPalIntentDescriptor{
		CaptureMode:     CaptureMode(strings.ToLower(strings.TrimSpace(string(mode)))),
		TransactionID:   strings.TrimSpace(desc.TransactionID),
		PaymentIntentID: strings.TrimSpace(desc.PaymentIntentID),
		OrderID:         strings.TrimSpace(desc.OrderID),
	}
}

// ValidatePayPalIntentDescriptor checks provider-neutral PayPal order intent
// metadata.
func ValidatePayPalIntentDescriptor(desc PayPalIntentDescriptor) error {
	desc = NormalizePayPalIntentDescriptor(desc)
	var errs []error
	switch desc.CaptureMode {
	case CaptureModeAutomatic, CaptureModeManual:
	default:
		errs = append(errs, fmt.Errorf("%w: unsupported capture mode %q", ErrInvalidPayPalDescriptor, desc.CaptureMode))
	}
	if desc.TransactionID == "" && desc.PaymentIntentID == "" && desc.OrderID == "" {
		errs = append(errs, fmt.Errorf("%w: intent requires transaction, payment intent, or order id", ErrInvalidPayPalDescriptor))
	}
	return errors.Join(errs...)
}

// PayPalIntent returns the PayPal order intent value for the descriptor.
func (d PayPalIntentDescriptor) PayPalIntent() string {
	if NormalizePayPalIntentDescriptor(d).CaptureMode == CaptureModeManual {
		return "AUTHORIZE"
	}
	return "CAPTURE"
}

// NormalizePayPalCaptureDescriptor trims capture identifiers.
func NormalizePayPalCaptureDescriptor(desc PayPalCaptureDescriptor) PayPalCaptureDescriptor {
	return PayPalCaptureDescriptor{
		PaymentID:       strings.TrimSpace(desc.PaymentID),
		OrderID:         strings.TrimSpace(desc.OrderID),
		AuthorizationID: strings.TrimSpace(desc.AuthorizationID),
		CaptureID:       strings.TrimSpace(desc.CaptureID),
		FinalCapture:    desc.FinalCapture,
	}
}

// ValidatePayPalCaptureDescriptor checks capture metadata. Empty descriptors
// are allowed so callers can plan credentials or webhooks independently.
func ValidatePayPalCaptureDescriptor(desc PayPalCaptureDescriptor) error {
	desc = NormalizePayPalCaptureDescriptor(desc)
	if desc.PaymentID == "" && desc.OrderID == "" && desc.AuthorizationID == "" && desc.CaptureID == "" {
		return nil
	}
	if desc.PaymentID == "" && desc.OrderID == "" {
		return fmt.Errorf("%w: capture requires payment or order id", ErrInvalidPayPalDescriptor)
	}
	return nil
}

// NormalizePayPalWebhookDescriptor trims webhook identifiers.
func NormalizePayPalWebhookDescriptor(desc PayPalWebhookDescriptor) PayPalWebhookDescriptor {
	return PayPalWebhookDescriptor{
		WebhookID:      strings.TrimSpace(desc.WebhookID),
		TransmissionID: strings.TrimSpace(desc.TransmissionID),
		EventID:        strings.TrimSpace(desc.EventID),
		EventType:      WebhookEventType(strings.TrimSpace(string(desc.EventType))),
	}
}

// ValidatePayPalWebhookDescriptor checks webhook ID metadata. Empty descriptors
// are allowed so the helper can be used for non-webhook plans.
func ValidatePayPalWebhookDescriptor(desc PayPalWebhookDescriptor) error {
	desc = NormalizePayPalWebhookDescriptor(desc)
	if desc.WebhookID == "" && desc.TransmissionID == "" && desc.EventID == "" && desc.EventType == "" {
		return nil
	}
	var errs []error
	if desc.WebhookID == "" {
		errs = append(errs, fmt.Errorf("%w: webhook id required", ErrInvalidPayPalDescriptor))
	}
	if desc.TransmissionID == "" && desc.EventID == "" {
		errs = append(errs, fmt.Errorf("%w: webhook requires transmission or event id", ErrInvalidPayPalDescriptor))
	}
	return errors.Join(errs...)
}

// NormalizePayPalIdempotencyDescriptor trims PayPal request id metadata.
func NormalizePayPalIdempotencyDescriptor(desc PayPalIdempotencyDescriptor) PayPalIdempotencyDescriptor {
	return PayPalIdempotencyDescriptor{
		Operation:     Operation(strings.TrimSpace(string(desc.Operation))),
		RequestID:     strings.TrimSpace(desc.RequestID),
		TransactionID: strings.TrimSpace(desc.TransactionID),
		Subject:       strings.TrimSpace(desc.Subject),
	}
}

// ValidatePayPalIdempotencyDescriptor checks PayPal-Request-Id metadata. Empty
// descriptors are allowed for read-only or webhook-only planning.
func ValidatePayPalIdempotencyDescriptor(desc PayPalIdempotencyDescriptor) error {
	desc = NormalizePayPalIdempotencyDescriptor(desc)
	if desc.Operation == "" && desc.RequestID == "" && desc.TransactionID == "" && desc.Subject == "" {
		return nil
	}
	var errs []error
	switch desc.Operation {
	case OperationCreateIntent, OperationConfirm, OperationCapture, OperationRefund, OperationWebhook:
	default:
		errs = append(errs, fmt.Errorf("%w: unsupported idempotency operation %q", ErrInvalidPayPalDescriptor, desc.Operation))
	}
	if desc.RequestID == "" {
		errs = append(errs, fmt.Errorf("%w: idempotency request id required", ErrInvalidPayPalDescriptor))
	}
	return errors.Join(errs...)
}

// PayPalIdempotencyKey returns a provider-neutral key for the descriptor.
func (d PayPalIdempotencyDescriptor) PayPalIdempotencyKey() IdempotencyKey {
	d = NormalizePayPalIdempotencyDescriptor(d)
	return IdempotencyKey{
		Operation:     d.Operation,
		Provider:      payPalProvider,
		TransactionID: d.TransactionID,
		Subject:       firstNonEmpty(d.Subject, d.RequestID),
	}
}

// PlanPayPalDescriptor validates and normalizes PayPal metadata into a
// deterministic safe summary. It performs no SDK, network, or HTTP work.
func PlanPayPalDescriptor(
	credentials PayPalCredentials,
	intent PayPalIntentDescriptor,
	capture PayPalCaptureDescriptor,
	webhook PayPalWebhookDescriptor,
	idempotency PayPalIdempotencyDescriptor,
) (PayPalDescriptorPlan, error) {
	credentials = NormalizePayPalCredentials(credentials)
	intent = NormalizePayPalIntentDescriptor(intent)
	capture = NormalizePayPalCaptureDescriptor(capture)
	webhook = NormalizePayPalWebhookDescriptor(webhook)
	idempotency = NormalizePayPalIdempotencyDescriptor(idempotency)

	if err := errors.Join(
		ValidatePayPalCredentials(credentials),
		ValidatePayPalIntentDescriptor(intent),
		ValidatePayPalCaptureDescriptor(capture),
		ValidatePayPalWebhookDescriptor(webhook),
		ValidatePayPalIdempotencyDescriptor(idempotency),
	); err != nil {
		return PayPalDescriptorPlan{}, err
	}

	plan := PayPalDescriptorPlan{
		Credentials: credentials,
		Intent:      intent,
		Capture:     capture,
		Webhook:     webhook,
		Idempotency: idempotency,
	}
	plan.Summary = SummarizePayPalDescriptor(plan)
	return plan, nil
}

// SummarizePayPalDescriptor returns a redacted, deterministic summary. The
// input may be a fully planned descriptor or a partially populated one.
func SummarizePayPalDescriptor(plan PayPalDescriptorPlan) PayPalDescriptorSummary {
	credentials := NormalizePayPalCredentials(plan.Credentials)
	intent := NormalizePayPalIntentDescriptor(plan.Intent)
	capture := NormalizePayPalCaptureDescriptor(plan.Capture)
	webhook := NormalizePayPalWebhookDescriptor(plan.Webhook)
	idempotency := NormalizePayPalIdempotencyDescriptor(plan.Idempotency)
	return PayPalDescriptorSummary{
		Provider:             payPalProvider,
		Environment:          credentials.Environment,
		BaseURL:              RedactPayPalBaseURL(credentials.BaseURL),
		ClientID:             RedactPayPalClientID(credentials.ClientID),
		ClientSecret:         RedactPayPalClientSecret(credentials.ClientSecret),
		Intent:               intent.PayPalIntent(),
		PaymentIntentID:      intent.PaymentIntentID,
		OrderID:              firstNonEmpty(intent.OrderID, capture.OrderID),
		PaymentID:            capture.PaymentID,
		AuthorizationID:      capture.AuthorizationID,
		CaptureID:            capture.CaptureID,
		WebhookID:            webhook.WebhookID,
		TransmissionID:       webhook.TransmissionID,
		EventID:              webhook.EventID,
		EventType:            webhook.EventType,
		IdempotencyOperation: idempotency.Operation,
		IdempotencyRequestID: RedactPayPalClientID(idempotency.RequestID),
		IdempotencyKey:       payPalIdempotencyKeyString(idempotency),
	}
}

func isPayPalBaseURL(raw string) bool {
	u, err := url.Parse(strings.TrimSpace(raw))
	return err == nil && u != nil && u.Scheme == "https" && u.Host != "" && u.User == nil && u.RawQuery == "" && u.Fragment == ""
}

func redactPayPalValue(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return "[redacted]"
	}
	if len(value) <= 8 {
		return value[:payPalMinInt(4, len(value))] + "...[redacted]"
	}
	return value[:4] + "..." + value[len(value)-4:]
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}

func payPalIdempotencyKeyString(desc PayPalIdempotencyDescriptor) string {
	if desc.Operation == "" && desc.RequestID == "" && desc.TransactionID == "" && desc.Subject == "" {
		return ""
	}
	return desc.PayPalIdempotencyKey().String()
}

func payPalMinInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}
