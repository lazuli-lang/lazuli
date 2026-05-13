package queues

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"sort"
	"strings"
	"time"
)

const (
	DefaultKafkaBrokerPort      = "9092"
	KafkaAnyPartition           = -1
	DefaultKafkaDeliveryTimeout = 30 * time.Second
	MinKafkaDeliveryTimeout     = time.Second
	MaxKafkaDeliveryTimeout     = 10 * time.Minute
)

var (
	ErrKafkaBrokerInvalid          = errors.New("lazuli/queues: kafka broker is invalid")
	ErrKafkaTopicInvalid           = errors.New("lazuli/queues: kafka topic is invalid")
	ErrKafkaGroupIDInvalid         = errors.New("lazuli/queues: kafka group id is invalid")
	ErrKafkaClientIDInvalid        = errors.New("lazuli/queues: kafka client id is invalid")
	ErrKafkaPartitionKeyInvalid    = errors.New("lazuli/queues: kafka partition/key metadata is invalid")
	ErrKafkaDeliveryTimeoutInvalid = errors.New("lazuli/queues: kafka delivery timeout is invalid")
	ErrKafkaTLSInvalid             = errors.New("lazuli/queues: kafka tls metadata is invalid")
	ErrKafkaSASLInvalid            = errors.New("lazuli/queues: kafka sasl metadata is invalid")
)

type KafkaDescriptor struct {
	Brokers         []string
	Topic           string
	GroupID         string
	ClientID        string
	Partition       int
	Key             string
	DeliveryTimeout time.Duration
	TLS             KafkaTLSMetadata
	SASL            KafkaSASLMetadata
}

type KafkaTLSMetadata struct {
	Enabled    bool
	ServerName string
	CAFile     string
	CertFile   string
	KeyFile    string
}

type KafkaSASLMetadata struct {
	Mechanism string
	Username  string
	Password  string
	Token     string
}

type KafkaPlan struct {
	Descriptor KafkaDescriptor
	Summary    KafkaDescriptorSummary
}

type KafkaDescriptorSummary struct {
	Brokers                []string
	Topic                  string
	GroupID                string
	ClientID               string
	Partition              int
	KeySet                 bool
	DeliveryTimeout        string
	TLSEnabled             bool
	TLSServerName          string
	TLSClientCertificate   bool
	TLSCustomCA            bool
	SASLEnabled            bool
	SASLMechanism          string
	SASLUsername           string
	SASLPasswordConfigured bool
	SASLTokenConfigured    bool
}

func (d KafkaDescriptor) Normalize() KafkaDescriptor {
	return NormalizeKafkaDescriptor(d)
}

func (d KafkaDescriptor) Validate() error {
	return ValidateKafkaDescriptor(d)
}

func (d KafkaDescriptor) Summary() KafkaDescriptorSummary {
	return RedactedKafkaDescriptorSummary(d)
}

func NormalizeKafkaDescriptor(desc KafkaDescriptor) KafkaDescriptor {
	desc.Brokers = NormalizeKafkaBrokerAddresses(desc.Brokers)
	desc.Topic = strings.TrimSpace(desc.Topic)
	desc.GroupID = strings.TrimSpace(desc.GroupID)
	desc.ClientID = strings.TrimSpace(desc.ClientID)
	desc.Key = strings.TrimSpace(desc.Key)
	if desc.DeliveryTimeout == 0 {
		desc.DeliveryTimeout = DefaultKafkaDeliveryTimeout
	}
	desc.TLS = NormalizeKafkaTLSMetadata(desc.TLS)
	desc.SASL = NormalizeKafkaSASLMetadata(desc.SASL)
	return desc
}

