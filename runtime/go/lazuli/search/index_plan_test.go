package search

import (
	"errors"
	"reflect"
	"testing"
)

func TestPlanIndexBuildsDeterministicShardPlans(t *testing.T) {
	opts := IndexPlanOptions{
		Name:         "customer.search",
		Mode:         " FULL ",
		ShardCount:   2,
		MaxBatchSize: 3,
		Sources: []IndexSourceResource{
			{Resource: "orders", Tenant: " 7 ", Count: 5},
			{Name: "customers", Count: 10},
		},
	}

	plan, err := PlanIndex(opts)
	if err != nil {
		t.Fatalf("PlanIndex() error = %v", err)
	}
	opts.Sources[1].Name = "mutated"

	if plan.Name != "customer.search" || plan.Mode != RebuildModeFull {
		t.Fatalf("plan metadata = %q/%q, want customer.search/full", plan.Name, plan.Mode)
	}
	if plan.SourceCount != 2 || plan.ShardCount != 2 || plan.ShardPlanCount != 4 || plan.BatchCount != 6 {
		t.Fatalf("plan counts = sources %d shards %d shard plans %d batches %d, want 2/2/4/6",
			plan.SourceCount,
			plan.ShardCount,
			plan.ShardPlanCount,
			plan.BatchCount,
		)
	}

	wantSources := []IndexSourceResource{
		{Name: "customers", Count: 10},
		{Name: "orders", Tenant: "7", Count: 5},
	}
	if !reflect.DeepEqual(plan.Sources, wantSources) {
		t.Fatalf("Sources = %#v, want %#v", plan.Sources, wantSources)
	}

	first := plan.Shards[0]
	if first.Index != "customer.search" ||
		first.Source != (IndexSourceResource{Name: "customers", Count: 10}) ||
		first.Shard != "shard-00-of-02" ||
		first.ShardIndex != 0 ||
		first.ShardCount != 2 ||
		first.CheckpointID != "search_index:customer.search:customers::full:shard-00-of-02" {
		t.Fatalf("first shard plan = %#v, want customer shard 0", first)
	}

	wantWindows := []IndexBatchWindow{
		{Index: 1, Count: 2, Start: 0, End: 3, Offset: 0, Limit: 3},
		{Index: 2, Count: 2, Start: 3, End: 5, Offset: 3, Limit: 2},
	}
	if !reflect.DeepEqual(first.Windows, wantWindows) {
		t.Fatalf("first shard windows = %#v, want %#v", first.Windows, wantWindows)
	}

	last := plan.Shards[3]
	if last.Source.Name != "orders" ||
		last.Shard != "shard-01-of-02" ||
		last.CheckpointID != "search_index:customer.search:orders:7:full:shard-01-of-02" {
		t.Fatalf("last shard plan = %#v, want orders shard 1 with tenant checkpoint", last)
	}
	if !reflect.DeepEqual(last.Windows, []IndexBatchWindow{{Index: 1, Count: 1, Start: 0, End: 2, Offset: 0, Limit: 2}}) {
		t.Fatalf("last shard windows = %#v, want one two-row window", last.Windows)
	}
}

func TestPlanIndexBatchWindowsSplitsRanges(t *testing.T) {
	got := PlanIndexBatchWindows(7, 3)
	want := []IndexBatchWindow{
		{Index: 1, Count: 3, Start: 0, End: 3, Offset: 0, Limit: 3},
		{Index: 2, Count: 3, Start: 3, End: 6, Offset: 3, Limit: 3},
		{Index: 3, Count: 3, Start: 6, End: 7, Offset: 6, Limit: 1},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("PlanIndexBatchWindows() = %#v, want %#v", got, want)
	}

	if got := PlanIndexBatchWindows(0, 3); got != nil {
		t.Fatalf("PlanIndexBatchWindows(0, 3) = %#v, want nil", got)
	}

	got = PlanIndexBatchWindows(7, 0)
	want = []IndexBatchWindow{{Index: 1, Count: 1, Start: 0, End: 7, Offset: 0, Limit: 7}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("PlanIndexBatchWindows(7, 0) = %#v, want %#v", got, want)
	}
}

