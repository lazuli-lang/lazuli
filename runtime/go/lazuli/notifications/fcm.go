package notifications

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/mail"
	"net/url"
	"regexp"
	"sort"
	"strings"
	"time"
	"unicode"
)

const (
	// FCMProviderName is the provider identifier used in descriptor metadata.
	FCMProviderName = "firebase-cloud-messaging"
	// FCMProviderDisplayName is the human-facing provider name.
	FCMProviderDisplayName = "Firebase Cloud Messaging"
	// DefaultFCMBaseURL is the FCM HTTP v1 API origin.
	DefaultFCMBaseURL = "https://fcm.googleapis.com"
	// FCMMessagesPathTemplate is the FCM HTTP v1 send path shape.
	FCMMessagesPathTemplate = "/v1/projects/{ProjectID}/messages:send"
	// MaxFCMTTL is the maximum message TTL accepted by FCM.
	MaxFCMTTL = 28 * 24 * time.Hour
)

var (
	// ErrFCMConfigInvalid means FCM config is malformed.
	ErrFCMConfigInvalid = errors.New("notifications: fcm config invalid")
	// ErrFCMProjectIDMissing means the Firebase project ID is absent.
	ErrFCMProjectIDMissing = errors.New("notifications: fcm project id missing")
	// ErrFCMProjectIDInvalid means the Firebase project ID has an invalid shape.
	ErrFCMProjectIDInvalid = errors.New("notifications: fcm project id invalid")
	// ErrFCMServiceAccountEmailMissing means service-account email metadata is absent.
	ErrFCMServiceAccountEmailMissing = errors.New("notifications: fcm service account email missing")
	// ErrFCMServiceAccountEmailInvalid means service-account email metadata is malformed.
	ErrFCMServiceAccountEmailInvalid = errors.New("notifications: fcm service account email invalid")
	// ErrFCMPrivateKeyMissing means service-account private key metadata is absent.
	ErrFCMPrivateKeyMissing = errors.New("notifications: fcm private key missing")
	// ErrFCMPrivateKeyInvalid means service-account private key metadata is malformed.
	ErrFCMPrivateKeyInvalid = errors.New("notifications: fcm private key invalid")
	// ErrFCMBaseURLInvalid means the configured provider base URL is malformed.
	ErrFCMBaseURLInvalid = errors.New("notifications: fcm base url invalid")
	// ErrFCMTargetInvalid means a push target is missing or ambiguous.
	ErrFCMTargetInvalid = errors.New("notifications: fcm target invalid")
	// ErrFCMMessageInvalid means a planned FCM message has no deliverable content.
	ErrFCMMessageInvalid = errors.New("notifications: fcm message invalid")
	// ErrFCMTTLInvalid means a TTL is outside FCM's accepted range.
	ErrFCMTTLInvalid = errors.New("notifications: fcm ttl invalid")
	// ErrFCMOptionsInvalid means platform option metadata is malformed.
	ErrFCMOptionsInvalid = errors.New("notifications: fcm options invalid")
)

// FCMDescriptor describes FCM adapter metadata for generated code,
// diagnostics, and deploy adapters. It makes no network calls.
type FCMDescriptor struct {
	ProviderName         string
	ProviderDisplayName  string
	Channel              Channel
	DefaultBaseURL       string
	MessagesPathTemplate string
	MaxTTL               time.Duration
}

// FCMProviderDescriptor returns the canonical FCM descriptor.
func FCMProviderDescriptor() FCMDescriptor {
	return FCMDescriptor{
		ProviderName:         FCMProviderName,
		ProviderDisplayName:  FCMProviderDisplayName,
		Channel:              ChannelPush,
		DefaultBaseURL:       DefaultFCMBaseURL,
		MessagesPathTemplate: FCMMessagesPathTemplate,
		MaxTTL:               MaxFCMTTL,
	}
}

// FCMConfig is service-account metadata required to plan FCM requests.
type FCMConfig struct {
	ProjectID           string
	ServiceAccountEmail string
	PrivateKey          string
	BaseURL             string
}

// Validate checks config without contacting Firebase.
func (c FCMConfig) Validate() error {
	return ValidateFCMConfig(c)
}

