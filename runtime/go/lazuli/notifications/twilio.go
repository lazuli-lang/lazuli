package notifications

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"unicode"
	"unicode/utf16"
)

// ChannelSMS is the SMS notification channel.
const ChannelSMS Channel = "sms"

const (
	// TwilioProviderName is the provider identifier used in descriptor metadata.
	TwilioProviderName = "twilio"
	// TwilioProviderDisplayName is the human-facing provider name.
	TwilioProviderDisplayName = "Twilio"
	// DefaultTwilioBaseURL is the Twilio REST API origin.
	DefaultTwilioBaseURL = "https://api.twilio.com"
	// TwilioMessagesPathTemplate is the Twilio Messages API path shape.
	TwilioMessagesPathTemplate = "/2010-04-01/Accounts/{AccountSid}/Messages.json"
)

var (
	// ErrTwilioSMSConfigInvalid means Twilio SMS config is malformed.
	ErrTwilioSMSConfigInvalid = errors.New("notifications: twilio sms config invalid")
	// ErrTwilioAccountSIDMissing means the Twilio account SID is absent.
	ErrTwilioAccountSIDMissing = errors.New("notifications: twilio account sid missing")
	// ErrTwilioAccountSIDInvalid means the Twilio account SID has an invalid shape.
	ErrTwilioAccountSIDInvalid = errors.New("notifications: twilio account sid invalid")
	// ErrTwilioAuthTokenMissing means the Twilio auth token is absent.
	ErrTwilioAuthTokenMissing = errors.New("notifications: twilio auth token missing")
	// ErrTwilioAuthTokenInvalid means the Twilio auth token contains unsafe characters.
	ErrTwilioAuthTokenInvalid = errors.New("notifications: twilio auth token invalid")
	// ErrTwilioFromNumberMissing means the configured sender number is absent.
	ErrTwilioFromNumberMissing = errors.New("notifications: twilio from number missing")
	// ErrTwilioPhoneNumberInvalid means a phone number is not normalized E.164.
	ErrTwilioPhoneNumberInvalid = errors.New("notifications: twilio e164 phone number invalid")
	// ErrTwilioMessageInvalid means an outbound SMS body is empty or too large.
	ErrTwilioMessageInvalid = errors.New("notifications: twilio sms message invalid")
	// ErrTwilioBaseURLInvalid means the configured provider base URL is malformed.
	ErrTwilioBaseURLInvalid = errors.New("notifications: twilio base url invalid")
	// ErrTwilioStatusCallbackURLInvalid means a status callback URL is malformed.
	ErrTwilioStatusCallbackURLInvalid = errors.New("notifications: twilio status callback url invalid")
)

// TwilioSMSDescriptor describes Twilio SMS adapter metadata for generated code,
// diagnostics, and deploy adapters. It makes no network calls.
type TwilioSMSDescriptor struct {
	ProviderName         string
	ProviderDisplayName  string
	Channel              Channel
	DefaultBaseURL       string
	MessagesPathTemplate string
}

// TwilioSMSProviderDescriptor returns the canonical Twilio SMS descriptor.
func TwilioSMSProviderDescriptor() TwilioSMSDescriptor {
	return TwilioSMSDescriptor{
		ProviderName:         TwilioProviderName,
		ProviderDisplayName:  TwilioProviderDisplayName,
		Channel:              ChannelSMS,
		DefaultBaseURL:       DefaultTwilioBaseURL,
		MessagesPathTemplate: TwilioMessagesPathTemplate,
	}
}

// TwilioSMSConfig is metadata required to plan Twilio SMS requests.
type TwilioSMSConfig struct {
	AccountSID string
	AuthToken  string
	FromNumber string
	BaseURL    string
}

// Validate checks config without contacting Twilio.
func (c TwilioSMSConfig) Validate() error {
	return ValidateTwilioSMSConfig(c)
}

// Redacted returns a copy suitable for logs and diagnostics.
func (c TwilioSMSConfig) Redacted() TwilioSMSConfig {
	c.AccountSID = RedactTwilioSecret(c.AccountSID)
	c.AuthToken = RedactTwilioSecret(c.AuthToken)
	c.BaseURL = RedactTwilioURL(c.BaseURL)
	return c
}

