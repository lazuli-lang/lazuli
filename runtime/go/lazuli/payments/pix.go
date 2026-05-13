package payments

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

const (
	pixMaxMerchantName = 25
	pixMaxMerchantCity = 15
	pixMaxTxID         = 25
)

var (
	// ErrPIXDescriptorInvalid is returned when a provider-neutral PIX
	// descriptor cannot be normalized into a portable request shape.
	ErrPIXDescriptorInvalid = errors.New("payments: pix descriptor invalid")
)

// PIXKeyType names the kind of Brazilian PIX key a descriptor references.
type PIXKeyType string

const (
	PIXKeyTypeCPF    PIXKeyType = "cpf"
	PIXKeyTypeCNPJ   PIXKeyType = "cnpj"
	PIXKeyTypeEmail  PIXKeyType = "email"
	PIXKeyTypePhone  PIXKeyType = "phone"
	PIXKeyTypeRandom PIXKeyType = "random"
)

// Valid reports whether the PIX key type is known.
func (t PIXKeyType) Valid() bool {
	switch t {
	case PIXKeyTypeCPF, PIXKeyTypeCNPJ, PIXKeyTypeEmail, PIXKeyTypePhone, PIXKeyTypeRandom:
		return true
	default:
		return false
	}
}

// PIXExpiration carries provider-neutral expiration metadata. CreatedAt and
// ExpiresAt are absolute instants; TTL is derived during normalization.
type PIXExpiration struct {
	CreatedAt time.Time
	ExpiresAt time.Time
	TTL       time.Duration
}

// Validate checks that expiration metadata is internally coherent when set.
func (e PIXExpiration) Validate() error {
	if e.CreatedAt.IsZero() && e.ExpiresAt.IsZero() && e.TTL == 0 {
		return nil
	}
	if e.CreatedAt.IsZero() {
		return fmt.Errorf("%w: expiration created_at must be set", ErrPIXDescriptorInvalid)
	}
	if e.ExpiresAt.IsZero() {
		return fmt.Errorf("%w: expiration expires_at must be set", ErrPIXDescriptorInvalid)
	}
	if !e.CreatedAt.Before(e.ExpiresAt) {
		return fmt.Errorf("%w: expiration created_at must be before expires_at", ErrPIXDescriptorInvalid)
	}
	if e.TTL <= 0 {
		return fmt.Errorf("%w: expiration ttl must be positive", ErrPIXDescriptorInvalid)
	}
	if e.TTL != e.ExpiresAt.Sub(e.CreatedAt) {
		return fmt.Errorf("%w: expiration ttl must match expires_at minus created_at", ErrPIXDescriptorInvalid)
	}
	return nil
}

// PIXDescriptor is a provider-neutral Brazilian PIX payment descriptor. It is
// intentionally not a QR payload; adapters may translate it to their provider's
// checkout, copy-and-paste, or QR request shape.
type PIXDescriptor struct {
	KeyType      PIXKeyType
	Key          string
	MerchantName string
	MerchantCity string
	TxID         string
	Amount       Money
	Expiration   PIXExpiration
	Metadata     map[string]string
}

// NormalizePIXDescriptor returns a copy with deterministic casing, spacing,
// currency fallback, expiration TTL, and cloned metadata.
func NormalizePIXDescriptor(desc PIXDescriptor) PIXDescriptor {
	desc.KeyType = normalizePIXKeyType(desc.KeyType)
	desc.Key = strings.TrimSpace(desc.Key)
	desc.MerchantName = normalizePIXText(desc.MerchantName)
	desc.MerchantCity = normalizePIXText(desc.MerchantCity)
	desc.TxID = strings.TrimSpace(desc.TxID)
	desc.Amount.Currency = strings.ToUpper(strings.TrimSpace(desc.Amount.Currency))
	desc.Expiration = normalizePIXExpiration(desc.Expiration)
	desc.Metadata = cloneStringMap(desc.Metadata)
	return desc
}

// ValidatePIXDescriptor checks that desc is portable across PIX providers.
func ValidatePIXDescriptor(desc PIXDescriptor) error {
	return NormalizePIXDescriptor(desc).Validate()
}

// Validate checks that the normalized PIX descriptor is portable across PIX
// providers without depending on a QR generation library.
func (d PIXDescriptor) Validate() error {
	if !d.KeyType.Valid() {
		return fmt.Errorf("%w: key_type %q is unknown", ErrPIXDescriptorInvalid, d.KeyType)
	}
	if d.Key == "" {
		return fmt.Errorf("%w: key must be non-empty", ErrPIXDescriptorInvalid)
	}
	if d.MerchantName == "" {
		return fmt.Errorf("%w: merchant_name must be non-empty", ErrPIXDescriptorInvalid)
	}
	if utf8.RuneCountInString(d.MerchantName) > pixMaxMerchantName {
		return fmt.Errorf("%w: merchant_name must be at most %d characters", ErrPIXDescriptorInvalid, pixMaxMerchantName)
	}
	if d.MerchantCity == "" {
		return fmt.Errorf("%w: merchant_city must be non-empty", ErrPIXDescriptorInvalid)
	}
	if utf8.RuneCountInString(d.MerchantCity) > pixMaxMerchantCity {
		return fmt.Errorf("%w: merchant_city must be at most %d characters", ErrPIXDescriptorInvalid, pixMaxMerchantCity)
	}
	if d.TxID == "" {
		return fmt.Errorf("%w: txid must be non-empty", ErrPIXDescriptorInvalid)
	}
	if utf8.RuneCountInString(d.TxID) > pixMaxTxID {
		return fmt.Errorf("%w: txid must be at most %d characters", ErrPIXDescriptorInvalid, pixMaxTxID)
	}
	if !pixTxIDPortable(d.TxID) {
		return fmt.Errorf("%w: txid must contain only ASCII letters and digits", ErrPIXDescriptorInvalid)
	}
	if d.Amount.Amount <= 0 {
		return fmt.Errorf("%w: amount must be positive", ErrPIXDescriptorInvalid)
	}
	if d.Amount.Currency != "BRL" {
		return fmt.Errorf("%w: amount currency must be BRL", ErrPIXDescriptorInvalid)
	}
	if err := d.Expiration.Validate(); err != nil {
		return err
	}
	return nil
}

