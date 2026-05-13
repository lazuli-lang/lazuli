package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestNewVersionIDIsDeterministicAndUTC(t *testing.T) {
	t.Parallel()

	brt := time.FixedZone("BRT", -3*60*60)
	now := time.Date(2026, 5, 12, 9, 30, 0, 123456789, brt)

	got := storage.NewVersionID(now, 42)
	want := storage.VersionID("20260512T123000.123456789Z-000000000000002a")
	if got != want {
		t.Fatalf("NewVersionID() = %q, want %q", got, want)
	}
	if got.String() != string(want) {
		t.Fatalf("VersionID.String() = %q, want %q", got.String(), want)
	}
}

func TestCurrentObjectVersionRespectsLatestTombstone(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	versions := []storage.ObjectVersion{
		version("avatars/alice.png", "v1", now.Add(-3*time.Hour), false),
		version("avatars/alice.png", "v3", now.Add(-1*time.Hour), true),
		version("avatars/alice.png", "v2", now.Add(-2*time.Hour), false),
	}

	latest, ok := storage.LatestObjectVersion(versions)
	if !ok {
		t.Fatal("LatestObjectVersion() returned ok=false")
	}
	if latest.VersionID != "v3" || !latest.IsTombstone() {
		t.Fatalf("LatestObjectVersion() = %#v, want tombstone v3", latest)
	}

	if current, ok := storage.CurrentObjectVersion(versions); ok {
		t.Fatalf("CurrentObjectVersion() = %#v, want no current version", current)
	}

	live, ok := storage.LatestLiveObjectVersion(versions)
	if !ok {
		t.Fatal("LatestLiveObjectVersion() returned ok=false")
	}
	if live.VersionID != "v2" {
		t.Fatalf("LatestLiveObjectVersion() = %q, want v2", live.VersionID)
	}
}

func TestBuildVersionListingPlanIsDeterministic(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	versions := []storage.ObjectVersion{
		version("reports/monthly.csv", "v1", now.Add(-4*time.Hour), false),
		version("avatars/alice.png", "v1", now.Add(-3*time.Hour), false),
		version("reports/monthly.csv", "v2", now.Add(-1*time.Hour), false),
		version("avatars/alice.png", "v2", now.Add(-2*time.Hour), true),
	}

	current, err := storage.BuildVersionListingPlan(versions, storage.VersionListingOptions{})
	if err != nil {
		t.Fatalf("BuildVersionListingPlan() current error = %v", err)
	}
	if len(current.Entries) != 1 {
		t.Fatalf("current entries len = %d, want 1", len(current.Entries))
	}
	if got := current.Entries[0]; got.Key != "reports/monthly.csv" || got.VersionID != "v2" || !got.Current {
		t.Fatalf("current entry = %#v, want reports/monthly.csv v2 current", got)
	}

	all, err := storage.BuildVersionListingPlan(versions, storage.VersionListingOptions{
		IncludeNoncurrent: true,
		IncludeTombstones: true,
	})
	if err != nil {
		t.Fatalf("BuildVersionListingPlan() all error = %v", err)
	}

	want := []struct {
		key       storage.Key
		versionID storage.VersionID
		latest    bool
		current   bool
		tombstone bool
	}{
		{"avatars/alice.png", "v2", true, false, true},
		{"avatars/alice.png", "v1", false, false, false},
		{"reports/monthly.csv", "v2", true, true, false},
		{"reports/monthly.csv", "v1", false, false, false},
	}
	if len(all.Entries) != len(want) {
		t.Fatalf("all entries len = %d, want %d", len(all.Entries), len(want))
	}
	for i, wantEntry := range want {
		got := all.Entries[i]
		if got.Key != wantEntry.key ||
			got.VersionID != wantEntry.versionID ||
			got.IsLatest != wantEntry.latest ||
			got.Current != wantEntry.current ||
			got.Tombstone != wantEntry.tombstone {
			t.Fatalf("entry %d = %#v, want %#v", i, got, wantEntry)
		}
	}
}

func TestBuildVersionListingPlanFiltersPrefixAndLimit(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	versions := []storage.ObjectVersion{
		version("reports/2024.csv", "v1", now.Add(-3*time.Hour), false),
		version("avatars/alice.png", "v1", now.Add(-2*time.Hour), false),
		version("reports/2025.csv", "v1", now.Add(-1*time.Hour), false),
	}

	plan, err := storage.BuildVersionListingPlan(versions, storage.VersionListingOptions{
		Prefix: "reports/",
		Limit:  1,
	})
	if err != nil {
		t.Fatalf("BuildVersionListingPlan() error = %v", err)
	}
	if len(plan.Entries) != 1 {
		t.Fatalf("entries len = %d, want 1", len(plan.Entries))
	}
	if got := plan.Entries[0].Key; got != "reports/2024.csv" {
		t.Fatalf("first reports key = %q, want reports/2024.csv", got)
	}
}

