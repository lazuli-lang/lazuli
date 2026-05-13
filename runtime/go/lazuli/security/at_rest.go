package security

import (
	"errors"
	"fmt"
	"strings"
	"unicode"
)

const (
	// AtRestAlgorithmAES256GCM is the canonical token for AES-256-GCM field or
	// object encryption metadata.
	AtRestAlgorithmAES256GCM = "aes-256-gcm"
)

// ErrAtRestPolicyInvalid is returned when at-rest encryption policy metadata
// cannot be safely interpreted by generated code or adapter bindings.
var ErrAtRestPolicyInvalid = errors.New("lazuli/security: at_rest_policy_invalid")

// AtRestRequirement records whether encryption is mandatory for a target.
type AtRestRequirement int

const (
	// AtRestOptional means callers may persist the target without at-rest
	// encryption. This is the zero value.
	AtRestOptional AtRestRequirement = iota
	// AtRestRequired means callers must use the selected key and algorithm
	// metadata before persisting the target.
	AtRestRequired
)

// String renders the requirement as a stable lowercase token.
func (r AtRestRequirement) String() string {
	switch r {
	case AtRestOptional:
		return "optional"
	case AtRestRequired:
		return "required"
	default:
		return "unknown"
	}
}

// Required reports whether the requirement mandates encryption.
func (r AtRestRequirement) Required() bool {
	return r == AtRestRequired
}

// AtRestTargetKind is the normalized kind of a policy target.
type AtRestTargetKind int

const (
	AtRestTargetUnknown AtRestTargetKind = iota
	AtRestTargetResource
	AtRestTargetField
	AtRestTargetBucket
)

// String renders the target kind as a stable lowercase token.
func (k AtRestTargetKind) String() string {
	switch k {
	case AtRestTargetResource:
		return "resource"
	case AtRestTargetField:
		return "field"
	case AtRestTargetBucket:
		return "bucket"
	default:
		return "unknown"
	}
}

// AtRestTarget identifies a resource, resource field, or logical storage
// bucket governed by an at-rest encryption policy.
type AtRestTarget struct {
	Resource string
	Field    string
	Bucket   string
}

// ResourceAtRestTarget returns a target for every field of resource unless a
// more specific field rule overrides it.
func ResourceAtRestTarget(resource string) AtRestTarget {
	return AtRestTarget{Resource: resource}
}

// FieldAtRestTarget returns a target for one resource field.
func FieldAtRestTarget(resource, field string) AtRestTarget {
	return AtRestTarget{Resource: resource, Field: field}
}

// BucketAtRestTarget returns a target for one logical storage bucket.
func BucketAtRestTarget(bucket string) AtRestTarget {
	return AtRestTarget{Bucket: bucket}
}

// Normalize trims target names read from generated metadata or configuration.
func (t AtRestTarget) Normalize() AtRestTarget {
	return AtRestTarget{
		Resource: strings.TrimSpace(t.Resource),
		Field:    strings.TrimSpace(t.Field),
		Bucket:   strings.TrimSpace(t.Bucket),
	}
}

// Kind returns the normalized target kind.
func (t AtRestTarget) Kind() AtRestTargetKind {
	t = t.Normalize()
	switch {
	case t.Bucket != "" && t.Resource == "" && t.Field == "":
		return AtRestTargetBucket
	case t.Resource != "" && t.Field != "" && t.Bucket == "":
		return AtRestTargetField
	case t.Resource != "" && t.Field == "" && t.Bucket == "":
		return AtRestTargetResource
	default:
		return AtRestTargetUnknown
	}
}

// String renders the target as a stable diagnostic label.
func (t AtRestTarget) String() string {
	t = t.Normalize()
	switch t.Kind() {
	case AtRestTargetResource:
		return "resource=" + t.Resource
	case AtRestTargetField:
		return "resource=" + t.Resource + ",field=" + t.Field
	case AtRestTargetBucket:
		return "bucket=" + t.Bucket
	default:
		return "unknown"
	}
}

