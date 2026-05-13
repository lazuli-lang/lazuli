package search

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	// DefaultAlgoliaSyncBatchSize is used when a descriptor does not request a
	// positive sync batch size.
	DefaultAlgoliaSyncBatchSize = 100
	// MinAlgoliaSyncBatchSize is the smallest supported dry-run sync batch.
	MinAlgoliaSyncBatchSize = 1
	// MaxAlgoliaSyncBatchSize is the largest supported dry-run sync batch.
	MaxAlgoliaSyncBatchSize = 1000
)

var (
	errAlgoliaAppIDRequired      = errors.New("lazuli/search: algolia app id is required")
	errAlgoliaIndexRequired      = errors.New("lazuli/search: algolia index name is required")
	errAlgoliaAPIKeyRequired     = errors.New("lazuli/search: algolia api key is required")
	errInvalidAlgoliaDescriptor  = errors.New("lazuli/search: invalid algolia descriptor")
	errDuplicateAlgoliaReplica   = errors.New("lazuli/search: duplicate algolia replica")
	errDuplicateAlgoliaFacet     = errors.New("lazuli/search: duplicate algolia facet")
	errDuplicateAlgoliaRanking   = errors.New("lazuli/search: duplicate algolia ranking rule")
	errInvalidAlgoliaSyncBatch   = errors.New("lazuli/search: invalid algolia sync batch size")
	errInvalidAlgoliaEndpointURL = errors.New("lazuli/search: invalid algolia endpoint url")
)

// AlgoliaDescriptor records Algolia index metadata for a future adapter. It is
// side-effect-free and does not perform SDK, HTTP, or DNS work.
type AlgoliaDescriptor struct {
	AppID         string
	IndexName     string
	APIKey        string
	EndpointURL   string
	Replicas      []AlgoliaReplica
	Ranking       []string
	Facets        []AlgoliaFacet
	SyncBatchSize int
}

// AlgoliaReplica describes replica index metadata and optional replica-specific
// ranking rules.
type AlgoliaReplica struct {
	Name    string
	Ranking []string
}

// AlgoliaFacet describes one searchable/filterable attribute exposed as facet
// metadata.
type AlgoliaFacet struct {
	Attribute  string
	Searchable bool
	FilterOnly bool
}

// AlgoliaSyncPlan is a deterministic dry-run sync plan.
type AlgoliaSyncPlan struct {
	IndexName     string
	TotalRecords  uint64
	SyncBatchSize int
	BatchCount    int
	Batches       []AlgoliaSyncBatch
}

// AlgoliaSyncBatch is one dry-run record range for a future sync adapter.
type AlgoliaSyncBatch struct {
	Index  int
	Count  int
	Start  uint64
	End    uint64
	Offset uint64
	Limit  uint64
}

// AlgoliaRedactedSummary is safe to log or expose in diagnostics.
type AlgoliaRedactedSummary struct {
	AppIDRedacted string
	IndexName     string
	APIKey        string
	EndpointURL   string
	HasAppID      bool
	HasAPIKey     bool
	ReplicaCount  int
	RankingCount  int
	FacetCount    int
	SyncBatchSize int
	Replicas      []string
	Ranking       []string
	Facets        []AlgoliaFacet
}

// Normalize returns a canonical descriptor copy.
func (d AlgoliaDescriptor) Normalize() (AlgoliaDescriptor, error) {
	return NormalizeAlgoliaDescriptor(d)
}

// Validate checks descriptor metadata without performing any provider calls.
func (d AlgoliaDescriptor) Validate() error {
	return ValidateAlgoliaDescriptor(d)
}

// PlanSync returns a deterministic dry-run sync plan for totalRecords.
func (d AlgoliaDescriptor) PlanSync(totalRecords uint64) (AlgoliaSyncPlan, error) {
	normalized, err := NormalizeAlgoliaDescriptor(d)
	if err != nil {
		return AlgoliaSyncPlan{}, err
	}
	return PlanAlgoliaSync(normalized.IndexName, totalRecords, normalized.SyncBatchSize)
}

