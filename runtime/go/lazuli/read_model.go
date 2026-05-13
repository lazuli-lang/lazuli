package lazuli

import (
	"strconv"
	"time"
)

// ReadModelProjection describes the event sources that maintain a read model.
//
// It is metadata only: helpers in this file plan replay/list filters and
// checkpoint keys without appending to or mutating an EventStore.
type ReadModelProjection struct {
	// Name is the stable projection name used in checkpoints and logs.
	Name string

	// Sources are the event filters that can update the projection. Empty means
	// every source event is eligible.
	Sources []ReadModelSource
}

// NewReadModelProjection returns projection metadata with cloned sources.
func NewReadModelProjection(name string, sources ...ReadModelSource) ReadModelProjection {
	return ReadModelProjection{
		Name:    name,
		Sources: cloneReadModelSources(sources),
	}
}

// ReadModelSource selects source events for a projection.
type ReadModelSource struct {
	// Names limits the source to specific event names. Empty means all names.
	Names []string

	// Tenant limits the source to a tenant scope. Nil leaves tenant selection to
	// the caller or store implementation.
	Tenant *Tenant

	// Since is an inclusive OccurredAt lower bound for replay filters.
	Since time.Time

	// Until is an exclusive OccurredAt upper bound for replay filters.
	Until time.Time
}

// ReadModelSourceForEvents returns a source filter for event names.
func ReadModelSourceForEvents(names ...string) ReadModelSource {
	return ReadModelSource{Names: append([]string(nil), names...)}
}

// ReplayFilter converts source to the existing EventReplayFilter shape.
func (s ReadModelSource) ReplayFilter() EventReplayFilter {
	return EventReplayFilter{
		Names:  append([]string(nil), s.Names...),
		Tenant: cloneReadModelTenant(s.Tenant),
		Since:  s.Since,
		Until:  s.Until,
	}
}

// ListFilters converts source to EventListFilter values starting after
// sinceSequence. EventListFilter supports one event name per filter, so a
// source with multiple names expands into one filter per name.
func (s ReadModelSource) ListFilters(sinceSequence uint64) []EventListFilter {
	tenant := cloneReadModelTenant(s.Tenant)
	if len(s.Names) == 0 {
		return []EventListFilter{{
			Tenant:        tenant,
			SinceSequence: sinceSequence,
		}}
	}

	filters := make([]EventListFilter, 0, len(s.Names))
	for _, name := range s.Names {
		filters = append(filters, EventListFilter{
			Name:          name,
			Tenant:        cloneReadModelTenant(tenant),
			SinceSequence: sinceSequence,
		})
	}
	return filters
}

// ReplayFilters returns projection source filters for event replay.
func (p ReadModelProjection) ReplayFilters() []EventReplayFilter {
	sources := readModelProjectionSources(p)
	filters := make([]EventReplayFilter, 0, len(sources))
	for _, source := range sources {
		filters = append(filters, source.ReplayFilter())
	}
	return filters
}

// ListFilters returns EventStore.List filters for projection sources starting
// after sinceSequence.
func (p ReadModelProjection) ListFilters(sinceSequence uint64) []EventListFilter {
	sources := readModelProjectionSources(p)
	var filters []EventListFilter
	for _, source := range sources {
		filters = append(filters, source.ListFilters(sinceSequence)...)
	}
	return filters
}

// ReadModelCheckpoint records the last stored event incorporated into a
// projection rebuild or catch-up run.
type ReadModelCheckpoint struct {
	Projection string
	Tenant     string

	// Sequence is the last EventStore sequence applied to the projection.
	Sequence uint64

	// EventName and EventOccurredAt describe the event at Sequence for
	// diagnostics and lag summaries.
	EventName       string
	EventOccurredAt time.Time

	// UpdatedAt is when the checkpoint was recorded.
	UpdatedAt time.Time
}

// ReadModelCheckpointForEvent builds a checkpoint from a stored event.
func ReadModelCheckpointForEvent(
	projection string,
	stored StoredEvent,
	updatedAt time.Time,
) ReadModelCheckpoint {
	return ReadModelCheckpoint{
		Projection:      projection,
		Tenant:          readModelTenantString(stored.Event.Tenant),
		Sequence:        stored.Sequence,
		EventName:       stored.Event.Name,
		EventOccurredAt: stored.Event.OccurredAt,
		UpdatedAt:       updatedAt,
	}
}

// ReadModelRebuildPlan is a pure plan for replaying source events after a
// checkpoint.
type ReadModelRebuildPlan struct {
	Projection string
	PlannedAt  time.Time
	Checkpoint ReadModelCheckpoint
	Sources    []ReadModelSource

	// ReplayFilters preserve time-window metadata for EventReplayStore
	// implementations.
	ReplayFilters []EventReplayFilter

	// ListFilters preserve sequence checkpoints for EventStore.List users.
	ListFilters []EventListFilter
}

