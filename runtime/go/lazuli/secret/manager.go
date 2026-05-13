package secret

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
	"unicode"
)

// ProviderName is the provider-neutral name for a secret manager backend.
//
// Examples include "aws.secretsmanager", "gcp.secretmanager", or a Lazuli
// adapter reference such as "@runtime/local". Lazuli core treats the name as
// metadata and does not load any provider SDK from it.
type ProviderName string

// SourceName is the stable application-facing name for a group of managed
// secrets.
//
// Source names let generated code and adapters agree on a logical source
// without embedding cloud account, project, or vault details in Lazuli source.
type SourceName string

// PathTemplate renders provider-side secret paths from descriptor metadata and
// a SecretRef.
//
// Supported placeholders are {provider}, {source}, {name}, {version}, and
// {revision}. Templates are parsed by Lazuli and are not provider SDK syntax.
type PathTemplate string

// Validate checks that template uses supported placeholders and safe path text.
func (template PathTemplate) Validate() error {
	_, err := normalizePathTemplate(template)
	return err
}

// Render renders template using provider-neutral path values.
func (template PathTemplate) Render(values ManagerPathValues) (string, error) {
	return RenderManagerPath(template, values)
}

var (
	// ErrManagerProviderRequired is returned when a manager descriptor has no provider name.
	ErrManagerProviderRequired = errors.New("lazuli/secret: manager provider required")
	// ErrManagerSourceRequired is returned when a manager descriptor has no source name.
	ErrManagerSourceRequired = errors.New("lazuli/secret: manager source required")
	// ErrManagerPathTemplateRequired is returned when a manager descriptor has no path template.
	ErrManagerPathTemplateRequired = errors.New("lazuli/secret: manager path template required")
	// ErrManagerPathTemplateInvalid is returned when a manager path template cannot be rendered safely.
	ErrManagerPathTemplateInvalid = errors.New("lazuli/secret: manager path template invalid")
	// ErrManagerVersionPinInvalid is returned when a manager version pin is malformed.
	ErrManagerVersionPinInvalid = errors.New("lazuli/secret: manager version pin invalid")
	// ErrManagerRotationInvalid is returned when manager rotation metadata is malformed.
	ErrManagerRotationInvalid = errors.New("lazuli/secret: manager rotation invalid")
	// ErrDuplicateManagerSource is returned when a catalog contains the same source twice.
	ErrDuplicateManagerSource = errors.New("lazuli/secret: duplicate manager source")
)

// VersionPin describes a provider-neutral version label pinned to an optional
// opaque provider revision.
//
// Label is the Lazuli-facing label used by SecretRef, such as "active" or
// "previous". Revision is an adapter-owned immutable version identifier.
// Lazuli validates it as safe metadata but does not interpret it.
type VersionPin struct {
	Label    VersionLabel
	Revision string
}

// PinVersion returns a version pin for label.
func PinVersion(label VersionLabel) VersionPin {
	return VersionPin{Label: label}
}

// PinRevision returns a version pin for an opaque provider revision.
func PinRevision(revision string) VersionPin {
	return VersionPin{Revision: revision}
}

// IsZero reports whether pin carries no version metadata.
func (pin VersionPin) IsZero() bool {
	return strings.TrimSpace(string(pin.Label)) == "" && strings.TrimSpace(pin.Revision) == ""
}

// Validate checks that pin can be attached to a manager descriptor.
func (pin VersionPin) Validate() error {
	_, err := normalizeVersionPin(pin)
	return err
}

// Apply returns ref with pin.Label applied when ref has no explicit version.
func (pin VersionPin) Apply(ref SecretRef) SecretRef {
	pin, _ = normalizeVersionPin(pin)
	ref.Version = VersionLabel(strings.TrimSpace(string(ref.Version)))
	if ref.Version == "" {
		ref.Version = pin.Label
	}
	return ref
}

// RotationMetadata describes provider-neutral secret rotation metadata for a
// manager descriptor.
//
// The metadata is descriptive: adapters and deploy tooling may use it to
// verify rotation posture, while Lazuli core does not rotate provider secrets.
type RotationMetadata struct {
	Purpose       string
	Cadence       time.Duration
	Overlap       time.Duration
	LastRotatedAt time.Time
	NextRotatesAt time.Time
}