// RedactedSummary returns diagnostic metadata with secret-bearing values
// redacted.
func (d AlgoliaDescriptor) RedactedSummary() AlgoliaRedactedSummary {
	normalized, err := NormalizeAlgoliaDescriptor(d)
	if err == nil {
		d = normalized
	} else {
		d.AppID = normalizeAlgoliaAppID(d.AppID)
		d.IndexName = strings.TrimSpace(d.IndexName)
		d.APIKey = strings.TrimSpace(d.APIKey)
		d.EndpointURL = strings.TrimSpace(d.EndpointURL)
		d.Replicas, _ = normalizeAlgoliaReplicas(d.Replicas)
		d.Ranking, _ = normalizeAlgoliaRanking(d.Ranking)
		d.Facets, _ = normalizeAlgoliaFacets(d.Facets)
		d.SyncBatchSize, _ = NormalizeAlgoliaSyncBatchSize(d.SyncBatchSize)
	}

	return AlgoliaRedactedSummary{
		AppIDRedacted: redactAlgoliaSecret(d.AppID),
		IndexName:     d.IndexName,
		APIKey:        redactAlgoliaSecret(d.APIKey),
		EndpointURL:   redactAlgoliaURL(d.EndpointURL),
		HasAppID:      strings.TrimSpace(d.AppID) != "",
		HasAPIKey:     strings.TrimSpace(d.APIKey) != "",
		ReplicaCount:  len(d.Replicas),
		RankingCount:  len(d.Ranking),
		FacetCount:    len(d.Facets),
		SyncBatchSize: d.SyncBatchSize,
		Replicas:      algoliaReplicaNames(d.Replicas),
		Ranking:       append([]string(nil), d.Ranking...),
		Facets:        cloneAlgoliaFacets(d.Facets),
	}
}

// NormalizeAlgoliaDescriptor returns a canonical descriptor copy.
func NormalizeAlgoliaDescriptor(descriptor AlgoliaDescriptor) (AlgoliaDescriptor, error) {
	var err error
	descriptor.AppID = normalizeAlgoliaAppID(descriptor.AppID)
	descriptor.IndexName = strings.TrimSpace(descriptor.IndexName)
	descriptor.APIKey = strings.TrimSpace(descriptor.APIKey)
	descriptor.EndpointURL, err = NormalizeAlgoliaEndpointURL(descriptor.EndpointURL)
	if err != nil {
		return AlgoliaDescriptor{}, err
	}
	descriptor.Replicas, err = normalizeAlgoliaReplicas(descriptor.Replicas)
	if err != nil {
		return AlgoliaDescriptor{}, err
	}
	descriptor.Ranking, err = normalizeAlgoliaRanking(descriptor.Ranking)
	if err != nil {
		return AlgoliaDescriptor{}, err
	}
	descriptor.Facets, err = normalizeAlgoliaFacets(descriptor.Facets)
	if err != nil {
		return AlgoliaDescriptor{}, err
	}
	descriptor.SyncBatchSize, err = NormalizeAlgoliaSyncBatchSize(descriptor.SyncBatchSize)
	if err != nil {
		return AlgoliaDescriptor{}, err
	}
	return descriptor, nil
}

// ValidateAlgoliaDescriptor checks app, index, key, endpoint, replica, ranking,
// facet, and sync batch metadata.
func ValidateAlgoliaDescriptor(descriptor AlgoliaDescriptor) error {
	descriptor, err := NormalizeAlgoliaDescriptor(descriptor)
	if err != nil {
		return err
	}
	return errors.Join(
		ValidateAlgoliaAppID(descriptor.AppID),
		ValidateAlgoliaIndexName(descriptor.IndexName),
		ValidateAlgoliaAPIKey(descriptor.APIKey),
		ValidateAlgoliaEndpointURL(descriptor.EndpointURL),
	)
}

// ValidateAlgoliaAppID checks local application ID metadata.
func ValidateAlgoliaAppID(appID string) error {
	appID = normalizeAlgoliaAppID(appID)
	if appID == "" {
		return errAlgoliaAppIDRequired
	}
	return validateAlgoliaText("app id", appID)
}

// ValidateAlgoliaIndexName checks local index name metadata.
func ValidateAlgoliaIndexName(indexName string) error {
	indexName = strings.TrimSpace(indexName)
	if indexName == "" {
		return errAlgoliaIndexRequired
	}
	return validateAlgoliaText("index name", indexName)
}

