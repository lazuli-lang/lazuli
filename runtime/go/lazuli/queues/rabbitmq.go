package queues

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strings"
)

const (
	RabbitMQMinPrefetch = 0
	RabbitMQMaxPrefetch = 65535
)

var (
	ErrRabbitMQURLInvalid        = errors.New("lazuli/queues: rabbitmq url is invalid")
	ErrRabbitMQExchangeInvalid   = errors.New("lazuli/queues: rabbitmq exchange metadata is invalid")
	ErrRabbitMQQueueInvalid      = errors.New("lazuli/queues: rabbitmq queue metadata is invalid")
	ErrRabbitMQRoutingKeyInvalid = errors.New("lazuli/queues: rabbitmq routing key is invalid")
	ErrRabbitMQPrefetchInvalid   = errors.New("lazuli/queues: rabbitmq prefetch is invalid")
	ErrRabbitMQDLXInvalid        = errors.New("lazuli/queues: rabbitmq dead-letter metadata is invalid")
)

type RabbitMQDescriptor struct {
	URL        string
	Exchange   RabbitMQExchangeMetadata
	Queue      RabbitMQQueueMetadata
	RoutingKey string
	Prefetch   int
}

type RabbitMQExchangeMetadata struct {
	Name       string
	Type       string
	Durable    bool
	AutoDelete bool
}

type RabbitMQQueueMetadata struct {
	Name       string
	Durable    bool
	AutoDelete bool
	Exclusive  bool
	DeadLetter RabbitMQDeadLetterMetadata
}

type RabbitMQDeadLetterMetadata struct {
	Exchange   string
	RoutingKey string
}

type RabbitMQPlan struct {
	Descriptor RabbitMQDescriptor
	Summary    RabbitMQDescriptorSummary
}

type RabbitMQDescriptorSummary struct {
	URL                  string
	VHost                string
	Exchange             string
	ExchangeType         string
	ExchangeDurable      bool
	ExchangeAutoDelete   bool
	Queue                string
	QueueDurable         bool
	QueueAutoDelete      bool
	QueueExclusive       bool
	RoutingKey           string
	Prefetch             int
	DeadLetter           bool
	DeadLetterExchange   string
	DeadLetterRoutingKey string
}

func (d RabbitMQDescriptor) Normalize() RabbitMQDescriptor {
	return NormalizeRabbitMQDescriptor(d)
}

func (d RabbitMQDescriptor) Validate() error {
	return ValidateRabbitMQDescriptor(d)
}

func (d RabbitMQDescriptor) Summary() RabbitMQDescriptorSummary {
	return RedactedRabbitMQDescriptorSummary(d)
}

func NormalizeRabbitMQDescriptor(desc RabbitMQDescriptor) RabbitMQDescriptor {
	desc.URL = normalizeRabbitMQURL(desc.URL)
	desc.Exchange = NormalizeRabbitMQExchangeMetadata(desc.Exchange)
	desc.Queue = NormalizeRabbitMQQueueMetadata(desc.Queue)
	desc.RoutingKey = strings.TrimSpace(desc.RoutingKey)
	if desc.RoutingKey == "" {
		desc.RoutingKey = desc.Queue.Name
	}
	return desc
}