// ValidateTwilioSMSConfig checks required Twilio SMS config.
func ValidateTwilioSMSConfig(config TwilioSMSConfig) error {
	_, err := NormalizeTwilioSMSConfig(config)
	return err
}

// NormalizeTwilioSMSConfig trims config, applies defaults, and validates values.
func NormalizeTwilioSMSConfig(config TwilioSMSConfig) (TwilioSMSConfig, error) {
	accountSID, accountErr := NormalizeTwilioAccountSID(config.AccountSID)
	authToken, authErr := NormalizeTwilioAuthToken(config.AuthToken)
	fromNumber, fromErr := NormalizeE164PhoneNumber(config.FromNumber)
	baseURL, baseErr := NormalizeTwilioBaseURL(config.BaseURL)

	var errs []error
	if accountErr != nil {
		errs = append(errs, twilioConfigError(accountErr))
	}
	if authErr != nil {
		errs = append(errs, twilioConfigError(authErr))
	}
	if fromErr != nil {
		if errors.Is(fromErr, ErrTwilioPhoneNumberInvalid) && strings.TrimSpace(config.FromNumber) == "" {
			fromErr = ErrTwilioFromNumberMissing
		}
		errs = append(errs, twilioConfigError(fromErr))
	}
	if baseErr != nil {
		errs = append(errs, twilioConfigError(baseErr))
	}
	if err := errors.Join(errs...); err != nil {
		return TwilioSMSConfig{}, err
	}

	return TwilioSMSConfig{
		AccountSID: accountSID,
		AuthToken:  authToken,
		FromNumber: fromNumber,
		BaseURL:    baseURL,
	}, nil
}

// NormalizeTwilioAccountSID trims and validates Twilio's account SID shape.
func NormalizeTwilioAccountSID(accountSID string) (string, error) {
	accountSID = strings.TrimSpace(accountSID)
	if accountSID == "" {
		return "", ErrTwilioAccountSIDMissing
	}
	if len(accountSID) != 34 || !strings.HasPrefix(accountSID, "AC") || !allHex(accountSID[2:]) {
		return "", ErrTwilioAccountSIDInvalid
	}
	return accountSID, nil
}

// ValidateTwilioAccountSID checks Twilio's account SID shape.
func ValidateTwilioAccountSID(accountSID string) error {
	_, err := NormalizeTwilioAccountSID(accountSID)
	return err
}

// NormalizeTwilioAuthToken trims and validates a Twilio auth token for config use.
func NormalizeTwilioAuthToken(authToken string) (string, error) {
	authToken = strings.TrimSpace(authToken)
	if authToken == "" {
		return "", ErrTwilioAuthTokenMissing
	}
	if hasTwilioSpaceOrControl(authToken) {
		return "", ErrTwilioAuthTokenInvalid
	}
	return authToken, nil
}

// ValidateTwilioAuthToken checks a Twilio auth token for config use.
func ValidateTwilioAuthToken(authToken string) error {
	_, err := NormalizeTwilioAuthToken(authToken)
	return err
}

// NormalizeE164PhoneNumber trims and validates a phone number in E.164 form.
func NormalizeE164PhoneNumber(number string) (string, error) {
	number = strings.TrimSpace(number)
	if !IsE164PhoneNumber(number) {
		return "", ErrTwilioPhoneNumberInvalid
	}
	return number, nil
}

// ValidateE164PhoneNumber checks whether number is normalized E.164.
func ValidateE164PhoneNumber(number string) error {
	_, err := NormalizeE164PhoneNumber(number)
	return err
}

// IsE164PhoneNumber reports whether number is in +[country][subscriber] form.
func IsE164PhoneNumber(number string) bool {
	if len(number) < 3 || len(number) > 16 || number[0] != '+' {
		return false
	}
	if number[1] < '1' || number[1] > '9' {
		return false
	}
	for _, r := range number[2:] {
		if r < '0' || r > '9' {
			return false
		}
	}
	return true
}

// TwilioSMSMessage is the provider-neutral outbound SMS shape.
type TwilioSMSMessage struct {
	To                string
	Body              string
	StatusCallbackURL string
	Idempotency       IdempotencyKey
}

// TwilioSMSMessageSegments describes deterministic SMS encoding and segment use.
type TwilioSMSMessageSegments struct {
	Encoding string
	Units    int
	Segments int
}