// IsZero reports whether metadata carries no rotation information.
func (m RotationMetadata) IsZero() bool {
	return strings.TrimSpace(m.Purpose) == "" &&
		m.Cadence == 0 &&
		m.Overlap == 0 &&
		m.LastRotatedAt.IsZero() &&
		m.NextRotatesAt.IsZero()
}

// Validate checks that rotation metadata is coherent.
func (m RotationMetadata) Validate() error {
	_, err := normalizeRotationMetadata(m)
	return err
}

// ManagerDescriptor describes one provider-neutral secret manager source.
//
// The descriptor carries enough metadata for generated code, deployment
// adapters, and diagnostics to agree on where a logical SecretRef should live
// without linking a provider SDK into Lazuli core.
type ManagerDescriptor struct {
	Provider     ProviderName
	Source       SourceName
	PathTemplate PathTemplate
	VersionPin   VersionPin
	Rotation     RotationMetadata
}

// Manager returns a manager descriptor for provider, source, and pathTemplate.
func Manager(provider, source, pathTemplate string) ManagerDescriptor {
	return ManagerDescriptor{
		Provider:     ProviderName(provider),
		Source:       SourceName(source),
		PathTemplate: PathTemplate(pathTemplate),
	}
}

// ValidateManagerDescriptor checks that descriptor can render safe secret
// paths and metadata.
func ValidateManagerDescriptor(descriptor ManagerDescriptor) error {
	return descriptor.Validate()
}

// Validate checks that descriptor can render safe secret paths and metadata.
func (d ManagerDescriptor) Validate() error {
	_, err := normalizeManagerDescriptor(d)
	return err
}

// Ref returns a SecretRef for name with the descriptor's version pin applied.
func (d ManagerDescriptor) Ref(name string) SecretRef {
	return d.VersionPin.Apply(Ref(name))
}

// Path renders the descriptor path for ref.
func (d ManagerDescriptor) Path(ref SecretRef) (string, error) {
	normalized, err := normalizeManagerDescriptor(d)
	if err != nil {
		return "", err
	}

	ref = normalized.VersionPin.Apply(ref)
	ref, err = normalizeRef(ref)
	if err != nil {
		return "", err
	}

	return RenderManagerPath(normalized.PathTemplate, ManagerPathValues{
		Provider: ProviderName(normalized.Provider),
		Source:   SourceName(normalized.Source),
		Name:     ref.Name,
		Version:  ref.Version,
		Revision: normalized.VersionPin.Revision,
	})
}

// ManagerPathValues are the values available while rendering a PathTemplate.
type ManagerPathValues struct {
	Provider ProviderName
	Source   SourceName
	Name     string
	Version  VersionLabel
	Revision string
}

// RenderManagerPath renders template using provider-neutral path values.
func RenderManagerPath(template PathTemplate, values ManagerPathValues) (string, error) {
	template, err := normalizePathTemplate(template)
	if err != nil {
		return "", err
	}
	values = normalizeManagerPathValues(values)

	rendered, err := renderManagerPathTemplate(template, values)
	if err != nil {
		return "", err
	}
	if err := validateRenderedManagerPath(rendered); err != nil {
		return "", err
	}
	return rendered, nil
}

// ManagerCatalog groups secret manager descriptors for lookup by source name.
type ManagerCatalog struct {
	Descriptors []ManagerDescriptor
}

// ValidateManagerCatalog checks every descriptor and rejects duplicate sources.
func ValidateManagerCatalog(catalog ManagerCatalog) error {
	return catalog.Validate()
}

// Validate checks every descriptor and rejects duplicate sources.
func (c ManagerCatalog) Validate() error {
	_, err := normalizeManagerCatalog(c)
	return err
}

// LookupSource returns the descriptor for source.
func (c ManagerCatalog) LookupSource(source string) (ManagerDescriptor, bool) {
	sourceName, ok := normalizeManagerName(source)
	if !ok {
		return ManagerDescriptor{}, false
	}
	for _, descriptor := range c.Descriptors {
		normalized, err := normalizeManagerDescriptor(descriptor)
		if err != nil {
			continue
		}
		if string(normalized.Source) == sourceName {
			return normalized, true
		}
	}
	return ManagerDescriptor{}, false
}

