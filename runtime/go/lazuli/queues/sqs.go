package queues

import (
	"errors"
	"fmt"
	"net/url"
	"path"
	"regexp"
	"strconv"
	"strings"
	"time"
)

var (
	// ErrSQSQueueInvalid reports invalid Amazon SQS queue descriptor metadata.
	ErrSQSQueueInvalid = errors.New("lazuli/queues: sqs_queue_invalid")
)

const (
	SQSMinVisibilityTimeout = 0
	SQSMaxVisibilityTimeout = 12 * time.Hour
	SQSMinDelay             = 0
	SQSMaxDelay             = 15 * time.Minute
)

var sqsAccountIDPattern = regexp.MustCompile(`\b\d{12}\b`)

// SQSQueueDescriptor is the provider-neutral metadata needed to address and
// validate an SQS-compatible queue before a concrete adapter sends messages.
type SQSQueueDescriptor struct {
	Name                      string
	URL                       string
	ARN                       string
	Region                    string
	FIFO                      bool
	ContentBasedDeduplication bool
	MessageGroupID            string
	MessageDeduplicationID    string
	VisibilityTimeout         time.Duration
	Delay                     time.Duration
}

// SQSQueuePlan is a normalized, side-effect-free send plan for a queue.
type SQSQueuePlan struct {
	Descriptor SQSQueueDescriptor
	Summary    SQSQueueSummary
}

// SQSQueueSummary is safe to log. URL and ARN account identifiers are masked.
type SQSQueueSummary struct {
	Name                         string
	URL                          string
	ARN                          string
	Region                       string
	FIFO                         bool
	ContentBasedDeduplication    bool
	RequiresMessageGroupID       bool
	RequiresMessageDeduplication bool
	VisibilityTimeoutSeconds     int
	DelaySeconds                 int
}

// Normalize returns a copy with trimmed names, canonical region casing,
// normalized SQS URL/ARN strings, and inferred FIFO metadata.
func (d SQSQueueDescriptor) Normalize() SQSQueueDescriptor {
	return NormalizeSQSQueueDescriptor(d)
}

// Validate checks whether the descriptor can be used by a future SQS adapter.
func (d SQSQueueDescriptor) Validate() error {
	return ValidateSQSQueueDescriptor(d)
}

// RedactedSummary returns deterministic queue metadata suitable for logs.
func (d SQSQueueDescriptor) RedactedSummary() SQSQueueSummary {
	plan, err := PlanSQSQueue(d)
	if err != nil {
		normalized := NormalizeSQSQueueDescriptor(d)
		return sqsQueueSummary(normalized)
	}
	return plan.Summary
}

// NormalizeSQSQueueDescriptor returns queue metadata in deterministic form.
func NormalizeSQSQueueDescriptor(d SQSQueueDescriptor) SQSQueueDescriptor {
	d.Name = strings.TrimSpace(d.Name)
	d.URL = normalizeSQSQueueURL(d.URL)
	d.ARN = normalizeSQSQueueARN(d.ARN)
	d.Region = strings.ToLower(strings.TrimSpace(d.Region))
	d.MessageGroupID = strings.TrimSpace(d.MessageGroupID)
	d.MessageDeduplicationID = strings.TrimSpace(d.MessageDeduplicationID)

	if d.Name == "" {
		d.Name = queueNameFromSQSURL(d.URL)
	}
	if d.Name == "" {
		d.Name = queueNameFromSQSARN(d.ARN)
	}
	if d.Region == "" {
		d.Region = regionFromSQSARN(d.ARN)
	}
	if d.Region == "" {
		d.Region = regionFromSQSURL(d.URL)
	}
	if strings.HasSuffix(d.Name, ".fifo") || strings.HasSuffix(queueNameFromSQSURL(d.URL), ".fifo") || strings.HasSuffix(queueNameFromSQSARN(d.ARN), ".fifo") {
		d.FIFO = true
	}
	return d
}