// Redacted returns a copy suitable for logs and diagnostics.
func (c FCMConfig) Redacted() FCMConfig {
	c.ServiceAccountEmail = RedactFCMServiceAccountEmail(c.ServiceAccountEmail)
	c.PrivateKey = RedactFCMSecret(c.PrivateKey)
	c.BaseURL = RedactFCMURL(c.BaseURL)
	return c
}

// ValidateFCMConfig checks required FCM config.
func ValidateFCMConfig(config FCMConfig) error {
	_, err := NormalizeFCMConfig(config)
	return err
}

// NormalizeFCMConfig trims config, applies defaults, and validates values.
func NormalizeFCMConfig(config FCMConfig) (FCMConfig, error) {
	projectID, projectErr := NormalizeFCMProjectID(config.ProjectID)
	email, emailErr := NormalizeFCMServiceAccountEmail(config.ServiceAccountEmail)
	privateKey, keyErr := NormalizeFCMPrivateKey(config.PrivateKey)
	baseURL, baseErr := NormalizeFCMBaseURL(config.BaseURL)

	var errs []error
	if projectErr != nil {
		errs = append(errs, fcmConfigError(projectErr))
	}
	if emailErr != nil {
		errs = append(errs, fcmConfigError(emailErr))
	}
	if keyErr != nil {
		errs = append(errs, fcmConfigError(keyErr))
	}
	if baseErr != nil {
		errs = append(errs, fcmConfigError(baseErr))
	}
	if err := errors.Join(errs...); err != nil {
		return FCMConfig{}, err
	}

	return FCMConfig{
		ProjectID:           projectID,
		ServiceAccountEmail: email,
		PrivateKey:          privateKey,
		BaseURL:             baseURL,
	}, nil
}

// NormalizeFCMProjectID trims and validates Firebase/GCP project ID metadata.
func NormalizeFCMProjectID(projectID string) (string, error) {
	projectID = strings.ToLower(strings.TrimSpace(projectID))
	if projectID == "" {
		return "", ErrFCMProjectIDMissing
	}
	if !fcmProjectIDPattern.MatchString(projectID) {
		return "", ErrFCMProjectIDInvalid
	}
	return projectID, nil
}

// ValidateFCMProjectID checks Firebase/GCP project ID metadata.
func ValidateFCMProjectID(projectID string) error {
	_, err := NormalizeFCMProjectID(projectID)
	return err
}

// NormalizeFCMServiceAccountEmail trims and validates service-account email metadata.
func NormalizeFCMServiceAccountEmail(email string) (string, error) {
	email = strings.ToLower(strings.TrimSpace(email))
	if email == "" {
		return "", ErrFCMServiceAccountEmailMissing
	}
	address, err := mail.ParseAddress(email)
	if err != nil || address.Address != email || !strings.HasSuffix(email, ".gserviceaccount.com") {
		return "", ErrFCMServiceAccountEmailInvalid
	}
	return email, nil
}

// ValidateFCMServiceAccountEmail checks service-account email metadata.
func ValidateFCMServiceAccountEmail(email string) error {
	_, err := NormalizeFCMServiceAccountEmail(email)
	return err
}

// NormalizeFCMPrivateKey trims and validates service-account private key metadata.
func NormalizeFCMPrivateKey(privateKey string) (string, error) {
	privateKey = strings.TrimSpace(privateKey)
	if privateKey == "" {
		return "", ErrFCMPrivateKeyMissing
	}
	if hasFCMUnsafeControl(privateKey) ||
		!strings.HasPrefix(privateKey, "-----BEGIN ") ||
		!strings.Contains(privateKey, "PRIVATE KEY-----") ||
		!strings.Contains(privateKey, "-----END ") {
		return "", ErrFCMPrivateKeyInvalid
	}
	return privateKey, nil
}

// ValidateFCMPrivateKey checks service-account private key metadata.
func ValidateFCMPrivateKey(privateKey string) error {
	_, err := NormalizeFCMPrivateKey(privateKey)
	return err
}

// FCMTargetType is the normalized FCM target kind.
type FCMTargetType string

const (
	FCMTargetToken FCMTargetType = "token"
	FCMTargetTopic FCMTargetType = "topic"
)

// FCMTarget describes exactly one FCM destination.
type FCMTarget struct {
	Token string
	Topic string
}