// Validate checks that target identifies exactly one resource, field, or
// bucket and that its names are safe metadata tokens.
func (t AtRestTarget) Validate() error {
	return ValidateAtRestTarget(t)
}

// AtRestKeyRef identifies a provider-neutral encryption key. Provider and
// Version are adapter metadata; Name is the stable logical key reference.
type AtRestKeyRef struct {
	Provider string
	Name     string
	Version  string
}

// AtRestKey returns a key reference for name.
func AtRestKey(name string) AtRestKeyRef {
	return AtRestKeyRef{Name: name}
}

// WithProvider returns a copy of ref bound to provider metadata.
func (ref AtRestKeyRef) WithProvider(provider string) AtRestKeyRef {
	ref.Provider = provider
	return ref
}

// WithVersion returns a copy of ref bound to version metadata.
func (ref AtRestKeyRef) WithVersion(version string) AtRestKeyRef {
	ref.Version = version
	return ref
}

// Normalize trims key reference metadata and lowercases the provider token.
func (ref AtRestKeyRef) Normalize() AtRestKeyRef {
	return AtRestKeyRef{
		Provider: strings.ToLower(strings.TrimSpace(ref.Provider)),
		Name:     strings.TrimSpace(ref.Name),
		Version:  strings.TrimSpace(ref.Version),
	}
}

// IsZero reports whether ref carries no key metadata.
func (ref AtRestKeyRef) IsZero() bool {
	ref = ref.Normalize()
	return ref.Provider == "" && ref.Name == "" && ref.Version == ""
}

// String renders the key reference as a stable diagnostic label.
func (ref AtRestKeyRef) String() string {
	ref = ref.Normalize()
	parts := make([]string, 0, 3)
	if ref.Provider != "" {
		parts = append(parts, "provider="+ref.Provider)
	}
	if ref.Name != "" {
		parts = append(parts, "name="+ref.Name)
	}
	if ref.Version != "" {
		parts = append(parts, "version="+ref.Version)
	}
	if len(parts) == 0 {
		return "unset"
	}
	return strings.Join(parts, ",")
}

// Validate checks that ref can identify a key.
func (ref AtRestKeyRef) Validate() error {
	return ValidateAtRestKeyRef(ref)
}

// AtRestAlgorithm describes encryption algorithm metadata. The runtime does
// not implement encryption here; adapters and generated code use this metadata
// to select their own concrete implementation.
type AtRestAlgorithm struct {
	Name            string
	KeySizeBits     int
	Envelope        bool
	ProviderManaged bool
}

// AtRestAES256GCM returns AES-256-GCM algorithm metadata.
func AtRestAES256GCM() AtRestAlgorithm {
	return AtRestAlgorithm{
		Name:        AtRestAlgorithmAES256GCM,
		KeySizeBits: 256,
	}
}

// Normalize trims algorithm metadata and lowercases the algorithm name.
func (a AtRestAlgorithm) Normalize() AtRestAlgorithm {
	a.Name = strings.ToLower(strings.TrimSpace(a.Name))
	return a
}

// IsZero reports whether a carries no algorithm metadata.
func (a AtRestAlgorithm) IsZero() bool {
	a = a.Normalize()
	return a.Name == "" && a.KeySizeBits == 0 && !a.Envelope && !a.ProviderManaged
}

// Validate checks that algorithm metadata is structurally safe.
func (a AtRestAlgorithm) Validate() error {
	return ValidateAtRestAlgorithm(a)
}

// AtRestRule binds one target to a required or optional encryption decision.
// Required rules must carry both a key reference and algorithm metadata.
type AtRestRule struct {
	Target      AtRestTarget
	Requirement AtRestRequirement
	KeyRef      AtRestKeyRef
	Algorithm   AtRestAlgorithm
}