func ValidateKafkaDescriptor(desc KafkaDescriptor) error {
	desc = NormalizeKafkaDescriptor(desc)

	var errs []error
	if len(desc.Brokers) == 0 {
		errs = append(errs, fmt.Errorf("%w: at least one broker is required", ErrKafkaBrokerInvalid))
	}
	for i, broker := range desc.Brokers {
		if err := ValidateKafkaBrokerAddress(broker); err != nil {
			errs = append(errs, fmt.Errorf("broker[%d]: %w", i, err))
		}
	}
	if err := ValidateKafkaTopic(desc.Topic); err != nil {
		errs = append(errs, err)
	}
	if desc.GroupID != "" {
		if err := ValidateKafkaGroupID(desc.GroupID); err != nil {
			errs = append(errs, err)
		}
	}
	if desc.ClientID != "" {
		if err := ValidateKafkaClientID(desc.ClientID); err != nil {
			errs = append(errs, err)
		}
	}
	if err := ValidateKafkaPartitionKeyMetadata(desc.Partition, desc.Key); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateKafkaDeliveryTimeout(desc.DeliveryTimeout); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateKafkaTLSMetadata(desc.TLS); err != nil {
		errs = append(errs, err)
	}
	if err := ValidateKafkaSASLMetadata(desc.SASL); err != nil {
		errs = append(errs, err)
	}
	return errors.Join(errs...)
}

func PlanKafkaDescriptor(desc KafkaDescriptor) (KafkaPlan, error) {
	normalized := NormalizeKafkaDescriptor(desc)
	plan := KafkaPlan{
		Descriptor: cloneKafkaDescriptor(normalized),
		Summary:    RedactedKafkaDescriptorSummary(normalized),
	}
	return plan, ValidateKafkaDescriptor(normalized)
}

func RedactedKafkaDescriptorSummary(desc KafkaDescriptor) KafkaDescriptorSummary {
	desc = NormalizeKafkaDescriptor(desc)
	return KafkaDescriptorSummary{
		Brokers:                redactKafkaBrokerAddresses(desc.Brokers),
		Topic:                  desc.Topic,
		GroupID:                desc.GroupID,
		ClientID:               desc.ClientID,
		Partition:              desc.Partition,
		KeySet:                 desc.Key != "",
		DeliveryTimeout:        desc.DeliveryTimeout.String(),
		TLSEnabled:             desc.TLS.Enabled,
		TLSServerName:          desc.TLS.ServerName,
		TLSClientCertificate:   desc.TLS.CertFile != "" || desc.TLS.KeyFile != "",
		TLSCustomCA:            desc.TLS.CAFile != "",
		SASLEnabled:            desc.SASL.Mechanism != "" || desc.SASL.Username != "" || desc.SASL.Password != "" || desc.SASL.Token != "",
		SASLMechanism:          desc.SASL.Mechanism,
		SASLUsername:           redactKafkaSecret(desc.SASL.Username),
		SASLPasswordConfigured: desc.SASL.Password != "",
		SASLTokenConfigured:    desc.SASL.Token != "",
	}
}

func NormalizeKafkaBrokerAddresses(brokers []string) []string {
	if len(brokers) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(brokers))
	normalized := make([]string, 0, len(brokers))
	for _, broker := range brokers {
		broker, err := NormalizeKafkaBrokerAddress(broker)
		if err != nil {
			broker = strings.TrimSpace(broker)
		}
		if broker == "" {
			continue
		}
		if _, ok := seen[broker]; ok {
			continue
		}
		seen[broker] = struct{}{}
		normalized = append(normalized, broker)
	}
	sort.Strings(normalized)
	return normalized
}

func NormalizeKafkaBrokerAddress(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", fmt.Errorf("%w: empty address", ErrKafkaBrokerInvalid)
	}

	hostport := raw
	if strings.Contains(raw, "://") {
		parsed, err := url.Parse(raw)
		if err != nil {
			return "", fmt.Errorf("%w: %v", ErrKafkaBrokerInvalid, err)
		}
		if parsed.User != nil || parsed.Path != "" && parsed.Path != "/" || parsed.RawQuery != "" || parsed.Fragment != "" {
			return "", fmt.Errorf("%w: broker url must only include scheme and host", ErrKafkaBrokerInvalid)
		}
		hostport = parsed.Host
	}

	host, port, err := kafkaSplitHostPort(hostport)
	if err != nil {
		return "", err
	}
	host = strings.ToLower(host)
	if port == "" {
		port = DefaultKafkaBrokerPort
	}
	normalized := net.JoinHostPort(host, port)
	if strings.Count(host, ":") == 0 {
		normalized = host + ":" + port
	}
	if err := ValidateKafkaBrokerAddress(normalized); err != nil {
		return "", err
	}
	return normalized, nil
}