// FCMPlannedTarget is the normalized target metadata used in request plans.
type FCMPlannedTarget struct {
	Type  FCMTargetType
	Value string
}

// NormalizeFCMTarget trims and validates a token or topic destination.
func NormalizeFCMTarget(target FCMTarget) (FCMPlannedTarget, error) {
	token := strings.TrimSpace(target.Token)
	topic := normalizeFCMTopic(target.Topic)
	if (token == "" && topic == "") || (token != "" && topic != "") {
		return FCMPlannedTarget{}, ErrFCMTargetInvalid
	}
	if token != "" {
		if hasFCMSpaceOrControl(token) {
			return FCMPlannedTarget{}, ErrFCMTargetInvalid
		}
		return FCMPlannedTarget{Type: FCMTargetToken, Value: token}, nil
	}
	if !fcmTopicPattern.MatchString(topic) {
		return FCMPlannedTarget{}, ErrFCMTargetInvalid
	}
	return FCMPlannedTarget{Type: FCMTargetTopic, Value: topic}, nil
}

// ValidateFCMTarget checks whether target describes exactly one valid destination.
func ValidateFCMTarget(target FCMTarget) error {
	_, err := NormalizeFCMTarget(target)
	return err
}

// FCMAndroidOptions describes Android-specific FCM message metadata.
type FCMAndroidOptions struct {
	CollapseKey string
	Priority    string
	TTL         time.Duration
	ChannelID   string
}

// FCMAPNSOptions describes APNs-specific FCM message metadata.
type FCMAPNSOptions struct {
	Headers        map[string]string
	AnalyticsLabel string
}

// FCMWebPushOptions describes WebPush-specific FCM message metadata.
type FCMWebPushOptions struct {
	Headers        map[string]string
	Link           string
	AnalyticsLabel string
}

// FCMMessage is the provider-neutral outbound push shape used for planning.
type FCMMessage struct {
	Target      FCMTarget
	Title       string
	Body        string
	Data        map[string]string
	TTL         time.Duration
	Android     FCMAndroidOptions
	APNS        FCMAPNSOptions
	WebPush     FCMWebPushOptions
	Idempotency IdempotencyKey
}

// FCMIdempotencyMetadata describes deterministic dedupe metadata for a planned
// FCM request. Key is never sent to FCM by this helper.
type FCMIdempotencyMetadata struct {
	Provider      string
	Channel       Channel
	Notification  string
	Tenant        string
	TargetType    FCMTargetType
	Target        string
	MessageSHA256 string
	Key           IdempotencyKey
}

// FCMPlannedMessage is the normalized message metadata used in a request plan.
type FCMPlannedMessage struct {
	Target      FCMPlannedTarget
	Title       string
	Body        string
	Data        map[string]string
	TTL         time.Duration
	TTLSeconds  int
	Android     FCMAndroidOptions
	APNS        FCMAPNSOptions
	WebPush     FCMWebPushOptions
	Idempotency FCMIdempotencyMetadata
}

// FCMRequestPlan is a side-effect-free request plan. Adapters may turn it into
// an HTTP request, but this package intentionally does not.
type FCMRequestPlan struct {
	Provider       string
	Channel        Channel
	ProjectID      string
	BaseURL        string
	EndpointPath   string
	EndpointURL    string
	Message        FCMPlannedMessage
	RedactedConfig FCMConfig
}