// ValidateSQSQueueDescriptor checks queue address metadata, FIFO send metadata,
// and SQS visibility/delay bounds.
func ValidateSQSQueueDescriptor(d SQSQueueDescriptor) error {
	d = NormalizeSQSQueueDescriptor(d)

	var errs []error
	if d.Name == "" && d.URL == "" && d.ARN == "" {
		errs = append(errs, fmt.Errorf("%w: name, url, or arn must be set", ErrSQSQueueInvalid))
	}
	if d.Name != "" && !validSQSQueueName(d.Name) {
		errs = append(errs, fmt.Errorf("%w: queue name %q is invalid", ErrSQSQueueInvalid, d.Name))
	}
	if d.URL != "" {
		if err := validateSQSQueueURL(d.URL); err != nil {
			errs = append(errs, err)
		}
	}
	if d.ARN != "" {
		if err := validateSQSQueueARN(d.ARN); err != nil {
			errs = append(errs, err)
		}
	}
	if d.Region != "" && !validSQSRegion(d.Region) {
		errs = append(errs, fmt.Errorf("%w: region %q is invalid", ErrSQSQueueInvalid, d.Region))
	}
	if d.FIFO {
		if !strings.HasSuffix(d.Name, ".fifo") {
			errs = append(errs, fmt.Errorf("%w: fifo queue name must end with .fifo", ErrSQSQueueInvalid))
		}
		if d.MessageGroupID == "" {
			errs = append(errs, fmt.Errorf("%w: fifo queue requires message group id", ErrSQSQueueInvalid))
		}
		if !d.ContentBasedDeduplication && d.MessageDeduplicationID == "" {
			errs = append(errs, fmt.Errorf("%w: fifo queue requires deduplication id when content-based deduplication is disabled", ErrSQSQueueInvalid))
		}
	} else {
		if d.MessageGroupID != "" || d.MessageDeduplicationID != "" || d.ContentBasedDeduplication {
			errs = append(errs, fmt.Errorf("%w: standard queue must not set fifo metadata", ErrSQSQueueInvalid))
		}
	}
	if err := ValidateSQSVisibilityTimeout(d.VisibilityTimeout); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateSQSDelay(d.Delay); err != nil {
		errs = append(errs, err)
	}

	return errors.Join(errs...)
}

// PlanSQSQueue normalizes and validates descriptor metadata without performing
// network I/O.
func PlanSQSQueue(d SQSQueueDescriptor) (SQSQueuePlan, error) {
	normalized := NormalizeSQSQueueDescriptor(d)
	plan := SQSQueuePlan{
		Descriptor: normalized,
		Summary:    sqsQueueSummary(normalized),
	}
	if err := ValidateSQSQueueDescriptor(normalized); err != nil {
		return plan, err
	}
	return plan, nil
}

// ValidateSQSVisibilityTimeout checks Amazon SQS visibility timeout bounds.
func ValidateSQSVisibilityTimeout(timeout time.Duration) error {
	return validateSQSDuration("visibility timeout", timeout, SQSMinVisibilityTimeout, SQSMaxVisibilityTimeout)
}

// ValidateSQSDelay checks Amazon SQS per-message delay bounds.
func ValidateSQSDelay(delay time.Duration) error {
	return validateSQSDuration("delay", delay, SQSMinDelay, SQSMaxDelay)
}

func sqsQueueSummary(d SQSQueueDescriptor) SQSQueueSummary {
	return SQSQueueSummary{
		Name:                         d.Name,
		URL:                          redactSQSQueueIdentifier(d.URL),
		ARN:                          redactSQSQueueIdentifier(d.ARN),
		Region:                       d.Region,
		FIFO:                         d.FIFO,
		ContentBasedDeduplication:    d.ContentBasedDeduplication,
		RequiresMessageGroupID:       d.FIFO && d.MessageGroupID == "",
		RequiresMessageDeduplication: d.FIFO && !d.ContentBasedDeduplication && d.MessageDeduplicationID == "",
		VisibilityTimeoutSeconds:     durationSeconds(d.VisibilityTimeout),
		DelaySeconds:                 durationSeconds(d.Delay),
	}
}

func normalizeSQSQueueURL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return raw
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.RawQuery = ""
	parsed.Fragment = ""
	parsed.Path = path.Clean("/" + strings.TrimSpace(parsed.EscapedPath()))
	if parsed.Path == "/" {
		parsed.Path = ""
	}
	return parsed.String()
}

func normalizeSQSQueueARN(raw string) string {
	parts := strings.Split(strings.TrimSpace(raw), ":")
	if len(parts) != 6 {
		return strings.TrimSpace(raw)
	}
	if strings.ToLower(parts[0]) != "arn" || strings.ToLower(parts[2]) != "sqs" {
		return strings.TrimSpace(raw)
	}
	parts[0] = "arn"
	parts[1] = strings.ToLower(parts[1])
	parts[2] = strings.ToLower(parts[2])
	parts[3] = strings.ToLower(parts[3])
	parts[4] = strings.TrimSpace(parts[4])
	parts[5] = strings.TrimSpace(parts[5])
	return strings.Join(parts, ":")
}

