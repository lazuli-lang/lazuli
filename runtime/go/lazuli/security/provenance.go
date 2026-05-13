package security

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net/url"
	"sort"
	"strings"
	"time"
	"unicode"
)

const (
	// ProvenanceStatementType is the in-toto Statement v1 type URI used by
	// SLSA provenance attestations.
	ProvenanceStatementType = "https://in-toto.io/Statement/v1"

	// SLSAProvenanceStatementType aliases ProvenanceStatementType for callers
	// that keep SLSA constants grouped by prefix.
	SLSAProvenanceStatementType = ProvenanceStatementType

	// SLSAProvenancePredicateType is the stable SLSA provenance predicate type
	// URI. The URI intentionally omits the minor spec version.
	SLSAProvenancePredicateType = "https://slsa.dev/provenance/v1"
)

// ErrInvalidProvenance is returned when a provenance descriptor cannot be
// normalized or rendered as SLSA provenance JSON.
var ErrInvalidProvenance = errors.New("lazuli/security: invalid_provenance")

// ProvenanceStatement is an in-toto Statement v1 carrying a SLSA provenance
// predicate.
type ProvenanceStatement struct {
	Type          string               `json:"_type"`
	Subject       []ProvenanceResource `json:"subject"`
	PredicateType string               `json:"predicateType"`
	Predicate     ProvenancePredicate  `json:"predicate"`
}

// ProvenancePredicate describes where, when, and how a build produced the
// statement subjects.
type ProvenancePredicate struct {
	BuildDefinition ProvenanceBuildDefinition `json:"buildDefinition"`
	RunDetails      ProvenanceRunDetails      `json:"runDetails"`
}

// ProvenanceBuildDefinition describes the top-level build inputs and resolved
// dependency materials.
type ProvenanceBuildDefinition struct {
	BuildType            string               `json:"buildType"`
	ExternalParameters   map[string]any       `json:"externalParameters"`
	InternalParameters   map[string]any       `json:"internalParameters,omitempty"`
	ResolvedDependencies []ProvenanceResource `json:"resolvedDependencies,omitempty"`
}

// ProvenanceRunDetails describes the build platform and one build execution.
type ProvenanceRunDetails struct {
	Builder    ProvenanceBuilder        `json:"builder"`
	Metadata   *ProvenanceBuildMetadata `json:"metadata,omitempty"`
	Byproducts []ProvenanceResource     `json:"byproducts,omitempty"`
}

// ProvenanceBuilder identifies the trusted build platform. Builder IDs are
// SLSA TypeURI values and should uniquely represent the trusted builder base.
type ProvenanceBuilder struct {
	ID                  string               `json:"id"`
	Version             map[string]string    `json:"version,omitempty"`
	BuilderDependencies []ProvenanceResource `json:"builderDependencies,omitempty"`
}

// ProvenanceBuildMetadata carries optional execution metadata.
type ProvenanceBuildMetadata struct {
	InvocationID string `json:"invocationId,omitempty"`
	StartedOn    string `json:"startedOn,omitempty"`
	FinishedOn   string `json:"finishedOn,omitempty"`
}

// ProvenanceResource is the SLSA/in-toto ResourceDescriptor shape used for
// subjects, material sources, builder dependencies, and byproducts.
type ProvenanceResource struct {
	URI              string            `json:"uri,omitempty"`
	Digest           map[string]string `json:"digest,omitempty"`
	Name             string            `json:"name,omitempty"`
	DownloadLocation string            `json:"downloadLocation,omitempty"`
	MediaType        string            `json:"mediaType,omitempty"`
	Content          []byte            `json:"content,omitempty"`
	Annotations      map[string]any    `json:"annotations,omitempty"`
}

// ProvenanceResourceDescriptor is an alias for the SLSA/in-toto resource
// descriptor shape.
type ProvenanceResourceDescriptor = ProvenanceResource

// NewProvenanceStatement returns a SLSA provenance statement with the standard
// in-toto statement and predicate type URIs populated.
func NewProvenanceStatement(subjects []ProvenanceResource, predicate ProvenancePredicate) ProvenanceStatement {
	return ProvenanceStatement{
		Type:          ProvenanceStatementType,
		Subject:       cloneProvenanceResources(subjects),
		PredicateType: SLSAProvenancePredicateType,
		Predicate:     predicate,
	}
}

