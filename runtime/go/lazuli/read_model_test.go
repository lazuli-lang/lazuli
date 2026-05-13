package lazuli

import (
	"reflect"
	"testing"
	"time"
)

func TestReadModelProjectionFiltersCloneSourceMetadata(t *testing.T) {
	tenant := &Tenant{OrgID: 7}
	since := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	until := since.Add(time.Hour)
	source := ReadModelSourceForEvents("customer.created", "customer.updated")
	source.Tenant = tenant
	source.Since = since
	source.Until = until

	projection := NewReadModelProjection("customer.search", source)
	source.Names[0] = "mutated"
	tenant.OrgID = 99

	replayFilters := projection.ReplayFilters()
	if len(replayFilters) != 1 {
		t.Fatalf("ReplayFilters() len = %d, want 1", len(replayFilters))
	}
	wantNames := []string{"customer.created", "customer.updated"}
	if !reflect.DeepEqual(replayFilters[0].Names, wantNames) {
		t.Fatalf("ReplayFilters()[0].Names = %#v, want %#v", replayFilters[0].Names, wantNames)
	}
	if replayFilters[0].Tenant == nil || replayFilters[0].Tenant.OrgID != 7 {
		t.Fatalf("ReplayFilters()[0].Tenant = %+v, want org 7", replayFilters[0].Tenant)
	}
	if !replayFilters[0].Since.Equal(since) || !replayFilters[0].Until.Equal(until) {
		t.Fatalf("ReplayFilters()[0] window = (%v, %v), want (%v, %v)",
			replayFilters[0].Since, replayFilters[0].Until, since, until)
	}

	replayFilters[0].Names[0] = "returned mutation"
	replayFilters[0].Tenant.OrgID = 88
	replayFilters = projection.ReplayFilters()
	if !reflect.DeepEqual(replayFilters[0].Names, wantNames) {
		t.Fatalf("ReplayFilters() after mutation names = %#v, want %#v", replayFilters[0].Names, wantNames)
	}
	if replayFilters[0].Tenant.OrgID != 7 {
		t.Fatalf("ReplayFilters() after mutation tenant = %d, want 7", replayFilters[0].Tenant.OrgID)
	}

	listFilters := projection.ListFilters(42)
	if len(listFilters) != 2 {
		t.Fatalf("ListFilters() len = %d, want 2", len(listFilters))
	}
	for i, filter := range listFilters {
		if filter.Name != wantNames[i] {
			t.Fatalf("ListFilters()[%d].Name = %q, want %q", i, filter.Name, wantNames[i])
		}
		if filter.Tenant == nil || filter.Tenant.OrgID != 7 {
			t.Fatalf("ListFilters()[%d].Tenant = %+v, want org 7", i, filter.Tenant)
		}
		if filter.SinceSequence != 42 {
			t.Fatalf("ListFilters()[%d].SinceSequence = %d, want 42", i, filter.SinceSequence)
		}
	}
}

func TestReadModelProjectionPlanRebuildIncludesCheckpointAndFilters(t *testing.T) {
	plannedAt := time.Date(2026, 5, 12, 13, 0, 0, 0, time.UTC)
	projection := NewReadModelProjection(
		"customer.search",
		ReadModelSourceForEvents("customer.created"),
		ReadModelSourceForEvents("customer.deleted"),
	)

	plan := projection.PlanRebuild(ReadModelCheckpoint{Sequence: 12}, plannedAt)
	if plan.Projection != "customer.search" {
		t.Fatalf("Projection = %q, want customer.search", plan.Projection)
	}
	if !plan.PlannedAt.Equal(plannedAt) {
		t.Fatalf("PlannedAt = %v, want %v", plan.PlannedAt, plannedAt)
	}
	if plan.Checkpoint.Projection != "customer.search" || plan.Checkpoint.Sequence != 12 {
		t.Fatalf("Checkpoint = %+v, want projection customer.search at sequence 12", plan.Checkpoint)
	}
	if len(plan.Sources) != 2 || len(plan.ReplayFilters) != 2 || len(plan.ListFilters) != 2 {
		t.Fatalf("plan sizes = sources %d replay %d list %d, want 2 each",
			len(plan.Sources), len(plan.ReplayFilters), len(plan.ListFilters))
	}
	if plan.ListFilters[0].Name != "customer.created" || plan.ListFilters[0].SinceSequence != 12 {
		t.Fatalf("first list filter = %+v, want customer.created since 12", plan.ListFilters[0])
	}
	if plan.ListFilters[1].Name != "customer.deleted" || plan.ListFilters[1].SinceSequence != 12 {
		t.Fatalf("second list filter = %+v, want customer.deleted since 12", plan.ListFilters[1])
	}

	plan.Sources[0].Names[0] = "returned mutation"
	nextPlan := projection.PlanRebuild(ReadModelCheckpoint{Sequence: 12}, plannedAt)
	if nextPlan.Sources[0].Names[0] != "customer.created" {
		t.Fatalf("projection source mutated through plan: got %q", nextPlan.Sources[0].Names[0])
	}
}

