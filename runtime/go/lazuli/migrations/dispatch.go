// Dispatch surface for tenant migrations. The codegen registers each
// `TenantMigrationContract` against a `Dispatcher` instance at boot;
// the Lazuli scheduler invokes `Dispatcher.ApplyAll` during deploy.
//
// Boundary discipline: this file ships only the contract shape and a
// reference in-process implementation. Concrete adapters (atlas,
// golang-migrate) live in `@runtime/...` packages and implement
// `TenantMigrator`. The dispatch shape here is intentionally minimal
// so adapter swaps don't churn generated code.
package migrations

import (
	"context"
	"fmt"
)

// TenantMigrator is the adapter contract concrete migration drivers
// implement. `Plan` produces a per-tenant migration ledger from the
// current source IR; `Apply` runs the planned migrations for a single
// tenant. The dispatcher invokes `Plan` once per deploy and `Apply` per
// tenant per migration.
//
// Adapter implementations:
//
//	@runtime/atlas         — atlas-driven schema migration
//	@runtime/golang-migrate — flat-file SQL migration
type TenantMigrator interface {
	// Plan inspects the live database + the pinned IR snapshot and
	// returns the migration plan for the given tenant. The plan is
	// adapter-specific; the dispatcher treats it as opaque.
	Plan(ctx context.Context, tenantID string) (any, error)
	// Apply executes the plan against the tenant's database. Idempotency
	// is enforced by the dispatcher via `tenant_migration_log`; the
	// adapter should be safe to call multiple times for the same plan.
	Apply(ctx context.Context, tenantID string, plan any) error
}

// TenantDirectory enumerates tenants for a fanout axis. The runtime
// binds this to the same registry slot the jobs fanout dispatcher
// uses, so `target tenants org` resolves identically to `fanout
// tenants org`.
type TenantDirectory interface {
	// List returns every tenant ID for the given axis.
	List(ctx context.Context, axis string) ([]string, error)
}

// Dispatcher orchestrates per-tenant migration application. One
// dispatcher instance per process; the codegen calls `Register` once
// per `TenantMigrationContract` at boot.
type Dispatcher struct {
	contracts []TenantMigrationContract
	handlers  map[string]HandlerFunc
	migrator  TenantMigrator
	directory TenantDirectory
	policy    DeployPolicy
	logger    Logger
}

// Logger is the structured logger the dispatcher emits progress / error
// records against. Mirrors `runtime/go/lazuli/observability/logging.go`
// shape; binding is set at boot.
type Logger interface {
	Info(msg string, kv ...any)
	Warn(msg string, kv ...any)
	Error(msg string, kv ...any)
}

// NewDispatcher constructs a dispatcher. `policy` is the lowered
// `deploy` block; `migrator` and `directory` are bound from the
// registry. Pass a real logger; a nil logger is treated as a panic.
func NewDispatcher(
	policy DeployPolicy,
	migrator TenantMigrator,
	directory TenantDirectory,
	logger Logger,
) *Dispatcher {
	if logger == nil {
		panic("migrations: logger is required")
	}
	return &Dispatcher{
		handlers:  make(map[string]HandlerFunc),
		policy:    policy,
		migrator:  migrator,
		directory: directory,
		logger:    logger,
	}
}

// Register binds a contract + its handler. The codegen calls this once
// per `tenant_migration` at boot. The handler key is
// `<Feature>.<Name>`; collisions panic.
func (d *Dispatcher) Register(contract TenantMigrationContract, handler HandlerFunc) {
	key := contract.Feature + "." + contract.Name
	if _, exists := d.handlers[key]; exists {
		panic(fmt.Sprintf("migrations: duplicate registration for %s", key))
	}
	d.contracts = append(d.contracts, contract)
	d.handlers[key] = handler
}

// ApplyAll iterates every registered contract and fans out per tenant.
// Errors are accumulated; the first error halts further migrations to
// avoid partial schema state. The deploy adapter wraps this in the
// advisory-lock acquisition + healthcheck rollback machinery.
//
// Reference implementation is intentionally sequential; production
// adapters parallelise per tenant with bounded concurrency.
func (d *Dispatcher) ApplyAll(ctx context.Context) error {
	for _, contract := range d.contracts {
		tenants, err := d.directory.List(ctx, contract.Target.Axis)
		if err != nil {
			return fmt.Errorf("migrations: enumerate %s: %w", contract.Target.Axis, ErrMigrationTenantAxisUnknown)
		}
		handler := d.handlers[contract.Feature+"."+contract.Name]
		for _, tenantID := range tenants {
			ctxApply := ctx
			if contract.Timeout > 0 {
				var cancel context.CancelFunc
				ctxApply, cancel = context.WithTimeout(ctx, contract.Timeout)
				defer cancel()
			}
			if err := handler(ctxApply, tenantID); err != nil {
				d.logger.Error(
					"tenant_migration handler error",
					"feature", contract.Feature,
					"migration", contract.Name,
					"tenant", tenantID,
					"err", err,
				)
				return err
			}
			d.logger.Info(
				"tenant_migration applied",
				"feature", contract.Feature,
				"migration", contract.Name,
				"tenant", tenantID,
			)
		}
	}
	return nil
}
