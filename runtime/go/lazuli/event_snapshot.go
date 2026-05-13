package lazuli

import (
	"errors"
	"fmt"
	"math"
	"sort"
	"strings"
	"time"
)

var (
	// ErrEventSnapshotAggregateRequired is returned when snapshot metadata does
	// not identify an aggregate type and id.
	ErrEventSnapshotAggregateRequired = errors.New("lazuli: event snapshot aggregate is required")

	// ErrEventSnapshotVersionWindowInvalid is returned when snapshot metadata
	// has an incoherent aggregate version window.
	ErrEventSnapshotVersionWindowInvalid = errors.New("lazuli: event snapshot version window is invalid")

	// ErrEventSnapshotRetentionInvalid is returned when a retention policy uses
	// negative limits.
	ErrEventSnapshotRetentionInvalid = errors.New("lazuli: event snapshot retention is invalid")

	// ErrEventSnapshotAggregateMismatch is returned when a single-aggregate
	// compaction plan receives snapshots from more than one aggregate.
	ErrEventSnapshotAggregateMismatch = errors.New("lazuli: event snapshot aggregate mismatch")
)

// EventAggregateRef identifies one event-sourced aggregate stream.
type EventAggregateRef struct {
	Type string
	ID   string
}

// NewEventAggregateRef returns a normalized aggregate reference.
func NewEventAggregateRef(aggregateType, aggregateID string) EventAggregateRef {
	return EventAggregateRef{Type: aggregateType, ID: aggregateID}.Normalize()
}

// Normalize trims aggregate type and id labels.
func (r EventAggregateRef) Normalize() EventAggregateRef {
	r.Type = strings.TrimSpace(r.Type)
	r.ID = strings.TrimSpace(r.ID)
	return r
}

// Validate reports whether ref identifies an aggregate stream.
func (r EventAggregateRef) Validate() error {
	r = r.Normalize()
	if r.Type == "" || r.ID == "" {
		return ErrEventSnapshotAggregateRequired
	}
	return nil
}

// Equal reports whether two aggregate refs identify the same stream.
func (r EventAggregateRef) Equal(other EventAggregateRef) bool {
	r = r.Normalize()
	other = other.Normalize()
	return r.Type == other.Type && r.ID == other.ID
}

// String returns a log-friendly aggregate label.
func (r EventAggregateRef) String() string {
	r = r.Normalize()
	if r.Type == "" {
		return r.ID
	}
	if r.ID == "" {
		return r.Type
	}
	return r.Type + "/" + r.ID
}

// EventSnapshotVersionWindow is the inclusive aggregate-version range covered
// by a snapshot.
type EventSnapshotVersionWindow struct {
	FromVersion uint64
	ToVersion   uint64
}

// NewEventSnapshotVersionWindow returns a validated version window.
func NewEventSnapshotVersionWindow(fromVersion, toVersion uint64) (EventSnapshotVersionWindow, error) {
	window := EventSnapshotVersionWindow{FromVersion: fromVersion, ToVersion: toVersion}
	if err := window.Validate(); err != nil {
		return EventSnapshotVersionWindow{}, err
	}
	return window, nil
}

// Validate reports whether the window bounds are coherent.
func (w EventSnapshotVersionWindow) Validate() error {
	if w.FromVersion > w.ToVersion {
		return fmt.Errorf("%w: from version %d is after to version %d", ErrEventSnapshotVersionWindowInvalid, w.FromVersion, w.ToVersion)
	}
	return nil
}

// Contains reports whether version is covered by the inclusive window.
func (w EventSnapshotVersionWindow) Contains(version uint64) bool {
	return w.FromVersion <= version && version <= w.ToVersion
}

// Covers reports whether w fully covers other.
func (w EventSnapshotVersionWindow) Covers(other EventSnapshotVersionWindow) bool {
	return w.FromVersion <= other.FromVersion && other.ToVersion <= w.ToVersion
}

// Stale reports whether currentVersion is newer than the snapshot window.
func (w EventSnapshotVersionWindow) Stale(currentVersion uint64) bool {
	return currentVersion > w.ToVersion
}

// IsStale is an alias for Stale.
func (w EventSnapshotVersionWindow) IsStale(currentVersion uint64) bool {
	return w.Stale(currentVersion)
}

