package search

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
)

const (
	// DefaultTypesenseBatchSize is used when a descriptor does not request a
	// specific indexing batch size.
	DefaultTypesenseBatchSize = 100
	// MinTypesenseBatchSize is the lowest supported dry-run indexing batch.
	MinTypesenseBatchSize = 1
	// MaxTypesenseBatchSize caps dry-run indexing batches to avoid describing
	// unbounded adapter work.
	MaxTypesenseBatchSize = 1000
)

var (
	// ErrTypesenseDescriptorInvalid is returned when a Typesense descriptor
	// contains invalid node, collection, schema, sorting, or batch metadata.
	ErrTypesenseDescriptorInvalid = errors.New("lazuli/search: typesense_descriptor_invalid")
)

// TypesenseDescriptor describes local metadata a future Typesense adapter may
// bind. It performs no SDK, HTTP, or network calls.
type TypesenseDescriptor struct {
	Nodes       []TypesenseNode
	APIKey      string
	Collections []TypesenseCollection
	BatchSize   int
}

// TypesenseNode identifies a Typesense node by URL.
type TypesenseNode struct {
	URL string
}

// TypesenseCollection describes schema metadata for one collection.
type TypesenseCollection struct {
	Name                string
	Fields              []TypesenseField
	DefaultSortingField string
	BatchSize           int
}

// TypesenseField describes one collection schema field.
type TypesenseField struct {
	Name     string
	Type     string
	Facet    bool
	Optional bool
	Index    bool
	Sort     bool
}

// TypesenseBatchPlan is a dry-run indexing batch plan.
type TypesenseBatchPlan struct {
	Collection string
	BatchSize  int
	Total      uint64
	Windows    []TypesenseBatchWindow
}

// TypesenseBatchWindow is one ordered document window in a dry-run plan.
type TypesenseBatchWindow struct {
	Index  int
	Count  int
	Start  uint64
	End    uint64
	Offset uint64
	Limit  uint64
}

// TypesenseRedactedSummary is safe to log or expose in diagnostics.
type TypesenseRedactedSummary struct {
	Nodes       []string
	Collections []TypesenseCollectionSummary
	BatchSize   int
	APIKey      string
	HasAPIKey   bool
}

// TypesenseCollectionSummary is the non-secret collection portion of a
// redacted descriptor summary.
type TypesenseCollectionSummary struct {
	Name                string
	FieldCount          int
	Fields              []TypesenseField
	DefaultSortingField string
	BatchSize           int
}

// Normalize returns a descriptor with canonical nodes, collections, and batch
// metadata.
func (d TypesenseDescriptor) Normalize() (TypesenseDescriptor, error) {
	nodes, err := NormalizeTypesenseNodes(d.Nodes)
	if err != nil {
		return TypesenseDescriptor{}, err
	}
	collections, err := NormalizeTypesenseCollections(d.Collections)
	if err != nil {
		return TypesenseDescriptor{}, err
	}
	batchSize, err := NormalizeTypesenseBatchSize(d.BatchSize)
	if err != nil {
		return TypesenseDescriptor{}, err
	}
	return TypesenseDescriptor{
		Nodes:       nodes,
		APIKey:      strings.TrimSpace(d.APIKey),
		Collections: collections,
		BatchSize:   batchSize,
	}, nil
}

// Validate checks descriptor metadata without contacting Typesense.
func (d TypesenseDescriptor) Validate() error {
	return ValidateTypesenseDescriptor(d)
}

// PlanBatch returns a deterministic document batch plan for collection.
func (d TypesenseDescriptor) PlanBatch(collection string, total uint64) (TypesenseBatchPlan, error) {
	normalized, err := NormalizeTypesenseDescriptor(d)
	if err != nil {
		return TypesenseBatchPlan{}, err
	}
	collection = strings.TrimSpace(collection)
	if _, err := quoteDottedIdent(collection); err != nil {
		return TypesenseBatchPlan{}, fmt.Errorf("%w: collection name is invalid", ErrTypesenseDescriptorInvalid)
	}
	for _, candidate := range normalized.Collections {
		if candidate.Name != collection {
			continue
		}
		batchSize := candidate.BatchSize
		if batchSize == 0 {
			batchSize = normalized.BatchSize
		}
		return PlanTypesenseBatch(collection, total, batchSize)
	}
	return TypesenseBatchPlan{}, fmt.Errorf("%w: collection %q is unknown", ErrTypesenseDescriptorInvalid, collection)
}