// ValidateAlgoliaAPIKey checks local API key metadata without verifying the key
// against a provider.
func ValidateAlgoliaAPIKey(apiKey string) error {
	apiKey = strings.TrimSpace(apiKey)
	if apiKey == "" {
		return errAlgoliaAPIKeyRequired
	}
	return validateAlgoliaText("api key", apiKey)
}

// NormalizeAlgoliaEndpointURL returns a canonical optional endpoint URL.
func NormalizeAlgoliaEndpointURL(endpoint string) (string, error) {
	endpoint = strings.TrimSpace(endpoint)
	if endpoint == "" {
		return "", nil
	}
	parsed, err := url.Parse(endpoint)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return "", errInvalidAlgoliaEndpointURL
	}
	parsed.Scheme = strings.ToLower(parsed.Scheme)
	if parsed.Scheme != "https" && parsed.Scheme != "http" {
		return "", errInvalidAlgoliaEndpointURL
	}
	parsed.Host = strings.ToLower(parsed.Host)
	parsed.Path = strings.TrimRight(parsed.Path, "/")
	parsed.User = nil
	parsed.RawQuery = ""
	parsed.Fragment = ""
	return parsed.String(), nil
}

// ValidateAlgoliaEndpointURL checks optional endpoint URL metadata.
func ValidateAlgoliaEndpointURL(endpoint string) error {
	_, err := NormalizeAlgoliaEndpointURL(endpoint)
	return err
}

// NormalizeAlgoliaSyncBatchSize applies the descriptor batch-size bounds.
func NormalizeAlgoliaSyncBatchSize(size int) (int, error) {
	if size == 0 {
		return DefaultAlgoliaSyncBatchSize, nil
	}
	if size < MinAlgoliaSyncBatchSize || size > MaxAlgoliaSyncBatchSize {
		return 0, fmt.Errorf("%w: must be between %d and %d", errInvalidAlgoliaSyncBatch, MinAlgoliaSyncBatchSize, MaxAlgoliaSyncBatchSize)
	}
	return size, nil
}

// PlanAlgoliaSync returns a deterministic dry-run batch plan.
func PlanAlgoliaSync(indexName string, totalRecords uint64, batchSize int) (AlgoliaSyncPlan, error) {
	indexName = strings.TrimSpace(indexName)
	if err := ValidateAlgoliaIndexName(indexName); err != nil {
		return AlgoliaSyncPlan{}, err
	}
	size, err := NormalizeAlgoliaSyncBatchSize(batchSize)
	if err != nil {
		return AlgoliaSyncPlan{}, err
	}
	batches := PlanAlgoliaSyncBatches(totalRecords, size)
	return AlgoliaSyncPlan{
		IndexName:     indexName,
		TotalRecords:  totalRecords,
		SyncBatchSize: size,
		BatchCount:    len(batches),
		Batches:       cloneAlgoliaSyncBatches(batches),
	}, nil
}

// PlanAlgoliaSyncBatches splits total records into deterministic sync windows.
func PlanAlgoliaSyncBatches(totalRecords uint64, batchSize int) []AlgoliaSyncBatch {
	size, err := NormalizeAlgoliaSyncBatchSize(batchSize)
	if err != nil || totalRecords == 0 {
		return nil
	}
	count := int((totalRecords + uint64(size) - 1) / uint64(size))
	batches := make([]AlgoliaSyncBatch, 0, count)
	for start := uint64(0); start < totalRecords; start += uint64(size) {
		end := start + uint64(size)
		if end > totalRecords {
			end = totalRecords
		}
		batches = append(batches, AlgoliaSyncBatch{
			Index:  len(batches) + 1,
			Count:  count,
			Start:  start,
			End:    end,
			Offset: start,
			Limit:  end - start,
		})
	}
	return batches
}

func normalizeAlgoliaAppID(appID string) string {
	return strings.ToUpper(strings.TrimSpace(appID))
}