// NextVersion returns the first aggregate version not covered by the window.
func (w EventSnapshotVersionWindow) NextVersion() uint64 {
	if w.ToVersion == math.MaxUint64 {
		return math.MaxUint64
	}
	return w.ToVersion + 1
}

// EventSnapshotMetadata records adapter-neutral metadata for an event-sourced
// aggregate snapshot. It intentionally carries no storage location or payload.
type EventSnapshotMetadata struct {
	ID            string
	AggregateType string
	AggregateID   string
	FromVersion   uint64
	ToVersion     uint64
	CreatedAt     time.Time
}

// NewEventSnapshotMetadata returns normalized, validated snapshot metadata.
func NewEventSnapshotMetadata(
	id string,
	aggregateType string,
	aggregateID string,
	fromVersion uint64,
	toVersion uint64,
	createdAt time.Time,
) (EventSnapshotMetadata, error) {
	metadata := EventSnapshotMetadata{
		ID:            id,
		AggregateType: aggregateType,
		AggregateID:   aggregateID,
		FromVersion:   fromVersion,
		ToVersion:     toVersion,
		CreatedAt:     createdAt,
	}.Normalize()
	if err := metadata.Validate(); err != nil {
		return EventSnapshotMetadata{}, err
	}
	return metadata, nil
}

// Normalize trims snapshot and aggregate labels.
func (m EventSnapshotMetadata) Normalize() EventSnapshotMetadata {
	m.ID = strings.TrimSpace(m.ID)
	m.AggregateType = strings.TrimSpace(m.AggregateType)
	m.AggregateID = strings.TrimSpace(m.AggregateID)
	return m
}

// Validate reports whether metadata is coherent.
func (m EventSnapshotMetadata) Validate() error {
	return ValidateEventSnapshotMetadata(m)
}

// ValidateEventSnapshotMetadata reports whether metadata is coherent.
func ValidateEventSnapshotMetadata(metadata EventSnapshotMetadata) error {
	metadata = metadata.Normalize()
	return errors.Join(
		metadata.Aggregate().Validate(),
		metadata.Window().Validate(),
	)
}

// Aggregate returns the snapshot aggregate reference.
func (m EventSnapshotMetadata) Aggregate() EventAggregateRef {
	return EventAggregateRef{Type: m.AggregateType, ID: m.AggregateID}.Normalize()
}

// Window returns the aggregate-version range covered by the snapshot.
func (m EventSnapshotMetadata) Window() EventSnapshotVersionWindow {
	return EventSnapshotVersionWindow{FromVersion: m.FromVersion, ToVersion: m.ToVersion}
}

// Stale reports whether currentVersion is newer than the snapshot.
func (m EventSnapshotMetadata) Stale(currentVersion uint64) bool {
	return m.Window().Stale(currentVersion)
}

// IsStale is an alias for Stale.
func (m EventSnapshotMetadata) IsStale(currentVersion uint64) bool {
	return m.Stale(currentVersion)
}

// ReplayFromVersion returns the first aggregate version callers must replay
// after loading this snapshot.
func (m EventSnapshotMetadata) ReplayFromVersion() uint64 {
	return m.Window().NextVersion()
}

// LatestEventSnapshot returns the newest valid snapshot by aggregate version,
// creation time, then id. The returned bool is false when snapshots is empty.
func LatestEventSnapshot(snapshots []EventSnapshotMetadata) (EventSnapshotMetadata, bool, error) {
	normalized, err := normalizeEventSnapshotMetadataList(snapshots)
	if err != nil {
		return EventSnapshotMetadata{}, false, err
	}
	if len(normalized) == 0 {
		return EventSnapshotMetadata{}, false, nil
	}
	sortEventSnapshotsNewestFirst(normalized)
	return normalized[0], true, nil
}

// EventSnapshotRetentionPolicy configures snapshot retention planning.
//
// KeepLatest keeps at least this many newest snapshots per aggregate. MaxAge
// keeps snapshots created at or after now-MaxAge. When both fields are zero, no
// snapshots are selected for deletion.
type EventSnapshotRetentionPolicy struct {
	KeepLatest int
	MaxAge     time.Duration
}

// Validate reports whether policy can be used for retention planning.
func (p EventSnapshotRetentionPolicy) Validate() error {
	if p.KeepLatest < 0 || p.MaxAge < 0 {
		return ErrEventSnapshotRetentionInvalid
	}
	return nil
}

