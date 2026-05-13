package lazuli

import (
	"errors"
	"reflect"
	"testing"
	"time"
)

func TestEventAggregateRefNormalizesValidatesAndCompares(t *testing.T) {
	ref := NewEventAggregateRef(" customer ", " 42 ")

	if ref.Type != "customer" || ref.ID != "42" {
		t.Fatalf("NewEventAggregateRef() = %#v, want trimmed fields", ref)
	}
	if err := ref.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}
	if !ref.Equal(EventAggregateRef{Type: "customer", ID: "42"}) {
		t.Fatalf("Equal() = false, want true")
	}
	if got := ref.String(); got != "customer/42" {
		t.Fatalf("String() = %q, want customer/42", got)
	}

	if err := (EventAggregateRef{Type: "customer"}).Validate(); !errors.Is(err, ErrEventSnapshotAggregateRequired) {
		t.Fatalf("Validate(missing id) error = %v, want ErrEventSnapshotAggregateRequired", err)
	}
	if err := (EventAggregateRef{ID: "42"}).Validate(); !errors.Is(err, ErrEventSnapshotAggregateRequired) {
		t.Fatalf("Validate(missing type) error = %v, want ErrEventSnapshotAggregateRequired", err)
	}
}

func TestEventSnapshotVersionWindowCoversVersionsAndDetectsStale(t *testing.T) {
	window, err := NewEventSnapshotVersionWindow(10, 20)
	if err != nil {
		t.Fatalf("NewEventSnapshotVersionWindow() error = %v", err)
	}

	if !window.Contains(10) || !window.Contains(20) || window.Contains(9) || window.Contains(21) {
		t.Fatalf("Contains() returned unexpected coverage for %#v", window)
	}
	if !window.Covers(EventSnapshotVersionWindow{FromVersion: 12, ToVersion: 18}) {
		t.Fatalf("Covers() = false, want true")
	}
	if window.Stale(20) {
		t.Fatalf("Stale(20) = true, want false")
	}
	if !window.IsStale(21) {
		t.Fatalf("IsStale(21) = false, want true")
	}
	if got := window.NextVersion(); got != 21 {
		t.Fatalf("NextVersion() = %d, want 21", got)
	}

	if _, err := NewEventSnapshotVersionWindow(20, 10); !errors.Is(err, ErrEventSnapshotVersionWindowInvalid) {
		t.Fatalf("NewEventSnapshotVersionWindow(invalid) error = %v, want ErrEventSnapshotVersionWindowInvalid", err)
	}
}

func TestEventSnapshotMetadataNormalizesValidatesAndDetectsStale(t *testing.T) {
	createdAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	metadata, err := NewEventSnapshotMetadata(" snap-20 ", " customer ", " 42 ", 1, 20, createdAt)
	if err != nil {
		t.Fatalf("NewEventSnapshotMetadata() error = %v", err)
	}

	if metadata.ID != "snap-20" || metadata.AggregateType != "customer" || metadata.AggregateID != "42" {
		t.Fatalf("metadata was not normalized: %#v", metadata)
	}
	if got := metadata.Aggregate(); got != (EventAggregateRef{Type: "customer", ID: "42"}) {
		t.Fatalf("Aggregate() = %#v, want customer/42", got)
	}
	if got := metadata.Window(); got != (EventSnapshotVersionWindow{FromVersion: 1, ToVersion: 20}) {
		t.Fatalf("Window() = %#v, want 1..20", got)
	}
	if metadata.IsStale(20) {
		t.Fatalf("IsStale(20) = true, want false")
	}
	if !metadata.Stale(25) {
		t.Fatalf("Stale(25) = false, want true")
	}
	if got := metadata.ReplayFromVersion(); got != 21 {
		t.Fatalf("ReplayFromVersion() = %d, want 21", got)
	}

	err = ValidateEventSnapshotMetadata(EventSnapshotMetadata{AggregateType: "customer", AggregateID: "42", FromVersion: 5, ToVersion: 4})
	if !errors.Is(err, ErrEventSnapshotVersionWindowInvalid) {
		t.Fatalf("ValidateEventSnapshotMetadata(invalid window) error = %v, want ErrEventSnapshotVersionWindowInvalid", err)
	}
	err = ValidateEventSnapshotMetadata(EventSnapshotMetadata{FromVersion: 1, ToVersion: 1})
	if !errors.Is(err, ErrEventSnapshotAggregateRequired) {
		t.Fatalf("ValidateEventSnapshotMetadata(missing aggregate) error = %v, want ErrEventSnapshotAggregateRequired", err)
	}
}

