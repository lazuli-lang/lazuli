package migrations

import (
	"errors"
	"fmt"
	"strings"
)

var (
	// ErrTenantMigrationPlanTargetRequired is returned when a tenant migration
	// plan has no target, or a target selects no tenant ID, schema, or database.
	ErrTenantMigrationPlanTargetRequired = errors.New("migrations: tenant migration plan target required")
	// ErrInvalidTenantMigrationPlanFailurePolicy is returned when a tenant
	// migration plan declares an unknown failure policy.
	ErrInvalidTenantMigrationPlanFailurePolicy = errors.New("migrations: tenant migration plan failure policy invalid")
)

// TenantMigrationPlanFailurePolicy controls how a future executor should react
// to per-target failures. The planner records this metadata but never executes
// migrations.
type TenantMigrationPlanFailurePolicy string

const (
	// TenantMigrationPlanFailureStop stops on the first failed tenant target.
	// It is the default when TenantMigrationPlanOptions.FailurePolicy is empty.
	TenantMigrationPlanFailureStop TenantMigrationPlanFailurePolicy = "stop"
	// TenantMigrationPlanFailureContinue records failures while allowing later
	// tenant targets to continue.
	TenantMigrationPlanFailureContinue TenantMigrationPlanFailurePolicy = "continue"
)

// TenantMigrationPlanTarget identifies one tenant-scoped migration target. A
// target may be addressed by opaque tenant ID, SQL schema, SQL database, or any
// combination of those fields.
type TenantMigrationPlanTarget struct {
	// TenantID is the runtime tenant identifier from the tenant directory. It
	// is opaque to this package.
	TenantID string
	// Schema is the optional tenant schema name.
	Schema string
	// Database is the optional tenant database name.
	Database string
}

// TenantMigrationPlanOptions configures PlanTenantMigrations.
type TenantMigrationPlanOptions struct {
	// Migration optionally identifies the generated tenant_migration contract
	// represented by the plan.
	Migration MigrationRecord
	// Targets are the tenant IDs, schemas, and databases to plan. At least one
	// target is required.
	Targets []TenantMigrationPlanTarget
	// MaxBatchSize caps target count per batch. Zero means all targets are
	// placed in one batch.
	MaxBatchSize uint32
	// DryRun is metadata for callers that want to render/apply the plan without
	// mutating tenant databases. Planning itself is always side-effect free.
	DryRun bool
	// FailurePolicy is recorded on the plan and each batch. Empty means stop.
	FailurePolicy TenantMigrationPlanFailurePolicy
}

// TenantMigrationPlan is a side-effect-free migration rollout plan over tenant
// IDs, schemas, and databases.
type TenantMigrationPlan struct {
	Migration     MigrationRecord
	DryRun        bool
	FailurePolicy TenantMigrationPlanFailurePolicy
	MaxBatchSize  uint32
	TargetCount   int
	BatchCount    int
	Batches       []TenantMigrationPlanBatch
}

// TenantMigrationPlanBatch is one ordered batch of tenant migration targets.
type TenantMigrationPlanBatch struct {
	Index         int
	Count         int
	DryRun        bool
	FailurePolicy TenantMigrationPlanFailurePolicy
	Targets       []TenantMigrationPlanTarget
}

// PlanTenantMigrations validates and batches tenant-scoped migration targets.
// It does not open database connections and never executes migration handlers.
func PlanTenantMigrations(opts TenantMigrationPlanOptions) (TenantMigrationPlan, error) {
	plan := TenantMigrationPlan{
		Migration:    tenantMigrationPlanRecord(opts.Migration),
		DryRun:       opts.DryRun,
		MaxBatchSize: opts.MaxBatchSize,
	}

	policy, err := normalizeTenantMigrationPlanFailurePolicy(opts.FailurePolicy)
	if err != nil {
		return plan, err
	}
	plan.FailurePolicy = policy

	targets, err := normalizeTenantMigrationPlanTargets(opts.Targets)
	if err != nil {
		return plan, err
	}
	plan.TargetCount = len(targets)

	chunks := tenantMigrationPlanChunks(targets, opts.MaxBatchSize)
	plan.BatchCount = len(chunks)
	plan.Batches = make([]TenantMigrationPlanBatch, 0, len(chunks))
	for i, chunk := range chunks {
		plan.Batches = append(plan.Batches, TenantMigrationPlanBatch{
			Index:         i + 1,
			Count:         len(chunks),
			DryRun:        opts.DryRun,
			FailurePolicy: policy,
			Targets:       cloneTenantMigrationPlanTargets(chunk),
		})
	}
	return plan, nil
}