func TestBuildIndexCheckpointIDDefaultsAndNormalizes(t *testing.T) {
	got, err := BuildIndexCheckpointID(IndexCheckpointKey{
		Index:    " customer.search ",
		Resource: " customer ",
		Tenant:   " 42 ",
	})
	if err != nil {
		t.Fatalf("BuildIndexCheckpointID() error = %v", err)
	}

	want := "search_index:customer.search:customer:42:incremental:shard-00-of-01"
	if got != want {
		t.Fatalf("BuildIndexCheckpointID() = %q, want %q", got, want)
	}

	key := IndexCheckpointKey{Index: "customer.search", Resource: "customer", Mode: RebuildModeBackfill, Shard: "shard-a"}
	if got := key.String(); got != "search_index:customer.search:customer::backfill:shard-a" {
		t.Fatalf("IndexCheckpointKey.String() = %q, want backfill checkpoint", got)
	}
}

func TestIndexShardNameIsDeterministic(t *testing.T) {
	got, err := IndexShardName(3, 12)
	if err != nil {
		t.Fatalf("IndexShardName() error = %v", err)
	}
	if got != "shard-03-of-12" {
		t.Fatalf("IndexShardName(3, 12) = %q, want shard-03-of-12", got)
	}

	if _, err := IndexShardName(12, 12); !errors.Is(err, errInvalidIndexShard) {
		t.Fatalf("IndexShardName(12, 12) error = %v, want %v", err, errInvalidIndexShard)
	}
}

func TestIndexPlanHelpersRejectInvalidInput(t *testing.T) {
	tests := []struct {
		name    string
		run     func() error
		wantErr error
	}{
		{
			name: "missing index name",
			run: func() error {
				_, err := PlanIndex(IndexPlanOptions{Sources: []IndexSourceResource{{Name: "customer"}}})
				return err
			},
			wantErr: errIndexPlanNameRequired,
		},
		{
			name: "missing sources",
			run: func() error {
				_, err := PlanIndex(IndexPlanOptions{Name: "customer.search"})
				return err
			},
			wantErr: errIndexPlanSourceRequired,
		},
		{
			name: "missing source name",
			run: func() error {
				return ValidateIndexSourceResources([]IndexSourceResource{{Tenant: "7"}})
			},
			wantErr: errIndexPlanSourceNameRequired,
		},
		{
			name: "invalid source name",
			run: func() error {
				return ValidateIndexSourceResources([]IndexSourceResource{{Name: "bad-name"}})
			},
			wantErr: errInvalidColumn,
		},
		{
			name: "duplicate source and tenant",
			run: func() error {
				return ValidateIndexSourceResources([]IndexSourceResource{
					{Name: "customer", Tenant: "7"},
					{Resource: "customer", Tenant: " 7 "},
				})
			},
			wantErr: errDuplicateIndexSource,
		},
		{
			name: "invalid mode",
			run: func() error {
				_, err := NormalizeRebuildMode("replace")
				return err
			},
			wantErr: errInvalidIndexRebuildMode,
		},
		{
			name: "invalid tenant metadata",
			run: func() error {
				_, err := PlanIndex(IndexPlanOptions{
					Name:    "customer.search",
					Sources: []IndexSourceResource{{Name: "customer", Tenant: "7\n8"}},
				})
				return err
			},
			wantErr: errInvalidIndexPlanMetadata,
		},
		{
			name: "invalid checkpoint resource",
			run: func() error {
				_, err := BuildIndexCheckpointID(IndexCheckpointKey{Index: "customer.search"})
				return err
			},
			wantErr: errIndexPlanSourceNameRequired,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.run(); !errors.Is(err, tt.wantErr) {
				t.Fatalf("%s error = %v, want %v", tt.name, err, tt.wantErr)
			}
		})
	}
}