// EventSnapshotRetentionPlan is the deterministic output of snapshot retention
// planning. Delete contains only snapshot metadata; callers own actual storage
// deletion.
type EventSnapshotRetentionPlan struct {
	Keep   []EventSnapshotMetadata
	Delete []EventSnapshotMetadata
}

// PlanEventSnapshotRetention groups snapshots by aggregate and selects old
// snapshots for deletion according to policy.
func PlanEventSnapshotRetention(
	snapshots []EventSnapshotMetadata,
	policy EventSnapshotRetentionPolicy,
	now time.Time,
) (EventSnapshotRetentionPlan, error) {
	if err := policy.Validate(); err != nil {
		return EventSnapshotRetentionPlan{}, err
	}

	normalized, err := normalizeEventSnapshotMetadataList(snapshots)
	if err != nil {
		return EventSnapshotRetentionPlan{}, err
	}
	if len(normalized) == 0 {
		return EventSnapshotRetentionPlan{}, nil
	}
	if policy.KeepLatest == 0 && policy.MaxAge == 0 {
		sortEventSnapshotsForPlan(normalized)
		return EventSnapshotRetentionPlan{Keep: normalized}, nil
	}

	groups := groupEventSnapshotsByAggregate(normalized)
	keepKeys := make(map[eventSnapshotKey]struct{}, len(normalized))
	cutoff := time.Time{}
	if policy.MaxAge > 0 {
		cutoff = now.Add(-policy.MaxAge)
	}

	for _, group := range groups {
		sortEventSnapshotsNewestFirst(group)
		for i, snapshot := range group {
			if policy.KeepLatest > 0 && i < policy.KeepLatest {
				keepKeys[eventSnapshotMetadataKey(snapshot)] = struct{}{}
				continue
			}
			if policy.MaxAge > 0 && !snapshot.CreatedAt.IsZero() && !snapshot.CreatedAt.Before(cutoff) {
				keepKeys[eventSnapshotMetadataKey(snapshot)] = struct{}{}
			}
		}
	}

	plan := EventSnapshotRetentionPlan{
		Keep:   make([]EventSnapshotMetadata, 0, len(keepKeys)),
		Delete: make([]EventSnapshotMetadata, 0, len(normalized)-len(keepKeys)),
	}
	for _, snapshot := range normalized {
		if _, ok := keepKeys[eventSnapshotMetadataKey(snapshot)]; ok {
			plan.Keep = append(plan.Keep, snapshot)
			continue
		}
		plan.Delete = append(plan.Delete, snapshot)
	}
	sortEventSnapshotsForPlan(plan.Keep)
	sortEventSnapshotsForPlan(plan.Delete)
	return plan, nil
}

// EventSnapshotCompactionPlan describes how far a stream can be compacted from
// its newest snapshot and which snapshot metadata retention would remove.
type EventSnapshotCompactionPlan struct {
	Aggregate             EventAggregateRef
	HasSnapshot           bool
	Snapshot              EventSnapshotMetadata
	CompactThroughVersion uint64
	ReplayFromVersion     uint64
	Stale                 bool
	Retention             EventSnapshotRetentionPlan
}

// PlanEventSnapshotCompaction builds a single-aggregate compaction plan from
// snapshot metadata. It does not delete snapshots or events.
func PlanEventSnapshotCompaction(
	snapshots []EventSnapshotMetadata,
	currentVersion uint64,
	retention EventSnapshotRetentionPolicy,
	now time.Time,
) (EventSnapshotCompactionPlan, error) {
	normalized, err := normalizeEventSnapshotMetadataList(snapshots)
	if err != nil {
		return EventSnapshotCompactionPlan{}, err
	}
	if err := validateSingleEventSnapshotAggregate(normalized); err != nil {
		return EventSnapshotCompactionPlan{}, err
	}

	retentionPlan, err := PlanEventSnapshotRetention(normalized, retention, now)
	if err != nil {
		return EventSnapshotCompactionPlan{}, err
	}

	latest, ok, err := LatestEventSnapshot(normalized)
	if err != nil {
		return EventSnapshotCompactionPlan{}, err
	}
	plan := EventSnapshotCompactionPlan{Retention: retentionPlan}
	if !ok {
		return plan, nil
	}

	plan.Aggregate = latest.Aggregate()
	plan.HasSnapshot = true
	plan.Snapshot = latest
	plan.CompactThroughVersion = latest.ToVersion
	plan.ReplayFromVersion = latest.ReplayFromVersion()
	plan.Stale = latest.Stale(currentVersion)
	return plan, nil
}

