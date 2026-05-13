package queues

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
)

const (
	DefaultNATSAckWait        = 30 * time.Second
	DefaultNATSConnectTimeout = 5 * time.Second
	MinNATSAckWait            = time.Second
	MaxNATSAckWait            = 30 * time.Minute
	MinNATSConnectTimeout     = time.Second
	MaxNATSConnectTimeout     = time.Minute
)

var (
	ErrNATSServerURLInvalid  = errors.New("lazuli/queues: nats server url is invalid")
	ErrNATSSubjectInvalid    = errors.New("lazuli/queues: nats subject is invalid")
	ErrNATSQueueGroupInvalid = errors.New("lazuli/queues: nats queue group is invalid")
	ErrNATSJetStreamInvalid  = errors.New("lazuli/queues: nats jetstream metadata is invalid")
	ErrNATSAckWaitInvalid    = errors.New("lazuli/queues: nats ack wait is invalid")
	ErrNATSWaitInvalid       = errors.New("lazuli/queues: nats wait timeout is invalid")
)

type NATSDescriptor struct {
	Servers        []string
	Subject        string
	QueueGroup     string
	JetStream      NATSJetStreamMetadata
	AckWait        time.Duration
	ConnectTimeout time.Duration
}

type NATSJetStreamMetadata struct {
	Stream        string
	Subjects      []string
	Consumer      string
	FilterSubject string
}

type NATSPlan struct {
	Servers        []string
	Subject        string
	QueueGroup     string
	JetStream      NATSJetStreamMetadata
	AckWait        time.Duration
	ConnectTimeout time.Duration
	Summary        NATSDescriptorSummary
}

type NATSDescriptorSummary struct {
	Servers        []string
	Subject        string
	QueueGroup     string
	Stream         string
	Consumer       string
	AckWait        string
	ConnectTimeout string
	JetStream      bool
}

func (d NATSDescriptor) Normalize() NATSDescriptor {
	return NormalizeNATSDescriptor(d)
}

func (d NATSDescriptor) Validate() error {
	return ValidateNATSDescriptor(d)
}

func (d NATSDescriptor) Summary() NATSDescriptorSummary {
	return RedactedNATSDescriptorSummary(d)
}

func NormalizeNATSDescriptor(desc NATSDescriptor) NATSDescriptor {
	desc.Servers = normalizeNATSServerURLs(desc.Servers)
	desc.Subject = strings.TrimSpace(desc.Subject)
	desc.QueueGroup = strings.TrimSpace(desc.QueueGroup)
	desc.JetStream = NormalizeNATSJetStreamMetadata(desc.JetStream)
	if desc.AckWait == 0 {
		desc.AckWait = DefaultNATSAckWait
	}
	if desc.ConnectTimeout == 0 {
		desc.ConnectTimeout = DefaultNATSConnectTimeout
	}
	return desc
}

