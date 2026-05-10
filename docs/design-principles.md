# Lazuli Design Principles

This document captures design constraints that are stronger than individual
syntax choices. Treat it as project memory: when a future change conflicts with
these principles, the change needs a deliberate design decision, not a local
convenience argument.

## Rule Zero: Vocabulary Over Mechanism

Lazuli grows by adding shared vocabulary, not user-defined mechanism.

Good growth gives the language a new concept with fixed semantics, static
checks, inspect output, and generated behavior: `workflow`, `event_group`,
`audience`, `@cap.Token`, or a future `collection`.

Bad growth gives each project a mechanism to invent its own dialect:
`template`, macro, mixin, partial override, cascade, or project-local sugar.
Those mechanisms look efficient early and become unreadable once every project
has a different convention.

When repetition appears in real source, ask:

1. Which product/runtime concept is repeating but lacks a name?
2. Can the compiler infer the repetition safely without hiding semantics?
3. Is this repetition honest local declaration that should remain explicit?

Do not answer repetition with a generic abstraction by default.

## Self-Contained Declarations

Declarations are local truths. A reader should understand what a block means by
reading that block plus its explicit typed imports.

Imports are controlled exceptions: a `.lzx` experience may import `.lzi`
features, and a platform surface may use an abstract experience. Cascades are
not controlled exceptions because the final state exists only after mentally
merging layers.

## Operational Systems First

Lazuli is optimized for traditional business software where correctness,
auditability, permissions, workflow, and long-lived evolution matter more than
bespoke interaction mechanics.

Strong fits include SaaS back offices, CRMs, heavy ERPs, inventory, billing,
procurement, approvals, compliance surfaces, and mobile companion apps for
operational workflows. These products are full of repeated contracts that
benefit from static vocabulary: tenant axes, company/branch scope, command
policies, document lifecycles, approval transitions, audit trails, jobs,
webhooks, reports, and integration boundaries.

ERP is a pressure test, not a namespace. When ERP exposes a reusable
operational invariant, name the generic contract (`retention`, `write_window`,
policy/rule separation) instead of adding vertical syntax such as
`fiscal_period` or `chart_of_accounts` to the core language.

Lazuli should not try to own the singular mechanics of games, media engines,
visual editors, realtime canvases, or highly custom creative tools. Those can
still use Lazuli for the operational shell around them: auth, billing, orgs,
admin, jobs, events, and integration contracts.

Use `docs/capability-layering.md` for the standing boundary between Lazuli
language primitives, the Lazuli compiler, the runtime packs, runtime, and
adapters.

## Total Override Only

Overrides are whole-block replacements, never partial diffs.

Valid:

```lazuli
audience admin tenant acme
  view list Table
    source customer.query.list
    columns name, email, tier, score, account_manager
    actions reassign, archive
```

Invalid:

```lazuli
audience admin tenant acme
  view list
    columns += account_manager
```

The repeated columns are intentional. They make the tenant-specific view a
complete statement of what that audience sees.

## No Cascade

Lazuli does not use CSS-style specificity, inheritance chains, or base-plus-diff
composition for product semantics. If a user, tenant, or platform gets different
fields, actions, or flows, declare the resulting view explicitly.

## Layer Direction

Source layers point in one direction:

```txt
.lzi              domain/capability contract
  ^
  |
.lzx              abstract experience/view model
  ^
  |
.<platform>.lzx   concrete web/mobile projection
```

`.lzi` never knows that UI exists. Abstract `.lzx` knows the `.lzi` capabilities
it imports. Concrete `.web.lzx` and `.mobile.lzx` project an abstract
experience; they do not call domain features directly unless a future escape
hatch explicitly says so.

## Technical Axis In Filename, Product Axis In Syntax

Protected platform suffixes are technical compound suffixes: `.web.lzx` and
`.mobile.lzx`. The protected platform segment stays immediately before `.lzx`.

Product axes live in source syntax: `audience admin`, `tenant acme`, and future
first-class product axes if they earn their place. Do not create semantic file
suffixes such as `.admin.lzx` as language features. A physical split such as
`customer.public.web.lzx` may help organization, but the header remains the
truth the compiler validates and the file still ends in `.web.lzx`.

## File Name Organizes, Header Decides

File names provide convention and editor ergonomics. The source header decides
semantics.

For example, `customer.web.lzx` should contain `surface customer web`, but the
compiler reasons from the header. A mismatch is a diagnostic; the file name is
not a hidden import or configuration channel.

## Experiments Are Out Of V0

Experiments want runtime assignment, telemetry, attribution, and often partial
diffs. Those pressures conflict with the no-cascade rule. Keep experiments out
of v0 surface semantics; integrate external experimentation systems later with
explicit runtime context and generated telemetry contracts.

## Theme Is Not Semantics

Theme/skin changes that only affect visual presentation belong in design tokens
or target adapter configuration. If a "theme" changes fields, actions,
navigation, or data exposure, it is not a theme; model it as an audience/tenant
product variant with a complete declaration.