type eventSnapshotKey struct {
	id            string
	aggregateType string
	aggregateID   string
	fromVersion   uint64
	toVersion     uint64
	createdAt     time.Time
}

func normalizeEventSnapshotMetadataList(snapshots []EventSnapshotMetadata) ([]EventSnapshotMetadata, error) {
	normalized := make([]EventSnapshotMetadata, 0, len(snapshots))
	for i, snapshot := range snapshots {
		snapshot = snapshot.Normalize()
		if err := snapshot.Validate(); err != nil {
			return nil, fmt.Errorf("lazuli: event snapshot %d: %w", i, err)
		}
		normalized = append(normalized, snapshot)
	}
	return normalized, nil
}

func validateSingleEventSnapshotAggregate(snapshots []EventSnapshotMetadata) error {
	if len(snapshots) == 0 {
		return nil
	}

	aggregate := snapshots[0].Aggregate()
	for _, snapshot := range snapshots[1:] {
		if !snapshot.Aggregate().Equal(aggregate) {
			return fmt.Errorf("%w: got %s and %s", ErrEventSnapshotAggregateMismatch, aggregate, snapshot.Aggregate())
		}
	}
	return nil
}

func groupEventSnapshotsByAggregate(snapshots []EventSnapshotMetadata) [][]EventSnapshotMetadata {
	groupsByKey := make(map[EventAggregateRef][]EventSnapshotMetadata)
	for _, snapshot := range snapshots {
		aggregate := snapshot.Aggregate()
		groupsByKey[aggregate] = append(groupsByKey[aggregate], snapshot)
	}

	aggregates := make([]EventAggregateRef, 0, len(groupsByKey))
	for aggregate := range groupsByKey {
		aggregates = append(aggregates, aggregate)
	}
	sort.Slice(aggregates, func(i, j int) bool {
		if aggregates[i].Type != aggregates[j].Type {
			return aggregates[i].Type < aggregates[j].Type
		}
		return aggregates[i].ID < aggregates[j].ID
	})

	groups := make([][]EventSnapshotMetadata, 0, len(aggregates))
	for _, aggregate := range aggregates {
		group := append([]EventSnapshotMetadata(nil), groupsByKey[aggregate]...)
		groups = append(groups, group)
	}
	return groups
}

func eventSnapshotMetadataKey(snapshot EventSnapshotMetadata) eventSnapshotKey {
	return eventSnapshotKey{
		id:            snapshot.ID,
		aggregateType: snapshot.AggregateType,
		aggregateID:   snapshot.AggregateID,
		fromVersion:   snapshot.FromVersion,
		toVersion:     snapshot.ToVersion,
		createdAt:     snapshot.CreatedAt,
	}
}

func sortEventSnapshotsNewestFirst(snapshots []EventSnapshotMetadata) {
	sort.Slice(snapshots, func(i, j int) bool {
		if snapshots[i].ToVersion != snapshots[j].ToVersion {
			return snapshots[i].ToVersion > snapshots[j].ToVersion
		}
		if !snapshots[i].CreatedAt.Equal(snapshots[j].CreatedAt) {
			return snapshots[i].CreatedAt.After(snapshots[j].CreatedAt)
		}
		return eventSnapshotMetadataLess(snapshots[i], snapshots[j])
	})
}

func sortEventSnapshotsForPlan(snapshots []EventSnapshotMetadata) {
	sort.Slice(snapshots, func(i, j int) bool {
		return eventSnapshotMetadataLess(snapshots[i], snapshots[j])
	})
}

func eventSnapshotMetadataLess(left, right EventSnapshotMetadata) bool {
	switch {
	case left.AggregateType != right.AggregateType:
		return left.AggregateType < right.AggregateType
	case left.AggregateID != right.AggregateID:
		return left.AggregateID < right.AggregateID
	case left.ToVersion != right.ToVersion:
		return left.ToVersion > right.ToVersion
	case left.FromVersion != right.FromVersion:
		return left.FromVersion > right.FromVersion
	case !left.CreatedAt.Equal(right.CreatedAt):
		return left.CreatedAt.After(right.CreatedAt)
	default:
		return left.ID < right.ID
	}
}
