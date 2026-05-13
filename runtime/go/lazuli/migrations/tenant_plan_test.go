package migrations

import (
	"errors"
	"reflect"
	"testing"
)

func TestPlanTenantMigrationsBatchesTargetsAndMetadata(t *testing.T) {
	plan, err := PlanTenantMigrations(TenantMigrationPlanOptions{
		Migration: MigrationRecord{Feature: " customer ", Name: " backfill_score "},
		Targets: []TenantMigrationPlanTarget{
			{TenantID: " acme ", Schema: "tenant_acme"},
			{TenantID: "globex", Database: "tenant_globex"},
			{Schema: "tenant_shared", Database: "shared_app"},
		},
		MaxBatchSize:  2,
		DryRun:        true,
		FailurePolicy: TenantMigrationPlanFailureContinue,
	})
	if err != nil {
		t.Fatalf("PlanTenantMigrations returned %v", err)
	}

	if plan.Migration.ID != "customer.backfill_score" {
		t.Fatalf("plan migration ID = %q, want customer.backfill_score", plan.Migration.ID)
	}
	if !plan.DryRun {
		t.Fatal("plan DryRun = false, want true")
	}
	if plan.FailurePolicy != TenantMigrationPlanFailureContinue {
		t.Fatalf("plan failure policy = %q, want continue", plan.FailurePolicy)
	}
	if plan.TargetCount != 3 || plan.BatchCount != 2 {
		t.Fatalf("plan counts = %d targets, %d batches; want 3 targets, 2 batches", plan.TargetCount, plan.BatchCount)
	}

	wantFirst := []TenantMigrationPlanTarget{
		{TenantID: "acme", Schema: "tenant_acme"},
		{TenantID: "globex", Database: "tenant_globex"},
	}
	assertTenantMigrationBatch(t, plan.Batches[0], 1, 2, true, TenantMigrationPlanFailureContinue, wantFirst)

	wantSecond := []TenantMigrationPlanTarget{
		{Schema: "tenant_shared", Database: "shared_app"},
	}
	assertTenantMigrationBatch(t, plan.Batches[1], 2, 2, true, TenantMigrationPlanFailureContinue, wantSecond)
}

func TestPlanTenantMigrationsDefaultsToSingleStopBatch(t *testing.T) {
	plan, err := PlanTenantMigrations(TenantMigrationPlanOptions{
		Targets: []TenantMigrationPlanTarget{
			{TenantID: "tenant-1"},
			{TenantID: "tenant-2"},
		},
	})
	if err != nil {
		t.Fatalf("PlanTenantMigrations returned %v", err)
	}

	if plan.FailurePolicy != TenantMigrationPlanFailureStop {
		t.Fatalf("plan failure policy = %q, want stop", plan.FailurePolicy)
	}
	if plan.MaxBatchSize != 0 {
		t.Fatalf("plan max batch size = %d, want 0", plan.MaxBatchSize)
	}
	if len(plan.Batches) != 1 {
		t.Fatalf("batch count = %d, want 1", len(plan.Batches))
	}
	assertTenantMigrationBatch(t, plan.Batches[0], 1, 1, false, TenantMigrationPlanFailureStop, []TenantMigrationPlanTarget{
		{TenantID: "tenant-1"},
		{TenantID: "tenant-2"},
	})
}

func TestPlanTenantMigrationsTargetsReturnsCopy(t *testing.T) {
	plan, err := PlanTenantMigrations(TenantMigrationPlanOptions{
		Targets: []TenantMigrationPlanTarget{
			{TenantID: "tenant-1", Schema: "tenant_1"},
		},
	})
	if err != nil {
		t.Fatalf("PlanTenantMigrations returned %v", err)
	}

	targets := plan.Targets()
	targets[0].TenantID = "changed"

	got := plan.Targets()
	if got[0].TenantID != "tenant-1" {
		t.Fatalf("plan target was mutated through returned slice: %#v", got[0])
	}
}

func TestPlanTenantMigrationsValidatesTargetsAndPolicy(t *testing.T) {
	tests := []struct {
		name string
		opts TenantMigrationPlanOptions
		want error
	}{
		{
			name: "no targets",
			opts: TenantMigrationPlanOptions{},
			want: ErrTenantMigrationPlanTargetRequired,
		},
		{
			name: "blank target",
			opts: TenantMigrationPlanOptions{
				Targets: []TenantMigrationPlanTarget{{TenantID: " ", Schema: " ", Database: " "}},
			},
			want: ErrTenantMigrationPlanTargetRequired,
		},
		{
			name: "invalid schema",
			opts: TenantMigrationPlanOptions{
				Targets: []TenantMigrationPlanTarget{{TenantID: "tenant-1", Schema: "bad-schema"}},
			},
			want: ErrInvalidSQLIdentifier,
		},
		{
			name: "invalid database",
			opts: TenantMigrationPlanOptions{
				Targets: []TenantMigrationPlanTarget{{Database: "1tenant"}},
			},
			want: ErrInvalidSQLIdentifier,
		},
		{
			name: "invalid failure policy",
			opts: TenantMigrationPlanOptions{
				Targets:       []TenantMigrationPlanTarget{{TenantID: "tenant-1"}},
				FailurePolicy: TenantMigrationPlanFailurePolicy("ignore"),
			},
			want: ErrInvalidTenantMigrationPlanFailurePolicy,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := PlanTenantMigrations(tt.opts)
			if !errors.Is(err, tt.want) {
				t.Fatalf("PlanTenantMigrations error = %v, want %v", err, tt.want)
			}
		})
	}
}

func assertTenantMigrationBatch(
	t *testing.T,
	got TenantMigrationPlanBatch,
	index int,
	count int,
	dryRun bool,
	failurePolicy TenantMigrationPlanFailurePolicy,
	targets []TenantMigrationPlanTarget,
) {
	t.Helper()
	if got.Index != index || got.Count != count {
		t.Fatalf("batch position = %d/%d, want %d/%d", got.Index, got.Count, index, count)
	}
	if got.DryRun != dryRun {
		t.Fatalf("batch DryRun = %v, want %v", got.DryRun, dryRun)
	}
	if got.FailurePolicy != failurePolicy {
		t.Fatalf("batch failure policy = %q, want %q", got.FailurePolicy, failurePolicy)
	}
	if !reflect.DeepEqual(got.Targets, targets) {
		t.Fatalf("batch targets = %#v, want %#v", got.Targets, targets)
	}
}
