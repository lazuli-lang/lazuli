package search

import (
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	// DefaultIndexShardCount is used when an index plan does not request
	// explicit sharding.
	DefaultIndexShardCount uint32 = 1
)

var (
	errIndexPlanNameRequired       = errors.New("lazuli/search: index plan name is required")
	errIndexPlanSourceRequired     = errors.New("lazuli/search: at least one index source resource is required")
	errIndexPlanSourceNameRequired = errors.New("lazuli/search: index source resource name is required")
	errDuplicateIndexSource        = errors.New("lazuli/search: duplicate index source resource")
	errInvalidIndexRebuildMode     = errors.New("lazuli/search: invalid index rebuild mode")
	errInvalidIndexShard           = errors.New("lazuli/search: invalid index shard")
	errInvalidIndexPlanMetadata    = errors.New("lazuli/search: invalid index plan metadata")
)

// RebuildMode describes how a future indexer should treat existing indexed
// documents. The planner records this metadata but never talks to a search
// backend.
type RebuildMode string

const (
	// RebuildModeIncremental plans work from the current checkpoints. It is the
	// default when no mode is specified.
	RebuildModeIncremental RebuildMode = "incremental"
	// RebuildModeFull plans a complete rebuild of every source resource.
	RebuildModeFull RebuildMode = "full"
	// RebuildModeBackfill plans historical fill-in work without implying a
	// destructive index replacement.
	RebuildModeBackfill RebuildMode = "backfill"
)

func (m RebuildMode) String() string {
	return string(m)
}

// IndexSourceResource identifies one Lazuli resource that feeds a search index.
//
// Name is the preferred resource identifier. Resource is accepted as an alias
// for Name. Tenant is optional metadata for tenant-scoped plans. Count is the
// caller-provided number of source rows to divide into batch windows.
type IndexSourceResource struct {
	Name     string
	Resource string
	Tenant   string
	Count    uint64
}

// IndexBatchWindow is one ordered batch of source rows inside a source shard.
//
// Start is inclusive and End is exclusive. Offset mirrors Start and Limit is
// End-Start so generated adapters can use either range or offset terminology.
type IndexBatchWindow struct {
	Index  int
	Count  int
	Start  uint64
	End    uint64
	Offset uint64
	Limit  uint64
}

// IndexCheckpointKey scopes checkpoint storage for a source resource shard.
type IndexCheckpointKey struct {
	Index    string
	Resource string
	Tenant   string
	Mode     RebuildMode
	Shard    string
}

// IndexPlanOptions configures PlanIndex.
type IndexPlanOptions struct {
	// Name is the stable search index name.
	Name string
	// Mode records the rebuild/catch-up behavior requested by the caller.
	Mode RebuildMode
	// Sources are the Lazuli resources that feed the search index.
	Sources []IndexSourceResource
	// ShardCount is the number of deterministic source shards to plan. Zero
	// means DefaultIndexShardCount.
	ShardCount uint32
	// MaxBatchSize caps source row count per batch window. Zero means all rows
	// for the source shard are placed in one window.
	MaxBatchSize uint32
}

// IndexPlan is a side-effect-free async indexing plan.
type IndexPlan struct {
	Name           string
	Mode           RebuildMode
	SourceCount    int
	ShardCount     uint32
	ShardPlanCount int
	BatchCount     int
	MaxBatchSize   uint32
	Sources        []IndexSourceResource
	Shards         []IndexShardPlan
}

// IndexShardPlan is one resource/shard unit of async indexing work.
type IndexShardPlan struct {
	Index        string
	Source       IndexSourceResource
	Shard        string
	ShardIndex   int
	ShardCount   int
	CheckpointID string
	BatchCount   int
	Windows      []IndexBatchWindow
}

// BuildIndexPlan is an alias for PlanIndex.
func BuildIndexPlan(opts IndexPlanOptions) (IndexPlan, error) {
	return PlanIndex(opts)
}