func normalizeManagerCatalog(catalog ManagerCatalog) ([]ManagerDescriptor, error) {
	normalized := make([]ManagerDescriptor, 0, len(catalog.Descriptors))
	seenSources := make(map[SourceName]int, len(catalog.Descriptors))
	var errs []error

	for i, descriptor := range catalog.Descriptors {
		clean, err := normalizeManagerDescriptor(descriptor)
		if err != nil {
			errs = append(errs, fmt.Errorf("manager descriptor %d: %w", i, err))
			continue
		}
		if first, ok := seenSources[clean.Source]; ok {
			errs = append(errs, fmt.Errorf("%w %q at descriptors %d and %d", ErrDuplicateManagerSource, clean.Source, first, i))
			continue
		}
		seenSources[clean.Source] = i
		normalized = append(normalized, clean)
	}
	if err := errors.Join(errs...); err != nil {
		return nil, err
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		return managerDescriptorLess(normalized[i], normalized[j])
	})
	return normalized, nil
}

func normalizeManagerDescriptor(d ManagerDescriptor) (ManagerDescriptor, error) {
	provider, ok := normalizeManagerName(string(d.Provider))
	if !ok {
		return d, ErrManagerProviderRequired
	}
	source, ok := normalizeManagerName(string(d.Source))
	if !ok {
		return d, ErrManagerSourceRequired
	}
	pathTemplate, err := normalizePathTemplate(d.PathTemplate)
	if err != nil {
		return d, err
	}
	versionPin, err := normalizeVersionPin(d.VersionPin)
	if err != nil {
		return d, err
	}
	if pathTemplateReferences(pathTemplate, "revision") && versionPin.Revision == "" {
		return d, fmt.Errorf("%w: revision placeholder requires a revision pin", ErrManagerPathTemplateInvalid)
	}
	rotation, err := normalizeRotationMetadata(d.Rotation)
	if err != nil {
		return d, err
	}

	d.Provider = ProviderName(provider)
	d.Source = SourceName(source)
	d.PathTemplate = pathTemplate
	d.VersionPin = versionPin
	d.Rotation = rotation
	return d, nil
}

func normalizeManagerName(value string) (string, bool) {
	value = strings.TrimSpace(value)
	if value == "" || !safeManagerToken(value) {
		return "", false
	}
	return value, true
}

func normalizePathTemplate(template PathTemplate) (PathTemplate, error) {
	value := strings.TrimSpace(string(template))
	if value == "" {
		return "", ErrManagerPathTemplateRequired
	}
	if !safeManagerPathText(value) {
		return "", fmt.Errorf("%w: template contains whitespace or control characters", ErrManagerPathTemplateInvalid)
	}
	if err := validateManagerPathTemplate(value); err != nil {
		return "", err
	}
	return PathTemplate(value), nil
}

func normalizeVersionPin(pin VersionPin) (VersionPin, error) {
	pin.Label = VersionLabel(strings.TrimSpace(string(pin.Label)))
	pin.Revision = strings.TrimSpace(pin.Revision)
	if pin.Label != "" && !safeManagerToken(string(pin.Label)) {
		return pin, fmt.Errorf("%w: label %q is invalid", ErrManagerVersionPinInvalid, pin.Label)
	}
	if pin.Revision != "" && !safeManagerToken(pin.Revision) {
		return pin, fmt.Errorf("%w: revision %q is invalid", ErrManagerVersionPinInvalid, pin.Revision)
	}
	return pin, nil
}

func normalizeRotationMetadata(m RotationMetadata) (RotationMetadata, error) {
	m.Purpose = strings.TrimSpace(m.Purpose)
	var errs []error

	if m.Purpose != "" && !safeManagerToken(m.Purpose) {
		errs = append(errs, fmt.Errorf("%w: purpose %q is invalid", ErrManagerRotationInvalid, m.Purpose))
	}
	if m.Cadence < 0 {
		errs = append(errs, fmt.Errorf("%w: cadence must be non-negative", ErrManagerRotationInvalid))
	}
	if m.Overlap < 0 {
		errs = append(errs, fmt.Errorf("%w: overlap must be non-negative", ErrManagerRotationInvalid))
	}
	if m.Cadence > 0 && m.Overlap >= m.Cadence {
		errs = append(errs, fmt.Errorf("%w: overlap must be shorter than cadence", ErrManagerRotationInvalid))
	}
	if !m.LastRotatedAt.IsZero() && !m.NextRotatesAt.IsZero() && !m.LastRotatedAt.Before(m.NextRotatesAt) {
		errs = append(errs, fmt.Errorf("%w: last rotation must be before next rotation", ErrManagerRotationInvalid))
	}

	if err := errors.Join(errs...); err != nil {
		return m, err
	}
	return m, nil
}