func ValidateNATSDescriptor(desc NATSDescriptor) error {
	desc = NormalizeNATSDescriptor(desc)

	var errs []error
	if len(desc.Servers) == 0 {
		errs = append(errs, fmt.Errorf("%w: at least one server is required", ErrNATSServerURLInvalid))
	}
	for i, server := range desc.Servers {
		if err := ValidateNATSServerURL(server); err != nil {
			errs = append(errs, fmt.Errorf("server[%d]: %w", i, err))
		}
	}
	if err := ValidateNATSSubject(desc.Subject); err != nil {
		errs = append(errs, err)
	}
	if desc.QueueGroup != "" {
		if err := ValidateNATSQueueGroup(desc.QueueGroup); err != nil {
			errs = append(errs, err)
		}
	}
	if err := ValidateNATSJetStreamMetadata(desc.JetStream); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateNATSAckWait(desc.AckWait); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateNATSWaitTimeout(desc.ConnectTimeout); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

func PlanNATSDescriptor(desc NATSDescriptor) (NATSPlan, error) {
	normalized := NormalizeNATSDescriptor(desc)
	plan := NATSPlan{
		Servers:        append([]string(nil), normalized.Servers...),
		Subject:        normalized.Subject,
		QueueGroup:     normalized.QueueGroup,
		JetStream:      cloneNATSJetStreamMetadata(normalized.JetStream),
		AckWait:        normalized.AckWait,
		ConnectTimeout: normalized.ConnectTimeout,
		Summary:        RedactedNATSDescriptorSummary(normalized),
	}
	return plan, ValidateNATSDescriptor(normalized)
}

func RedactedNATSDescriptorSummary(desc NATSDescriptor) NATSDescriptorSummary {
	desc = NormalizeNATSDescriptor(desc)
	return NATSDescriptorSummary{
		Servers:        redactNATSServerURLs(desc.Servers),
		Subject:        desc.Subject,
		QueueGroup:     desc.QueueGroup,
		Stream:         desc.JetStream.Stream,
		Consumer:       desc.JetStream.Consumer,
		AckWait:        desc.AckWait.String(),
		ConnectTimeout: desc.ConnectTimeout.String(),
		JetStream:      desc.JetStream.Stream != "" || desc.JetStream.Consumer != "" || len(desc.JetStream.Subjects) > 0 || desc.JetStream.FilterSubject != "",
	}
}

func NormalizeNATSServerURL(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if err := ValidateNATSServerURL(raw); err != nil {
		return "", err
	}
	parsed, _ := url.Parse(raw)
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	parsed.Host = strings.ToLower(parsed.Host)
	return parsed.String(), nil
}

func ValidateNATSServerURL(raw string) error {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return fmt.Errorf("%w: empty url", ErrNATSServerURLInvalid)
	}
	parsed, err := url.Parse(raw)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrNATSServerURLInvalid, err)
	}
	switch strings.ToLower(parsed.Scheme) {
	case "nats", "tls", "ws", "wss":
	default:
		return fmt.Errorf("%w: unsupported scheme %q", ErrNATSServerURLInvalid, parsed.Scheme)
	}
	if parsed.Host == "" {
		return fmt.Errorf("%w: host is required", ErrNATSServerURLInvalid)
	}
	if parsed.Path != "" && parsed.Path != "/" {
		return fmt.Errorf("%w: path must be empty", ErrNATSServerURLInvalid)
	}
	if parsed.RawQuery != "" || parsed.Fragment != "" {
		return fmt.Errorf("%w: query and fragment must be empty", ErrNATSServerURLInvalid)
	}
	host := parsed.Hostname()
	if host == "" {
		return fmt.Errorf("%w: host is required", ErrNATSServerURLInvalid)
	}
	if strings.ContainsAny(host, " \t\r\n") {
		return fmt.Errorf("%w: host contains whitespace", ErrNATSServerURLInvalid)
	}
	if port := parsed.Port(); port != "" {
		if _, err := net.LookupPort("tcp", port); err != nil {
			return fmt.Errorf("%w: port %q is invalid", ErrNATSServerURLInvalid, port)
		}
	}
	return nil
}

func ValidateNATSSubject(subject string) error {
	subject = strings.TrimSpace(subject)
	if subject == "" {
		return fmt.Errorf("%w: subject is required", ErrNATSSubjectInvalid)
	}
	if strings.ContainsAny(subject, " \t\r\n") {
		return fmt.Errorf("%w: %q contains whitespace", ErrNATSSubjectInvalid, subject)
	}
	for _, token := range strings.Split(subject, ".") {
		if token == "" {
			return fmt.Errorf("%w: %q has an empty token", ErrNATSSubjectInvalid, subject)
		}
		if token == "*" || token == ">" || strings.ContainsAny(token, "*>") {
			return fmt.Errorf("%w: %q must not use wildcards", ErrNATSSubjectInvalid, subject)
		}
	}
	return nil
}

func ValidateNATSQueueGroup(group string) error {
	group = strings.TrimSpace(group)
	if group == "" {
		return fmt.Errorf("%w: queue group is required", ErrNATSQueueGroupInvalid)
	}
	if strings.Contains(group, ".") || strings.ContainsAny(group, "*>/\\") {
		return fmt.Errorf("%w: %q contains a reserved character", ErrNATSQueueGroupInvalid, group)
	}
	if !validNATSIdentifier(group) {
		return fmt.Errorf("%w: %q", ErrNATSQueueGroupInvalid, group)
	}
	return nil
}