// EstimateTwilioSMSMessageSegments estimates SMS body encoding and billing
// segments using GSM-7 and UCS-2 limits.
func EstimateTwilioSMSMessageSegments(body string) TwilioSMSMessageSegments {
	if body == "" {
		return TwilioSMSMessageSegments{}
	}
	units, gsm7 := gsm7Units(body)
	if gsm7 {
		return TwilioSMSMessageSegments{
			Encoding: "GSM-7",
			Units:    units,
			Segments: smsSegments(units, 160, 153),
		}
	}
	units = len(utf16.Encode([]rune(body)))
	return TwilioSMSMessageSegments{
		Encoding: "UCS-2",
		Units:    units,
		Segments: smsSegments(units, 70, 67),
	}
}

// TwilioSMSIdempotencyMetadata describes deterministic dedupe metadata for a
// planned SMS request. Key is never sent to Twilio by this helper.
type TwilioSMSIdempotencyMetadata struct {
	Provider      string
	Channel       Channel
	Notification  string
	Tenant        string
	Recipient     string
	MessageSHA256 string
	Key           IdempotencyKey
}

// TwilioSMSRequestPlan is a side-effect-free request plan. Adapters may turn it
// into an HTTP request, but this package intentionally does not.
type TwilioSMSRequestPlan struct {
	Provider          string
	Channel           Channel
	AccountSID        string
	From              string
	To                string
	Body              string
	BaseURL           string
	EndpointPath      string
	EndpointURL       string
	StatusCallbackURL string
	Segments          TwilioSMSMessageSegments
	Idempotency       TwilioSMSIdempotencyMetadata
	Form              map[string]string
	RedactedConfig    TwilioSMSConfig
}

// PlanTwilioSMSRequest normalizes config and message metadata for a future
// adapter without creating an HTTP request or contacting Twilio.
func PlanTwilioSMSRequest(config TwilioSMSConfig, message TwilioSMSMessage) (TwilioSMSRequestPlan, error) {
	normalizedConfig, configErr := NormalizeTwilioSMSConfig(config)
	to, toErr := NormalizeE164PhoneNumber(message.To)
	body := strings.TrimSpace(message.Body)
	callbackURL, callbackErr := NormalizeTwilioStatusCallbackURL(message.StatusCallbackURL)

	var errs []error
	if configErr != nil {
		errs = append(errs, configErr)
	}
	if toErr != nil {
		errs = append(errs, toErr)
	}
	if body == "" {
		errs = append(errs, ErrTwilioMessageInvalid)
	}
	segments := EstimateTwilioSMSMessageSegments(body)
	if segments.Segments > 1600 {
		errs = append(errs, fmt.Errorf("%w: estimated segments %d exceeds 1600", ErrTwilioMessageInvalid, segments.Segments))
	}
	if callbackErr != nil {
		errs = append(errs, callbackErr)
	}
	if err := errors.Join(errs...); err != nil {
		return TwilioSMSRequestPlan{}, err
	}

	path := strings.ReplaceAll(TwilioMessagesPathTemplate, "{AccountSid}", normalizedConfig.AccountSID)
	endpoint := strings.TrimRight(normalizedConfig.BaseURL, "/") + path
	form := map[string]string{
		"From": normalizedConfig.FromNumber,
		"To":   to,
		"Body": body,
	}
	if callbackURL != "" {
		form["StatusCallback"] = callbackURL
	}

	return TwilioSMSRequestPlan{
		Provider:          TwilioProviderName,
		Channel:           ChannelSMS,
		AccountSID:        normalizedConfig.AccountSID,
		From:              normalizedConfig.FromNumber,
		To:                to,
		Body:              body,
		BaseURL:           normalizedConfig.BaseURL,
		EndpointPath:      path,
		EndpointURL:       endpoint,
		StatusCallbackURL: callbackURL,
		Segments:          segments,
		Idempotency:       twilioSMSIdempotencyMetadata(message.Idempotency, to, body),
		Form:              form,
		RedactedConfig:    normalizedConfig.Redacted(),
	}, nil
}