// PIXDescriptorSummary is safe for logs, diagnostics, and dry-run output.
type PIXDescriptorSummary struct {
	KeyType      PIXKeyType
	Key          string
	MerchantName string
	MerchantCity string
	TxID         string
	Amount       Money
	Expiration   PIXExpiration
	Metadata     map[string]string
}

// SafeSummary returns a redacted, deterministic summary of the descriptor.
func (d PIXDescriptor) SafeSummary() PIXDescriptorSummary {
	d = NormalizePIXDescriptor(d)
	return PIXDescriptorSummary{
		KeyType:      d.KeyType,
		Key:          redactPIXSecret(d.Key),
		MerchantName: d.MerchantName,
		MerchantCity: d.MerchantCity,
		TxID:         d.TxID,
		Amount:       d.Amount,
		Expiration:   d.Expiration,
		Metadata:     redactPIXMetadata(d.Metadata),
	}
}

// PIXDescriptorPlan is a dry-run result that callers can inspect before
// passing the descriptor to a gateway adapter.
type PIXDescriptorPlan struct {
	Descriptor PIXDescriptor
	Summary    PIXDescriptorSummary
}

// Validate checks that the plan contains a usable normalized descriptor and
// its safe summary.
func (p PIXDescriptorPlan) Validate() error {
	if err := p.Descriptor.Validate(); err != nil {
		return err
	}
	if p.Summary.Key == "" {
		return fmt.Errorf("%w: summary key must be set", ErrPIXDescriptorInvalid)
	}
	return nil
}

// BuildPIXDescriptorPlan normalizes and validates a provider-neutral PIX
// descriptor without mutating provider state.
func BuildPIXDescriptorPlan(desc PIXDescriptor) (PIXDescriptorPlan, error) {
	normalized := NormalizePIXDescriptor(desc)
	if err := normalized.Validate(); err != nil {
		return PIXDescriptorPlan{}, err
	}
	plan := PIXDescriptorPlan{
		Descriptor: normalized,
		Summary:    normalized.SafeSummary(),
	}
	if err := plan.Validate(); err != nil {
		return PIXDescriptorPlan{}, err
	}
	return plan, nil
}

func normalizePIXKeyType(t PIXKeyType) PIXKeyType {
	value := strings.ToLower(strings.TrimSpace(string(t)))
	switch value {
	case "evp":
		return PIXKeyTypeRandom
	default:
		return PIXKeyType(value)
	}
}

func normalizePIXText(value string) string {
	return strings.ToUpper(strings.Join(strings.Fields(value), " "))
}

func normalizePIXExpiration(expiration PIXExpiration) PIXExpiration {
	if !expiration.CreatedAt.IsZero() && !expiration.ExpiresAt.IsZero() {
		expiration.TTL = expiration.ExpiresAt.Sub(expiration.CreatedAt)
	}
	return expiration
}

func pixTxIDPortable(value string) bool {
	for _, r := range value {
		if r > unicode.MaxASCII || !unicode.IsLetter(r) && !unicode.IsDigit(r) {
			return false
		}
	}
	return true
}

func cloneStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	clone := make(map[string]string, len(values))
	for key, value := range values {
		clone[strings.TrimSpace(key)] = strings.TrimSpace(value)
	}
	return clone
}

func redactPIXMetadata(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	redacted := make(map[string]string, len(values))
	for _, key := range keys {
		value := strings.TrimSpace(values[key])
		switch {
		case pixMetadataKeySensitive(key), pixMetadataValueURL(value):
			redacted[key] = "[redacted]"
		default:
			redacted[key] = value
		}
	}
	return redacted
}

func pixMetadataKeySensitive(key string) bool {
	key = strings.ToLower(strings.TrimSpace(key))
	return strings.Contains(key, "secret") ||
		strings.Contains(key, "token") ||
		strings.Contains(key, "password") ||
		strings.Contains(key, "credential") ||
		strings.Contains(key, "url") ||
		strings.Contains(key, "uri") ||
		strings.Contains(key, "endpoint") ||
		strings.Contains(key, "webhook")
}

func pixMetadataValueURL(value string) bool {
	parsed, err := url.Parse(value)
	return err == nil && parsed.Scheme != "" && parsed.Host != ""
}

func redactPIXSecret(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return ""
	}
	runes := []rune(value)
	if len(runes) <= 4 {
		return "[redacted]"
	}
	return string(runes[:2]) + strings.Repeat("*", len(runes)-4) + string(runes[len(runes)-2:])
}
