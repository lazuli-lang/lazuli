package notifications

import (
	"crypto/ecdh"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

const (
	// WebPushMaxPayloadBytes is a conservative encrypted-payload planning
	// ceiling used by common push services.
	WebPushMaxPayloadBytes = 4096
	// WebPushMaxTopicRunes is the Web Push Topic header limit.
	WebPushMaxTopicRunes = 32

	defaultWebPushTTL = 4 * 7 * 24 * time.Hour
)

var (
	ErrInvalidWebPushVAPID        = errors.New("notifications: invalid web push vapid metadata")
	ErrInvalidWebPushSubject      = errors.New("notifications: invalid web push subject")
	ErrInvalidWebPushSubscription = errors.New("notifications: invalid web push subscription")
	ErrInvalidWebPushOptions      = errors.New("notifications: invalid web push options")
	ErrInvalidWebPushPayload      = errors.New("notifications: invalid web push payload")
)

// WebPushUrgency is the provider-neutral Web Push Urgency header catalog.
type WebPushUrgency string

const (
	WebPushUrgencyVeryLow WebPushUrgency = "very-low"
	WebPushUrgencyLow     WebPushUrgency = "low"
	WebPushUrgencyNormal  WebPushUrgency = "normal"
	WebPushUrgencyHigh    WebPushUrgency = "high"
)

// WebPushVAPIDMetadata describes VAPID credentials without binding to a JWT
// implementation or push-service provider.
type WebPushVAPIDMetadata struct {
	PublicKey  string
	PrivateKey string
	Subject    string
}

// WebPushSubscriptionDescriptor is the browser subscription metadata an
// adapter needs before encrypting and sending a Web Push message.
type WebPushSubscriptionDescriptor struct {
	Endpoint string
	P256DH   string
	Auth     string
}

// WebPushOptions configures provider-neutral request planning.
type WebPushOptions struct {
	TTL     time.Duration
	Urgency WebPushUrgency
	Topic   string
}

// WebPushMessage is the dry-run notification payload. When Payload is set it is
// used verbatim; otherwise Title, Body, and Data are marshaled as deterministic
// JSON by the standard library.
type WebPushMessage struct {
	Title          string
	Body           string
	Data           map[string]any
	Payload        []byte
	IdempotencyKey string
}

// WebPushPlan is a deterministic send descriptor. It never encrypts payloads,
// opens sockets, or contacts a push service.
type WebPushPlan struct {
	VAPID              WebPushVAPIDMetadata
	Subscription       WebPushSubscriptionDescriptor
	Options            WebPushOptions
	Headers            map[string]string
	Payload            []byte
	PayloadJSON        []byte
	PayloadBytes       int
	RedactedEndpoint   string
	RedactedPublicKey  string
	RedactedPrivateKey string
	IdempotencyKey     string
}

// NormalizeWebPushVAPIDMetadata trims VAPID metadata and returns a copy.
func NormalizeWebPushVAPIDMetadata(meta WebPushVAPIDMetadata) WebPushVAPIDMetadata {
	return WebPushVAPIDMetadata{
		PublicKey:  strings.TrimSpace(meta.PublicKey),
		PrivateKey: strings.TrimSpace(meta.PrivateKey),
		Subject:    strings.TrimSpace(meta.Subject),
	}
}

// ValidateWebPushVAPIDMetadata checks VAPID key material and contact subject
// shape without mutating metadata.
func ValidateWebPushVAPIDMetadata(meta WebPushVAPIDMetadata) error {
	meta = NormalizeWebPushVAPIDMetadata(meta)
	var errs []error
	if _, err := decodeWebPushPublicKey(meta.PublicKey); err != nil {
		errs = append(errs, err)
	}
	if _, err := decodeWebPushPrivateKey(meta.PrivateKey); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateWebPushSubject(meta.Subject); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// RedactWebPushVAPIDPublicKey keeps a short non-secret fingerprint for logs.
func RedactWebPushVAPIDPublicKey(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "[redacted]"
	}
	if len(raw) <= 12 {
		return raw[:min(len(raw), 4)] + "...[redacted]"
	}
	return raw[:6] + "..." + raw[len(raw)-6:]
}

// RedactWebPushVAPIDPrivateKey removes private key material from logs.
func RedactWebPushVAPIDPrivateKey(string) string {
	return "[redacted]"
}

// ValidateWebPushSubject accepts VAPID contact subjects in mailto: or https:
// URL form.
func ValidateWebPushSubject(subject string) error {
	subject = strings.TrimSpace(subject)
	if subject == "" {
		return fmt.Errorf("%w: subject required", ErrInvalidWebPushSubject)
	}
	u, err := url.Parse(subject)
	if err != nil || u == nil {
		return fmt.Errorf("%w: parse subject", ErrInvalidWebPushSubject)
	}
	switch u.Scheme {
	case "mailto":
		if u.Opaque == "" || strings.ContainsAny(u.Opaque, " \t\r\n") {
			return fmt.Errorf("%w: mailto subject must include an address", ErrInvalidWebPushSubject)
		}
	case "https":
		if u.Host == "" || u.User != nil {
			return fmt.Errorf("%w: https subject must include a host and no userinfo", ErrInvalidWebPushSubject)
		}
	default:
		return fmt.Errorf("%w: subject must use mailto or https", ErrInvalidWebPushSubject)
	}
	if u.Fragment != "" {
		return fmt.Errorf("%w: subject fragment not allowed", ErrInvalidWebPushSubject)
	}
	return nil
}

// NormalizeWebPushSubscriptionDescriptor trims browser subscription metadata.
func NormalizeWebPushSubscriptionDescriptor(desc WebPushSubscriptionDescriptor) WebPushSubscriptionDescriptor {
	return WebPushSubscriptionDescriptor{
		Endpoint: strings.TrimSpace(desc.Endpoint),
		P256DH:   strings.TrimSpace(desc.P256DH),
		Auth:     strings.TrimSpace(desc.Auth),
	}
}

// ValidateWebPushSubscriptionDescriptor checks endpoint and browser key
// material needed by future adapters.
func ValidateWebPushSubscriptionDescriptor(desc WebPushSubscriptionDescriptor) error {
	desc = NormalizeWebPushSubscriptionDescriptor(desc)
	var errs []error
	if !isWebPushEndpoint(desc.Endpoint) {
		errs = append(errs, fmt.Errorf("%w: endpoint must be https", ErrInvalidWebPushSubscription))
	}
	if _, err := decodeWebPushPublicKey(desc.P256DH); err != nil {
		errs = append(errs, fmt.Errorf("%w: p256dh: %v", ErrInvalidWebPushSubscription, err))
	}
	auth, err := decodeWebPushBase64URL(desc.Auth)
	if err != nil || len(auth) == 0 {
		errs = append(errs, fmt.Errorf("%w: auth secret must be base64url", ErrInvalidWebPushSubscription))
	}
	return errors.Join(errs...)
}

// RedactWebPushSubscriptionEndpoint returns a log-safe endpoint preserving
// only scheme, host, and the first path segment.
func RedactWebPushSubscriptionEndpoint(raw string) string {
	raw = strings.TrimSpace(raw)
	u, err := url.Parse(raw)
	if err != nil || u == nil || u.Scheme == "" || u.Host == "" {
		return "[redacted]"
	}
	first := ""
	for _, segment := range strings.Split(strings.Trim(u.EscapedPath(), "/"), "/") {
		if segment != "" {
			first = segment
			break
		}
	}
	if first == "" {
		return u.Scheme + "://" + u.Host + "/..."
	}
	return u.Scheme + "://" + u.Host + "/" + first + "/..."
}

// NormalizeWebPushOptions applies stable defaults and trims topic metadata.
func NormalizeWebPushOptions(opts WebPushOptions) WebPushOptions {
	ttl := opts.TTL
	if ttl == 0 {
		ttl = defaultWebPushTTL
	}
	urgency := opts.Urgency
	if urgency == "" {
		urgency = WebPushUrgencyNormal
	}
	return WebPushOptions{
		TTL:     ttl,
		Urgency: WebPushUrgency(strings.TrimSpace(string(urgency))),
		Topic:   strings.TrimSpace(opts.Topic),
	}
}

// ValidateWebPushOptions checks TTL, urgency, and topic header metadata.
func ValidateWebPushOptions(opts WebPushOptions) error {
	opts = NormalizeWebPushOptions(opts)
	var errs []error
	if opts.TTL < 0 {
		errs = append(errs, fmt.Errorf("%w: ttl must be non-negative", ErrInvalidWebPushOptions))
	}
	if opts.TTL%time.Second != 0 {
		errs = append(errs, fmt.Errorf("%w: ttl must resolve to whole seconds", ErrInvalidWebPushOptions))
	}
	switch opts.Urgency {
	case WebPushUrgencyVeryLow, WebPushUrgencyLow, WebPushUrgencyNormal, WebPushUrgencyHigh:
	default:
		errs = append(errs, fmt.Errorf("%w: unsupported urgency %q", ErrInvalidWebPushOptions, opts.Urgency))
	}
	if utf8.RuneCountInString(opts.Topic) > WebPushMaxTopicRunes {
		errs = append(errs, fmt.Errorf("%w: topic exceeds %d runes", ErrInvalidWebPushOptions, WebPushMaxTopicRunes))
	}
	for _, r := range opts.Topic {
		if r > unicode.MaxASCII || unicode.IsControl(r) || unicode.IsSpace(r) {
			errs = append(errs, fmt.Errorf("%w: topic must be visible ascii without spaces", ErrInvalidWebPushOptions))
			break
		}
	}
	return errors.Join(errs...)
}

// NormalizeWebPushMessage trims display payload metadata and copies mutable
// byte/map fields.
func NormalizeWebPushMessage(message WebPushMessage) WebPushMessage {
	return WebPushMessage{
		Title:          strings.TrimSpace(message.Title),
		Body:           strings.TrimSpace(message.Body),
		Data:           cloneNotificationPayload(message.Data),
		Payload:        append([]byte(nil), message.Payload...),
		IdempotencyKey: strings.TrimSpace(message.IdempotencyKey),
	}
}

// ValidateWebPushMessage checks dry-run payload size and shape.
func ValidateWebPushMessage(message WebPushMessage) error {
	_, _, err := buildWebPushPayload(NormalizeWebPushMessage(message))
	return err
}

// PlanWebPushPayload validates descriptors and returns provider-neutral request
// metadata for a future adapter. It never sends, encrypts, or signs a request.
func PlanWebPushPayload(
	vapid WebPushVAPIDMetadata,
	subscription WebPushSubscriptionDescriptor,
	message WebPushMessage,
	opts WebPushOptions,
) (WebPushPlan, error) {
	vapid = NormalizeWebPushVAPIDMetadata(vapid)
	subscription = NormalizeWebPushSubscriptionDescriptor(subscription)
	message = NormalizeWebPushMessage(message)
	opts = NormalizeWebPushOptions(opts)

	if err := errors.Join(
		ValidateWebPushVAPIDMetadata(vapid),
		ValidateWebPushSubscriptionDescriptor(subscription),
		ValidateWebPushOptions(opts),
	); err != nil {
		return WebPushPlan{}, err
	}

	payload, payloadJSON, err := buildWebPushPayload(message)
	if err != nil {
		return WebPushPlan{}, err
	}

	headers := map[string]string{
		"TTL":     fmt.Sprintf("%.0f", opts.TTL.Seconds()),
		"Urgency": string(opts.Urgency),
	}
	if opts.Topic != "" {
		headers["Topic"] = opts.Topic
	}

	return WebPushPlan{
		VAPID:              vapid,
		Subscription:       subscription,
		Options:            opts,
		Headers:            headers,
		Payload:            append([]byte(nil), payload...),
		PayloadJSON:        append([]byte(nil), payloadJSON...),
		PayloadBytes:       len(payload),
		RedactedEndpoint:   RedactWebPushSubscriptionEndpoint(subscription.Endpoint),
		RedactedPublicKey:  RedactWebPushVAPIDPublicKey(vapid.PublicKey),
		RedactedPrivateKey: RedactWebPushVAPIDPrivateKey(vapid.PrivateKey),
		IdempotencyKey:     message.IdempotencyKey,
	}, nil
}

func buildWebPushPayload(message WebPushMessage) ([]byte, []byte, error) {
	if len(message.Payload) > 0 {
		if len(message.Payload) > WebPushMaxPayloadBytes {
			return nil, nil, fmt.Errorf("%w: payload exceeds %d bytes", ErrInvalidWebPushPayload, WebPushMaxPayloadBytes)
		}
		return append([]byte(nil), message.Payload...), nil, nil
	}
	if message.Title == "" && message.Body == "" && len(message.Data) == 0 {
		return nil, nil, fmt.Errorf("%w: title, body, data, or payload required", ErrInvalidWebPushPayload)
	}

	payload := map[string]any{}
	if message.Title != "" {
		payload["title"] = message.Title
	}
	if message.Body != "" {
		payload["body"] = message.Body
	}
	if len(message.Data) > 0 {
		payload["data"] = cloneNotificationPayload(message.Data)
	}
	body, err := json.Marshal(payload)
	if err != nil {
		return nil, nil, fmt.Errorf("%w: marshal payload: %v", ErrInvalidWebPushPayload, err)
	}
	if len(body) > WebPushMaxPayloadBytes {
		return nil, nil, fmt.Errorf("%w: payload exceeds %d bytes", ErrInvalidWebPushPayload, WebPushMaxPayloadBytes)
	}
	return body, body, nil
}

func isWebPushEndpoint(raw string) bool {
	u, err := url.Parse(strings.TrimSpace(raw))
	return err == nil && u != nil && u.Scheme == "https" && u.Host != "" && u.User == nil && u.Fragment == ""
}

func decodeWebPushPublicKey(raw string) ([]byte, error) {
	key, err := decodeWebPushBase64URL(raw)
	if err != nil {
		return nil, fmt.Errorf("%w: public key must be base64url", ErrInvalidWebPushVAPID)
	}
	if len(key) != 65 {
		return nil, fmt.Errorf("%w: public key must be 65 bytes", ErrInvalidWebPushVAPID)
	}
	if _, err := ecdh.P256().NewPublicKey(key); err != nil {
		return nil, fmt.Errorf("%w: public key must be P-256 uncompressed point", ErrInvalidWebPushVAPID)
	}
	return key, nil
}

func decodeWebPushPrivateKey(raw string) ([]byte, error) {
	key, err := decodeWebPushBase64URL(raw)
	if err != nil {
		return nil, fmt.Errorf("%w: private key must be base64url", ErrInvalidWebPushVAPID)
	}
	if len(key) != 32 {
		return nil, fmt.Errorf("%w: private key must be 32 bytes", ErrInvalidWebPushVAPID)
	}
	if _, err := ecdh.P256().NewPrivateKey(key); err != nil {
		return nil, fmt.Errorf("%w: private key must be P-256 scalar", ErrInvalidWebPushVAPID)
	}
	return key, nil
}

func decodeWebPushBase64URL(raw string) ([]byte, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, errors.New("empty")
	}
	if decoded, err := base64.RawURLEncoding.DecodeString(raw); err == nil {
		return decoded, nil
	}
	return base64.URLEncoding.DecodeString(raw)
}
