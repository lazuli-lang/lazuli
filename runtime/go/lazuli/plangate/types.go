// Package plangate carries the wire types for plan-gate directives
// (`gate behind plan.feature: <name>` / `gate quota plan.limit:
// <name>` lowered from the DSL).
//
// The package has zero dependencies so every runtime contract —
// `lazuli.Query`, `lazuli.Api`, `jobs.JobContract`,
// `webhooks.WebhookContract` — can carry a `Prelude []GateRef`
// field without inducing a `lazuli ↔ jobs ↔ webhooks ↔ billing`
// import cycle.
//
// The `billing` package aliases the types back as
// `billing.GateRef` / `billing.GateKind` / `billing.GateBehind` /
// `billing.GateQuota` so historical spellings keep working in
// generated code and authored handlers.
package plangate

// GateKind is the closed enum of v0.1 gate axes mirrored from
// `docs/proposals/plan-and-gate-vocab.md`.
type GateKind uint8

const (
	// GateBehind corresponds to `gate behind plan.feature: <name>`.
	GateBehind GateKind = iota
	// GateQuota corresponds to `gate quota plan.limit: <name>`.
	GateQuota
)

// GateRef is the codegen-emitted record describing one gate
// directive on a callable. Carried on every gated runtime contract
// (`Query.Prelude`, `Api.Prelude`, `JobContract.Prelude`,
// `WebhookContract.Prelude`) and on the introspection map
// `plan.GatedCallables`.
type GateRef struct {
	Kind GateKind
	Name string
}