func validateSQSQueueURL(raw string) error {
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return fmt.Errorf("%w: queue url %q is invalid", ErrSQSQueueInvalid, raw)
	}
	if parsed.Scheme != "http" && parsed.Scheme != "https" {
		return fmt.Errorf("%w: queue url scheme %q is invalid", ErrSQSQueueInvalid, parsed.Scheme)
	}
	if queueNameFromSQSURL(raw) == "" {
		return fmt.Errorf("%w: queue url must include queue name", ErrSQSQueueInvalid)
	}
	return nil
}

func validateSQSQueueARN(raw string) error {
	parts := strings.Split(raw, ":")
	if len(parts) != 6 || parts[0] != "arn" || parts[2] != "sqs" {
		return fmt.Errorf("%w: queue arn %q is invalid", ErrSQSQueueInvalid, raw)
	}
	if parts[3] == "" || !validSQSRegion(parts[3]) {
		return fmt.Errorf("%w: queue arn region %q is invalid", ErrSQSQueueInvalid, parts[3])
	}
	if !validSQSAccountID(parts[4]) {
		return fmt.Errorf("%w: queue arn account id is invalid", ErrSQSQueueInvalid)
	}
	if !validSQSQueueName(parts[5]) {
		return fmt.Errorf("%w: queue arn name %q is invalid", ErrSQSQueueInvalid, parts[5])
	}
	return nil
}

func validateSQSDuration(name string, value, min, max time.Duration) error {
	if value < min {
		return fmt.Errorf("%w: %s must be at least %s", ErrSQSQueueInvalid, name, min)
	}
	if value > max {
		return fmt.Errorf("%w: %s %s exceeds %s", ErrSQSQueueInvalid, name, value, max)
	}
	if value%time.Second != 0 {
		return fmt.Errorf("%w: %s must be whole seconds", ErrSQSQueueInvalid, name)
	}
	return nil
}

func queueNameFromSQSURL(raw string) string {
	parsed, err := url.Parse(raw)
	if err != nil {
		return ""
	}
	segments := strings.Split(strings.Trim(parsed.Path, "/"), "/")
	if len(segments) == 0 {
		return ""
	}
	name, err := url.PathUnescape(segments[len(segments)-1])
	if err != nil {
		return ""
	}
	return strings.TrimSpace(name)
}

func queueNameFromSQSARN(raw string) string {
	parts := strings.Split(raw, ":")
	if len(parts) != 6 || strings.ToLower(parts[2]) != "sqs" {
		return ""
	}
	return strings.TrimSpace(parts[5])
}

func regionFromSQSARN(raw string) string {
	parts := strings.Split(raw, ":")
	if len(parts) != 6 || strings.ToLower(parts[2]) != "sqs" {
		return ""
	}
	return strings.ToLower(strings.TrimSpace(parts[3]))
}

func regionFromSQSURL(raw string) string {
	parsed, err := url.Parse(raw)
	if err != nil {
		return ""
	}
	host := strings.ToLower(parsed.Hostname())
	parts := strings.Split(host, ".")
	for i, part := range parts {
		if part == "sqs" && i+1 < len(parts) {
			return parts[i+1]
		}
	}
	return ""
}

func validSQSQueueName(name string) bool {
	if name == "" || len(name) > 80 {
		return false
	}
	for _, r := range name {
		if r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_' || r == '-' || r == '.' {
			continue
		}
		return false
	}
	return true
}

func validSQSRegion(region string) bool {
	if region == "" {
		return false
	}
	for _, r := range region {
		if r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || r == '-' {
			continue
		}
		return false
	}
	return strings.Contains(region, "-")
}

func validSQSAccountID(accountID string) bool {
	if len(accountID) != 12 {
		return false
	}
	_, err := strconv.ParseUint(accountID, 10, 64)
	return err == nil
}

func redactSQSQueueIdentifier(value string) string {
	return sqsAccountIDPattern.ReplaceAllString(value, "************")
}

func durationSeconds(value time.Duration) int {
	return int(value / time.Second)
}
