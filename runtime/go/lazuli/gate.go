// Plan-gate runtime types live in the leaf package `plangate` so
// every runtime contract — `lazuli.Query`, `lazuli.Api`,
// `jobs.JobContract`, `webhooks.WebhookContract` — can carry a
// `Prelude []GateRef` slot without inducing the
// `lazuli → jobs → … → lazuli` import cycle.
//
// This file aliases the leaf types into the `lazuli` package and
// exposes the dispatcher-side hooks `RunPrelude` / `RunIncrement`.
// The `billing` package installs the runner implementations at
// init.
package lazuli

import "lazuli.dev/runtime/lazuli/plangate"

// GateKind is the closed enum of v0.1 gate axes.
type GateKind = plangate.GateKind

// GateRef is the codegen-emitted record describing one gate
// directive on a callable.
type GateRef = plangate.GateRef

// GateBehind / GateQuota mirror the leaf-package constants.
const (
	GateBehind = plangate.GateBehind
	GateQuota  = plangate.GateQuota
)

// PreludeRunner is the package-level hook the `billing` package
// installs at init. Dispatchers call `RunPrelude(ctx, contract.Prelude)`
// before invoking the user handler; the implementation evaluates
// every behind-gate (CheckFeature) then every quota-gate
// (CheckQuota) against the registered plan catalog. Returns nil for
// an empty prelude (no-op).
var PreludeRunner func(ctx *Ctx, prelude []GateRef) error

// IncrementRunner is the post-success hook the `billing` package
// installs at init. Dispatchers call it after a successful handler
// returns so quota-gate counters advance. Failures are non-fatal
// (logged by the runtime; reconciliation jobs heal drift).
var IncrementRunner func(ctx *Ctx, prelude []GateRef) error

// RunPrelude is the dispatcher-side entry point. Returns nil when
// the prelude is empty or no runner is registered (the runtime
// behaves as if the gate did not exist when billing is not wired,
// preserving backward compatibility for callables without gates).
func RunPrelude(ctx *Ctx, prelude []GateRef) error {
	if len(prelude) == 0 || PreludeRunner == nil {
		return nil
	}
	return PreludeRunner(ctx, prelude)
}

// RunIncrement is the dispatcher-side post-success hook. Returns
// nil when the prelude is empty or no runner is registered.
func RunIncrement(ctx *Ctx, prelude []GateRef) error {
	if len(prelude) == 0 || IncrementRunner == nil {
		return nil
	}
	return IncrementRunner(ctx, prelude)
}
