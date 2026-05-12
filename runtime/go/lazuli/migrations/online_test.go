package migrations

import (
	"errors"
	"reflect"
	"testing"
)

func TestPlanOnlineMigrationSplitsStepsIntoPhases(t *testing.T) {
	steps := []OnlineMigrationStep{
		{
			Name:                "drop_legacy_status",
			Phase:               OnlineMigrationPhaseContract,
			TransactionMode:     OnlineMigrationTransactionTransactional,
			Destructive:         true,
			DestructiveApproved: true,
		},
		{
			Name:            "check_write_path",
			Phase:           OnlineMigrationPhasePreflight,
			TransactionMode: OnlineMigrationTransactionTransactional,
		},
		{
			Name:            "backfill_status_v2",
			Phase:           OnlineMigrationPhaseBackfill,
			TransactionMode: OnlineMigrationTransactionNonTransactional,
		},
		{
			Name:            "add_status_v2",
			Phase:           OnlineMigrationPhaseExpand,
			TransactionMode: OnlineMigrationTransactionTransactional,
		},
	}

	plan, err := PlanOnlineMigration(steps)
	if err != nil {
		t.Fatalf("PlanOnlineMigration returned %v", err)
	}

	if want := []string{"check_write_path"}; !reflect.DeepEqual(onlineStepNames(plan.Preflight), want) {
		t.Fatalf("preflight steps = %v, want %v", onlineStepNames(plan.Preflight), want)
	}
	if want := []string{"add_status_v2"}; !reflect.DeepEqual(onlineStepNames(plan.Expand), want) {
		t.Fatalf("expand steps = %v, want %v", onlineStepNames(plan.Expand), want)
	}
	if want := []string{"backfill_status_v2"}; !reflect.DeepEqual(onlineStepNames(plan.Backfill), want) {
		t.Fatalf("backfill steps = %v, want %v", onlineStepNames(plan.Backfill), want)
	}
	if want := []string{"drop_legacy_status"}; !reflect.DeepEqual(onlineStepNames(plan.Contract), want) {
		t.Fatalf("contract steps = %v, want %v", onlineStepNames(plan.Contract), want)
	}

	if want := []string{"check_write_path", "add_status_v2", "backfill_status_v2", "drop_legacy_status"}; !reflect.DeepEqual(onlineStepNames(plan.Steps()), want) {
		t.Fatalf("ordered steps = %v, want %v", onlineStepNames(plan.Steps()), want)
	}
}

func TestPlanOnlineMigrationPreservesSafetyFlags(t *testing.T) {
	plan, err := PlanOnlineMigration([]OnlineMigrationStep{
		{
			Name:            "create_concurrent_index",
			Phase:           OnlineMigrationPhaseExpand,
			TransactionMode: OnlineMigrationTransactionNonTransactional,
		},
		{
			Name:                "drop_old_index",
			Phase:               OnlineMigrationPhaseContract,
			TransactionMode:     OnlineMigrationTransactionTransactional,
			Destructive:         true,
			DestructiveApproved: true,
		},
	})
	if err != nil {
		t.Fatalf("PlanOnlineMigration returned %v", err)
	}

	if got := plan.Expand[0].TransactionMode; got != OnlineMigrationTransactionNonTransactional {
		t.Fatalf("expand transaction mode = %q, want %q", got, OnlineMigrationTransactionNonTransactional)
	}
	contract := plan.Contract[0]
	if got := contract.TransactionMode; got != OnlineMigrationTransactionTransactional {
		t.Fatalf("contract transaction mode = %q, want %q", got, OnlineMigrationTransactionTransactional)
	}
	if !contract.Destructive || !contract.DestructiveApproved {
		t.Fatalf("contract destructive flags = destructive:%v approved:%v, want both true", contract.Destructive, contract.DestructiveApproved)
	}
}

func TestPlanOnlineMigrationRequiresApprovalForDestructiveSteps(t *testing.T) {
	_, err := PlanOnlineMigration([]OnlineMigrationStep{
		{
			Name:            "drop_old_column",
			Phase:           OnlineMigrationPhaseContract,
			TransactionMode: OnlineMigrationTransactionTransactional,
			Destructive:     true,
		},
	})
	if !errors.Is(err, ErrDestructiveMigrationApprovalRequired) {
		t.Fatalf("expected ErrDestructiveMigrationApprovalRequired, got %v", err)
	}
}

func TestPlanOnlineMigrationValidatesStepDescriptors(t *testing.T) {
	tests := []struct {
		name string
		step OnlineMigrationStep
		want error
	}{
		{
			name: "missing name",
			step: OnlineMigrationStep{
				Phase:           OnlineMigrationPhasePreflight,
				TransactionMode: OnlineMigrationTransactionTransactional,
			},
			want: ErrOnlineMigrationStepNameRequired,
		},
		{
			name: "invalid phase",
			step: OnlineMigrationStep{
				Name:            "validate",
				Phase:           OnlineMigrationPhase("verify"),
				TransactionMode: OnlineMigrationTransactionTransactional,
			},
			want: ErrInvalidOnlineMigrationPhase,
		},
		{
			name: "invalid transaction mode",
			step: OnlineMigrationStep{
				Name:            "validate",
				Phase:           OnlineMigrationPhasePreflight,
				TransactionMode: OnlineMigrationTransactionMode("auto"),
			},
			want: ErrInvalidOnlineMigrationTransactionMode,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := PlanOnlineMigration([]OnlineMigrationStep{tt.step})
			if !errors.Is(err, tt.want) {
				t.Fatalf("expected %v, got %v", tt.want, err)
			}
		})
	}
}

func onlineStepNames(steps []OnlineMigrationStep) []string {
	names := make([]string, len(steps))
	for i, step := range steps {
		names[i] = step.Name
	}
	return names
}