// Targets returns all planned targets in batch order.
func (p TenantMigrationPlan) Targets() []TenantMigrationPlanTarget {
	targets := make([]TenantMigrationPlanTarget, 0, p.TargetCount)
	for _, batch := range p.Batches {
		targets = append(targets, batch.Targets...)
	}
	return cloneTenantMigrationPlanTargets(targets)
}

func tenantMigrationPlanRecord(record MigrationRecord) MigrationRecord {
	record.ID = strings.TrimSpace(record.ID)
	record.Feature = strings.TrimSpace(record.Feature)
	record.Name = strings.TrimSpace(record.Name)
	if record.ID == "" && record.Name != "" {
		record.ID = migrationID(record.Feature, record.Name)
	}
	return record
}

func normalizeTenantMigrationPlanTargets(targets []TenantMigrationPlanTarget) ([]TenantMigrationPlanTarget, error) {
	if len(targets) == 0 {
		return nil, ErrTenantMigrationPlanTargetRequired
	}

	normalized := make([]TenantMigrationPlanTarget, 0, len(targets))
	for i, target := range targets {
		target = normalizeTenantMigrationPlanTarget(target)
		if target.TenantID == "" && target.Schema == "" && target.Database == "" {
			return nil, fmt.Errorf("migrations: tenant migration target %d: %w", i, ErrTenantMigrationPlanTargetRequired)
		}
		if target.Schema != "" {
			if _, err := quoteSQLIdentifier("schema", target.Schema); err != nil {
				return nil, fmt.Errorf("migrations: tenant migration target %d: %w", i, err)
			}
		}
		if target.Database != "" {
			if _, err := quoteSQLIdentifier("database", target.Database); err != nil {
				return nil, fmt.Errorf("migrations: tenant migration target %d: %w", i, err)
			}
		}
		normalized = append(normalized, target)
	}
	return normalized, nil
}

func normalizeTenantMigrationPlanTarget(target TenantMigrationPlanTarget) TenantMigrationPlanTarget {
	target.TenantID = strings.TrimSpace(target.TenantID)
	target.Schema = strings.TrimSpace(target.Schema)
	target.Database = strings.TrimSpace(target.Database)
	return target
}

func normalizeTenantMigrationPlanFailurePolicy(
	policy TenantMigrationPlanFailurePolicy,
) (TenantMigrationPlanFailurePolicy, error) {
	switch policy {
	case "", TenantMigrationPlanFailureStop:
		return TenantMigrationPlanFailureStop, nil
	case TenantMigrationPlanFailureContinue:
		return TenantMigrationPlanFailureContinue, nil
	default:
		return "", fmt.Errorf("%w %q", ErrInvalidTenantMigrationPlanFailurePolicy, policy)
	}
}

func tenantMigrationPlanChunks(targets []TenantMigrationPlanTarget, maxBatchSize uint32) [][]TenantMigrationPlanTarget {
	if len(targets) == 0 {
		return nil
	}
	if maxBatchSize == 0 || uint64(maxBatchSize) >= uint64(len(targets)) {
		return [][]TenantMigrationPlanTarget{targets}
	}

	batchSize := int(maxBatchSize)
	chunks := make([][]TenantMigrationPlanTarget, 0, (len(targets)+batchSize-1)/batchSize)
	for start := 0; start < len(targets); start += batchSize {
		end := start + batchSize
		if end > len(targets) {
			end = len(targets)
		}
		chunks = append(chunks, targets[start:end])
	}
	return chunks
}

func cloneTenantMigrationPlanTargets(targets []TenantMigrationPlanTarget) []TenantMigrationPlanTarget {
	cloned := make([]TenantMigrationPlanTarget, len(targets))
	copy(cloned, targets)
	return cloned
}