func ValidateRabbitMQDescriptor(desc RabbitMQDescriptor) error {
	desc = NormalizeRabbitMQDescriptor(desc)

	var errs []error
	if err := ValidateRabbitMQURL(desc.URL); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateRabbitMQExchangeMetadata(desc.Exchange); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateRabbitMQQueueMetadata(desc.Queue); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateRabbitMQRoutingKey(desc.RoutingKey); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateRabbitMQPrefetch(desc.Prefetch); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

func PlanRabbitMQDescriptor(desc RabbitMQDescriptor) (RabbitMQPlan, error) {
	normalized := NormalizeRabbitMQDescriptor(desc)
	plan := RabbitMQPlan{
		Descriptor: normalized,
		Summary:    RedactedRabbitMQDescriptorSummary(normalized),
	}
	return plan, ValidateRabbitMQDescriptor(normalized)
}

func RedactedRabbitMQDescriptorSummary(desc RabbitMQDescriptor) RabbitMQDescriptorSummary {
	desc = NormalizeRabbitMQDescriptor(desc)
	return RabbitMQDescriptorSummary{
		URL:                  RedactRabbitMQURL(desc.URL),
		VHost:                rabbitMQVHost(desc.URL),
		Exchange:             desc.Exchange.Name,
		ExchangeType:         desc.Exchange.Type,
		ExchangeDurable:      desc.Exchange.Durable,
		ExchangeAutoDelete:   desc.Exchange.AutoDelete,
		Queue:                desc.Queue.Name,
		QueueDurable:         desc.Queue.Durable,
		QueueAutoDelete:      desc.Queue.AutoDelete,
		QueueExclusive:       desc.Queue.Exclusive,
		RoutingKey:           desc.RoutingKey,
		Prefetch:             desc.Prefetch,
		DeadLetter:           desc.Queue.DeadLetter.Exchange != "" || desc.Queue.DeadLetter.RoutingKey != "",
		DeadLetterExchange:   desc.Queue.DeadLetter.Exchange,
		DeadLetterRoutingKey: desc.Queue.DeadLetter.RoutingKey,
	}
}

func NormalizeRabbitMQURL(raw string) (string, error) {
	normalized := normalizeRabbitMQURL(raw)
	if err := ValidateRabbitMQURL(normalized); err != nil {
		return "", err
	}
	return normalized, nil
}

func ValidateRabbitMQURL(raw string) error {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return fmt.Errorf("%w: empty url", ErrRabbitMQURLInvalid)
	}
	parsed, err := url.Parse(raw)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrRabbitMQURLInvalid, err)
	}
	switch strings.ToLower(parsed.Scheme) {
	case "amqp", "amqps":
	default:
		return fmt.Errorf("%w: unsupported scheme %q", ErrRabbitMQURLInvalid, parsed.Scheme)
	}
	if parsed.Host == "" || parsed.Hostname() == "" {
		return fmt.Errorf("%w: host is required", ErrRabbitMQURLInvalid)
	}
	if strings.ContainsAny(parsed.Hostname(), " \t\r\n") {
		return fmt.Errorf("%w: host contains whitespace", ErrRabbitMQURLInvalid)
	}
	if port := parsed.Port(); port != "" {
		if _, err := net.LookupPort("tcp", port); err != nil {
			return fmt.Errorf("%w: port %q is invalid", ErrRabbitMQURLInvalid, port)
		}
	}
	if parsed.Fragment != "" {
		return fmt.Errorf("%w: fragment must be empty", ErrRabbitMQURLInvalid)
	}
	if strings.Count(strings.Trim(parsed.EscapedPath(), "/"), "/") > 0 {
		return fmt.Errorf("%w: path must contain at most one virtual host", ErrRabbitMQURLInvalid)
	}
	return nil
}

func RedactRabbitMQURL(raw string) string {
	raw = strings.TrimSpace(raw)
	parsed, err := url.Parse(raw)
	if err != nil {
		return raw
	}
	if parsed.User != nil {
		parsed.User = url.UserPassword("[REDACTED]", "[REDACTED]")
	}
	if parsed.RawQuery != "" {
		q := parsed.Query()
		for key := range q {
			if secretRabbitMQQueryKey(key) {
				q.Set(key, "[REDACTED]")
			}
		}
		parsed.RawQuery = q.Encode()
	}
	return parsed.String()
}

func NormalizeRabbitMQExchangeMetadata(meta RabbitMQExchangeMetadata) RabbitMQExchangeMetadata {
	meta.Name = strings.TrimSpace(meta.Name)
	meta.Type = strings.ToLower(strings.TrimSpace(meta.Type))
	if meta.Name != "" && meta.Type == "" {
		meta.Type = "direct"
	}
	return meta
}

func ValidateRabbitMQExchangeMetadata(meta RabbitMQExchangeMetadata) error {
	meta = NormalizeRabbitMQExchangeMetadata(meta)
	if meta.Name == "" && meta.Type == "" {
		return nil
	}
	var errs []error
	if meta.Name != "" && !validRabbitMQName(meta.Name) {
		errs = append(errs, fmt.Errorf("%w: exchange name %q", ErrRabbitMQExchangeInvalid, meta.Name))
	}
	if !validRabbitMQExchangeType(meta.Type) {
		errs = append(errs, fmt.Errorf("%w: exchange type %q", ErrRabbitMQExchangeInvalid, meta.Type))
	}
	if meta.Name == "" && (meta.Durable || meta.AutoDelete || meta.Type != "") {
		errs = append(errs, fmt.Errorf("%w: default exchange must not set metadata", ErrRabbitMQExchangeInvalid))
	}
	return errors.Join(errs...)
}

func NormalizeRabbitMQQueueMetadata(meta RabbitMQQueueMetadata) RabbitMQQueueMetadata {
	meta.Name = strings.TrimSpace(meta.Name)
	meta.DeadLetter = NormalizeRabbitMQDeadLetterMetadata(meta.DeadLetter)
	return meta
}