// RequiredAtRest returns a rule that requires encryption for target.
func RequiredAtRest(target AtRestTarget, key AtRestKeyRef, algorithm AtRestAlgorithm) AtRestRule {
	return AtRestRule{
		Target:      target,
		Requirement: AtRestRequired,
		KeyRef:      key,
		Algorithm:   algorithm,
	}
}

// OptionalAtRest returns a rule that allows target to remain unencrypted.
func OptionalAtRest(target AtRestTarget) AtRestRule {
	return AtRestRule{
		Target:      target,
		Requirement: AtRestOptional,
	}
}

// Validate checks that rule has a valid target and coherent key/algorithm
// metadata for its requirement.
func (r AtRestRule) Validate() error {
	return ValidateAtRestRule(r)
}

// Matches reports whether rule applies to target. Resource rules match both a
// resource target and fields under that resource; field and bucket rules match
// only their exact targets.
func (r AtRestRule) Matches(target AtRestTarget) bool {
	rule := r.Target.Normalize()
	target = target.Normalize()
	switch rule.Kind() {
	case AtRestTargetField:
		return target.Kind() == AtRestTargetField &&
			rule.Resource == target.Resource &&
			rule.Field == target.Field
	case AtRestTargetResource:
		return target.Bucket == "" &&
			target.Resource == rule.Resource &&
			target.Resource != ""
	case AtRestTargetBucket:
		return target.Kind() == AtRestTargetBucket &&
			rule.Bucket == target.Bucket
	default:
		return false
	}
}

// AtRestPolicy is a provider-neutral at-rest encryption policy. The default
// fields apply when no rule matches. More specific field rules override
// resource rules during decision resolution.
type AtRestPolicy struct {
	DefaultRequirement AtRestRequirement
	DefaultKeyRef      AtRestKeyRef
	DefaultAlgorithm   AtRestAlgorithm
	Rules              []AtRestRule
}

// Validate checks the default metadata and every rule.
func (p AtRestPolicy) Validate() error {
	return ValidateAtRestPolicy(p)
}

// Decision resolves policy for target.
func (p AtRestPolicy) Decision(target AtRestTarget) (AtRestDecision, error) {
	return DecideAtRest(p, target)
}

// RequiresEncryption reports whether policy requires encryption for target.
func (p AtRestPolicy) RequiresEncryption(target AtRestTarget) (bool, error) {
	decision, err := p.Decision(target)
	if err != nil {
		return false, err
	}
	return decision.Required(), nil
}

// AtRestDecision is the resolved at-rest encryption requirement and metadata
// for one target.
type AtRestDecision struct {
	Target      AtRestTarget
	Requirement AtRestRequirement
	KeyRef      AtRestKeyRef
	Algorithm   AtRestAlgorithm
	RuleIndex   int
}

// Required reports whether the decision mandates encryption.
func (d AtRestDecision) Required() bool {
	return d.Requirement.Required()
}

// Validate checks that decision target and metadata are coherent.
func (d AtRestDecision) Validate() error {
	if err := ValidateAtRestTarget(d.Target); err != nil {
		return err
	}
	_, _, _, err := normalizeAtRestSetting(d.Requirement, d.KeyRef, d.Algorithm)
	return err
}

// ValidateAtRestTarget checks that target identifies exactly one resource,
// field, or bucket.
func ValidateAtRestTarget(target AtRestTarget) error {
	target = target.Normalize()
	switch target.Kind() {
	case AtRestTargetResource:
		return validateAtRestTargetName("resource", target.Resource)
	case AtRestTargetField:
		if err := validateAtRestTargetName("resource", target.Resource); err != nil {
			return err
		}
		return validateAtRestTargetName("field", target.Field)
	case AtRestTargetBucket:
		return validateAtRestTargetName("bucket", target.Bucket)
	default:
		return fmt.Errorf("%w: target must identify exactly one resource, field, or bucket", ErrAtRestPolicyInvalid)
	}
}