// NewProvenancePredicate returns a minimal SLSA provenance predicate.
func NewProvenancePredicate(buildType, builderID string) ProvenancePredicate {
	return ProvenancePredicate{
		BuildDefinition: ProvenanceBuildDefinition{
			BuildType:          buildType,
			ExternalParameters: map[string]any{},
		},
		RunDetails: ProvenanceRunDetails{
			Builder: ProvenanceBuilderID(builderID),
		},
	}
}

// ProvenanceSubject returns a subject descriptor with a defensive copy of its
// digest set.
func ProvenanceSubject(name string, digests map[string]string) ProvenanceResource {
	return ProvenanceResource{
		Name:   name,
		Digest: cloneStringMap(digests),
	}
}

// ProvenanceSubjectDigest returns a subject descriptor for one digest.
func ProvenanceSubjectDigest(name, algorithm, digest string) ProvenanceResource {
	return ProvenanceSubject(name, map[string]string{algorithm: digest})
}

// ProvenanceMaterial returns a resolved dependency/material descriptor with a
// defensive copy of its digest set.
func ProvenanceMaterial(uri string, digests map[string]string) ProvenanceResource {
	return ProvenanceResource{
		URI:    uri,
		Digest: cloneStringMap(digests),
	}
}

// ProvenanceMaterialSource returns a resolved dependency/material descriptor
// for one source URI and digest.
func ProvenanceMaterialSource(uri, algorithm, digest string) ProvenanceResource {
	return ProvenanceMaterial(uri, map[string]string{algorithm: digest})
}

// ProvenanceBuilderID returns a builder descriptor for id.
func ProvenanceBuilderID(id string) ProvenanceBuilder {
	return ProvenanceBuilder{ID: id}
}

// Validate checks that statement can be normalized and rendered.
func (s ProvenanceStatement) Validate() error {
	_, err := normalizeProvenanceStatement(s)
	return err
}

// SLSAJSON renders statement as deterministic, indented SLSA provenance JSON.
func (s ProvenanceStatement) SLSAJSON() ([]byte, error) {
	return RenderSLSAProvenanceJSON(s)
}

// JSON renders statement as deterministic, indented SLSA provenance JSON.
func (s ProvenanceStatement) JSON() ([]byte, error) {
	return s.SLSAJSON()
}

// Validate checks that predicate can be normalized and rendered.
func (p ProvenancePredicate) Validate() error {
	return ValidateProvenancePredicate(p)
}

// ValidateProvenancePredicate checks the SLSA provenance predicate fields that
// Lazuli materializes directly.
func ValidateProvenancePredicate(predicate ProvenancePredicate) error {
	_, err := normalizeProvenancePredicate(predicate)
	return err
}

// RenderProvenanceJSON renders statement as deterministic, indented SLSA
// provenance JSON.
func RenderProvenanceJSON(statement ProvenanceStatement) ([]byte, error) {
	return RenderSLSAProvenanceJSON(statement)
}

// RenderSLSAProvenanceJSON renders statement as deterministic, indented SLSA
// provenance JSON.
func RenderSLSAProvenanceJSON(statement ProvenanceStatement) ([]byte, error) {
	normalized, err := normalizeProvenanceStatement(statement)
	if err != nil {
		return nil, err
	}
	return json.MarshalIndent(normalized, "", "  ")
}

func normalizeProvenanceStatement(statement ProvenanceStatement) (ProvenanceStatement, error) {
	statementType, err := normalizeProvenanceDefaultURI("_type", statement.Type, ProvenanceStatementType)
	if err != nil {
		return ProvenanceStatement{}, err
	}
	if statementType != ProvenanceStatementType {
		return ProvenanceStatement{}, invalidProvenance("_type %q, want %q", statementType, ProvenanceStatementType)
	}
	predicateType, err := normalizeProvenanceDefaultURI("predicateType", statement.PredicateType, SLSAProvenancePredicateType)
	if err != nil {
		return ProvenanceStatement{}, err
	}
	if predicateType != SLSAProvenancePredicateType {
		return ProvenanceStatement{}, invalidProvenance("predicateType %q, want %q", predicateType, SLSAProvenancePredicateType)
	}

	subjects, err := normalizeProvenanceResources("subject", statement.Subject, provenanceResourceSubject, true)
	if err != nil {
		return ProvenanceStatement{}, err
	}
	predicate, err := normalizeProvenancePredicate(statement.Predicate)
	if err != nil {
		return ProvenanceStatement{}, err
	}

	return ProvenanceStatement{
		Type:          statementType,
		Subject:       subjects,
		PredicateType: predicateType,
		Predicate:     predicate,
	}, nil
}

