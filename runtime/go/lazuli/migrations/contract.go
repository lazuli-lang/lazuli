// Package migrations implements the runtime side of the Lazuli
// `tenant_migration` block + the `deploy` block's migration policy
// (strategy / lock_timeout / hooks / checkpoint). The language declares
// the migration contract (target axis, idempotency, retry, timeout,
// handler path) and the deploy policy; this package owns dispatch +
// per-tenant fanout + idempotency tracking + advisory locking.
//
// Concrete schema-migration adapters (`@runtime/atlas`,
// `@runtime/golang-migrate`) sit in `@runtime/...` packages and bind via
// `@adapter.migrations.*` resolution at boot. This package never names
// a concrete tool — it declares the contract a `TenantMigrator`
// implementation must satisfy.
//
// Migrations bucket cycle Route C — this file ships the typed contract
// shape the codegen emits per `tenant_migration` plus the deploy
// strategy/lock_timeout/hook/checkpoint slots. Dispatch implementation
// lives in `dispatch.go`; planning in `plan.go`.
package migrations

import (
	"context"
	"errors"
	"time"
)

// BackoffStrategy names a closed-catalog retry backoff strategy. Mirrors
// the `jobs.BackoffStrategy` shape so the same retry helpers can be
// reused across schema and business work.
type BackoffStrategy string

const (
	BackoffFixed       BackoffStrategy = "fixed"
	BackoffExponential BackoffStrategy = "exponential"
)

// RetryPolicy is the lowered `retry <count> backoff <strategy>` block.
type RetryPolicy struct {
	Count   uint32
	Backoff BackoffStrategy
}

// IdempotencyKeySpec is the lowered `idempotency by <path>` directive.
// Path is a dot-segmented expression evaluated against the migration
// envelope (e.g. `tenant.org_id`, `payload.batch_id`).
type IdempotencyKeySpec struct {
	Path string
}

// TenantMigrationTarget is the lowered `target tenants <axis>` directive.
// `Axis` carries the tenancy axis name (`org`, `team`, custom). The
// runtime resolves the axis through the same tenant directory used by
// the jobs fanout dispatcher.
type TenantMigrationTarget struct {
	Axis string
}

// TenantMigrationContract is the lowered `tenant_migration <name>`
// shape. The codegen emits
// `var <FeatureCamel>Migration<NameCamel>Contract = migrations.TenantMigrationContract{...}`
// per migration; dispatchers read it to wire target axis, idempotency,
// retry, timeout, and the handler callback.
type TenantMigrationContract struct {
	// Feature is the owning feature name (e.g. `customer`).
	Feature string
	// Name is the migration identifier.
	Name string
	// Target declares the per-tenant fanout axis.
	Target TenantMigrationTarget
	// Idempotency is mandatory; the dispatcher records a row in the
	// `tenant_migration_log` table keyed by `(Feature, Name, Idempotency.Path eval, TenantID)`.
	Idempotency IdempotencyKeySpec
	// Retry declares attempt count + backoff strategy. Nil means no retry.
	Retry *RetryPolicy
	// Timeout is the per-tenant execution timeout (`"5m"`, `"1h"`).
	// Zero means adapter default.
	Timeout time.Duration
	// HandlerPath is the `./path.go` handler reference. The codegen
	// resolves this to a `HandlerFunc` and binds it at boot.
	HandlerPath string
}

// HandlerFunc is the signature the codegen wires for migration
// handlers. Returns nil on success; the dispatcher classifies errors
// against the retry policy. The `tenantID` argument is opaque — the
// handler dispatches schema operations against the resolved tenant
// database/schema through the `*ctx.TenantContext`.
type HandlerFunc func(ctx context.Context, tenantID string) error

// DeployStrategy names a closed-catalog migration rollout pattern.
// Mirrors `deploy.strategy` enforced by `DEPLOY-STRATEGY-001`.
type DeployStrategy string

const (
	StrategyRolling   DeployStrategy = "rolling"
	StrategyBlueGreen DeployStrategy = "blue_green"
	StrategyCanary    DeployStrategy = "canary"
)

// DeployPolicy is the lowered `deploy` block subset relevant to
// migrations. The codegen emits a single `var AppDeployPolicy =
// migrations.DeployPolicy{...}` consumed by the `Apply` entry point.
type DeployPolicy struct {
	// Strategy is `rolling | blue_green | canary`. Empty means
	// adapter default.
	Strategy DeployStrategy
	// LockTimeout is the max wait for the migration advisory lock.
	// Zero means adapter default.
	LockTimeout time.Duration
	// PreHook is an optional shell hook executed before applying
	// migrations. Empty when no hook is declared.
	PreHook string
	// PostHook is an optional shell hook executed after applying
	// migrations. Empty when no hook is declared.
	PostHook string
	// Migrations gates timing: `before_deploy | manual | disabled`.
	Migrations string
	// MigrationLock is `required | optional | none`.
	MigrationLock string
	// DestructiveMigrations is `require_approval | allow | block`.
	DestructiveMigrations string
	// Rollback is `on_failed_healthcheck | manual | disabled`.
	Rollback string
}

// Checkpoint is the lowered `deploy.checkpoint <name> "<path>"` shape.
// The runtime uses it for snapshot integrity (`lazuli plan --check`)
// and, post-Tier-4, typed field diff against the current source IR.
type Checkpoint struct {
	Name string
	Path string
}

// Typed errors returned by the migration dispatcher.
var (
	// ErrMigrationTimeout is returned when a per-tenant migration
	// exceeds its declared `timeout`.
	ErrMigrationTimeout = errors.New("migrations: timeout")
	// ErrMigrationLockTimeout is returned when the advisory lock
	// cannot be acquired within `deploy.lock_timeout`.
	ErrMigrationLockTimeout = errors.New("migrations: lock acquisition timeout")
	// ErrMigrationMaxRetries is returned when the retry budget is
	// exhausted for a tenant.
	ErrMigrationMaxRetries = errors.New("migrations: retry budget exhausted")
	// ErrMigrationTenantAxisUnknown is returned when `target tenants <axis>`
	// references an axis the tenant directory cannot enumerate.
	ErrMigrationTenantAxisUnknown = errors.New("migrations: tenant axis unknown")
	// ErrMigrationCheckpointStale is returned when a checkpoint's
	// snapshot lazuli_version lags the running analyzer.
	ErrMigrationCheckpointStale = errors.New("migrations: checkpoint snapshot stale")
)