func ValidateKafkaBrokerAddress(raw string) error {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return fmt.Errorf("%w: empty address", ErrKafkaBrokerInvalid)
	}
	if strings.ContainsAny(raw, " \t\r\n") {
		return fmt.Errorf("%w: address contains whitespace", ErrKafkaBrokerInvalid)
	}
	if strings.Contains(raw, "://") {
		parsed, err := url.Parse(raw)
		if err != nil {
			return fmt.Errorf("%w: %v", ErrKafkaBrokerInvalid, err)
		}
		if parsed.User != nil {
			return fmt.Errorf("%w: userinfo is not allowed", ErrKafkaBrokerInvalid)
		}
		if parsed.Path != "" && parsed.Path != "/" || parsed.RawQuery != "" || parsed.Fragment != "" {
			return fmt.Errorf("%w: broker url must only include scheme and host", ErrKafkaBrokerInvalid)
		}
		raw = parsed.Host
	}
	host, port, err := kafkaSplitHostPort(raw)
	if err != nil {
		return err
	}
	if host == "" {
		return fmt.Errorf("%w: host is required", ErrKafkaBrokerInvalid)
	}
	if strings.ContainsAny(host, "/\\") {
		return fmt.Errorf("%w: host contains a reserved character", ErrKafkaBrokerInvalid)
	}
	if port == "" {
		return nil
	}
	if _, err := net.LookupPort("tcp", port); err != nil {
		return fmt.Errorf("%w: port %q is invalid", ErrKafkaBrokerInvalid, port)
	}
	return nil
}

func ValidateKafkaTopic(topic string) error {
	topic = strings.TrimSpace(topic)
	if topic == "" {
		return fmt.Errorf("%w: topic is required", ErrKafkaTopicInvalid)
	}
	if len(topic) > 249 {
		return fmt.Errorf("%w: %q exceeds 249 bytes", ErrKafkaTopicInvalid, topic)
	}
	if topic == "." || topic == ".." {
		return fmt.Errorf("%w: %q is reserved", ErrKafkaTopicInvalid, topic)
	}
	if !validKafkaID(topic) {
		return fmt.Errorf("%w: %q", ErrKafkaTopicInvalid, topic)
	}
	return nil
}

func ValidateKafkaGroupID(groupID string) error {
	groupID = strings.TrimSpace(groupID)
	if groupID == "" {
		return fmt.Errorf("%w: group id is required", ErrKafkaGroupIDInvalid)
	}
	if len(groupID) > 249 || !validKafkaID(groupID) {
		return fmt.Errorf("%w: %q", ErrKafkaGroupIDInvalid, groupID)
	}
	return nil
}

func ValidateKafkaClientID(clientID string) error {
	clientID = strings.TrimSpace(clientID)
	if clientID == "" {
		return fmt.Errorf("%w: client id is required", ErrKafkaClientIDInvalid)
	}
	if len(clientID) > 249 || strings.ContainsAny(clientID, "\x00\r\n\t") {
		return fmt.Errorf("%w: %q", ErrKafkaClientIDInvalid, clientID)
	}
	return nil
}

func ValidateKafkaPartitionKeyMetadata(partition int, key string) error {
	if partition < KafkaAnyPartition {
		return fmt.Errorf("%w: partition must be -1 or greater", ErrKafkaPartitionKeyInvalid)
	}
	if strings.ContainsAny(key, "\x00\r\n") {
		return fmt.Errorf("%w: key contains a control character", ErrKafkaPartitionKeyInvalid)
	}
	return nil
}

func ValidateKafkaDeliveryTimeout(timeout time.Duration) error {
	if timeout < MinKafkaDeliveryTimeout || timeout > MaxKafkaDeliveryTimeout {
		return fmt.Errorf("%w: must be between %s and %s", ErrKafkaDeliveryTimeoutInvalid, MinKafkaDeliveryTimeout, MaxKafkaDeliveryTimeout)
	}
	return nil
}

func NormalizeKafkaTLSMetadata(meta KafkaTLSMetadata) KafkaTLSMetadata {
	meta.ServerName = strings.ToLower(strings.TrimSpace(meta.ServerName))
	meta.CAFile = strings.TrimSpace(meta.CAFile)
	meta.CertFile = strings.TrimSpace(meta.CertFile)
	meta.KeyFile = strings.TrimSpace(meta.KeyFile)
	if meta.ServerName != "" || meta.CAFile != "" || meta.CertFile != "" || meta.KeyFile != "" {
		meta.Enabled = true
	}
	return meta
}

