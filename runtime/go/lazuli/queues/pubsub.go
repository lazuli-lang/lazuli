package queues

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"path"
	"sort"
	"strings"
	"time"
	"unicode"
)

var (
	// ErrGooglePubSubDescriptorInvalid reports invalid Google Pub/Sub queue descriptor metadata.
	ErrGooglePubSubDescriptorInvalid = errors.New("lazuli/queues: google_pubsub_descriptor_invalid")
)

const (
	GooglePubSubMinAckDeadline = 10 * time.Second
	GooglePubSubMaxAckDeadline = 10 * time.Minute

	DefaultGooglePubSubMaxDeliveryAttempts = 5
	MinGooglePubSubMaxDeliveryAttempts     = 5
	MaxGooglePubSubMaxDeliveryAttempts     = 100
)

// GooglePubSubDescriptor is provider-neutral metadata for a Google Pub/Sub
// topic/subscription pair. It does not depend on the Google Cloud SDK.
type GooglePubSubDescriptor struct {
	ProjectID             string
	Topic                 string
	Subscription          string
	AckDeadline           time.Duration
	EnableMessageOrdering bool
	OrderingKey           string
	DeadLetter            GooglePubSubDeadLetterPolicy
	EmulatorEndpoint      string
	Labels                map[string]string
}

// GooglePubSubDeadLetterPolicy describes dead-letter delivery metadata.
type GooglePubSubDeadLetterPolicy struct {
	Topic               string
	MaxDeliveryAttempts int
}

// GooglePubSubPlan is a normalized, side-effect-free descriptor plan.
type GooglePubSubPlan struct {
	Descriptor       GooglePubSubDescriptor
	TopicPath        string
	SubscriptionPath string
	DeadLetterPath   string
	Summary          GooglePubSubSummary
}

// GooglePubSubSummary is deterministic metadata suitable for logs.
type GooglePubSubSummary struct {
	ProjectID             string
	Topic                 string
	Subscription          string
	TopicPath             string
	SubscriptionPath      string
	AckDeadlineSeconds    int
	EnableMessageOrdering bool
	OrderingKey           string
	DeadLetterTopic       string
	DeadLetterPath        string
	MaxDeliveryAttempts   int
	EmulatorEndpoint      string
	Labels                map[string]string
}

// Normalize returns a copy with trimmed resource names, inferred project id,
// normalized duration fields, and deterministic labels.
func (d GooglePubSubDescriptor) Normalize() GooglePubSubDescriptor {
	return NormalizeGooglePubSubDescriptor(d)
}

// Validate checks whether the descriptor can be used by a future Pub/Sub adapter.
func (d GooglePubSubDescriptor) Validate() error {
	return ValidateGooglePubSubDescriptor(d)
}

// Summary returns deterministic descriptor metadata suitable for logs.
func (d GooglePubSubDescriptor) Summary() GooglePubSubSummary {
	plan, err := PlanGooglePubSubDescriptor(d)
	if err != nil {
		normalized := NormalizeGooglePubSubDescriptor(d)
		return googlePubSubSummary(normalized)
	}
	return plan.Summary
}

// NormalizeGooglePubSubDescriptor returns Pub/Sub metadata in deterministic form.
func NormalizeGooglePubSubDescriptor(d GooglePubSubDescriptor) GooglePubSubDescriptor {
	d.ProjectID = strings.TrimSpace(d.ProjectID)
	d.Topic = strings.TrimSpace(d.Topic)
	d.Subscription = strings.TrimSpace(d.Subscription)
	d.OrderingKey = strings.TrimSpace(d.OrderingKey)
	d.DeadLetter = NormalizeGooglePubSubDeadLetterPolicy(d.DeadLetter)
	d.EmulatorEndpoint = normalizeGooglePubSubEmulatorEndpoint(d.EmulatorEndpoint)
	d.Labels = normalizeGooglePubSubLabels(d.Labels)

	topicProject, topic := splitGooglePubSubResource(d.Topic, "topics")
	subProject, subscription := splitGooglePubSubResource(d.Subscription, "subscriptions")
	deadLetterProject, deadLetterTopic := splitGooglePubSubResource(d.DeadLetter.Topic, "topics")

	if d.ProjectID == "" {
		d.ProjectID = firstNonEmpty(topicProject, subProject, deadLetterProject)
	}
	if topic != "" {
		d.Topic = topic
	}
	if subscription != "" {
		d.Subscription = subscription
	}
	if deadLetterTopic != "" {
		d.DeadLetter.Topic = deadLetterTopic
	}
	if d.DeadLetter.Topic != "" && d.DeadLetter.MaxDeliveryAttempts == 0 {
		d.DeadLetter.MaxDeliveryAttempts = DefaultGooglePubSubMaxDeliveryAttempts
	}
	return d
}