func normalizeAlgoliaReplicas(replicas []AlgoliaReplica) ([]AlgoliaReplica, error) {
	if len(replicas) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{}, len(replicas))
	normalized := make([]AlgoliaReplica, 0, len(replicas))
	for _, replica := range replicas {
		name := strings.TrimSpace(replica.Name)
		if err := ValidateAlgoliaIndexName(name); err != nil {
			return nil, err
		}
		if _, ok := seen[name]; ok {
			return nil, errDuplicateAlgoliaReplica
		}
		ranking, err := normalizeAlgoliaRanking(replica.Ranking)
		if err != nil {
			return nil, err
		}
		seen[name] = struct{}{}
		normalized = append(normalized, AlgoliaReplica{Name: name, Ranking: ranking})
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Name < normalized[j].Name
	})
	return normalized, nil
}

func normalizeAlgoliaRanking(ranking []string) ([]string, error) {
	if len(ranking) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{}, len(ranking))
	normalized := make([]string, 0, len(ranking))
	for _, rule := range ranking {
		rule = strings.TrimSpace(rule)
		if rule == "" {
			continue
		}
		if err := validateAlgoliaText("ranking rule", rule); err != nil {
			return nil, err
		}
		key := strings.ToLower(rule)
		if _, ok := seen[key]; ok {
			return nil, errDuplicateAlgoliaRanking
		}
		seen[key] = struct{}{}
		normalized = append(normalized, rule)
	}
	if len(normalized) == 0 {
		return nil, nil
	}
	return normalized, nil
}

func normalizeAlgoliaFacets(facets []AlgoliaFacet) ([]AlgoliaFacet, error) {
	if len(facets) == 0 {
		return nil, nil
	}
	seen := make(map[string]struct{}, len(facets))
	normalized := make([]AlgoliaFacet, 0, len(facets))
	for _, facet := range facets {
		attribute := strings.TrimSpace(facet.Attribute)
		if attribute == "" {
			return nil, fmt.Errorf("%w: facet attribute is required", errInvalidAlgoliaDescriptor)
		}
		if err := validateAlgoliaAttribute("facet attribute", attribute); err != nil {
			return nil, err
		}
		key := strings.ToLower(attribute)
		if _, ok := seen[key]; ok {
			return nil, errDuplicateAlgoliaFacet
		}
		seen[key] = struct{}{}
		normalized = append(normalized, AlgoliaFacet{
			Attribute:  attribute,
			Searchable: facet.Searchable,
			FilterOnly: facet.FilterOnly,
		})
	}
	sort.SliceStable(normalized, func(i, j int) bool {
		return normalized[i].Attribute < normalized[j].Attribute
	})
	return normalized, nil
}

func validateAlgoliaAttribute(label, value string) error {
	if _, err := quoteDottedIdent(value); err != nil {
		return fmt.Errorf("%w: %s is invalid", errInvalidAlgoliaDescriptor, label)
	}
	return nil
}

func validateAlgoliaText(label, value string) error {
	if value == "" {
		return nil
	}
	if !utf8.ValidString(value) {
		return fmt.Errorf("%w: %s must be valid utf-8", errInvalidAlgoliaDescriptor, label)
	}
	for _, r := range value {
		if unicode.IsControl(r) {
			return fmt.Errorf("%w: %s contains control character", errInvalidAlgoliaDescriptor, label)
		}
	}
	return nil
}

func redactAlgoliaSecret(value string) string {
	if strings.TrimSpace(value) == "" {
		return ""
	}
	return "[redacted]"
}

func redactAlgoliaURL(raw string) string {
	normalized, err := NormalizeAlgoliaEndpointURL(raw)
	if err != nil {
		return "[redacted]"
	}
	return normalized
}

func algoliaReplicaNames(replicas []AlgoliaReplica) []string {
	if len(replicas) == 0 {
		return nil
	}
	names := make([]string, 0, len(replicas))
	for _, replica := range replicas {
		names = append(names, replica.Name)
	}
	return names
}

func cloneAlgoliaFacets(facets []AlgoliaFacet) []AlgoliaFacet {
	if len(facets) == 0 {
		return nil
	}
	out := make([]AlgoliaFacet, len(facets))
	copy(out, facets)
	return out
}

func cloneAlgoliaSyncBatches(batches []AlgoliaSyncBatch) []AlgoliaSyncBatch {
	if len(batches) == 0 {
		return nil
	}
	out := make([]AlgoliaSyncBatch, len(batches))
	copy(out, batches)
	return out
}