func TestLatestEventSnapshotSelectsNewestSnapshot(t *testing.T) {
	base := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	snapshots := []EventSnapshotMetadata{
		testEventSnapshot("older", "customer", "42", 1, 20, base.Add(-time.Hour)),
		testEventSnapshot("latest-b", "customer", "42", 21, 30, base),
		testEventSnapshot("latest-a", "customer", "42", 21, 30, base),
		testEventSnapshot("newer-created", "customer", "42", 21, 30, base.Add(time.Hour)),
	}

	got, ok, err := LatestEventSnapshot(snapshots)
	if err != nil {
		t.Fatalf("LatestEventSnapshot() error = %v", err)
	}
	if !ok {
		t.Fatal("LatestEventSnapshot() ok = false, want true")
	}
	if got.ID != "newer-created" {
		t.Fatalf("LatestEventSnapshot().ID = %q, want newer-created", got.ID)
	}

	_, ok, err = LatestEventSnapshot(nil)
	if err != nil {
		t.Fatalf("LatestEventSnapshot(nil) error = %v", err)
	}
	if ok {
		t.Fatal("LatestEventSnapshot(nil) ok = true, want false")
	}
}

func TestPlanEventSnapshotRetentionKeepsLatestAndRecentPerAggregate(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	snapshots := []EventSnapshotMetadata{
		testEventSnapshot("cust-old", "customer", "cust-1", 1, 10, now.Add(-72*time.Hour)),
		testEventSnapshot("cust-middle", "customer", "cust-1", 11, 20, now.Add(-48*time.Hour)),
		testEventSnapshot("cust-new", "customer", "cust-1", 21, 30, now.Add(-2*time.Hour)),
		testEventSnapshot("acct-old", "customer", "acct-1", 1, 5, now.Add(-72*time.Hour)),
		testEventSnapshot("acct-new", "customer", "acct-1", 6, 10, now.Add(-48*time.Hour)),
	}

	plan, err := PlanEventSnapshotRetention(snapshots, EventSnapshotRetentionPolicy{
		KeepLatest: 1,
		MaxAge:     24 * time.Hour,
	}, now)
	if err != nil {
		t.Fatalf("PlanEventSnapshotRetention() error = %v", err)
	}

	if got, want := eventSnapshotIDs(plan.Keep), []string{"acct-new", "cust-new"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Keep IDs = %v, want %v", got, want)
	}
	if got, want := eventSnapshotIDs(plan.Delete), []string{"acct-old", "cust-middle", "cust-old"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Delete IDs = %v, want %v", got, want)
	}
}

func TestPlanEventSnapshotRetentionZeroPolicyKeepsAllAndRejectsInvalid(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	snapshots := []EventSnapshotMetadata{
		testEventSnapshot("new", "customer", "42", 11, 20, now),
		testEventSnapshot("old", "customer", "42", 1, 10, now.Add(-time.Hour)),
	}

	plan, err := PlanEventSnapshotRetention(snapshots, EventSnapshotRetentionPolicy{}, now)
	if err != nil {
		t.Fatalf("PlanEventSnapshotRetention(zero policy) error = %v", err)
	}
	if got, want := eventSnapshotIDs(plan.Keep), []string{"new", "old"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Keep IDs = %v, want %v", got, want)
	}
	if len(plan.Delete) != 0 {
		t.Fatalf("Delete len = %d, want 0", len(plan.Delete))
	}

	_, err = PlanEventSnapshotRetention(snapshots, EventSnapshotRetentionPolicy{KeepLatest: -1}, now)
	if !errors.Is(err, ErrEventSnapshotRetentionInvalid) {
		t.Fatalf("PlanEventSnapshotRetention(invalid) error = %v, want ErrEventSnapshotRetentionInvalid", err)
	}
}