func TestBuildVersionRetentionPlanAppliesLifecycleDecisions(t *testing.T) {
	t.Parallel()

	const day = 24 * time.Hour
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	versions := []storage.ObjectVersion{
		version("avatars/alice.png", "v1", now.Add(-45*day), false),
		version("avatars/alice.png", "v3", now.Add(-1*day), false),
		version("avatars/alice.png", "v2", now.Add(-10*day), true),
		version("avatars/bob.png", "v1", now.Add(-20*day), true),
	}
	policy := storage.VersionRetentionPolicy{
		RetainNoncurrentFor: 30 * day,
		RetainTombstonesFor: 7 * day,
	}

	plan, err := storage.BuildVersionRetentionPlan(policy, versions, now)
	if err != nil {
		t.Fatalf("BuildVersionRetentionPlan() error = %v", err)
	}
	if !plan.DryRun {
		t.Fatal("plan DryRun = false, want true")
	}
	if !plan.GeneratedAt.Equal(now) {
		t.Fatalf("GeneratedAt = %s, want %s", plan.GeneratedAt, now)
	}

	want := []struct {
		key        storage.Key
		versionID  storage.VersionID
		transition storage.LifecycleTransition
		latest     bool
		reason     string
	}{
		{"avatars/alice.png", "v3", storage.LifecycleRetain, true, "within latest retention"},
		{"avatars/alice.png", "v2", storage.LifecycleDelete, false, "tombstone retention elapsed"},
		{"avatars/alice.png", "v1", storage.LifecycleDelete, false, "noncurrent retention elapsed"},
		{"avatars/bob.png", "v1", storage.LifecycleRetain, true, "within latest retention"},
	}
	if len(plan.Entries) != len(want) {
		t.Fatalf("entries len = %d, want %d", len(plan.Entries), len(want))
	}
	for i, wantEntry := range want {
		got := plan.Entries[i]
		if got.Key != wantEntry.key ||
			got.VersionID != wantEntry.versionID ||
			got.Transition != wantEntry.transition ||
			got.IsLatest != wantEntry.latest ||
			got.Reason != wantEntry.reason {
			t.Fatalf("entry %d = %#v, want %#v", i, got, wantEntry)
		}
	}
	if wantEligible := now.Add(-3 * day); !plan.Entries[1].EligibleAt.Equal(wantEligible) {
		t.Fatalf("tombstone EligibleAt = %s, want %s", plan.Entries[1].EligibleAt, wantEligible)
	}
}

func TestVersioningValidationRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	if err := storage.ValidateVersionRetentionPolicy(storage.VersionRetentionPolicy{RetainLatest: -1}); !errors.Is(err, storage.ErrVersioningPolicyInvalid) {
		t.Fatalf("ValidateVersionRetentionPolicy() error = %v, want ErrVersioningPolicyInvalid", err)
	}

	_, err := storage.BuildVersionListingPlan([]storage.ObjectVersion{
		{
			Key:        "avatars/alice.png",
			VersionID:  "v1",
			Visibility: storage.FileVisibility(99),
		},
	}, storage.VersionListingOptions{})
	if !errors.Is(err, storage.ErrVersioningObjectInvalid) {
		t.Fatalf("BuildVersionListingPlan() unknown visibility error = %v, want ErrVersioningObjectInvalid", err)
	}

	_, err = storage.BuildVersionListingPlan([]storage.ObjectVersion{
		{
			Key:        "avatars/alice.png",
			VersionID:  "v1",
			Visibility: storage.VisibilityPrivate,
		},
	}, storage.VersionListingOptions{Limit: -1})
	if !errors.Is(err, storage.ErrVersioningPolicyInvalid) {
		t.Fatalf("BuildVersionListingPlan() negative limit error = %v, want ErrVersioningPolicyInvalid", err)
	}
}

func version(key storage.Key, id storage.VersionID, createdAt time.Time, tombstone bool) storage.ObjectVersion {
	return storage.ObjectVersion{
		Key:         key,
		VersionID:   id,
		CreatedAt:   createdAt,
		Visibility:  storage.VisibilityPrivate,
		ContentType: "text/plain",
		Size:        12,
		Tombstone:   tombstone,
	}
}
