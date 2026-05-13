package webhooks

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"
	"unicode"
)

var (
	// ErrNilWebhookEventRegistry is returned when registering without a registry.
	ErrNilWebhookEventRegistry = errors.New("webhooks: event registry is nil")

	// ErrInvalidWebhookEventDescriptor reports structurally invalid generated
	// webhook event metadata.
	ErrInvalidWebhookEventDescriptor = errors.New("webhooks: invalid event descriptor")

	// ErrDuplicateWebhookEventDescriptor reports duplicate webhook event names.
	ErrDuplicateWebhookEventDescriptor = errors.New("webhooks: duplicate event descriptor")
)

// WebhookPayloadSchemaRef names a generated payload schema for one inbound
// webhook event.
type WebhookPayloadSchemaRef string

// String returns the raw schema reference.
func (r WebhookPayloadSchemaRef) String() string {
	return string(r)
}

// WebhookEventVersion carries schema version metadata for a generated webhook
// event descriptor.
type WebhookEventVersion struct {
	// Version is the current payload schema version for this event.
	Version string

	// IntroducedIn optionally records the Lazuli schema/runtime version that
	// first emitted this event shape.
	IntroducedIn string

	// Deprecated marks the event shape as retained for compatibility but no
	// longer preferred.
	Deprecated bool

	// DeprecatedIn optionally records the Lazuli schema/runtime version that
	// deprecated this event shape.
	DeprecatedIn string

	// ReplacedBy optionally names the preferred event descriptor.
	ReplacedBy string
}

// WebhookEventDescriptor describes one generated webhook_events.<name>
// declaration. Generated webhook contracts can reference Name through
// WebhookEventRef while tooling can read the schema and version metadata here.
type WebhookEventDescriptor struct {
	// Feature is optional owning feature metadata for docs and diagnostics.
	Feature string

	// Name is the stable webhook event name referenced by payload from
	// webhook_events.<name>.
	Name string

	// PayloadSchemaRef points at the generated payload schema/type.
	PayloadSchemaRef WebhookPayloadSchemaRef

	// Version carries payload schema lifecycle metadata.
	Version WebhookEventVersion

	// Summary is optional one-line docs text for the event.
	Summary string
}

// WebhookEventDocSummary is the normalized, deterministic row shape used by
// docs generators.
type WebhookEventDocSummary struct {
	Feature          string
	Name             string
	PayloadSchemaRef WebhookPayloadSchemaRef
	Version          string
	IntroducedIn     string
	Deprecated       bool
	DeprecatedIn     string
	ReplacedBy       string
	Summary          string
}

// WebhookEventRegistry stores generated webhook event descriptors.
//
// The zero value is ready to use.
type WebhookEventRegistry struct {
	mu     sync.RWMutex
	events map[string]WebhookEventDescriptor
}

// NewWebhookEventRegistry returns an empty webhook event registry.
func NewWebhookEventRegistry() *WebhookEventRegistry {
	return &WebhookEventRegistry{}
}

// Register records one webhook event descriptor.
func (r *WebhookEventRegistry) Register(descriptor WebhookEventDescriptor) error {
	if r == nil {
		return ErrNilWebhookEventRegistry
	}

	normalized, err := normalizeWebhookEventDescriptor(descriptor, -1)
	if err != nil {
		return err
	}
	key := normalized.registryKey()

	r.mu.Lock()
	defer r.mu.Unlock()
	if r.events == nil {
		r.events = make(map[string]WebhookEventDescriptor)
	}
	if _, ok := r.events[key]; ok {
		return fmt.Errorf("%w: %q", ErrDuplicateWebhookEventDescriptor, normalized.Name)
	}
	r.events[key] = normalized
	return nil
}

// Lookup returns the descriptor registered for name.
func (r *WebhookEventRegistry) Lookup(name string) (WebhookEventDescriptor, bool) {
	if r == nil {
		return WebhookEventDescriptor{}, false
	}

	name = strings.TrimSpace(name)
	r.mu.RLock()
	defer r.mu.RUnlock()
	descriptor, ok := r.events[name]
	return descriptor, ok
}