func normalizeProvenancePredicate(predicate ProvenancePredicate) (ProvenancePredicate, error) {
	buildDefinition, err := normalizeProvenanceBuildDefinition(predicate.BuildDefinition)
	if err != nil {
		return ProvenancePredicate{}, err
	}
	runDetails, err := normalizeProvenanceRunDetails(predicate.RunDetails)
	if err != nil {
		return ProvenancePredicate{}, err
	}
	return ProvenancePredicate{
		BuildDefinition: buildDefinition,
		RunDetails:      runDetails,
	}, nil
}

func normalizeProvenanceBuildDefinition(def ProvenanceBuildDefinition) (ProvenanceBuildDefinition, error) {
	buildType, err := normalizeProvenanceURI("buildDefinition.buildType", def.BuildType, true)
	if err != nil {
		return ProvenanceBuildDefinition{}, err
	}
	externalParameters, err := normalizeProvenanceJSONObject("buildDefinition.externalParameters", def.ExternalParameters, true)
	if err != nil {
		return ProvenanceBuildDefinition{}, err
	}
	internalParameters, err := normalizeProvenanceJSONObject("buildDefinition.internalParameters", def.InternalParameters, false)
	if err != nil {
		return ProvenanceBuildDefinition{}, err
	}
	dependencies, err := normalizeProvenanceResources("buildDefinition.resolvedDependencies", def.ResolvedDependencies, provenanceResourceMaterial, false)
	if err != nil {
		return ProvenanceBuildDefinition{}, err
	}
	return ProvenanceBuildDefinition{
		BuildType:            buildType,
		ExternalParameters:   externalParameters,
		InternalParameters:   internalParameters,
		ResolvedDependencies: dependencies,
	}, nil
}

func normalizeProvenanceRunDetails(details ProvenanceRunDetails) (ProvenanceRunDetails, error) {
	builder, err := normalizeProvenanceBuilder(details.Builder)
	if err != nil {
		return ProvenanceRunDetails{}, err
	}
	metadata, err := normalizeProvenanceMetadata(details.Metadata)
	if err != nil {
		return ProvenanceRunDetails{}, err
	}
	byproducts, err := normalizeProvenanceResources("runDetails.byproducts", details.Byproducts, provenanceResourceMaterial, false)
	if err != nil {
		return ProvenanceRunDetails{}, err
	}
	return ProvenanceRunDetails{
		Builder:    builder,
		Metadata:   metadata,
		Byproducts: byproducts,
	}, nil
}

func normalizeProvenanceBuilder(builder ProvenanceBuilder) (ProvenanceBuilder, error) {
	id, err := normalizeProvenanceURI("runDetails.builder.id", builder.ID, true)
	if err != nil {
		return ProvenanceBuilder{}, err
	}
	version, err := normalizeProvenanceStringMap("runDetails.builder.version", builder.Version)
	if err != nil {
		return ProvenanceBuilder{}, err
	}
	dependencies, err := normalizeProvenanceResources("runDetails.builder.builderDependencies", builder.BuilderDependencies, provenanceResourceMaterial, false)
	if err != nil {
		return ProvenanceBuilder{}, err
	}
	return ProvenanceBuilder{
		ID:                  id,
		Version:             version,
		BuilderDependencies: dependencies,
	}, nil
}

func normalizeProvenanceMetadata(metadata *ProvenanceBuildMetadata) (*ProvenanceBuildMetadata, error) {
	if metadata == nil {
		return nil, nil
	}
	invocationID, err := normalizeProvenanceText("runDetails.metadata.invocationId", metadata.InvocationID, false)
	if err != nil {
		return nil, err
	}
	startedOn, startedAt, err := normalizeProvenanceTimestamp("runDetails.metadata.startedOn", metadata.StartedOn)
	if err != nil {
		return nil, err
	}
	finishedOn, finishedAt, err := normalizeProvenanceTimestamp("runDetails.metadata.finishedOn", metadata.FinishedOn)
	if err != nil {
		return nil, err
	}
	if startedAt != nil && finishedAt != nil && finishedAt.Before(*startedAt) {
		return nil, invalidProvenance("runDetails.metadata.finishedOn is before startedOn")
	}
	if invocationID == "" && startedOn == "" && finishedOn == "" {
		return nil, nil
	}
	return &ProvenanceBuildMetadata{
		InvocationID: invocationID,
		StartedOn:    startedOn,
		FinishedOn:   finishedOn,
	}, nil
}

