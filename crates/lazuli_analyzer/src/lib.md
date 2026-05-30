Lowering from `lazuli_syntax` canonical AST slices into `lazuli_ir`.

## Role in the compile pipeline

`lazuli_analyzer` sits between `lazuli_syntax` (canonical AST) and
`lazuli_ir` (typed lowered shape). Its job is **mechanical
projection plus structural validation**: lift the parser's verbatim
AST onto the IR shape that downstream consumers (codegen, doctor,
LSP, inspect) read. Anything that needs cross-module reasoning
lives in `lazuli_cli` (the `expand` pass) or `lazuli_doctor`;
anything per-file lives here.

## Submodule layout (R3-E — rails-style refactor)

The lowering pipeline is organised into per-concern sibling
modules. Each one carries the projection rules for a single
"slot" in the vocabulary:

### Cross-cutting primitives

* [`helpers`] — pure utility predicates (case conversion, span
bridging, edit-distance, balanced-paren walkers). No AST shape,
no IR shape larger than `SpanRef`. Shared by every slice.
* [`expr`] — pure mechanical "text → IR atom" projections
(paths, qualified names, raw exprs, policy atoms, translation
keys). Every other slice calls into this slot.
* [`source_map`] — source-position bookkeeping consumed by LSP.
* [`symbol_origin`] — origin tagging (handwritten vs synthesized
vs pack-derived) used by inspect and doctor.

### Per-domain lowering (R2 — Wave 4.6)

* [`command`] — command effect cluster (`creates|updates|deletes`),
target / let / named-arg / assignment leaves, and the
`invalidates query.<name>` cross-feature reference resolver.
* [`workflow`] — async-work leaf lowerings shared by `job`,
`poller`, `webhook`, `tenant_migration`, `channel`,
`notification`, `mcp_server`, `event_group`: retry, fanout,
external-call refs, emit predicates, MCP leaves, digest /
throttle, event-variant fields, job body / trigger.
* [`lzx`] — `.lzx` *app layer* (routes, experiences, platform
surfaces). One entry point: `lower_lzx_document`.
* [`surface`] — `.lzx` *ViewModel layer* (per-feature audiences +
views + cells + drawers + route params). One entry point:
`lower_surface`.

### Per-domain lowering (R3-E)

* [`resource`] — `resource <Foo> { ... }` decl + field-level
lowering (`@cap.PII` extraction, modifier recovery,
inline-validator constraint lift, the four `validate_constraint_*`
gates) + rate-limit literal projection.
* [`query`] — `query.list` / `query.lookup` / `query.sql` lowering,
filter line parser (WAR-VOCAB-QUERY-ENUM-01), cache profile
resolution (CL.C.3), and `lower_command_input_to_typed` for
typed query/command input slots.
* [`auth`] — `auth { identity | password | sessions | mfa | oauth }`
lowering. The non-trivial bit is `<Resource>.<field>` ->
`FieldRef` splitting; the rest is structural.
* [`agent`] — LLM capability lowering: input slots, policy atom,
output projection (text|stream|enum|record-discriminator),
tool reference resolution (Adapter|Local|CrossFeature), eval
case + closed-predicate parser, HTTP expose.
* [`design`] — closed-catalog design token lowering (colors,
typography, spaces, radii, shadows, motion, breakpoints,
z-indices, custom). Cheap structural validation per group.
* [`plan_gate`] — package-wide `PlanGateFacts` aggregator
(subscription anchor + plan catalog + per-callable gates)
and the six PG diagnostic codes.
* [`lifecycle`] — resource lifecycle synthesis hooks.
* [`checks`] — public per-file structural checks invoked by
`lazuli_cli` / `lazuli_doctor`. Stays public because external
tools depend on it.
* [`rbac`] — RBAC closure construction over a feature's policies.

Per-feature orchestration (`lower_feature_skeleton`, jobs / pollers
/ webhooks / notifications / channels / event groups orchestration,
reports, conventions / CRUD synthesis, auto-photo synthesis) still
lives in this file. The per-domain leaves above are called from
there.

## Vocabulary cross-reference

Source AST shapes are defined in `lazuli_syntax::ast` (Wave 4.4).
Destination IR shapes are defined in `lazuli_ir` (Wave 4.1). When
a lowering function feels like it's "thinking" rather than just
"translating", the design pressure belongs upstream (parser
enforcement, IR shape change) — not here.

## ABI guarantee

Public items historically reachable at `lazuli_analyzer::Foo`
remain reachable at the same path. Internal helpers used across
sibling modules are `pub(crate)`.