// Events returns registered descriptors in deterministic order.
func (r *WebhookEventRegistry) Events() []WebhookEventDescriptor {
	if r == nil {
		return nil
	}

	r.mu.RLock()
	defer r.mu.RUnlock()

	out := make([]WebhookEventDescriptor, 0, len(r.events))
	for _, descriptor := range r.events {
		out = append(out, descriptor)
	}
	sortWebhookEventDescriptors(out)
	return out
}

// DocSummaries returns normalized docs summary rows in deterministic order.
func (r *WebhookEventRegistry) DocSummaries() []WebhookEventDocSummary {
	return webhookEventDocSummariesFromSorted(r.Events())
}

// ValidateWebhookEventDescriptors checks descriptors without mutating the input
// slice.
func ValidateWebhookEventDescriptors(descriptors []WebhookEventDescriptor) error {
	_, err := normalizeWebhookEventDescriptors(descriptors)
	return err
}

// SortedWebhookEventDescriptors returns a validated, normalized, deterministic
// copy of descriptors.
func SortedWebhookEventDescriptors(descriptors []WebhookEventDescriptor) ([]WebhookEventDescriptor, error) {
	normalized, err := normalizeWebhookEventDescriptors(descriptors)
	if err != nil {
		return nil, err
	}
	sortWebhookEventDescriptors(normalized)
	return normalized, nil
}

// WebhookEventDocSummaries returns normalized docs summary rows in
// deterministic order.
func WebhookEventDocSummaries(descriptors []WebhookEventDescriptor) ([]WebhookEventDocSummary, error) {
	normalized, err := SortedWebhookEventDescriptors(descriptors)
	if err != nil {
		return nil, err
	}
	return webhookEventDocSummariesFromSorted(normalized), nil
}

func normalizeWebhookEventDescriptors(descriptors []WebhookEventDescriptor) ([]WebhookEventDescriptor, error) {
	normalized := make([]WebhookEventDescriptor, 0, len(descriptors))
	seen := make(map[string]int, len(descriptors))

	var errs []error
	for i, descriptor := range descriptors {
		clean, err := normalizeWebhookEventDescriptor(descriptor, i)
		if err != nil {
			errs = append(errs, err)
			continue
		}

		key := clean.registryKey()
		if first, ok := seen[key]; ok {
			errs = append(errs, fmt.Errorf("%w: event[%d] %q also appears at event[%d]", ErrDuplicateWebhookEventDescriptor, i, clean.Name, first))
			continue
		}
		seen[key] = i
		normalized = append(normalized, clean)
	}

	if err := errors.Join(errs...); err != nil {
		return nil, err
	}
	return normalized, nil
}

func normalizeWebhookEventDescriptor(descriptor WebhookEventDescriptor, index int) (WebhookEventDescriptor, error) {
	clean := WebhookEventDescriptor{
		Feature:          strings.TrimSpace(descriptor.Feature),
		Name:             strings.TrimSpace(descriptor.Name),
		PayloadSchemaRef: WebhookPayloadSchemaRef(strings.TrimSpace(descriptor.PayloadSchemaRef.String())),
		Version:          normalizeWebhookEventVersion(descriptor.Version),
		Summary:          strings.TrimSpace(descriptor.Summary),
	}

	var errs []error
	if clean.Name == "" {
		errs = append(errs, invalidWebhookEventField(index, "name", "is required"))
	} else if hasWebhookEventControl(clean.Name) || strings.ContainsFunc(clean.Name, unicode.IsSpace) {
		errs = append(errs, invalidWebhookEventField(index, "name", "must not contain whitespace or control characters"))
	}
	if clean.PayloadSchemaRef == "" {
		errs = append(errs, invalidWebhookEventField(index, "payload_schema_ref", "is required"))
	} else if hasWebhookEventControl(clean.PayloadSchemaRef.String()) || strings.ContainsFunc(clean.PayloadSchemaRef.String(), unicode.IsSpace) {
		errs = append(errs, invalidWebhookEventField(index, "payload_schema_ref", "must not contain whitespace or control characters"))
	}
	if clean.Version.Version == "" {
		errs = append(errs, invalidWebhookEventField(index, "version.version", "is required"))
	} else if !validWebhookEventToken(clean.Version.Version) {
		errs = append(errs, invalidWebhookEventField(index, "version.version", "must not contain whitespace or control characters"))
	}
	if clean.Feature != "" && hasWebhookEventControl(clean.Feature) {
		errs = append(errs, invalidWebhookEventField(index, "feature", "contains control characters"))
	}
	if hasWebhookEventControl(clean.Summary) {
		errs = append(errs, invalidWebhookEventField(index, "summary", "contains control characters"))
	}
	if clean.Version.IntroducedIn != "" && !validWebhookEventToken(clean.Version.IntroducedIn) {
		errs = append(errs, invalidWebhookEventField(index, "version.introduced_in", "must not contain whitespace or control characters"))
	}
	if clean.Version.DeprecatedIn != "" && !validWebhookEventToken(clean.Version.DeprecatedIn) {
		errs = append(errs, invalidWebhookEventField(index, "version.deprecated_in", "must not contain whitespace or control characters"))
	}
	if clean.Version.ReplacedBy != "" && !validWebhookEventToken(clean.Version.ReplacedBy) {
		errs = append(errs, invalidWebhookEventField(index, "version.replaced_by", "must not contain whitespace or control characters"))
	}

	if err := errors.Join(errs...); err != nil {
		return WebhookEventDescriptor{}, err
	}
	return clean, nil
}