type provenanceResourceRole uint8

const (
	provenanceResourceSubject provenanceResourceRole = iota
	provenanceResourceMaterial
)

func normalizeProvenanceResources(field string, resources []ProvenanceResource, role provenanceResourceRole, required bool) ([]ProvenanceResource, error) {
	if len(resources) == 0 {
		if required {
			return nil, invalidProvenance("%s is required", field)
		}
		return nil, nil
	}

	normalized := make([]ProvenanceResource, 0, len(resources))
	for i, resource := range resources {
		next, err := normalizeProvenanceResource(fmt.Sprintf("%s[%d]", field, i), resource, role)
		if err != nil {
			return nil, err
		}
		normalized = append(normalized, next)
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return provenanceResourceLess(normalized[i], normalized[j])
	})
	return normalized, nil
}

func normalizeProvenanceResource(field string, resource ProvenanceResource, role provenanceResourceRole) (ProvenanceResource, error) {
	uri, err := normalizeProvenanceOptionalURI(field+".uri", resource.URI)
	if err != nil {
		return ProvenanceResource{}, err
	}
	name, err := normalizeProvenanceText(field+".name", resource.Name, false)
	if err != nil {
		return ProvenanceResource{}, err
	}
	downloadLocation, err := normalizeProvenanceOptionalURI(field+".downloadLocation", resource.DownloadLocation)
	if err != nil {
		return ProvenanceResource{}, err
	}
	mediaType, err := normalizeProvenanceText(field+".mediaType", resource.MediaType, false)
	if err != nil {
		return ProvenanceResource{}, err
	}
	digest, err := normalizeProvenanceDigestSet(field+".digest", resource.Digest, role == provenanceResourceSubject)
	if err != nil {
		return ProvenanceResource{}, err
	}
	annotations, err := normalizeProvenanceJSONObject(field+".annotations", resource.Annotations, false)
	if err != nil {
		return ProvenanceResource{}, err
	}
	content := append([]byte(nil), resource.Content...)

	normalized := ProvenanceResource{
		URI:              uri,
		Digest:           digest,
		Name:             name,
		DownloadLocation: downloadLocation,
		MediaType:        mediaType,
		Content:          content,
		Annotations:      annotations,
	}
	if role != provenanceResourceSubject && provenanceResourceEmpty(normalized) {
		return ProvenanceResource{}, invalidProvenance("%s must identify a resource", field)
	}
	return normalized, nil
}

func normalizeProvenanceDigestSet(field string, digests map[string]string, required bool) (map[string]string, error) {
	if len(digests) == 0 {
		if required {
			return nil, invalidProvenance("%s is required", field)
		}
		return nil, nil
	}

	normalized := make(map[string]string, len(digests))
	for algorithm, digest := range digests {
		normalizedAlgorithm, err := normalizeProvenanceDigestAlgorithm(algorithm)
		if err != nil {
			return nil, invalidProvenance("%s algorithm %q: %v", field, algorithm, err)
		}
		if _, ok := normalized[normalizedAlgorithm]; ok {
			return nil, invalidProvenance("%s has duplicate algorithm %q", field, normalizedAlgorithm)
		}
		normalizedDigest, err := normalizeProvenanceDigestValue(normalizedAlgorithm, digest)
		if err != nil {
			return nil, invalidProvenance("%s.%s: %v", field, normalizedAlgorithm, err)
		}
		normalized[normalizedAlgorithm] = normalizedDigest
	}
	return normalized, nil
}

func normalizeProvenanceDigestAlgorithm(algorithm string) (string, error) {
	trimmed := strings.TrimSpace(algorithm)
	if trimmed == "" {
		return "", errors.New("required")
	}
	if trimmed != algorithm {
		return "", errors.New("must not have leading or trailing whitespace")
	}

	compact := strings.NewReplacer("-", "", "_", "", " ", "").Replace(strings.ToLower(trimmed))
	switch compact {
	case "md5":
		return "md5", nil
	case "sha1":
		return "sha1", nil
	case "sha224":
		return "sha224", nil
	case "sha256":
		return "sha256", nil
	case "sha384":
		return "sha384", nil
	case "sha512":
		return "sha512", nil
	case "gitcommit":
		return "gitCommit", nil
	}

	for _, r := range trimmed {
		switch {
		case r >= '0' && r <= '9':
		case r >= 'A' && r <= 'Z':
		case r >= 'a' && r <= 'z':
		case r == '.' || r == '_' || r == '-':
		default:
			return "", fmt.Errorf("contains unsupported character %q", r)
		}
	}
	return trimmed, nil
}