// FormValues returns a sorted copy of the planned Twilio form fields.
func (p TwilioSMSRequestPlan) FormValues() []string {
	keys := make([]string, 0, len(p.Form))
	for key := range p.Form {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	values := make([]string, 0, len(keys))
	for _, key := range keys {
		values = append(values, key+"="+p.Form[key])
	}
	return values
}

// NormalizeTwilioBaseURL trims and validates a Twilio API base URL.
func NormalizeTwilioBaseURL(baseURL string) (string, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		baseURL = DefaultTwilioBaseURL
	}
	parsed, err := url.Parse(baseURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrTwilioBaseURLInvalid, err)
	}
	if !validTwilioURL(parsed) || parsed.RawQuery != "" {
		return "", ErrTwilioBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// ValidateTwilioBaseURL checks whether baseURL is an absolute http(s) URL.
func ValidateTwilioBaseURL(baseURL string) error {
	_, err := NormalizeTwilioBaseURL(baseURL)
	return err
}

// NormalizeTwilioStatusCallbackURL trims and validates an optional absolute
// http(s) callback URL. Empty values are valid.
func NormalizeTwilioStatusCallbackURL(callbackURL string) (string, error) {
	callbackURL = strings.TrimSpace(callbackURL)
	if callbackURL == "" {
		return "", nil
	}
	parsed, err := url.Parse(callbackURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrTwilioStatusCallbackURLInvalid, err)
	}
	if !validTwilioURL(parsed) {
		return "", ErrTwilioStatusCallbackURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	return parsed.String(), nil
}

// ValidateTwilioStatusCallbackURL checks whether callbackURL is empty or an
// absolute http(s) URL.
func ValidateTwilioStatusCallbackURL(callbackURL string) error {
	_, err := NormalizeTwilioStatusCallbackURL(callbackURL)
	return err
}

// RedactTwilioSecret keeps a short stable hint while removing credential value.
func RedactTwilioSecret(secret string) string {
	secret = strings.TrimSpace(secret)
	if secret == "" {
		return ""
	}
	if len(secret) <= 8 {
		return "<redacted>"
	}
	return secret[:4] + "..." + secret[len(secret)-4:]
}

// RedactTwilioURL removes userinfo and query parameters from diagnostic URLs.
func RedactTwilioURL(rawURL string) string {
	rawURL = strings.TrimSpace(rawURL)
	if rawURL == "" {
		return ""
	}
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return "<redacted-url>"
	}
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String()
}

func twilioSMSIdempotencyMetadata(key IdempotencyKey, recipient, body string) TwilioSMSIdempotencyMetadata {
	key.Notification = strings.TrimSpace(key.Notification)
	key.Tenant = strings.TrimSpace(key.Tenant)
	key.Key = strings.TrimSpace(key.Key)
	sum := sha256.Sum256([]byte(body))
	if key.Key == "" {
		key.Key = recipient + ":" + hex.EncodeToString(sum[:])
	}
	return TwilioSMSIdempotencyMetadata{
		Provider:      TwilioProviderName,
		Channel:       ChannelSMS,
		Notification:  key.Notification,
		Tenant:        key.Tenant,
		Recipient:     recipient,
		MessageSHA256: hex.EncodeToString(sum[:]),
		Key:           key,
	}
}

func smsSegments(units, singleLimit, multipartLimit int) int {
	if units <= 0 {
		return 0
	}
	if units <= singleLimit {
		return 1
	}
	return (units + multipartLimit - 1) / multipartLimit
}

func gsm7Units(s string) (int, bool) {
	units := 0
	for _, r := range s {
		switch {
		case strings.ContainsRune(gsm7BasicRunes, r):
			units++
		case strings.ContainsRune(gsm7ExtendedRunes, r):
			units += 2
		default:
			return 0, false
		}
	}
	return units, true
}

func allHex(s string) bool {
	for _, r := range s {
		if (r >= '0' && r <= '9') || (r >= 'a' && r <= 'f') || (r >= 'A' && r <= 'F') {
			continue
		}
		return false
	}
	return true
}

func hasTwilioSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func validTwilioURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func twilioConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrTwilioSMSConfigInvalid, err)
}

const gsm7BasicRunes = "@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞ" +
	" !\"#¤%&'()*+,-./0123456789:;<=>?" +
	"¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà"

const gsm7ExtendedRunes = "^{}\\[~]|€"