// PlanFCMRequest normalizes config and message metadata for a future adapter
// without creating an HTTP request or contacting Firebase.
func PlanFCMRequest(config FCMConfig, message FCMMessage) (FCMRequestPlan, error) {
	normalizedConfig, configErr := NormalizeFCMConfig(config)
	target, targetErr := NormalizeFCMTarget(message.Target)
	ttl, ttlErr := NormalizeFCMTTL(message.TTL)
	android, androidErr := NormalizeFCMAndroidOptions(message.Android)
	apns, apnsErr := NormalizeFCMAPNSOptions(message.APNS)
	webpush, webpushErr := NormalizeFCMWebPushOptions(message.WebPush)
	title := strings.TrimSpace(message.Title)
	body := strings.TrimSpace(message.Body)
	data := normalizeFCMStringMap(message.Data)

	var errs []error
	for _, err := range []error{configErr, targetErr, ttlErr, androidErr, apnsErr, webpushErr} {
		if err != nil {
			errs = append(errs, err)
		}
	}
	if title == "" && body == "" && len(data) == 0 {
		errs = append(errs, ErrFCMMessageInvalid)
	}
	if err := errors.Join(errs...); err != nil {
		return FCMRequestPlan{}, err
	}

	path := strings.ReplaceAll(FCMMessagesPathTemplate, "{ProjectID}", normalizedConfig.ProjectID)
	endpoint := strings.TrimRight(normalizedConfig.BaseURL, "/") + path
	plannedMessage := FCMPlannedMessage{
		Target:     target,
		Title:      title,
		Body:       body,
		Data:       data,
		TTL:        ttl,
		TTLSeconds: int(ttl / time.Second),
		Android:    android,
		APNS:       apns,
		WebPush:    webpush,
	}
	plannedMessage.Idempotency = fcmIdempotencyMetadata(message.Idempotency, plannedMessage)

	return FCMRequestPlan{
		Provider:       FCMProviderName,
		Channel:        ChannelPush,
		ProjectID:      normalizedConfig.ProjectID,
		BaseURL:        normalizedConfig.BaseURL,
		EndpointPath:   path,
		EndpointURL:    endpoint,
		Message:        plannedMessage,
		RedactedConfig: normalizedConfig.Redacted(),
	}, nil
}

// RequestBody returns a provider-shaped metadata map suitable for deterministic
// JSON encoding by adapters. It does not include auth headers or credentials.
func (p FCMRequestPlan) RequestBody() map[string]any {
	message := map[string]any{
		string(p.Message.Target.Type): p.Message.Target.Value,
	}
	if p.Message.Title != "" || p.Message.Body != "" {
		notification := map[string]any{}
		if p.Message.Title != "" {
			notification["title"] = p.Message.Title
		}
		if p.Message.Body != "" {
			notification["body"] = p.Message.Body
		}
		message["notification"] = notification
	}
	if len(p.Message.Data) > 0 {
		message["data"] = cloneFCMStringMap(p.Message.Data)
	}
	if p.Message.TTL > 0 {
		message["ttl"] = formatFCMDuration(p.Message.TTL)
	}
	if android := p.androidBody(); len(android) > 0 {
		message["android"] = android
	}
	if apns := p.apnsBody(); len(apns) > 0 {
		message["apns"] = apns
	}
	if webpush := p.webpushBody(); len(webpush) > 0 {
		message["webpush"] = webpush
	}
	return map[string]any{"message": message}
}

// DataValues returns a sorted copy of the planned data payload.
func (m FCMPlannedMessage) DataValues() []string {
	return fcmMapValues(m.Data)
}

// APNSHeaderValues returns a sorted copy of the planned APNs headers.
func (m FCMPlannedMessage) APNSHeaderValues() []string {
	return fcmMapValues(m.APNS.Headers)
}

// WebPushHeaderValues returns a sorted copy of the planned WebPush headers.
func (m FCMPlannedMessage) WebPushHeaderValues() []string {
	return fcmMapValues(m.WebPush.Headers)
}

// NormalizeFCMTTL validates and rounds TTL to whole seconds. Empty TTL is valid.
func NormalizeFCMTTL(ttl time.Duration) (time.Duration, error) {
	if ttl < 0 || ttl > MaxFCMTTL {
		return 0, ErrFCMTTLInvalid
	}
	return ttl.Truncate(time.Second), nil
}

// ValidateFCMTTL checks FCM TTL bounds.
func ValidateFCMTTL(ttl time.Duration) error {
	_, err := NormalizeFCMTTL(ttl)
	return err
}

// NormalizeFCMAndroidOptions trims Android-specific metadata.
func NormalizeFCMAndroidOptions(options FCMAndroidOptions) (FCMAndroidOptions, error) {
	options.CollapseKey = strings.TrimSpace(options.CollapseKey)
	options.Priority = strings.ToLower(strings.TrimSpace(options.Priority))
	options.ChannelID = strings.TrimSpace(options.ChannelID)
	ttl, ttlErr := NormalizeFCMTTL(options.TTL)
	if ttlErr != nil {
		return FCMAndroidOptions{}, fmt.Errorf("%w: android ttl: %w", ErrFCMOptionsInvalid, ttlErr)
	}
	options.TTL = ttl
	switch options.Priority {
	case "", "normal", "high":
		return options, nil
	default:
		return FCMAndroidOptions{}, fmt.Errorf("%w: android priority %q", ErrFCMOptionsInvalid, options.Priority)
	}
}