func normalizeProvenanceDigestValue(algorithm, digest string) (string, error) {
	trimmed := strings.TrimSpace(digest)
	if trimmed == "" {
		return "", errors.New("required")
	}
	if trimmed != digest {
		return "", errors.New("must not have leading or trailing whitespace")
	}
	if want := provenanceDigestHexLength(algorithm); want > 0 {
		value := strings.ToLower(trimmed)
		if !isLowerHex(value) {
			return "", errors.New("must be hex")
		}
		if len(value) != want {
			return "", fmt.Errorf("length %d, want %d for %s", len(value), want, algorithm)
		}
		return value, nil
	}
	if algorithm == "gitCommit" {
		value := strings.ToLower(trimmed)
		if !isLowerHex(value) {
			return "", errors.New("must be hex")
		}
		if len(value) != 40 && len(value) != 64 {
			return "", fmt.Errorf("length %d, want 40 or 64 for gitCommit", len(value))
		}
		return value, nil
	}
	for _, r := range trimmed {
		if unicode.IsControl(r) {
			return "", errors.New("contains control characters")
		}
	}
	return trimmed, nil
}

func provenanceDigestHexLength(algorithm string) int {
	switch algorithm {
	case "md5":
		return 32
	case "sha1":
		return 40
	case "sha224":
		return 56
	case "sha256":
		return 64
	case "sha384":
		return 96
	case "sha512":
		return 128
	default:
		return 0
	}
}

func normalizeProvenanceDefaultURI(field, value, defaultValue string) (string, error) {
	if value == "" {
		return defaultValue, nil
	}
	return normalizeProvenanceURI(field, value, true)
}

func normalizeProvenanceOptionalURI(field, value string) (string, error) {
	return normalizeProvenanceURI(field, value, false)
}

func normalizeProvenanceURI(field, value string, required bool) (string, error) {
	normalized, err := normalizeProvenanceText(field, value, required)
	if err != nil {
		return "", err
	}
	if normalized == "" {
		return "", nil
	}
	for _, r := range normalized {
		if unicode.IsSpace(r) {
			return "", invalidProvenance("%s must not contain whitespace", field)
		}
	}
	parsed, err := url.Parse(normalized)
	if err != nil || parsed.Scheme == "" {
		return "", invalidProvenance("%s must be a URI", field)
	}
	return normalized, nil
}

func normalizeProvenanceText(field, value string, required bool) (string, error) {
	normalized := strings.TrimSpace(value)
	if normalized == "" {
		if required {
			return "", invalidProvenance("%s is required", field)
		}
		return "", nil
	}
	if normalized != value {
		return "", invalidProvenance("%s must not have leading or trailing whitespace", field)
	}
	for _, r := range normalized {
		if unicode.IsControl(r) {
			return "", invalidProvenance("%s contains control characters", field)
		}
	}
	return normalized, nil
}

func normalizeProvenanceJSONObject(field string, values map[string]any, required bool) (map[string]any, error) {
	if len(values) == 0 {
		if required {
			return map[string]any{}, nil
		}
		return nil, nil
	}

	normalized := make(map[string]any, len(values))
	for key, value := range values {
		normalizedKey, err := normalizeProvenanceText(field+" key", key, true)
		if err != nil {
			return nil, err
		}
		normalizedValue, err := normalizeProvenanceJSONValue(field+"."+normalizedKey, value)
		if err != nil {
			return nil, err
		}
		normalized[normalizedKey] = normalizedValue
	}
	return normalized, nil
}