func TestReadModelCheckpointAndIdempotencyKeyForEvent(t *testing.T) {
	occurredAt := time.Date(2026, 5, 12, 12, 30, 0, 0, time.UTC)
	updatedAt := occurredAt.Add(time.Second)
	stored := StoredEvent{
		Sequence: 15,
		Event: Event{
			Name:       "customer.created",
			Tenant:     &Tenant{OrgID: 7},
			OccurredAt: occurredAt,
		},
	}

	checkpoint := ReadModelCheckpointForEvent("customer.search", stored, updatedAt)
	if checkpoint.Projection != "customer.search" ||
		checkpoint.Tenant != "7" ||
		checkpoint.Sequence != 15 ||
		checkpoint.EventName != "customer.created" ||
		!checkpoint.EventOccurredAt.Equal(occurredAt) ||
		!checkpoint.UpdatedAt.Equal(updatedAt) {
		t.Fatalf("checkpoint = %+v, want stored event checkpoint", checkpoint)
	}

	key := ReadModelIdempotencyKeyForEvent("customer.search", stored)
	if key.Empty() {
		t.Fatal("idempotency key should not be empty")
	}
	if got, want := key.String(), "customer.search:7:customer.created:15"; got != want {
		t.Fatalf("String() = %q, want %q", got, want)
	}
	if !(ReadModelIdempotencyKey{Projection: "customer.search"}).Empty() {
		t.Fatal("key without sequence should be empty")
	}
	if got := (ReadModelIdempotencyKey{}).String(); got != "" {
		t.Fatalf("zero key String() = %q, want empty", got)
	}
}

func TestSummarizeReadModelLag(t *testing.T) {
	checkpointAt := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	highWatermarkAt := checkpointAt.Add(2 * time.Minute)
	observedAt := highWatermarkAt.Add(time.Minute)
	checkpoint := ReadModelCheckpoint{
		Projection:      "customer.search",
		Tenant:          "7",
		Sequence:        10,
		EventOccurredAt: checkpointAt,
	}
	highWatermark := StoredEvent{
		Sequence: 15,
		Event: Event{
			Name:       "customer.updated",
			Tenant:     &Tenant{OrgID: 7},
			OccurredAt: highWatermarkAt,
		},
	}

	summary := SummarizeReadModelLag("", checkpoint, highWatermark, observedAt)
	if summary.Projection != "customer.search" || summary.Tenant != "7" {
		t.Fatalf("summary identity = (%q, %q), want customer.search tenant 7", summary.Projection, summary.Tenant)
	}
	if summary.CheckpointSequence != 10 || summary.HighWatermarkSequence != 15 || summary.SequenceLag != 5 {
		t.Fatalf("summary sequences = %+v, want lag 5 from 10 to 15", summary)
	}
	if summary.EventTimeLag != 2*time.Minute {
		t.Fatalf("EventTimeLag = %v, want 2m", summary.EventTimeLag)
	}
	if summary.CaughtUp() {
		t.Fatal("summary should report projection behind")
	}

	caughtUp := SummarizeReadModelLag("customer.search", checkpoint, StoredEvent{
		Sequence: 9,
		Event: Event{
			Tenant:     &Tenant{OrgID: 7},
			OccurredAt: checkpointAt.Add(-time.Minute),
		},
	}, observedAt)
	if caughtUp.SequenceLag != 0 || caughtUp.EventTimeLag != 0 || !caughtUp.CaughtUp() {
		t.Fatalf("caught-up summary = %+v, want zero lag", caughtUp)
	}
}