// NormalizeGooglePubSubDeadLetterPolicy trims dead-letter metadata.
func NormalizeGooglePubSubDeadLetterPolicy(policy GooglePubSubDeadLetterPolicy) GooglePubSubDeadLetterPolicy {
	policy.Topic = strings.TrimSpace(policy.Topic)
	return policy
}

// ValidateGooglePubSubDescriptor checks project, topic, subscription,
// ack-deadline, ordering, dead-letter, emulator, and label metadata.
func ValidateGooglePubSubDescriptor(d GooglePubSubDescriptor) error {
	raw := d
	d = NormalizeGooglePubSubDescriptor(d)

	var errs []error
	if d.ProjectID == "" {
		errs = append(errs, fmt.Errorf("%w: project id is required", ErrGooglePubSubDescriptorInvalid))
	} else if !validGooglePubSubProjectID(d.ProjectID) {
		errs = append(errs, fmt.Errorf("%w: project id %q is invalid", ErrGooglePubSubDescriptorInvalid, d.ProjectID))
	}
	if d.Topic == "" {
		errs = append(errs, fmt.Errorf("%w: topic is required", ErrGooglePubSubDescriptorInvalid))
	} else if !validGooglePubSubName(d.Topic) {
		errs = append(errs, fmt.Errorf("%w: topic %q is invalid", ErrGooglePubSubDescriptorInvalid, d.Topic))
	}
	if d.Subscription == "" {
		errs = append(errs, fmt.Errorf("%w: subscription is required", ErrGooglePubSubDescriptorInvalid))
	} else if !validGooglePubSubName(d.Subscription) {
		errs = append(errs, fmt.Errorf("%w: subscription %q is invalid", ErrGooglePubSubDescriptorInvalid, d.Subscription))
	}
	if err := ValidateGooglePubSubAckDeadline(d.AckDeadline); err != nil {
		errs = append(errs, err)
	}
	if !d.EnableMessageOrdering && d.OrderingKey != "" {
		errs = append(errs, fmt.Errorf("%w: ordering key requires message ordering", ErrGooglePubSubDescriptorInvalid))
	}
	if d.OrderingKey != "" && strings.ContainsAny(d.OrderingKey, "\r\n") {
		errs = append(errs, fmt.Errorf("%w: ordering key must not contain newlines", ErrGooglePubSubDescriptorInvalid))
	}
	if err := ValidateGooglePubSubDeadLetterPolicy(d.DeadLetter); err != nil {
		errs = append(errs, err)
	}
	if d.EmulatorEndpoint != "" {
		if err := ValidateGooglePubSubEmulatorEndpoint(d.EmulatorEndpoint); err != nil {
			errs = append(errs, err)
		}
	}
	if err := validateGooglePubSubLabelProjects(raw, d.ProjectID); err != nil {
		errs = append(errs, err)
	}
	if err := validateGooglePubSubLabels(d.Labels); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

// ValidateGooglePubSubDeadLetterPolicy checks dead-letter topic metadata.
func ValidateGooglePubSubDeadLetterPolicy(policy GooglePubSubDeadLetterPolicy) error {
	policy = NormalizeGooglePubSubDeadLetterPolicy(policy)
	if policy.Topic == "" {
		if policy.MaxDeliveryAttempts != 0 {
			return fmt.Errorf("%w: dead-letter max delivery attempts requires topic", ErrGooglePubSubDescriptorInvalid)
		}
		return nil
	}

	var errs []error
	_, topic := splitGooglePubSubResource(policy.Topic, "topics")
	if topic != "" {
		policy.Topic = topic
	}
	if !validGooglePubSubName(policy.Topic) {
		errs = append(errs, fmt.Errorf("%w: dead-letter topic %q is invalid", ErrGooglePubSubDescriptorInvalid, policy.Topic))
	}
	if policy.MaxDeliveryAttempts < MinGooglePubSubMaxDeliveryAttempts || policy.MaxDeliveryAttempts > MaxGooglePubSubMaxDeliveryAttempts {
		errs = append(errs, fmt.Errorf("%w: dead-letter max delivery attempts must be between %d and %d", ErrGooglePubSubDescriptorInvalid, MinGooglePubSubMaxDeliveryAttempts, MaxGooglePubSubMaxDeliveryAttempts))
	}
	return errors.Join(errs...)
}

// ValidateGooglePubSubAckDeadline checks Google Pub/Sub ack deadline bounds.
func ValidateGooglePubSubAckDeadline(deadline time.Duration) error {
	if deadline < GooglePubSubMinAckDeadline || deadline > GooglePubSubMaxAckDeadline {
		return fmt.Errorf("%w: ack deadline must be between %s and %s", ErrGooglePubSubDescriptorInvalid, GooglePubSubMinAckDeadline, GooglePubSubMaxAckDeadline)
	}
	if deadline%time.Second != 0 {
		return fmt.Errorf("%w: ack deadline must be whole seconds", ErrGooglePubSubDescriptorInvalid)
	}
	return nil
}

// PlanGooglePubSubDescriptor normalizes and validates descriptor metadata
// without performing network I/O.
func PlanGooglePubSubDescriptor(d GooglePubSubDescriptor) (GooglePubSubPlan, error) {
	normalized := NormalizeGooglePubSubDescriptor(d)
	topicPath := googlePubSubResourcePath(normalized.ProjectID, "topics", normalized.Topic)
	subscriptionPath := googlePubSubResourcePath(normalized.ProjectID, "subscriptions", normalized.Subscription)
	deadLetterPath := googlePubSubResourcePath(normalized.ProjectID, "topics", normalized.DeadLetter.Topic)
	plan := GooglePubSubPlan{
		Descriptor:       cloneGooglePubSubDescriptor(normalized),
		TopicPath:        topicPath,
		SubscriptionPath: subscriptionPath,
		DeadLetterPath:   deadLetterPath,
		Summary:          googlePubSubSummary(normalized),
	}
	if err := ValidateGooglePubSubDescriptor(normalized); err != nil {
		return plan, err
	}
	return plan, nil
}

// ValidateGooglePubSubEmulatorEndpoint checks local emulator endpoint metadata.
func ValidateGooglePubSubEmulatorEndpoint(endpoint string) error {
	endpoint = normalizeGooglePubSubEmulatorEndpoint(endpoint)
	if endpoint == "" {
		return nil
	}
	parsed, err := parseGooglePubSubEndpoint(endpoint)
	if err != nil {
		return err
	}
	if parsed.User != nil {
		return fmt.Errorf("%w: emulator endpoint must not include credentials", ErrGooglePubSubDescriptorInvalid)
	}
	if parsed.RawQuery != "" || parsed.Fragment != "" {
		return fmt.Errorf("%w: emulator endpoint must not include query or fragment", ErrGooglePubSubDescriptorInvalid)
	}
	if parsed.Hostname() == "" {
		return fmt.Errorf("%w: emulator endpoint host is required", ErrGooglePubSubDescriptorInvalid)
	}
	if port := parsed.Port(); port != "" {
		if _, err := net.LookupPort("tcp", port); err != nil {
			return fmt.Errorf("%w: emulator endpoint port %q is invalid", ErrGooglePubSubDescriptorInvalid, port)
		}
	}
	return nil
}

// RedactGooglePubSubEmulatorEndpoint removes credentials, query, and fragment
// values from emulator endpoints before they are logged.
func RedactGooglePubSubEmulatorEndpoint(endpoint string) string {
	endpoint = strings.TrimSpace(endpoint)
	parsed, err := parseGooglePubSubEndpoint(endpoint)
	if err != nil {
		return endpoint
	}
	if parsed.User != nil {
		parsed.User = url.UserPassword("[REDACTED]", "[REDACTED]")
	}
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String()
}

func googlePubSubSummary(d GooglePubSubDescriptor) GooglePubSubSummary {
	return GooglePubSubSummary{
		ProjectID:             d.ProjectID,
		Topic:                 d.Topic,
		Subscription:          d.Subscription,
		TopicPath:             googlePubSubResourcePath(d.ProjectID, "topics", d.Topic),
		SubscriptionPath:      googlePubSubResourcePath(d.ProjectID, "subscriptions", d.Subscription),
		AckDeadlineSeconds:    durationSeconds(d.AckDeadline),
		EnableMessageOrdering: d.EnableMessageOrdering,
		OrderingKey:           d.OrderingKey,
		DeadLetterTopic:       d.DeadLetter.Topic,
		DeadLetterPath:        googlePubSubResourcePath(d.ProjectID, "topics", d.DeadLetter.Topic),
		MaxDeliveryAttempts:   d.DeadLetter.MaxDeliveryAttempts,
		EmulatorEndpoint:      RedactGooglePubSubEmulatorEndpoint(d.EmulatorEndpoint),
		Labels:                cloneStringMap(d.Labels),
	}
}

func splitGooglePubSubResource(value, collection string) (string, string) {
	parts := strings.Split(strings.Trim(value, "/"), "/")
	if len(parts) != 4 || parts[0] != "projects" || parts[2] != collection {
		return "", ""
	}
	return strings.TrimSpace(parts[1]), strings.TrimSpace(parts[3])
}

func googlePubSubResourcePath(projectID, collection, name string) string {
	if projectID == "" || name == "" {
		return ""
	}
	return "projects/" + projectID + "/" + collection + "/" + name
}

func normalizeGooglePubSubEmulatorEndpoint(endpoint string) string {
	endpoint = strings.TrimSpace(endpoint)
	if endpoint == "" {
		return ""
	}
	parsed, err := parseGooglePubSubEndpoint(endpoint)
	if err != nil {
		return endpoint
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = path.Clean("/" + strings.TrimSpace(parsed.EscapedPath()))
	if parsed.Path == "/" {
		parsed.Path = ""
	}
	return parsed.String()
}

func parseGooglePubSubEndpoint(endpoint string) (*url.URL, error) {
	parsed, err := url.Parse(endpoint)
	if err != nil {
		return nil, fmt.Errorf("%w: emulator endpoint %q is invalid", ErrGooglePubSubDescriptorInvalid, endpoint)
	}
	if parsed.Scheme == "" || parsed.Host == "" && parsed.Opaque != "" {
		parsed, err = url.Parse("http://" + endpoint)
		if err != nil {
			return nil, fmt.Errorf("%w: emulator endpoint %q is invalid", ErrGooglePubSubDescriptorInvalid, endpoint)
		}
	}
	if parsed.Scheme != "http" && parsed.Scheme != "https" {
		return nil, fmt.Errorf("%w: emulator endpoint scheme %q is invalid", ErrGooglePubSubDescriptorInvalid, parsed.Scheme)
	}
	return parsed, nil
}

func validateGooglePubSubLabelProjects(raw GooglePubSubDescriptor, projectID string) error {
	var errs []error
	for label, value := range map[string]string{
		"topic":             raw.Topic,
		"subscription":      raw.Subscription,
		"dead-letter topic": raw.DeadLetter.Topic,
	} {
		resourceProject, _ := splitGooglePubSubResource(strings.TrimSpace(value), collectionForGooglePubSubLabel(label))
		if resourceProject != "" && projectID != "" && resourceProject != projectID {
			errs = append(errs, fmt.Errorf("%w: %s project %q does not match project id %q", ErrGooglePubSubDescriptorInvalid, label, resourceProject, projectID))
		}
	}
	return errors.Join(errs...)
}

func collectionForGooglePubSubLabel(label string) string {
	if label == "subscription" {
		return "subscriptions"
	}
	return "topics"
}

func normalizeGooglePubSubLabels(labels map[string]string) map[string]string {
	if len(labels) == 0 {
		return nil
	}
	normalized := make(map[string]string, len(labels))
	for key, value := range labels {
		key = strings.ToLower(strings.TrimSpace(key))
		value = strings.TrimSpace(value)
		if key == "" && value == "" {
			continue
		}
		normalized[key] = value
	}
	if len(normalized) == 0 {
		return nil
	}
	return normalized
}

func validateGooglePubSubLabels(labels map[string]string) error {
	var errs []error
	for key, value := range labels {
		if !validGooglePubSubLabelKey(key) {
			errs = append(errs, fmt.Errorf("%w: label key %q is invalid", ErrGooglePubSubDescriptorInvalid, key))
		}
		if len(value) > 63 {
			errs = append(errs, fmt.Errorf("%w: label value for %q is too long", ErrGooglePubSubDescriptorInvalid, key))
		}
	}
	return errors.Join(errs...)
}

func validGooglePubSubProjectID(projectID string) bool {
	if len(projectID) < 6 || len(projectID) > 30 {
		return false
	}
	if projectID[0] < 'a' || projectID[0] > 'z' {
		return false
	}
	last := projectID[len(projectID)-1]
	if last == '-' {
		return false
	}
	for _, r := range projectID {
		if r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-' {
			continue
		}
		return false
	}
	return true
}

func validGooglePubSubName(name string) bool {
	if len(name) < 3 || len(name) > 255 {
		return false
	}
	if strings.HasPrefix(name, "goog") {
		return false
	}
	first, _ := firstRune(name)
	if !unicode.IsLetter(first) {
		return false
	}
	for _, r := range name {
		if unicode.IsLetter(r) || unicode.IsDigit(r) || r == '-' || r == '_' || r == '.' || r == '~' || r == '+' || r == '%' {
			continue
		}
		return false
	}
	return true
}

func validGooglePubSubLabelKey(key string) bool {
	if len(key) < 1 || len(key) > 63 {
		return false
	}
	first, _ := firstRune(key)
	if !unicode.IsLetter(first) {
		return false
	}
	for _, r := range key {
		if unicode.IsLower(r) || unicode.IsDigit(r) || r == '_' || r == '-' {
			continue
		}
		return false
	}
	return true
}

func firstRune(value string) (rune, bool) {
	for _, r := range value {
		return r, true
	}
	return 0, false
}

func cloneGooglePubSubDescriptor(d GooglePubSubDescriptor) GooglePubSubDescriptor {
	d.Labels = cloneStringMap(d.Labels)
	return d
}

func cloneStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	cloned := make(map[string]string, len(values))
	for _, key := range keys {
		cloned[key] = values[key]
	}
	return cloned
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}