// PlanIndex validates source resources and expands them into deterministic
// resource/shard work plans. It does not create indexes, read rows, enqueue
// jobs, or depend on any external search backend.
func PlanIndex(opts IndexPlanOptions) (IndexPlan, error) {
	name, err := normalizeIndexPlanName(opts.Name)
	if err != nil {
		return IndexPlan{}, err
	}
	mode, err := NormalizeRebuildMode(opts.Mode)
	if err != nil {
		return IndexPlan{}, err
	}
	sources, err := NormalizeIndexSourceResources(opts.Sources)
	if err != nil {
		return IndexPlan{}, err
	}

	shardCount := opts.ShardCount
	if shardCount == 0 {
		shardCount = DefaultIndexShardCount
	}

	plan := IndexPlan{
		Name:           name,
		Mode:           mode,
		SourceCount:    len(sources),
		ShardCount:     shardCount,
		ShardPlanCount: len(sources) * int(shardCount),
		MaxBatchSize:   opts.MaxBatchSize,
		Sources:        cloneIndexSourceResources(sources),
		Shards:         make([]IndexShardPlan, 0, len(sources)*int(shardCount)),
	}

	for _, source := range sources {
		for shardIndex := uint32(0); shardIndex < shardCount; shardIndex++ {
			shard, err := IndexShardName(shardIndex, shardCount)
			if err != nil {
				return IndexPlan{}, err
			}
			checkpointID, err := BuildIndexCheckpointID(IndexCheckpointKey{
				Index:    name,
				Resource: source.Name,
				Tenant:   source.Tenant,
				Mode:     mode,
				Shard:    shard,
			})
			if err != nil {
				return IndexPlan{}, err
			}

			windows := PlanIndexBatchWindows(indexSourceShardCount(source.Count, shardIndex, shardCount), opts.MaxBatchSize)
			plan.BatchCount += len(windows)
			plan.Shards = append(plan.Shards, IndexShardPlan{
				Index:        name,
				Source:       source,
				Shard:        shard,
				ShardIndex:   int(shardIndex),
				ShardCount:   int(shardCount),
				CheckpointID: checkpointID,
				BatchCount:   len(windows),
				Windows:      cloneIndexBatchWindows(windows),
			})
		}
	}

	return plan, nil
}

// ValidateIndexSourceResources validates source resources after normalization.
func ValidateIndexSourceResources(sources []IndexSourceResource) error {
	_, err := NormalizeIndexSourceResources(sources)
	return err
}

// NormalizeIndexSourceResources returns a validated, deterministic copy of
// sources sorted by resource name and tenant.
func NormalizeIndexSourceResources(sources []IndexSourceResource) ([]IndexSourceResource, error) {
	if len(sources) == 0 {
		return nil, errIndexPlanSourceRequired
	}

	seen := make(map[string]struct{}, len(sources))
	normalized := make([]IndexSourceResource, 0, len(sources))
	for _, source := range sources {
		clean, err := normalizeIndexSourceResource(source)
		if err != nil {
			return nil, err
		}
		key := clean.Name + "\x00" + clean.Tenant
		if _, ok := seen[key]; ok {
			return nil, errDuplicateIndexSource
		}
		seen[key] = struct{}{}
		normalized = append(normalized, clean)
	}

	sort.SliceStable(normalized, func(i, j int) bool {
		if normalized[i].Name != normalized[j].Name {
			return normalized[i].Name < normalized[j].Name
		}
		return normalized[i].Tenant < normalized[j].Tenant
	})
	return normalized, nil
}

// NormalizeRebuildMode returns a supported rebuild mode, defaulting empty mode
// to RebuildModeIncremental.
func NormalizeRebuildMode(mode RebuildMode) (RebuildMode, error) {
	normalized := RebuildMode(strings.ToLower(strings.TrimSpace(string(mode))))
	switch normalized {
	case "", RebuildModeIncremental:
		return RebuildModeIncremental, nil
	case RebuildModeFull, RebuildModeBackfill:
		return normalized, nil
	default:
		return "", fmt.Errorf("%w: %q", errInvalidIndexRebuildMode, mode)
	}
}

// PlanIndexBatchWindows splits total source rows into deterministic windows.
func PlanIndexBatchWindows(total uint64, maxBatchSize uint32) []IndexBatchWindow {
	if total == 0 {
		return nil
	}
	if maxBatchSize == 0 || uint64(maxBatchSize) >= total {
		return []IndexBatchWindow{indexBatchWindow(1, 1, 0, total)}
	}

	size := uint64(maxBatchSize)
	count := int((total + size - 1) / size)
	windows := make([]IndexBatchWindow, 0, count)
	for start := uint64(0); start < total; start += size {
		end := start + size
		if end > total {
			end = total
		}
		windows = append(windows, indexBatchWindow(len(windows)+1, count, start, end))
	}
	return windows
}