func normalizeManagerPathValues(values ManagerPathValues) ManagerPathValues {
	values.Provider = ProviderName(strings.TrimSpace(string(values.Provider)))
	values.Source = SourceName(strings.TrimSpace(string(values.Source)))
	values.Name = strings.TrimSpace(trimEnvPrefix(values.Name))
	values.Version = VersionLabel(strings.TrimSpace(string(values.Version)))
	values.Revision = strings.TrimSpace(values.Revision)
	return values
}

func renderManagerPathTemplate(template PathTemplate, values ManagerPathValues) (string, error) {
	valueByPlaceholder := map[string]string{
		"provider": string(values.Provider),
		"source":   string(values.Source),
		"name":     values.Name,
		"version":  string(values.Version),
		"revision": values.Revision,
	}

	raw := string(template)
	var b strings.Builder
	for i := 0; i < len(raw); i++ {
		switch raw[i] {
		case '{':
			end := strings.IndexByte(raw[i+1:], '}')
			if end < 0 {
				return "", fmt.Errorf("%w: unterminated placeholder", ErrManagerPathTemplateInvalid)
			}
			name := raw[i+1 : i+1+end]
			value, ok := valueByPlaceholder[name]
			if !ok {
				return "", fmt.Errorf("%w: unknown placeholder %q", ErrManagerPathTemplateInvalid, name)
			}
			if value == "" {
				return "", fmt.Errorf("%w: placeholder %q has no value", ErrManagerPathTemplateInvalid, name)
			}
			b.WriteString(value)
			i += end + 1
		case '}':
			return "", fmt.Errorf("%w: unmatched placeholder close", ErrManagerPathTemplateInvalid)
		default:
			b.WriteByte(raw[i])
		}
	}
	return b.String(), nil
}

func validateManagerPathTemplate(template string) error {
	for i := 0; i < len(template); i++ {
		switch template[i] {
		case '{':
			end := strings.IndexByte(template[i+1:], '}')
			if end < 0 {
				return fmt.Errorf("%w: unterminated placeholder", ErrManagerPathTemplateInvalid)
			}
			name := template[i+1 : i+1+end]
			if !validManagerPathPlaceholder(name) {
				return fmt.Errorf("%w: unknown placeholder %q", ErrManagerPathTemplateInvalid, name)
			}
			i += end + 1
		case '}':
			return fmt.Errorf("%w: unmatched placeholder close", ErrManagerPathTemplateInvalid)
		}
	}
	return nil
}

func validManagerPathPlaceholder(name string) bool {
	switch name {
	case "provider", "source", "name", "version", "revision":
		return true
	default:
		return false
	}
}

func pathTemplateReferences(template PathTemplate, placeholder string) bool {
	raw := string(template)
	for i := 0; i < len(raw); i++ {
		if raw[i] != '{' {
			continue
		}
		end := strings.IndexByte(raw[i+1:], '}')
		if end < 0 {
			return false
		}
		if raw[i+1:i+1+end] == placeholder {
			return true
		}
		i += end + 1
	}
	return false
}

func validateRenderedManagerPath(path string) error {
	if strings.TrimSpace(path) == "" {
		return ErrManagerPathTemplateRequired
	}
	if !safeManagerPathText(path) {
		return fmt.Errorf("%w: rendered path contains whitespace or control characters", ErrManagerPathTemplateInvalid)
	}
	return nil
}

func safeManagerToken(value string) bool {
	if value == "" || strings.TrimSpace(value) != value {
		return false
	}
	return safeManagerPathText(value)
}

func safeManagerPathText(value string) bool {
	for _, r := range value {
		if unicode.IsSpace(r) || unicode.IsControl(r) {
			return false
		}
	}
	return true
}

func managerDescriptorLess(a, b ManagerDescriptor) bool {
	if a.Source != b.Source {
		return a.Source < b.Source
	}
	if a.Provider != b.Provider {
		return a.Provider < b.Provider
	}
	return a.PathTemplate < b.PathTemplate
}