func normalizeProvenanceJSONValue(field string, value any) (any, error) {
	if number, ok := value.(json.Number); ok {
		if _, err := number.Float64(); err != nil {
			return nil, invalidProvenance("%s is not a valid JSON number", field)
		}
	}
	if f, ok := value.(float64); ok && (math.IsInf(f, 0) || math.IsNaN(f)) {
		return nil, invalidProvenance("%s is not a finite JSON number", field)
	}
	if f, ok := value.(float32); ok && (math.IsInf(float64(f), 0) || math.IsNaN(float64(f))) {
		return nil, invalidProvenance("%s is not a finite JSON number", field)
	}

	data, err := json.Marshal(value)
	if err != nil {
		return nil, invalidProvenance("%s is not JSON-serializable: %v", field, err)
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	var normalized any
	if err := decoder.Decode(&normalized); err != nil {
		return nil, invalidProvenance("%s is not JSON-serializable: %v", field, err)
	}
	return normalized, nil
}

func normalizeProvenanceStringMap(field string, values map[string]string) (map[string]string, error) {
	if len(values) == 0 {
		return nil, nil
	}
	normalized := make(map[string]string, len(values))
	for key, value := range values {
		normalizedKey, err := normalizeProvenanceText(field+" key", key, true)
		if err != nil {
			return nil, err
		}
		normalizedValue, err := normalizeProvenanceText(field+"."+normalizedKey, value, true)
		if err != nil {
			return nil, err
		}
		normalized[normalizedKey] = normalizedValue
	}
	return normalized, nil
}

func normalizeProvenanceTimestamp(field, value string) (string, *time.Time, error) {
	normalized, err := normalizeProvenanceText(field, value, false)
	if err != nil {
		return "", nil, err
	}
	if normalized == "" {
		return "", nil, nil
	}
	parsed, err := time.Parse(time.RFC3339, normalized)
	if err != nil {
		return "", nil, invalidProvenance("%s must be RFC3339: %v", field, err)
	}
	utc := parsed.UTC()
	return utc.Format(time.RFC3339), &utc, nil
}

func provenanceResourceEmpty(resource ProvenanceResource) bool {
	return resource.URI == "" &&
		len(resource.Digest) == 0 &&
		resource.Name == "" &&
		resource.DownloadLocation == "" &&
		resource.MediaType == "" &&
		len(resource.Content) == 0 &&
		len(resource.Annotations) == 0
}

func provenanceResourceLess(left, right ProvenanceResource) bool {
	leftKey := provenanceResourceSortKey(left)
	rightKey := provenanceResourceSortKey(right)
	for i := range leftKey {
		if leftKey[i] != rightKey[i] {
			return leftKey[i] < rightKey[i]
		}
	}
	return false
}

func provenanceResourceSortKey(resource ProvenanceResource) []string {
	return []string{
		resource.URI,
		resource.Name,
		resource.DownloadLocation,
		resource.MediaType,
		provenanceDigestSortKey(resource.Digest),
		string(resource.Content),
		provenanceJSONSortKey(resource.Annotations),
	}
}

func provenanceDigestSortKey(digests map[string]string) string {
	if len(digests) == 0 {
		return ""
	}
	keys := make([]string, 0, len(digests))
	for key := range digests {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	var builder strings.Builder
	for _, key := range keys {
		builder.WriteString(key)
		builder.WriteByte('=')
		builder.WriteString(digests[key])
		builder.WriteByte(';')
	}
	return builder.String()
}

func provenanceJSONSortKey(value any) string {
	if value == nil {
		return ""
	}
	data, err := json.Marshal(value)
	if err != nil {
		return ""
	}
	return string(data)
}

func isLowerHex(value string) bool {
	if value == "" {
		return false
	}
	for _, r := range value {
		switch {
		case r >= '0' && r <= '9':
		case r >= 'a' && r <= 'f':
		default:
			return false
		}
	}
	return true
}

func cloneProvenanceResources(resources []ProvenanceResource) []ProvenanceResource {
	if len(resources) == 0 {
		return nil
	}
	out := make([]ProvenanceResource, len(resources))
	for i, resource := range resources {
		out[i] = resource
		out[i].Digest = cloneStringMap(resource.Digest)
		out[i].Content = append([]byte(nil), resource.Content...)
		out[i].Annotations = cloneAnyMap(resource.Annotations)
	}
	return out
}

func cloneStringMap(values map[string]string) map[string]string {
	if len(values) == 0 {
		return nil
	}
	out := make(map[string]string, len(values))
	for key, value := range values {
		out[key] = value
	}
	return out
}

func cloneAnyMap(values map[string]any) map[string]any {
	if len(values) == 0 {
		return nil
	}
	out := make(map[string]any, len(values))
	for key, value := range values {
		out[key] = value
	}
	return out
}

func invalidProvenance(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrInvalidProvenance, fmt.Sprintf(format, args...))
}