// NormalizeFCMAPNSOptions trims APNs-specific metadata.
func NormalizeFCMAPNSOptions(options FCMAPNSOptions) (FCMAPNSOptions, error) {
	headers, err := normalizeFCMHeaders(options.Headers)
	if err != nil {
		return FCMAPNSOptions{}, fmt.Errorf("%w: apns headers: %w", ErrFCMOptionsInvalid, err)
	}
	options.Headers = headers
	options.AnalyticsLabel = strings.TrimSpace(options.AnalyticsLabel)
	return options, nil
}

// NormalizeFCMWebPushOptions trims WebPush-specific metadata.
func NormalizeFCMWebPushOptions(options FCMWebPushOptions) (FCMWebPushOptions, error) {
	headers, err := normalizeFCMHeaders(options.Headers)
	if err != nil {
		return FCMWebPushOptions{}, fmt.Errorf("%w: webpush headers: %w", ErrFCMOptionsInvalid, err)
	}
	link, linkErr := normalizeFCMOptionalURL(options.Link)
	if linkErr != nil {
		return FCMWebPushOptions{}, fmt.Errorf("%w: webpush link: %w", ErrFCMOptionsInvalid, linkErr)
	}
	options.Headers = headers
	options.Link = link
	options.AnalyticsLabel = strings.TrimSpace(options.AnalyticsLabel)
	return options, nil
}