func NormalizeNATSJetStreamMetadata(meta NATSJetStreamMetadata) NATSJetStreamMetadata {
	meta.Stream = strings.TrimSpace(meta.Stream)
	meta.Consumer = strings.TrimSpace(meta.Consumer)
	meta.FilterSubject = strings.TrimSpace(meta.FilterSubject)
	meta.Subjects = normalizeNATSSubjects(meta.Subjects)
	return meta
}

func ValidateNATSJetStreamMetadata(meta NATSJetStreamMetadata) error {
	meta = NormalizeNATSJetStreamMetadata(meta)
	if meta.Stream == "" && meta.Consumer == "" && len(meta.Subjects) == 0 && meta.FilterSubject == "" {
		return nil
	}

	var errs []error
	if !validNATSIdentifier(meta.Stream) {
		errs = append(errs, fmt.Errorf("%w: stream %q", ErrNATSJetStreamInvalid, meta.Stream))
	}
	if meta.Consumer != "" && !validNATSIdentifier(meta.Consumer) {
		errs = append(errs, fmt.Errorf("%w: consumer %q", ErrNATSJetStreamInvalid, meta.Consumer))
	}
	for i, subject := range meta.Subjects {
		if err := ValidateNATSSubject(subject); err != nil {
			errs = append(errs, fmt.Errorf("%w: stream subject[%d]: %v", ErrNATSJetStreamInvalid, i, err))
		}
	}
	if meta.FilterSubject != "" {
		if err := ValidateNATSSubject(meta.FilterSubject); err != nil {
			errs = append(errs, fmt.Errorf("%w: filter subject: %v", ErrNATSJetStreamInvalid, err))
		}
	}
	return errors.Join(errs...)
}

func ValidateNATSAckWait(wait time.Duration) error {
	if wait < MinNATSAckWait || wait > MaxNATSAckWait {
		return fmt.Errorf("%w: must be between %s and %s", ErrNATSAckWaitInvalid, MinNATSAckWait, MaxNATSAckWait)
	}
	return nil
}

func ValidateNATSWaitTimeout(wait time.Duration) error {
	if wait < MinNATSConnectTimeout || wait > MaxNATSConnectTimeout {
		return fmt.Errorf("%w: must be between %s and %s", ErrNATSWaitInvalid, MinNATSConnectTimeout, MaxNATSConnectTimeout)
	}
	return nil
}

func cloneNATSJetStreamMetadata(meta NATSJetStreamMetadata) NATSJetStreamMetadata {
	meta.Subjects = append([]string(nil), meta.Subjects...)
	return meta
}

func normalizeNATSServerURLs(servers []string) []string {
	if len(servers) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(servers))
	normalized := make([]string, 0, len(servers))
	for _, server := range servers {
		server, err := NormalizeNATSServerURL(server)
		if err != nil {
			server = strings.TrimSpace(server)
		}
		if server == "" {
			continue
		}
		if _, ok := seen[server]; ok {
			continue
		}
		seen[server] = struct{}{}
		normalized = append(normalized, server)
	}
	sort.Strings(normalized)
	return normalized
}

func normalizeNATSSubjects(subjects []string) []string {
	if len(subjects) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(subjects))
	normalized := make([]string, 0, len(subjects))
	for _, subject := range subjects {
		subject = strings.TrimSpace(subject)
		if subject == "" {
			continue
		}
		if _, ok := seen[subject]; ok {
			continue
		}
		seen[subject] = struct{}{}
		normalized = append(normalized, subject)
	}
	sort.Strings(normalized)
	return normalized
}

func redactNATSServerURLs(servers []string) []string {
	if len(servers) == 0 {
		return nil
	}
	redacted := make([]string, 0, len(servers))
	for _, server := range servers {
		redacted = append(redacted, RedactNATSServerURL(server))
	}
	sort.Strings(redacted)
	return redacted
}

func RedactNATSServerURL(raw string) string {
	raw = strings.TrimSpace(raw)
	parsed, err := url.Parse(raw)
	if err != nil || parsed.User == nil {
		return raw
	}
	parsed.User = url.UserPassword("[REDACTED]", "[REDACTED]")
	return parsed.String()
}

func validNATSIdentifier(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		if unicode.IsLetter(r) || unicode.IsDigit(r) || r == '_' || r == '-' {
			continue
		}
		return false
	}
	return true
}
