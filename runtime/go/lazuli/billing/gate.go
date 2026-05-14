package billing

import "lazuli.dev/runtime/lazuli"

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