// RedactedSummary returns a copy of descriptor metadata with secret-bearing
// values redacted for diagnostics.
func (d TypesenseDescriptor) RedactedSummary() TypesenseRedactedSummary {
	normalized, err := d.Normalize()
	if err == nil {
		d = normalized
	} else {
		d.APIKey = strings.TrimSpace(d.APIKey)
		d.BatchSize, _ = NormalizeTypesenseBatchSize(d.BatchSize)
	}

	nodes := make([]string, 0, len(d.Nodes))
	for _, node := range d.Nodes {
		nodes = append(nodes, RedactTypesenseNodeURL(node.URL))
	}

	collections := make([]TypesenseCollectionSummary, 0, len(d.Collections))
	for _, collection := range d.Collections {
		fields := cloneTypesenseFields(collection.Fields)
		collections = append(collections, TypesenseCollectionSummary{
			Name:                collection.Name,
			FieldCount:          len(fields),
			Fields:              fields,
			DefaultSortingField: collection.DefaultSortingField,
			BatchSize:           collection.BatchSize,
		})
	}

	return TypesenseRedactedSummary{
		Nodes:       nodes,
		Collections: collections,
		BatchSize:   d.BatchSize,
		APIKey:      RedactTypesenseAPIKey(d.APIKey),
		HasAPIKey:   strings.TrimSpace(d.APIKey) != "",
	}
}

// NormalizeTypesenseDescriptor returns a canonical descriptor copy.
func NormalizeTypesenseDescriptor(descriptor TypesenseDescriptor) (TypesenseDescriptor, error) {
	return descriptor.Normalize()
}

// ValidateTypesenseDescriptor checks node, API key, collection, schema,
// sorting, and batch metadata.
func ValidateTypesenseDescriptor(descriptor TypesenseDescriptor) error {
	descriptor, err := descriptor.Normalize()
	if err != nil {
		return err
	}
	if len(descriptor.Nodes) == 0 {
		return fmt.Errorf("%w: at least one node is required", ErrTypesenseDescriptorInvalid)
	}
	if descriptor.APIKey == "" {
		return fmt.Errorf("%w: api key is required", ErrTypesenseDescriptorInvalid)
	}
	if len(descriptor.Collections) == 0 {
		return fmt.Errorf("%w: at least one collection is required", ErrTypesenseDescriptorInvalid)
	}
	return nil
}

// NormalizeTypesenseNodes returns validated, deduplicated, deterministic node
// URLs.
func NormalizeTypesenseNodes(nodes []TypesenseNode) ([]TypesenseNode, error) {
	if len(nodes) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{}, len(nodes))
	normalized := make([]TypesenseNode, 0, len(nodes))
	for _, node := range nodes {
		nodeURL, err := NormalizeTypesenseNodeURL(node.URL)
		if err != nil {
			return nil, err
		}
		if _, ok := seen[nodeURL]; ok {
			continue
		}
		seen[nodeURL] = struct{}{}
		normalized = append(normalized, TypesenseNode{URL: nodeURL})
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].URL < normalized[j].URL
	})
	return normalized, nil
}

// NormalizeTypesenseNodeURL returns a canonical node URL without credentials,
// query, fragment, or trailing slash.
func NormalizeTypesenseNodeURL(raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", fmt.Errorf("%w: node url is required", ErrTypesenseDescriptorInvalid)
	}
	parsed, err := url.Parse(raw)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "", fmt.Errorf("%w: node url is invalid", ErrTypesenseDescriptorInvalid)
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	if parsed.Scheme != "http" && parsed.Scheme != "https" {
		return "", fmt.Errorf("%w: node url scheme must be http or https", ErrTypesenseDescriptorInvalid)
	}
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.User = nil
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.RawPath = ""
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
}