// BuildIndexCheckpointID returns a deterministic checkpoint identifier for a
// resource shard.
func BuildIndexCheckpointID(key IndexCheckpointKey) (string, error) {
	index, err := normalizeIndexPlanName(key.Index)
	if err != nil {
		return "", err
	}
	resource, err := normalizeIndexPlanSourceName(key.Resource)
	if err != nil {
		return "", err
	}
	tenant := strings.TrimSpace(key.Tenant)
	if err := validateIndexPlanText("tenant", tenant); err != nil {
		return "", err
	}
	mode, err := NormalizeRebuildMode(key.Mode)
	if err != nil {
		return "", err
	}
	shard := strings.TrimSpace(key.Shard)
	if shard == "" {
		shard = "shard-00-of-01"
	}
	if err := validateIndexPlanText("shard", shard); err != nil {
		return "", err
	}

	return strings.Join([]string{
		"search_index",
		index,
		resource,
		tenant,
		mode.String(),
		shard,
	}, ":"), nil
}

// String returns BuildIndexCheckpointID's stable representation, or an empty
// string when key is incomplete or invalid.
func (key IndexCheckpointKey) String() string {
	id, err := BuildIndexCheckpointID(key)
	if err != nil {
		return ""
	}
	return id
}

// IndexShardName returns the stable shard label used by IndexShardPlan.
func IndexShardName(shardIndex, shardCount uint32) (string, error) {
	if shardCount == 0 || shardIndex >= shardCount {
		return "", fmt.Errorf("%w: %d of %d", errInvalidIndexShard, shardIndex, shardCount)
	}

	width := len(strconv.FormatUint(uint64(shardCount), 10))
	if width < 2 {
		width = 2
	}
	return fmt.Sprintf("shard-%0*d-of-%0*d", width, shardIndex, width, shardCount), nil
}

func normalizeIndexSourceResource(source IndexSourceResource) (IndexSourceResource, error) {
	name := strings.TrimSpace(source.Name)
	if name == "" {
		name = strings.TrimSpace(source.Resource)
	}
	name, err := normalizeIndexPlanSourceName(name)
	if err != nil {
		return IndexSourceResource{}, err
	}

	tenant := strings.TrimSpace(source.Tenant)
	if err := validateIndexPlanText("tenant", tenant); err != nil {
		return IndexSourceResource{}, err
	}

	return IndexSourceResource{
		Name:   name,
		Tenant: tenant,
		Count:  source.Count,
	}, nil
}

func normalizeIndexPlanName(name string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", errIndexPlanNameRequired
	}
	if _, err := quoteDottedIdent(name); err != nil {
		return "", err
	}
	return name, nil
}

func normalizeIndexPlanSourceName(name string) (string, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return "", errIndexPlanSourceNameRequired
	}
	if _, err := quoteDottedIdent(name); err != nil {
		return "", err
	}
	return name, nil
}

func validateIndexPlanText(label, value string) error {
	if value == "" {
		return nil
	}
	if !utf8.ValidString(value) {
		return fmt.Errorf("%w: %s must be valid utf-8", errInvalidIndexPlanMetadata, label)
	}
	for _, r := range value {
		if unicode.IsControl(r) {
			return fmt.Errorf("%w: %s contains control character", errInvalidIndexPlanMetadata, label)
		}
	}
	return nil
}

func indexSourceShardCount(total uint64, shardIndex, shardCount uint32) uint64 {
	base := total / uint64(shardCount)
	remainder := total % uint64(shardCount)
	if uint64(shardIndex) < remainder {
		return base + 1
	}
	return base
}

func indexBatchWindow(index, count int, start, end uint64) IndexBatchWindow {
	return IndexBatchWindow{
		Index:  index,
		Count:  count,
		Start:  start,
		End:    end,
		Offset: start,
		Limit:  end - start,
	}
}

func cloneIndexSourceResources(sources []IndexSourceResource) []IndexSourceResource {
	if len(sources) == 0 {
		return nil
	}
	out := make([]IndexSourceResource, len(sources))
	copy(out, sources)
	return out
}

func cloneIndexBatchWindows(windows []IndexBatchWindow) []IndexBatchWindow {
	if len(windows) == 0 {
		return nil
	}
	out := make([]IndexBatchWindow, len(windows))
	copy(out, windows)
	return out
}