// ValidateAtRestKeyRef checks that ref can identify a provider-neutral key.
func ValidateAtRestKeyRef(ref AtRestKeyRef) error {
	ref = ref.Normalize()
	if ref.Name == "" {
		return fmt.Errorf("%w: key ref name is required", ErrAtRestPolicyInvalid)
	}
	if err := validateAtRestMetadataToken("key provider", ref.Provider, true); err != nil {
		return err
	}
	if err := validateAtRestMetadataToken("key ref", ref.Name, false); err != nil {
		return err
	}
	return validateAtRestMetadataToken("key version", ref.Version, true)
}

// ValidateAtRestAlgorithm checks that algorithm metadata is structurally safe.
func ValidateAtRestAlgorithm(algorithm AtRestAlgorithm) error {
	algorithm = algorithm.Normalize()
	if algorithm.Name == "" {
		return fmt.Errorf("%w: algorithm name is required", ErrAtRestPolicyInvalid)
	}
	if err := validateAtRestMetadataToken("algorithm", algorithm.Name, false); err != nil {
		return err
	}
	if algorithm.KeySizeBits < 0 {
		return fmt.Errorf("%w: algorithm key size must be non-negative", ErrAtRestPolicyInvalid)
	}
	if algorithm.Name == AtRestAlgorithmAES256GCM && algorithm.KeySizeBits != 0 && algorithm.KeySizeBits != 256 {
		return fmt.Errorf("%w: aes-256-gcm key size must be 256 bits", ErrAtRestPolicyInvalid)
	}
	return nil
}

// ValidateAtRestRule checks that rule can be safely interpreted.
func ValidateAtRestRule(rule AtRestRule) error {
	if err := ValidateAtRestTarget(rule.Target); err != nil {
		return err
	}
	_, _, _, err := normalizeAtRestSetting(rule.Requirement, rule.KeyRef, rule.Algorithm)
	return err
}

// ValidateAtRestPolicy checks that policy can be safely interpreted and does
// not contain duplicate rule targets.
func ValidateAtRestPolicy(policy AtRestPolicy) error {
	_, err := normalizeAtRestPolicy(policy)
	return err
}

// DecideAtRest resolves the policy decision for target.
func DecideAtRest(policy AtRestPolicy, target AtRestTarget) (AtRestDecision, error) {
	normalized, err := normalizeAtRestPolicy(policy)
	if err != nil {
		return AtRestDecision{}, err
	}
	target = target.Normalize()
	if err := ValidateAtRestTarget(target); err != nil {
		return AtRestDecision{}, err
	}

	decision := AtRestDecision{
		Target:      target,
		Requirement: normalized.DefaultRequirement,
		KeyRef:      normalized.DefaultKeyRef,
		Algorithm:   normalized.DefaultAlgorithm,
		RuleIndex:   -1,
	}
	bestSpecificity := 0
	for i, rule := range normalized.Rules {
		if !rule.Matches(target) {
			continue
		}
		specificity := atRestTargetSpecificity(rule.Target)
		if specificity <= bestSpecificity {
			continue
		}
		decision.Requirement = rule.Requirement
		decision.KeyRef = rule.KeyRef
		decision.Algorithm = rule.Algorithm
		decision.RuleIndex = i
		bestSpecificity = specificity
	}
	return decision, nil
}