// RedactTypesenseNodeURL returns a URL safe for diagnostics.
func RedactTypesenseNodeURL(raw string) string {
	normalized, err := NormalizeTypesenseNodeURL(raw)
	if err != nil {
		return "[redacted]"
	}
	parsed, err := url.Parse(normalized)
	if err != nil || parsed.Host == "" {
		return "[redacted]"
	}
	return parsed.Scheme + "://" + parsed.Host
}

// RedactTypesenseAPIKey returns a fixed token for non-empty API keys.
func RedactTypesenseAPIKey(key string) string {
	if strings.TrimSpace(key) == "" {
		return ""
	}
	return "[redacted]"
}

// NormalizeTypesenseCollections returns validated, sorted collection metadata.
func NormalizeTypesenseCollections(collections []TypesenseCollection) ([]TypesenseCollection, error) {
	if len(collections) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{}, len(collections))
	normalized := make([]TypesenseCollection, 0, len(collections))
	for _, collection := range collections {
		clean, err := normalizeTypesenseCollection(collection)
		if err != nil {
			return nil, err
		}
		if _, ok := seen[clean.Name]; ok {
			return nil, fmt.Errorf("%w: duplicate collection %q", ErrTypesenseDescriptorInvalid, clean.Name)
		}
		seen[clean.Name] = struct{}{}
		normalized = append(normalized, clean)
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

// NormalizeTypesenseFields returns validated, sorted schema field metadata.
func NormalizeTypesenseFields(fields []TypesenseField) ([]TypesenseField, error) {
	if len(fields) == 0 {
		return nil, fmt.Errorf("%w: at least one schema field is required", ErrTypesenseDescriptorInvalid)
	}
	seen := make(map[string]struct{}, len(fields))
	normalized := make([]TypesenseField, 0, len(fields))
	for _, field := range fields {
		clean, err := normalizeTypesenseField(field)
		if err != nil {
			return nil, err
		}
		if _, ok := seen[clean.Name]; ok {
			return nil, fmt.Errorf("%w: duplicate schema field %q", ErrTypesenseDescriptorInvalid, clean.Name)
		}
		seen[clean.Name] = struct{}{}
		normalized = append(normalized, clean)
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

// NormalizeTypesenseBatchSize returns the default batch size when size is zero
// and rejects values outside the supported dry-run bounds.
func NormalizeTypesenseBatchSize(size int) (int, error) {
	if size == 0 {
		return DefaultTypesenseBatchSize, nil
	}
	if size < MinTypesenseBatchSize || size > MaxTypesenseBatchSize {
		return 0, fmt.Errorf("%w: batch size must be between %d and %d", ErrTypesenseDescriptorInvalid, MinTypesenseBatchSize, MaxTypesenseBatchSize)
	}
	return size, nil
}

// PlanTypesenseBatch returns deterministic document batch windows.
func PlanTypesenseBatch(collection string, total uint64, batchSize int) (TypesenseBatchPlan, error) {
	collection = strings.TrimSpace(collection)
	if _, err := quoteDottedIdent(collection); err != nil {
		return TypesenseBatchPlan{}, fmt.Errorf("%w: collection name is invalid", ErrTypesenseDescriptorInvalid)
	}
	batchSize, err := NormalizeTypesenseBatchSize(batchSize)
	if err != nil {
		return TypesenseBatchPlan{}, err
	}
	return TypesenseBatchPlan{
		Collection: collection,
		BatchSize:  batchSize,
		Total:      total,
		Windows:    PlanTypesenseBatchWindows(total, batchSize),
	}, nil
}

// PlanTypesenseBatchWindows splits total documents into deterministic windows.
func PlanTypesenseBatchWindows(total uint64, batchSize int) []TypesenseBatchWindow {
	if total == 0 {
		return nil
	}
	if batchSize <= 0 {
		batchSize = DefaultTypesenseBatchSize
	}
	size := uint64(batchSize)
	count := int((total + size - 1) / size)
	windows := make([]TypesenseBatchWindow, 0, count)
	for start := uint64(0); start < total; start += size {
		end := start + size
		if end > total {
			end = total
		}
		windows = append(windows, TypesenseBatchWindow{
			Index:  len(windows) + 1,
			Count:  count,
			Start:  start,
			End:    end,
			Offset: start,
			Limit:  end - start,
		})
	}
	return windows
}

func normalizeTypesenseCollection(collection TypesenseCollection) (TypesenseCollection, error) {
	name := strings.TrimSpace(collection.Name)
	if _, err := quoteDottedIdent(name); err != nil {
		return TypesenseCollection{}, fmt.Errorf("%w: collection name is invalid", ErrTypesenseDescriptorInvalid)
	}
	fields, err := NormalizeTypesenseFields(collection.Fields)
	if err != nil {
		return TypesenseCollection{}, err
	}
	defaultSortingField := strings.TrimSpace(collection.DefaultSortingField)
	if defaultSortingField != "" {
		if err := validateTypesenseDefaultSortingField(defaultSortingField, fields); err != nil {
			return TypesenseCollection{}, err
		}
	}
	batchSize := collection.BatchSize
	if batchSize != 0 {
		var err error
		batchSize, err = NormalizeTypesenseBatchSize(batchSize)
		if err != nil {
			return TypesenseCollection{}, err
		}
	}
	return TypesenseCollection{
		Name:                name,
		Fields:              fields,
		DefaultSortingField: defaultSortingField,
		BatchSize:           batchSize,
	}, nil
}

func normalizeTypesenseField(field TypesenseField) (TypesenseField, error) {
	name := strings.TrimSpace(field.Name)
	if _, err := quoteDottedIdent(name); err != nil {
		return TypesenseField{}, fmt.Errorf("%w: schema field name is invalid", ErrTypesenseDescriptorInvalid)
	}
	fieldType, err := normalizeTypesenseFieldType(field.Type)
	if err != nil {
		return TypesenseField{}, err
	}
	return TypesenseField{
		Name:     name,
		Type:     fieldType,
		Facet:    field.Facet,
		Optional: field.Optional,
		Index:    field.Index,
		Sort:     field.Sort,
	}, nil
}

func normalizeTypesenseFieldType(fieldType string) (string, error) {
	fieldType = strings.ToLower(strings.TrimSpace(fieldType))
	switch fieldType {
	case "string", "string[]", "int32", "int32[]", "int64", "int64[]", "float", "float[]", "bool", "bool[]", "geopoint", "geopoint[]", "object", "object[]", "string*":
		return fieldType, nil
	default:
		return "", fmt.Errorf("%w: schema field type %q is invalid", ErrTypesenseDescriptorInvalid, fieldType)
	}
}

func validateTypesenseDefaultSortingField(name string, fields []TypesenseField) error {
	if _, err := quoteDottedIdent(name); err != nil {
		return fmt.Errorf("%w: default sorting field is invalid", ErrTypesenseDescriptorInvalid)
	}
	for _, field := range fields {
		if field.Name != name {
			continue
		}
		if !typesenseFieldTypeSortable(field.Type) {
			return fmt.Errorf("%w: default sorting field %q has non-sortable type", ErrTypesenseDescriptorInvalid, name)
		}
		return nil
	}
	return fmt.Errorf("%w: default sorting field %q is unknown", ErrTypesenseDescriptorInvalid, name)
}

func typesenseFieldTypeSortable(fieldType string) bool {
	switch fieldType {
	case "int32", "int64", "float":
		return true
	default:
		return false
	}
}

func cloneTypesenseFields(fields []TypesenseField) []TypesenseField {
	if len(fields) == 0 {
		return nil
	}
	out := make([]TypesenseField, len(fields))
	copy(out, fields)
	return out
}
