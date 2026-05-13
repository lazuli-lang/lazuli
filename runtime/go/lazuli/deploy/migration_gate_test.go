package deploy_test

import (
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/deploy"
	"lazuli.dev/runtime/lazuli/migrations"
)

func TestRenderMigrationGateDryRunSummary(t *testing.T) {
	onlinePlan, err := migrations.PlanOnlineMigration([]migrations.OnlineMigrationStep{
		{
			Name:            "check_write_path",
			Phase:           migrations.OnlineMigrationPhasePreflight,
			TransactionMode: migrations.OnlineMigrationTransactionTransactional,
		},
		{
			Name:            "add_status_v2",
			Phase:           migrations.OnlineMigrationPhaseExpand,
			TransactionMode: migrations.OnlineMigrationTransactionTransactional,
		},
		{
			Name:            "backfill_status_v2",
			Phase:           migrations.OnlineMigrationPhaseBackfill,
			TransactionMode: migrations.OnlineMigrationTransactionNonTransactional,
		},
		{
			Name:                "drop_legacy_status",
			Phase:               migrations.OnlineMigrationPhaseContract,
			TransactionMode:     migrations.OnlineMigrationTransactionTransactional,
			Destructive:         true,
			DestructiveApproved: true,
		},
	})
	if err != nil {
		t.Fatalf("PlanOnlineMigration() error = %v", err)
	}

	tenantPlan, err := migrations.PlanTenantMigrations(migrations.TenantMigrationPlanOptions{
		Migration: migrations.MigrationRecord{Feature: "billing", Name: "backfill_status"},
		Targets: []migrations.TenantMigrationPlanTarget{
			{TenantID: "tenant-a"},
			{TenantID: "tenant-b"},
			{TenantID: "tenant-c"},
		},
		MaxBatchSize: 2,
		DryRun:       true,
	})
	if err != nil {
		t.Fatalf("PlanTenantMigrations() error = %v", err)
	}

	got, err := deploy.RenderMigrationGateDryRunSummary(deploy.MigrationGatePlanConfig{
		Policy: migrations.DeployPolicy{
			LockTimeout:           45 * time.Second,
			PreHook:               "./hooks/pre-migration.sh",
			PostHook:              "./hooks/post-migration.sh",
			Migrations:            "before_deploy",
			MigrationLock:         "required",
			DestructiveMigrations: "require_approval",
		},
		OnlinePlan: onlinePlan,
		TenantPlan: tenantPlan,
		UpgradePlan: migrations.UpgradePlan{
			FromVersion: "0.10",
			ToVersion:   "0.11",
			Recipes: []migrations.UpgradeRecipeDescriptor{
				{Name: "rename-status", FromVersion: "0.10", ToVersion: "0.11"},
			},
		},
		Contracts: []migrations.TenantMigrationContract{
			{Feature: "billing", Name: "backfill_status", Timeout: 10 * time.Minute},
			{Name: "seed_defaults"},
		},
	})
	if err != nil {
		t.Fatalf("RenderMigrationGateDryRunSummary() error = %v", err)
	}

	want := `migration_gate:
  dry_run: true
  timing: "before_deploy"
  lock:
    policy: "required"
    timeout: "45s"
    blocking: true
    timeout_action: "fail_deploy"
  destructive_migrations:
    policy: "require_approval"
    destructive_steps: 1
    approved_steps: 1
    blocking: false
    reason: "all destructive migration steps are approved"
  hooks:
    - name: "pre_migration"
      phase: "pre_migration"
      command: "./hooks/pre-migration.sh"
      blocking: true
    - name: "post_migration"
      phase: "post_migration"
      command: "./hooks/post-migration.sh"
      blocking: true
  gates:
    - name: "online preflight"
      phase: "before_deploy"
      blocking: true
      count: 1
      reason: "1 step in preflight online migration phase"
    - name: "online expand"
      phase: "before_deploy"
      blocking: true
      count: 1
      reason: "1 step in expand online migration phase"
    - name: "online backfill"
      phase: "post_migration"
      blocking: false
      count: 1
      reason: "1 step in backfill online migration phase"
    - name: "online contract"
      phase: "post_migration"
      blocking: false
      count: 1
      reason: "1 step in contract online migration phase"
    - name: "tenant migrations"
      phase: "before_deploy"
      blocking: true
      count: 3
      reason: "3 tenant targets across 2 batches"
    - name: "upgrade recipes"
      phase: "before_deploy"
      blocking: true
      count: 1
      reason: "1 upgrade recipe from \"0.10\" to \"0.11\""
  timeouts:
    - name: "migration_lock"
      scope: "lock"
      phase: "before_deploy"
      timeout: "45s"
      source: "deploy.lock_timeout"
      action: "fail_deploy"
    - name: "billing.backfill_status"
      scope: "tenant_migration"
      phase: "before_deploy"
      timeout: "10m0s"
      source: "tenant_migration.timeout"
      action: "fail_deploy"
    - name: "seed_defaults"
      scope: "tenant_migration"
      phase: "before_deploy"
      timeout: "adapter_default"
      source: "tenant_migration.timeout"
      action: "fail_deploy"
  summary:
    blocking_gates: 4
    nonblocking_gates: 2
    hooks: 2
    timeouts: 3
`
	if got != want {
		t.Fatalf("RenderMigrationGateDryRunSummary() =\n%s\nwant\n%s", got, want)
	}
}