func TestPlanEventSnapshotCompactionUsesLatestSnapshotAndRetention(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	snapshots := []EventSnapshotMetadata{
		testEventSnapshot("snap-10", "customer", "42", 1, 10, now.Add(-3*time.Hour)),
		testEventSnapshot("snap-20", "customer", "42", 11, 20, now.Add(-2*time.Hour)),
		testEventSnapshot("snap-30", "customer", "42", 21, 30, now.Add(-time.Hour)),
	}

	plan, err := PlanEventSnapshotCompaction(snapshots, 35, EventSnapshotRetentionPolicy{KeepLatest: 1}, now)
	if err != nil {
		t.Fatalf("PlanEventSnapshotCompaction() error = %v", err)
	}

	if plan.Aggregate != (EventAggregateRef{Type: "customer", ID: "42"}) {
		t.Fatalf("Aggregate = %#v, want customer/42", plan.Aggregate)
	}
	if !plan.HasSnapshot || plan.Snapshot.ID != "snap-30" {
		t.Fatalf("Snapshot = (%v, %q), want latest snap-30", plan.HasSnapshot, plan.Snapshot.ID)
	}
	if plan.CompactThroughVersion != 30 || plan.ReplayFromVersion != 31 {
		t.Fatalf("compaction versions = %d/%d, want 30/31", plan.CompactThroughVersion, plan.ReplayFromVersion)
	}
	if !plan.Stale {
		t.Fatal("Stale = false, want true")
	}
	if got, want := eventSnapshotIDs(plan.Retention.Keep), []string{"snap-30"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Retention.Keep IDs = %v, want %v", got, want)
	}
	if got, want := eventSnapshotIDs(plan.Retention.Delete), []string{"snap-20", "snap-10"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("Retention.Delete IDs = %v, want %v", got, want)
	}

	plan, err = PlanEventSnapshotCompaction(snapshots, 30, EventSnapshotRetentionPolicy{KeepLatest: 1}, now)
	if err != nil {
		t.Fatalf("PlanEventSnapshotCompaction(current) error = %v", err)
	}
	if plan.Stale {
		t.Fatal("Stale at current version = true, want false")
	}
}

func TestPlanEventSnapshotCompactionRejectsMixedAggregates(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	snapshots := []EventSnapshotMetadata{
		testEventSnapshot("one", "customer", "1", 1, 10, now),
		testEventSnapshot("two", "customer", "2", 1, 10, now),
	}

	_, err := PlanEventSnapshotCompaction(snapshots, 10, EventSnapshotRetentionPolicy{KeepLatest: 1}, now)
	if !errors.Is(err, ErrEventSnapshotAggregateMismatch) {
		t.Fatalf("PlanEventSnapshotCompaction(mixed aggregates) error = %v, want ErrEventSnapshotAggregateMismatch", err)
	}
}

func testEventSnapshot(id, aggregateType, aggregateID string, fromVersion, toVersion uint64, createdAt time.Time) EventSnapshotMetadata {
	return EventSnapshotMetadata{
		ID:            id,
		AggregateType: aggregateType,
		AggregateID:   aggregateID,
		FromVersion:   fromVersion,
		ToVersion:     toVersion,
		CreatedAt:     createdAt,
	}
}

func eventSnapshotIDs(snapshots []EventSnapshotMetadata) []string {
	ids := make([]string, 0, len(snapshots))
	for _, snapshot := range snapshots {
		ids = append(ids, snapshot.ID)
	}
	return ids
}