// NormalizeFCMBaseURL trims and validates an FCM API base URL.
func NormalizeFCMBaseURL(baseURL string) (string, error) {
	baseURL = strings.TrimSpace(baseURL)
	if baseURL == "" {
		baseURL = DefaultFCMBaseURL
	}
	parsed, err := url.Parse(baseURL)
	if err != nil {
		return "", fmt.Errorf("%w: %v", ErrFCMBaseURLInvalid, err)
	}
	if !validFCMURL(parsed) || parsed.RawQuery != "" {
		return "", ErrFCMBaseURLInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	return parsed.String(), nil
}

// ValidateFCMBaseURL checks whether baseURL is an absolute http(s) URL.
func ValidateFCMBaseURL(baseURL string) error {
	_, err := NormalizeFCMBaseURL(baseURL)
	return err
}

// RedactFCMSecret keeps a short stable hint while removing credential value.
func RedactFCMSecret(secret string) string {
	secret = strings.TrimSpace(secret)
	if secret == "" {
		return ""
	}
	sum := sha256.Sum256([]byte(secret))
	return "<redacted:" + hex.EncodeToString(sum[:4]) + ">"
}

// RedactFCMServiceAccountEmail removes the local-part while preserving domain metadata.
func RedactFCMServiceAccountEmail(email string) string {
	email = strings.TrimSpace(email)
	if email == "" {
		return ""
	}
	parts := strings.Split(email, "@")
	if len(parts) != 2 || parts[1] == "" {
		return "<redacted-email>"
	}
	return "<redacted>@" + parts[1]
}

// RedactFCMURL removes userinfo, query parameters, and fragments from diagnostic URLs.
func RedactFCMURL(rawURL string) string {
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

func (p FCMRequestPlan) androidBody() map[string]any {
	options := p.Message.Android
	body := map[string]any{}
	if options.CollapseKey != "" {
		body["collapse_key"] = options.CollapseKey
	}
	if options.Priority != "" {
		body["priority"] = options.Priority
	}
	if options.TTL > 0 {
		body["ttl"] = formatFCMDuration(options.TTL)
	}
	if options.ChannelID != "" {
		body["notification"] = map[string]any{"channel_id": options.ChannelID}
	}
	return body
}

func (p FCMRequestPlan) apnsBody() map[string]any {
	options := p.Message.APNS
	body := map[string]any{}
	if len(options.Headers) > 0 {
		body["headers"] = cloneFCMStringMap(options.Headers)
	}
	if options.AnalyticsLabel != "" {
		body["fcm_options"] = map[string]any{"analytics_label": options.AnalyticsLabel}
	}
	return body
}

func (p FCMRequestPlan) webpushBody() map[string]any {
	options := p.Message.WebPush
	body := map[string]any{}
	if len(options.Headers) > 0 {
		body["headers"] = cloneFCMStringMap(options.Headers)
	}
	if options.Link != "" || options.AnalyticsLabel != "" {
		fcmOptions := map[string]any{}
		if options.Link != "" {
			fcmOptions["link"] = options.Link
		}
		if options.AnalyticsLabel != "" {
			fcmOptions["analytics_label"] = options.AnalyticsLabel
		}
		body["fcm_options"] = fcmOptions
	}
	return body
}

func fcmConfigError(err error) error {
	return fmt.Errorf("%w: %w", ErrFCMConfigInvalid, err)
}

func normalizeFCMTopic(topic string) string {
	topic = strings.TrimSpace(topic)
	topic = strings.TrimPrefix(topic, "/topics/")
	return strings.TrimPrefix(topic, "topics/")
}

func normalizeFCMStringMap(values map[string]string) map[string]string {
	normalized := make(map[string]string, len(values))
	for key, value := range values {
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key == "" || value == "" {
			continue
		}
		normalized[key] = value
	}
	if len(normalized) == 0 {
		return nil
	}
	return normalized
}

func normalizeFCMHeaders(values map[string]string) (map[string]string, error) {
	normalized := make(map[string]string, len(values))
	for key, value := range values {
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key == "" && value == "" {
			continue
		}
		if key == "" || value == "" || hasFCMSpaceOrControl(key) || hasFCMUnsafeControl(value) {
			return nil, ErrFCMOptionsInvalid
		}
		normalized[key] = value
	}
	if len(normalized) == 0 {
		return nil, nil
	}
	return normalized, nil
}

func normalizeFCMOptionalURL(rawURL string) (string, error) {
	rawURL = strings.TrimSpace(rawURL)
	if rawURL == "" {
		return "", nil
	}
	parsed, err := url.Parse(rawURL)
	if err != nil || !validFCMURL(parsed) {
		return "", ErrFCMOptionsInvalid
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	return parsed.String(), nil
}

func cloneFCMStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	cloned := make(map[string]string, len(values))
	for key, value := range values {
		cloned[key] = value
	}
	return cloned
}

func fcmMapValues(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	result := make([]string, 0, len(keys))
	for _, key := range keys {
		result = append(result, key+"="+values[key])
	}
	return result
}

func fcmIdempotencyMetadata(key IdempotencyKey, message FCMPlannedMessage) FCMIdempotencyMetadata {
	key.Notification = strings.TrimSpace(key.Notification)
	key.Tenant = strings.TrimSpace(key.Tenant)
	key.Key = strings.TrimSpace(key.Key)
	sum := sha256.Sum256([]byte(message.Target.Value + "\n" + message.Title + "\n" + message.Body + "\n" + strings.Join(message.DataValues(), "\n")))
	if key.Key == "" {
		key.Key = string(message.Target.Type) + ":" + message.Target.Value + ":" + hex.EncodeToString(sum[:])
	}
	return FCMIdempotencyMetadata{
		Provider:      FCMProviderName,
		Channel:       ChannelPush,
		Notification:  key.Notification,
		Tenant:        key.Tenant,
		TargetType:    message.Target.Type,
		Target:        message.Target.Value,
		MessageSHA256: hex.EncodeToString(sum[:]),
		Key:           key,
	}
}

func hasFCMSpaceOrControl(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func hasFCMUnsafeControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) && r != '\n' && r != '\r' && r != '\t' {
			return true
		}
	}
	return false
}

func validFCMURL(parsed *url.URL) bool {
	if parsed == nil {
		return false
	}
	scheme := strings.ToLower(parsed.Scheme)
	return (scheme == "http" || scheme == "https") &&
		parsed.Host != "" &&
		parsed.User == nil &&
		parsed.Fragment == ""
}

func formatFCMDuration(ttl time.Duration) string {
	return fmt.Sprintf("%ds", int(ttl/time.Second))
}

var (
	fcmProjectIDPattern = regexp.MustCompile(`^[a-z][a-z0-9-]{4,28}[a-z0-9]$`)
	fcmTopicPattern     = regexp.MustCompile(`^[A-Za-z0-9\-_.~%]{1,900}$`)
)