func TestBuildMigrationGatePlanManualGatesAreNonblocking(t *testing.T) {
	plan, err := deploy.BuildMigrationGatePlan(deploy.MigrationGatePlanConfig{
		Policy: migrations.DeployPolicy{
			Migrations:            "manual",
			MigrationLock:         "optional",
			DestructiveMigrations: "allow",
		},
		OnlinePlan: migrations.OnlineMigrationPlan{
			Expand: []migrations.OnlineMigrationStep{
				{Name: "add_status_v2", Phase: migrations.OnlineMigrationPhaseExpand},
			},
		},
		Contracts: []migrations.TenantMigrationContract{
			{Feature: "billing", Name: "backfill_status", Timeout: 3 * time.Second},
		},
	})
	if err != nil {
		t.Fatalf("BuildMigrationGatePlan() error = %v", err)
	}

	if plan.Lock.Blocking {
		t.Fatal("Lock.Blocking = true, want false for optional lock")
	}
	if plan.Lock.TimeoutAction != deploy.MigrationTimeoutContinue {
		t.Fatalf("lock timeout action = %q, want continue", plan.Lock.TimeoutAction)
	}
	if got := len(plan.BlockingGates()); got != 0 {
		t.Fatalf("blocking gate count = %d, want 0", got)
	}
	if got := len(plan.NonBlockingGates()); got != 1 {
		t.Fatalf("nonblocking gate count = %d, want 1", got)
	}
	if got := plan.Gates[0].Phase; got != deploy.MigrationGatePhaseManual {
		t.Fatalf("gate phase = %q, want manual", got)
	}
	if got := len(plan.ReleaseGates()); got != 0 {
		t.Fatalf("ReleaseGates() count = %d, want 0 for manual nonblocking gate", got)
	}
	if got := plan.Timeouts[1].Action; got != deploy.MigrationTimeoutContinue {
		t.Fatalf("tenant timeout action = %q, want continue", got)
	}
}

func TestBuildMigrationGatePlanBlocksDestructiveMigrations(t *testing.T) {
	plan, err := deploy.BuildMigrationGatePlan(deploy.MigrationGatePlanConfig{
		Policy: migrations.DeployPolicy{
			Migrations:            "disabled",
			DestructiveMigrations: "block",
		},
		OnlinePlan: migrations.OnlineMigrationPlan{
			Contract: []migrations.OnlineMigrationStep{
				{
					Name:        "drop_legacy_status",
					Phase:       migrations.OnlineMigrationPhaseContract,
					Destructive: true,
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("BuildMigrationGatePlan() error = %v", err)
	}

	if !plan.Destructive.Blocking {
		t.Fatal("Destructive.Blocking = false, want true")
	}
	if got := len(plan.BlockingGates()); got != 1 {
		t.Fatalf("blocking gate count = %d, want 1", got)
	}
	gate := plan.BlockingGates()[0]
	if gate.Name != "destructive migrations" || !strings.Contains(gate.Reason, "blocked") {
		t.Fatalf("destructive gate = %#v, want blocking destructive migration gate", gate)
	}
}

func TestValidateMigrationGateConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name     string
		config   deploy.MigrationGatePlanConfig
		fragment string
	}{
		{
			name: "invalid migration timing",
			config: deploy.MigrationGatePlanConfig{
				Policy: migrations.DeployPolicy{Migrations: "during_deploy"},
			},
			fragment: "policy.migrations",
		},
		{
			name: "negative lock timeout",
			config: deploy.MigrationGatePlanConfig{
				Policy: migrations.DeployPolicy{LockTimeout: -time.Second},
			},
			fragment: "policy.lock_timeout",
		},
		{
			name: "control character hook",
			config: deploy.MigrationGatePlanConfig{
				Policy: migrations.DeployPolicy{PreHook: "./hooks/pre.sh\n"},
			},
			fragment: "policy.pre_hook",
		},
		{
			name: "negative contract timeout",
			config: deploy.MigrationGatePlanConfig{
				Contracts: []migrations.TenantMigrationContract{{Name: "bad", Timeout: -time.Second}},
			},
			fragment: "contracts[0].timeout",
		},
		{
			name: "online step in wrong phase",
			config: deploy.MigrationGatePlanConfig{
				OnlinePlan: migrations.OnlineMigrationPlan{
					Expand: []migrations.OnlineMigrationStep{
						{Name: "wrong", Phase: migrations.OnlineMigrationPhaseBackfill},
					},
				},
			},
			fragment: "expand[0]",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := deploy.ValidateMigrationGateConfig(tt.config)
			if !errors.Is(err, deploy.ErrInvalidMigrationGateConfig) {
				t.Fatalf("ValidateMigrationGateConfig() error = %v, want ErrInvalidMigrationGateConfig", err)
			}
			if !strings.Contains(err.Error(), tt.fragment) {
				t.Fatalf("ValidateMigrationGateConfig() error = %v, want fragment %q", err, tt.fragment)
			}
		})
	}
}