// PlanRebuild returns a replay/list plan that starts after checkpoint.Sequence.
func (p ReadModelProjection) PlanRebuild(
	checkpoint ReadModelCheckpoint,
	plannedAt time.Time,
) ReadModelRebuildPlan {
	projection := p.Name
	if projection == "" {
		projection = checkpoint.Projection
	}
	if checkpoint.Projection == "" {
		checkpoint.Projection = projection
	}
	sources := readModelProjectionSources(p)
	return ReadModelRebuildPlan{
		Projection:    projection,
		PlannedAt:     plannedAt,
		Checkpoint:    checkpoint,
		Sources:       cloneReadModelSources(sources),
		ReplayFilters: p.ReplayFilters(),
		ListFilters:   p.ListFilters(checkpoint.Sequence),
	}
}

// ReadModelIdempotencyKey scopes duplicate projection work to one stored event.
type ReadModelIdempotencyKey struct {
	Projection string
	Tenant     string
	EventName  string
	Sequence   uint64
}

// ReadModelIdempotencyKeyForEvent returns a deterministic key for applying a
// stored event to projection.
func ReadModelIdempotencyKeyForEvent(projection string, stored StoredEvent) ReadModelIdempotencyKey {
	return ReadModelIdempotencyKey{
		Projection: projection,
		Tenant:     readModelTenantString(stored.Event.Tenant),
		EventName:  stored.Event.Name,
		Sequence:   stored.Sequence,
	}
}

// Empty reports whether key cannot identify a stored event for a projection.
func (k ReadModelIdempotencyKey) Empty() bool {
	return k.Projection == "" || k.Sequence == 0
}

// String returns a stable diagnostic representation of key.
func (k ReadModelIdempotencyKey) String() string {
	if k.Empty() {
		return ""
	}
	return k.Projection + ":" + k.Tenant + ":" + k.EventName + ":" + strconv.FormatUint(k.Sequence, 10)
}

// ReadModelLagSummary compares a checkpoint with the current event high-water
// mark for a projection.
type ReadModelLagSummary struct {
	Projection string
	Tenant     string
	ObservedAt time.Time

	CheckpointSequence    uint64
	HighWatermarkSequence uint64
	SequenceLag           uint64

	CheckpointEventAt    time.Time
	HighWatermarkEventAt time.Time
	EventTimeLag         time.Duration
}

// CaughtUp reports whether the checkpoint has reached the high-water mark.
func (s ReadModelLagSummary) CaughtUp() bool {
	return s.SequenceLag == 0
}

// SummarizeReadModelLag returns a clamped lag summary. If highWatermark is
// behind checkpoint, lag is reported as zero because the projection is not
// behind the observed source.
func SummarizeReadModelLag(
	projection string,
	checkpoint ReadModelCheckpoint,
	highWatermark StoredEvent,
	observedAt time.Time,
) ReadModelLagSummary {
	if projection == "" {
		projection = checkpoint.Projection
	}
	tenant := checkpoint.Tenant
	if tenant == "" {
		tenant = readModelTenantString(highWatermark.Event.Tenant)
	}

	var sequenceLag uint64
	if highWatermark.Sequence > checkpoint.Sequence {
		sequenceLag = highWatermark.Sequence - checkpoint.Sequence
	}

	var eventTimeLag time.Duration
	if sequenceLag > 0 &&
		!checkpoint.EventOccurredAt.IsZero() &&
		!highWatermark.Event.OccurredAt.IsZero() &&
		highWatermark.Event.OccurredAt.After(checkpoint.EventOccurredAt) {
		eventTimeLag = highWatermark.Event.OccurredAt.Sub(checkpoint.EventOccurredAt)
	}

	return ReadModelLagSummary{
		Projection:            projection,
		Tenant:                tenant,
		ObservedAt:            observedAt,
		CheckpointSequence:    checkpoint.Sequence,
		HighWatermarkSequence: highWatermark.Sequence,
		SequenceLag:           sequenceLag,
		CheckpointEventAt:     checkpoint.EventOccurredAt,
		HighWatermarkEventAt:  highWatermark.Event.OccurredAt,
		EventTimeLag:          eventTimeLag,
	}
}

func readModelProjectionSources(p ReadModelProjection) []ReadModelSource {
	if len(p.Sources) == 0 {
		return []ReadModelSource{{}}
	}
	return p.Sources
}

func cloneReadModelSources(sources []ReadModelSource) []ReadModelSource {
	if sources == nil {
		return nil
	}
	out := make([]ReadModelSource, len(sources))
	for i, source := range sources {
		out[i] = cloneReadModelSource(source)
	}
	return out
}

func cloneReadModelSource(source ReadModelSource) ReadModelSource {
	source.Names = append([]string(nil), source.Names...)
	source.Tenant = cloneReadModelTenant(source.Tenant)
	return source
}

func cloneReadModelTenant(tenant *Tenant) *Tenant {
	if tenant == nil {
		return nil
	}
	out := *tenant
	return &out
}

func readModelTenantString(tenant *Tenant) string {
	if tenant == nil {
		return ""
	}
	return strconv.FormatInt(int64(tenant.OrgID), 10)
}