func normalizeAtRestPolicy(policy AtRestPolicy) (AtRestPolicy, error) {
	requirement, keyRef, algorithm, err := normalizeAtRestSetting(policy.DefaultRequirement, policy.DefaultKeyRef, policy.DefaultAlgorithm)
	if err != nil {
		return AtRestPolicy{}, fmt.Errorf("%w: default: %v", ErrAtRestPolicyInvalid, err)
	}
	normalized := AtRestPolicy{
		DefaultRequirement: requirement,
		DefaultKeyRef:      keyRef,
		DefaultAlgorithm:   algorithm,
		Rules:              make([]AtRestRule, 0, len(policy.Rules)),
	}

	seen := make(map[AtRestTarget]int, len(policy.Rules))
	for i, rule := range policy.Rules {
		rule.Target = rule.Target.Normalize()
		if err := ValidateAtRestTarget(rule.Target); err != nil {
			return AtRestPolicy{}, fmt.Errorf("%w: rule %d: %v", ErrAtRestPolicyInvalid, i, err)
		}
		rule.Requirement, rule.KeyRef, rule.Algorithm, err = normalizeAtRestSetting(rule.Requirement, rule.KeyRef, rule.Algorithm)
		if err != nil {
			return AtRestPolicy{}, fmt.Errorf("%w: rule %d: %v", ErrAtRestPolicyInvalid, i, err)
		}
		if previous, ok := seen[rule.Target]; ok {
			return AtRestPolicy{}, fmt.Errorf("%w: rule %d duplicates rule %d target %s", ErrAtRestPolicyInvalid, i, previous, rule.Target)
		}
		seen[rule.Target] = i
		normalized.Rules = append(normalized.Rules, rule)
	}
	return normalized, nil
}

func normalizeAtRestSetting(requirement AtRestRequirement, keyRef AtRestKeyRef, algorithm AtRestAlgorithm) (AtRestRequirement, AtRestKeyRef, AtRestAlgorithm, error) {
	if !isKnownAtRestRequirement(requirement) {
		return requirement, AtRestKeyRef{}, AtRestAlgorithm{}, fmt.Errorf("%w: unknown requirement %d", ErrAtRestPolicyInvalid, requirement)
	}

	keyRef = keyRef.Normalize()
	algorithm = algorithm.Normalize()
	hasKey := !keyRef.IsZero()
	hasAlgorithm := !algorithm.IsZero()
	if requirement == AtRestRequired {
		if !hasKey {
			return requirement, keyRef, algorithm, fmt.Errorf("%w: required encryption needs a key ref", ErrAtRestPolicyInvalid)
		}
		if !hasAlgorithm {
			return requirement, keyRef, algorithm, fmt.Errorf("%w: required encryption needs an algorithm", ErrAtRestPolicyInvalid)
		}
	}
	if hasKey != hasAlgorithm {
		return requirement, keyRef, algorithm, fmt.Errorf("%w: key ref and algorithm must be set together", ErrAtRestPolicyInvalid)
	}
	if hasKey {
		if err := ValidateAtRestKeyRef(keyRef); err != nil {
			return requirement, keyRef, algorithm, err
		}
	}
	if hasAlgorithm {
		if err := ValidateAtRestAlgorithm(algorithm); err != nil {
			return requirement, keyRef, algorithm, err
		}
	}
	return requirement, keyRef, algorithm, nil
}

func isKnownAtRestRequirement(requirement AtRestRequirement) bool {
	switch requirement {
	case AtRestOptional, AtRestRequired:
		return true
	default:
		return false
	}
}

func atRestTargetSpecificity(target AtRestTarget) int {
	switch target.Kind() {
	case AtRestTargetField:
		return 3
	case AtRestTargetResource, AtRestTargetBucket:
		return 2
	default:
		return 0
	}
}

func validateAtRestTargetName(kind, value string) error {
	if value == "" {
		return fmt.Errorf("%w: %s is required", ErrAtRestPolicyInvalid, kind)
	}
	for _, r := range value {
		if r == '/' || r == '\\' || unicode.IsControl(r) || unicode.IsSpace(r) {
			return fmt.Errorf("%w: invalid %s %q", ErrAtRestPolicyInvalid, kind, value)
		}
	}
	return nil
}

func validateAtRestMetadataToken(kind, value string, allowEmpty bool) error {
	if value == "" {
		if allowEmpty {
			return nil
		}
		return fmt.Errorf("%w: %s is required", ErrAtRestPolicyInvalid, kind)
	}
	for _, r := range value {
		if unicode.IsControl(r) || unicode.IsSpace(r) {
			return fmt.Errorf("%w: %s contains whitespace or control characters", ErrAtRestPolicyInvalid, kind)
		}
	}
	return nil
}
