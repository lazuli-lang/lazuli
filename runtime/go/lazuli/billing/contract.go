// Package billing implements the Lazuli runtime contract for the
// Plan & Gate vocabulary (docs/proposals/plan-and-gate-vocab.md).
//
// Wire-thin: every file here is < 100 effective LOC, with no
// reimplementation of subscription state machines, plan transitions,
// or webhook protocols. Provider integrations (Stripe, MercadoPago,
// Pagar.me, etc.) live in `@plugin/<provider>` repos and implement
// SubscriptionStore. The default postgres store uses the same pgx pool
// as the rest of the Lazuli runtime.
package billing

import (
	"context"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/plangate"
)

// PlanCatalog is the closed, package-wide set of plans lowered from
// `plan ...` blocks in `app.lzi` / `registry.lzi`.
type PlanCatalog struct {
	Plans  map[string]Plan
	Anchor SubscriptionAnchor
}

// Plan is one declared tier.
type Plan struct {
	Name     string
	Features FeatureSet
	Limits   map[string]LimitValue
	Trial    *TrialPolicy
}

// FeatureSet is a small typed wrapper around a presence-only map.
type FeatureSet map[string]struct{}

// NewFeatureSet constructs a FeatureSet from a string list.
func NewFeatureSet(features ...string) FeatureSet {
	out := make(FeatureSet, len(features))
	for _, f := range features {
		out[f] = struct{}{}
	}
	return out
}

// Has reports whether the set contains the named feature.
func (f FeatureSet) Has(name string) bool {
	_, ok := f[name]
	return ok
}

// LimitValue is `unlimited` or a positive integer.
type LimitValue struct {
	Unlimited bool
	Value     uint64
}

// LimitInt constructs a finite limit.
func LimitInt(v uint64) LimitValue { return LimitValue{Value: v} }

// LimitUnlimited is the singleton "no limit" value.
var LimitUnlimited = LimitValue{Unlimited: true}

// TrialPolicy declares the trial-revert contract.
type TrialPolicy struct {
	Duration time.Duration
	ThenPlan string
}

// SubscriptionAnchor names where the runtime finds the active
// subscription. Mirrors `app.lzi subscription resource <feature>.<field>`.
type SubscriptionAnchor struct {
	OwnerFeature   string
	OwnerResource  string
	Field          string
	TargetResource string
	TenancyAxis    string
}

// ActiveSubscription is the resolved state for one caller's
// subscription at request time.
type ActiveSubscription struct {
	SubscriptionID lazuli.ID
	PlanName       string
	Status         string // "active" | "trialing" | "expired" | "cancelled"
	ExpiresAt      time.Time
	StartedAt      time.Time
	TrialEndsAt    *time.Time
}

// SubscriptionStore is the adapter seam. The default postgres store
// reads from the table named by the anchor; hosted-billing adapters
// (Stripe, MercadoPago) implement this interface from their own
// `@plugin/<provider>` repos.
type SubscriptionStore interface {
	LookupActive(ctx context.Context, subjectID lazuli.ID) (*ActiveSubscription, error)
}

// store is the process-global store registered via Register. Default
// adapter wiring happens in `Register(...)` from the codegen-emitted
// boot sequence.
var store SubscriptionStore

// Register installs the active SubscriptionStore adapter. Wired by
// the generated `dist/go/main.go` boot path; tests substitute via the
// same hook.
func Register(s SubscriptionStore) { store = s }

// Resolve fetches the ActiveSubscription for the caller in ctx via
// the registered store. Returns ErrPlanLookupFailed when no store is
// registered or the store errors.
func Resolve(ctx *lazuli.Ctx) (*ActiveSubscription, error) {
	if store == nil {
		return nil, ErrPlanLookupFailed{Reason: "no SubscriptionStore registered"}
	}
	if ctx.User == nil {
		return nil, ErrPlanLookupFailed{Reason: "no authenticated caller"}
	}
	return store.LookupActive(ctx, ctx.User.ID)
}

// LookupPlan resolves the caller's active Plan via the registered
// store + catalog. Trial revert is delegated to the store; this
// function does the catalog-name -> Plan join only.
func LookupPlan(ctx *lazuli.Ctx, catalog PlanCatalog) (*Plan, error) {
	active, err := Resolve(ctx)
	if err != nil {
		return nil, err
	}
	plan, ok := catalog.Plans[active.PlanName]
	if !ok {
		return nil, ErrPlanLookupFailed{
			Reason: "active plan `" + active.PlanName + "` not in catalog",
		}
	}
	return &plan, nil
}

// ErrPlanFeatureForbidden is returned when `gate behind plan.feature`
// refuses dispatch (402, code `plan.feature_forbidden`).
type ErrPlanFeatureForbidden struct {
	Plan    string
	Feature string
}

func (e ErrPlanFeatureForbidden) Error() string {
	return "plan `" + e.Plan + "` does not include feature `" + e.Feature + "`"
}

// ErrPlanQuotaExceeded is returned when `gate quota plan.limit`
// refuses dispatch (402, code `plan.quota_exceeded`).
type ErrPlanQuotaExceeded struct {
	Plan  string
	Limit string
	Used  uint64
	Max   uint64
}

func (e ErrPlanQuotaExceeded) Error() string {
	return "plan `" + e.Plan + "` exceeded limit `" + e.Limit + "`"
}

// ErrPlanLookupFailed is returned when the subscription cannot be
// resolved (503, code `plan.lookup_failed`).
type ErrPlanLookupFailed struct {
	Reason string
}

func (e ErrPlanLookupFailed) Error() string {
	return "plan lookup failed: " + e.Reason
}

// GateRef is the codegen-emitted record describing one gate directive
// on a callable. The generated `dist/go/plan/catalog.gen.go` exposes
// a `GatedCallables` map of these so debug / introspection layers can
// enumerate every gated callable in the package.
//
// Defined in the leaf `plangate` package (see
// `runtime/go/lazuli/plangate/types.go`) so every runtime contract —
// `lazuli.Query`, `lazuli.Api`, `jobs.JobContract`,
// `webhooks.WebhookContract` — can carry a `Prelude []GateRef`
// field without cycling through `billing → lazuli → billing`.
// Aliased here so legacy spellings (`billing.GateRef{...}`,
// `billing.GateBehind`) keep compiling.
type GateRef = plangate.GateRef

// GateKind is the closed enum of v0.1 gate axes.
type GateKind = plangate.GateKind

const (
	// GateBehind corresponds to `gate behind plan.feature: <name>`.
	GateBehind = plangate.GateBehind
	// GateQuota corresponds to `gate quota plan.limit: <name>`.
	GateQuota = plangate.GateQuota
)