func ValidateRabbitMQQueueMetadata(meta RabbitMQQueueMetadata) error {
	meta = NormalizeRabbitMQQueueMetadata(meta)
	var errs []error
	if meta.Name == "" {
		errs = append(errs, fmt.Errorf("%w: queue name is required", ErrRabbitMQQueueInvalid))
	} else if !validRabbitMQName(meta.Name) {
		errs = append(errs, fmt.Errorf("%w: queue name %q", ErrRabbitMQQueueInvalid, meta.Name))
	}
	if meta.Durable && meta.AutoDelete {
		errs = append(errs, fmt.Errorf("%w: durable queue must not be auto-delete", ErrRabbitMQQueueInvalid))
	}
	if err := ValidateRabbitMQDeadLetterMetadata(meta.DeadLetter); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

func NormalizeRabbitMQDeadLetterMetadata(meta RabbitMQDeadLetterMetadata) RabbitMQDeadLetterMetadata {
	meta.Exchange = strings.TrimSpace(meta.Exchange)
	meta.RoutingKey = strings.TrimSpace(meta.RoutingKey)
	return meta
}

func ValidateRabbitMQDeadLetterMetadata(meta RabbitMQDeadLetterMetadata) error {
	meta = NormalizeRabbitMQDeadLetterMetadata(meta)
	if meta.Exchange == "" && meta.RoutingKey == "" {
		return nil
	}
	var errs []error
	if meta.Exchange == "" {
		errs = append(errs, fmt.Errorf("%w: exchange is required when dead-letter routing key is set", ErrRabbitMQDLXInvalid))
	} else if !validRabbitMQName(meta.Exchange) {
		errs = append(errs, fmt.Errorf("%w: exchange %q", ErrRabbitMQDLXInvalid, meta.Exchange))
	}
	if meta.RoutingKey != "" {
		if err := ValidateRabbitMQRoutingKey(meta.RoutingKey); err != nil {
			errs = append(errs, fmt.Errorf("%w: %v", ErrRabbitMQDLXInvalid, err))
		}
	}
	return errors.Join(errs...)
}

func ValidateRabbitMQRoutingKey(routingKey string) error {
	routingKey = strings.TrimSpace(routingKey)
	if routingKey == "" {
		return fmt.Errorf("%w: routing key is required", ErrRabbitMQRoutingKeyInvalid)
	}
	if len(routingKey) > 255 {
		return fmt.Errorf("%w: routing key exceeds 255 bytes", ErrRabbitMQRoutingKeyInvalid)
	}
	if strings.ContainsAny(routingKey, " \t\r\n") {
		return fmt.Errorf("%w: routing key %q contains whitespace", ErrRabbitMQRoutingKeyInvalid, routingKey)
	}
	for _, token := range strings.Split(routingKey, ".") {
		if token == "" {
			return fmt.Errorf("%w: routing key %q has an empty token", ErrRabbitMQRoutingKeyInvalid, routingKey)
		}
		if strings.ContainsAny(token, "*#") {
			return fmt.Errorf("%w: routing key %q must not use wildcards", ErrRabbitMQRoutingKeyInvalid, routingKey)
		}
	}
	return nil
}

func ValidateRabbitMQPrefetch(prefetch int) error {
	if prefetch < RabbitMQMinPrefetch || prefetch > RabbitMQMaxPrefetch {
		return fmt.Errorf("%w: must be between %d and %d", ErrRabbitMQPrefetchInvalid, RabbitMQMinPrefetch, RabbitMQMaxPrefetch)
	}
	return nil
}

func normalizeRabbitMQURL(raw string) string {
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
	parsed.RawQuery = normalizeRabbitMQQuery(parsed.RawQuery)
	return parsed.String()
}

func normalizeRabbitMQQuery(raw string) string {
	if raw == "" {
		return ""
	}
	q, err := url.ParseQuery(raw)
	if err != nil {
		return raw
	}
	keys := make([]string, 0, len(q))
	for key := range q {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	encoded := make(url.Values, len(q))
	for _, key := range keys {
		values := append([]string(nil), q[key]...)
		sort.Strings(values)
		encoded[key] = values
	}
	return encoded.Encode()
}

func rabbitMQVHost(raw string) string {
	parsed, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
		return ""
	}
	path := strings.TrimPrefix(parsed.EscapedPath(), "/")
	if path == "" {
		return "/"
	}
	vhost, err := url.PathUnescape(path)
	if err != nil {
		return path
	}
	return vhost
}

func secretRabbitMQQueryKey(key string) bool {
	key = strings.ToLower(key)
	return strings.Contains(key, "password") || strings.Contains(key, "secret") || strings.Contains(key, "token")
}

func validRabbitMQExchangeType(value string) bool {
	switch value {
	case "direct", "fanout", "headers", "topic":
		return true
	default:
		return false
	}
}

func validRabbitMQName(value string) bool {
	if value == "" || len(value) > 255 {
		return false
	}
	if strings.ContainsAny(value, "\x00\r\n") {
		return false
	}
	return true
}