func ValidateKafkaTLSMetadata(meta KafkaTLSMetadata) error {
	meta = NormalizeKafkaTLSMetadata(meta)
	if !meta.Enabled {
		return nil
	}
	var errs []error
	if meta.CertFile != "" && meta.KeyFile == "" || meta.CertFile == "" && meta.KeyFile != "" {
		errs = append(errs, fmt.Errorf("%w: cert file and key file must be set together", ErrKafkaTLSInvalid))
	}
	if strings.ContainsAny(meta.ServerName, " \t\r\n/\\") {
		errs = append(errs, fmt.Errorf("%w: server name %q is invalid", ErrKafkaTLSInvalid, meta.ServerName))
	}
	return errors.Join(errs...)
}

func NormalizeKafkaSASLMetadata(meta KafkaSASLMetadata) KafkaSASLMetadata {
	meta.Mechanism = strings.ToUpper(strings.TrimSpace(meta.Mechanism))
	meta.Username = strings.TrimSpace(meta.Username)
	meta.Password = strings.TrimSpace(meta.Password)
	meta.Token = strings.TrimSpace(meta.Token)
	return meta
}

func ValidateKafkaSASLMetadata(meta KafkaSASLMetadata) error {
	meta = NormalizeKafkaSASLMetadata(meta)
	if meta.Mechanism == "" && meta.Username == "" && meta.Password == "" && meta.Token == "" {
		return nil
	}
	var errs []error
	switch meta.Mechanism {
	case "PLAIN", "SCRAM-SHA-256", "SCRAM-SHA-512":
		if meta.Username == "" || meta.Password == "" {
			errs = append(errs, fmt.Errorf("%w: %s requires username and password", ErrKafkaSASLInvalid, meta.Mechanism))
		}
		if meta.Token != "" {
			errs = append(errs, fmt.Errorf("%w: %s must not set token", ErrKafkaSASLInvalid, meta.Mechanism))
		}
	case "OAUTHBEARER":
		if meta.Token == "" {
			errs = append(errs, fmt.Errorf("%w: oauthbearer requires token", ErrKafkaSASLInvalid))
		}
		if meta.Password != "" {
			errs = append(errs, fmt.Errorf("%w: oauthbearer must not set password", ErrKafkaSASLInvalid))
		}
	default:
		errs = append(errs, fmt.Errorf("%w: mechanism %q is unsupported", ErrKafkaSASLInvalid, meta.Mechanism))
	}
	return errors.Join(errs...)
}

func RedactKafkaBrokerAddress(raw string) string {
	raw = strings.TrimSpace(raw)
	parsed, err := url.Parse(raw)
	if err != nil || parsed.User == nil {
		return raw
	}
	parsed.User = url.UserPassword("[REDACTED]", "[REDACTED]")
	return parsed.String()
}

func redactKafkaBrokerAddresses(brokers []string) []string {
	if len(brokers) == 0 {
		return nil
	}
	redacted := make([]string, 0, len(brokers))
	for _, broker := range brokers {
		redacted = append(redacted, RedactKafkaBrokerAddress(broker))
	}
	sort.Strings(redacted)
	return redacted
}

func redactKafkaSecret(value string) string {
	if value == "" {
		return ""
	}
	return "[REDACTED]"
}

func cloneKafkaDescriptor(desc KafkaDescriptor) KafkaDescriptor {
	desc.Brokers = append([]string(nil), desc.Brokers...)
	return desc
}

func kafkaSplitHostPort(raw string) (string, string, error) {
	if strings.HasPrefix(raw, "[") {
		host, port, err := net.SplitHostPort(raw)
		if err != nil {
			return "", "", fmt.Errorf("%w: %v", ErrKafkaBrokerInvalid, err)
		}
		return strings.Trim(host, "[]"), port, nil
	}
	if strings.Count(raw, ":") > 1 {
		return raw, "", nil
	}
	host, port, err := net.SplitHostPort(raw)
	if err == nil {
		return host, port, nil
	}
	if strings.Contains(raw, ":") {
		return "", "", fmt.Errorf("%w: %v", ErrKafkaBrokerInvalid, err)
	}
	return raw, "", nil
}

func validKafkaID(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		if r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_' || r == '-' || r == '.' {
			continue
		}
		return false
	}
	return true
}