func normalizeWebhookEventVersion(version WebhookEventVersion) WebhookEventVersion {
	clean := WebhookEventVersion{
		Version:      strings.TrimSpace(version.Version),
		IntroducedIn: strings.TrimSpace(version.IntroducedIn),
		Deprecated:   version.Deprecated,
		DeprecatedIn: strings.TrimSpace(version.DeprecatedIn),
		ReplacedBy:   strings.TrimSpace(version.ReplacedBy),
	}
	if clean.DeprecatedIn != "" || clean.ReplacedBy != "" {
		clean.Deprecated = true
	}
	return clean
}

func (d WebhookEventDescriptor) registryKey() string {
	return d.Name
}

func sortWebhookEventDescriptors(descriptors []WebhookEventDescriptor) {
	sort.SliceStable(descriptors, func(i, j int) bool {
		left := descriptors[i]
		right := descriptors[j]
		for _, cmp := range []int{
			compareWebhookEventString(left.Feature, right.Feature),
			compareWebhookEventString(left.Name, right.Name),
			compareWebhookEventString(left.Version.Version, right.Version.Version),
			compareWebhookEventString(left.PayloadSchemaRef.String(), right.PayloadSchemaRef.String()),
		} {
			if cmp != 0 {
				return cmp < 0
			}
		}
		return false
	})
}

func webhookEventDocSummariesFromSorted(descriptors []WebhookEventDescriptor) []WebhookEventDocSummary {
	summaries := make([]WebhookEventDocSummary, 0, len(descriptors))
	for _, descriptor := range descriptors {
		summaries = append(summaries, WebhookEventDocSummary{
			Feature:          descriptor.Feature,
			Name:             descriptor.Name,
			PayloadSchemaRef: descriptor.PayloadSchemaRef,
			Version:          descriptor.Version.Version,
			IntroducedIn:     descriptor.Version.IntroducedIn,
			Deprecated:       descriptor.Version.Deprecated,
			DeprecatedIn:     descriptor.Version.DeprecatedIn,
			ReplacedBy:       descriptor.Version.ReplacedBy,
			Summary:          descriptor.Summary,
		})
	}
	return summaries
}

func invalidWebhookEventField(index int, field, reason string) error {
	if index >= 0 {
		return fmt.Errorf("%w: event[%d].%s %s", ErrInvalidWebhookEventDescriptor, index, field, reason)
	}
	return fmt.Errorf("%w: event.%s %s", ErrInvalidWebhookEventDescriptor, field, reason)
}

func validWebhookEventToken(value string) bool {
	return value != "" && !hasWebhookEventControl(value) && !strings.ContainsFunc(value, unicode.IsSpace)
}

func hasWebhookEventControl(value string) bool {
	for _, r := range value {
		if unicode.IsControl(r) {
			return true
		}
	}
	return false
}

func compareWebhookEventString(left, right string) int {
	leftFold := strings.ToLower(left)
	rightFold := strings.ToLower(right)
	switch {
	case leftFold < rightFold:
		return -1
	case leftFold > rightFold:
		return 1
	case left < right:
		return -1
	case left > right:
		return 1
	default:
		return 0
	}
}
