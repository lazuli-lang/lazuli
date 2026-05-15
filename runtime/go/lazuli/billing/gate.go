package billing

import (
	"context"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/jobs"
	"lazuli.dev/runtime/lazuli/plangate"
	"lazuli.dev/runtime/lazuli/webhooks"
)

// activeCatalog is the package-wide PlanCatalog dispatchers consult
// when running gate preludes registered on `Query` / `Api` /
// `jobs.JobContract` / `webhooks.WebhookContract`. Codegen-emitted
// boot (or test setup) calls `RegisterCatalog(plan.Catalog)` once;
// command-emitted code keeps passing its own `plan.Catalog` ref to
// `CheckFeature`/`CheckQuota` directly so this hook is opt-in for
// dispatcher-driven callables only.
var activeCatalog *PlanCatalog

// RegisterCatalog installs the package-wide plan catalog
// dispatchers read via `lazuli.RunPrelude` and `lazuli.RunIncrement`.
// Pass nil to clear (tests).
func RegisterCatalog(c *PlanCatalog) { activeCatalog = c }

func init() {
	lazuli.PreludeRunner = runPreludeLazuliCtx
	lazuli.IncrementRunner = runIncrementLazuliCtx
	jobs.RegisterPreludeRunner(runPreludeContext)
	jobs.RegisterIncrementRunner(runIncrementContext)
	webhooks.RegisterPreludeRunner(runPreludeContext)
	webhooks.RegisterIncrementRunner(runIncrementContext)
}

// runPreludeLazuliCtx is the `*lazuli.Ctx`-flavored runner installed
// on `lazuli.PreludeRunner`. Walks the gate slice in canonical
// order — every `gate behind` first (boolean feature check → 402
// plan.feature_forbidden), then every `gate quota` (counter check
// → 402 plan.quota_exceeded) — matching the command-emitter
// prelude shape from PG.C.1.
func runPreludeLazuliCtx(ctx *lazuli.Ctx, prelude []lazuli.GateRef) error {
	return walkPrelude(ctx, prelude)
}

func runIncrementLazuliCtx(ctx *lazuli.Ctx, prelude []lazuli.GateRef) error {
	return walkIncrement(ctx, prelude)
}

// runPreludeContext is the `context.Context`-flavored runner
// installed on `jobs` and `webhooks`. Wraps the raw context in a
// minimal `*lazuli.Ctx`; `CheckFeature` / `CheckQuota` surface
// `ErrPlanLookupFailed` when no authenticated caller is on the
// envelope (the right behaviour for gated jobs/webhooks running
// without a resolved subscriber).
func runPreludeContext(ctx context.Context, prelude []plangate.GateRef) error {
	return walkPrelude(adaptCtx(ctx), prelude)
}

func runIncrementContext(ctx context.Context, prelude []plangate.GateRef) error {
	return walkIncrement(adaptCtx(ctx), prelude)
}

// adaptCtx promotes a raw `context.Context` into a minimal
// `*lazuli.Ctx`. Job and webhook dispatchers carry no User on the
// envelope; gates that reach `CheckFeature` will surface
// `ErrPlanLookupFailed` until a future cell threads tenant/user
// resolution through the envelope.
func adaptCtx(ctx context.Context) *lazuli.Ctx {
	if ctx == nil {
		ctx = context.Background()
	}
	return &lazuli.Ctx{Context: ctx}
}

func walkPrelude(ctx *lazuli.Ctx, prelude []lazuli.GateRef) error {
	if len(prelude) == 0 || activeCatalog == nil {
		return nil
	}
	for _, gate := range prelude {
		if gate.Kind != lazuli.GateBehind {
			continue
		}
		if err := CheckFeature(ctx, *activeCatalog, gate.Name); err != nil {
			return err
		}
	}
	for _, gate := range prelude {
		if gate.Kind != lazuli.GateQuota {
			continue
		}
		if err := CheckQuota(ctx, *activeCatalog, gate.Name); err != nil {
			return err
		}
	}
	return nil
}

// walkIncrement bumps every `gate quota` counter on the active
// catalog. Behind-gates are ignored (boolean checks have no usage
// to record). Failures swallowed by the dispatcher path; this
// matches command-emitter behaviour where `_ = billing.IncrQuota(...)`.
func walkIncrement(ctx *lazuli.Ctx, prelude []lazuli.GateRef) error {
	if len(prelude) == 0 || activeCatalog == nil {
		return nil
	}
	for _, gate := range prelude {
		if gate.Kind != lazuli.GateQuota {
			continue
		}
		if err := IncrQuota(ctx, *activeCatalog, gate.Name); err != nil {
			return err
		}
	}
	return nil
}

// CheckFeature implements `gate behind plan.feature: <name>`. Returns
// nil when the caller's active plan includes `feature`, otherwise
// `ErrPlanFeatureForbidden{Plan, Feature}`.
//
// Caller (typically codegen-emitted handler prelude) is expected to
// short-circuit dispatch on a non-nil error and map it to the 402
// canonical response via the error registry.
func CheckFeature(ctx *lazuli.Ctx, catalog PlanCatalog, feature string) error {
	plan, err := LookupPlan(ctx, catalog)
	if err != nil {
		return err
	}
	if !plan.Features.Has(feature) {
		return ErrPlanFeatureForbidden{Plan: plan.Name, Feature: feature}
	}
	return nil
}

// CheckQuota implements `gate quota plan.limit: <name>` — the
// pre-dispatch counter check.
//
// Resolution:
//   - active plan's limit value is `unlimited` -> no-op (return nil).
//   - finite limit -> read current period usage, compare to value.
//
// Post-success increment is the caller's responsibility via
// `IncrQuota` after the handler completes successfully.
func CheckQuota(ctx *lazuli.Ctx, catalog PlanCatalog, limit string) error {
	plan, err := LookupPlan(ctx, catalog)
	if err != nil {
		return err
	}
	value, ok := plan.Limits[limit]
	if !ok {
		// Doctor enforces declaration on every plan; if we reach this
		// branch in production it means the catalog drifted.
		return ErrPlanLookupFailed{
			Reason: "limit `" + limit + "` missing from plan `" + plan.Name + "`",
		}
	}
	if value.Unlimited {
		return nil
	}
	active, err := Resolve(ctx)
	if err != nil {
		return err
	}
	used, err := readUsage(ctx, active.SubscriptionID, limit)
	if err != nil {
		return err
	}
	if used >= value.Value {
		return ErrPlanQuotaExceeded{
			Plan:  plan.Name,
			Limit: limit,
			Used:  used,
			Max:   value.Value,
		}
	}
	return nil
}

// IncrQuota records one unit of consumption for `limit` against the
// caller's active subscription for the current billing period.
//
// Failures are non-fatal at the request level: the handler already
// returned a result to the user. The caller logs the error; a
// reconciliation job heals counter drift on a periodic basis.
func IncrQuota(ctx *lazuli.Ctx, catalog PlanCatalog, limit string) error {
	plan, err := LookupPlan(ctx, catalog)
	if err != nil {
		return err
	}
	value, ok := plan.Limits[limit]
	if !ok || value.Unlimited {
		// Unlimited tiers don't count usage. Doctor enforces
		// declaration so missing entries are runtime-only.
		return nil
	}
	active, err := Resolve(ctx)
	if err != nil {
		return err
	}
	return incrUsage(ctx, active.SubscriptionID, limit)
}
