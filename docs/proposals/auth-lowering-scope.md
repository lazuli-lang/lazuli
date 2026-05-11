# Auth Lowering Scope (pre-design)

**Status**: pre-design investigation. Resolves a side-quest blocker
discovered during the `bucket=auth` pipeline Stage 1+2 inventory, so
Stage 3 (design-language) runs against the correct scope.

**Audience**: language team, runtime team, anyone touching the auth
bucket cycle.

**Date**: 2026-05-10.

## Erratum (2026-05-10, post-Stage 4)

This proposal originally claimed the IR shape was complete (see §Context and §Routes A vs B below). Stage 4 of the subsequent `bucket-auth-cycle` design pass (`docs/proposals/bucket-auth-cycle.md` §IR) found **two additive gaps** in the existing IR structs:

1. **`AuthPassword.algorithm` is missing** (`crates/lazuli_ir/src/lib.rs:1817-1826`). Without it, the cross-check vs `@cap.Hashed(algorithm:…)` has no IR axis to read — meaning the proposal's headline diagnostic `auth_password_algorithm_hash_mismatch` cannot be implemented against today's IR.
2. **`AuthMfa.enroll` and `AuthMfa.verify` are missing** (`crates/lazuli_ir/src/lib.rs:1839-1844`). Today's struct carries only `method` + `adapter`, but the fixture authors both `enroll @fn.enroll_customer_totp` and `verify @validator.verify_customer_totp` (`full-capsule.lzi:517-518`). MFA flows need both endpoints captured.

**Implications for the route recommendation**:

- **Route A still wins**, but the effort estimate changes: **"~2 cells of lowering"** in §"Comparison" should be read as **"~2 cells of lowering + 2 IR struct extensions"**. Both extensions are additive (no schema-breaking changes; no on-disk JSON consumer reads `Auth` yet because `Feature.auth` is always `None`).
- The implementation run must do **IR extensions before parser lowering** — not the other way around — or lowering will write fields that don't exist.
- The §"Closed-cycle criterion" remains valid as written; the gap only affects implementation ordering, not the acceptance gate.

**Where the design is now canonical**: `docs/proposals/bucket-auth-cycle.md` §IR is the source of truth for the IR shape and gaps. Treat the IR statements in this document (below) as **historical pre-investigation** — read them with the erratum applied.

## Context

The `bucket=auth` inventory cataloged 30 auth-related constructs across
the canonical fixture, IR, doctor, LSP, and registry. 14 land at L1
(diagnostics + IR + inspect coverage) but the dominant gap sits at L0:
the `auth` block inside `feature customer_auth`
(`examples/full-capsule/full-capsule.lzi:504-523`) has authored surface
syntax, has a typed IR shape defined in
`crates/lazuli_ir/src/lib.rs:1797-1853` (`Auth`, `AuthIdentity`,
`AuthPassword`, `AuthSessions`, `AuthMfa`, `AuthOAuthProvider`), and
gets a file-local LSP check
(`crates/lazuli_lsp/src/lib.rs:9424-9514`) — but **the canonical-indent
parser slice silently ignores the whole block**
(`crates/lazuli_syntax/src/parser.rs:1168-1173`). `Feature.auth` stays
`None` for every package on disk; `lazuli inspect` does not project
it; cross-feature doctor cannot reach inside; no enforced edge between
`auth password algorithm argon2id` (`full-capsule.lzi:508`) and the
`@cap.Hashed(algorithm:argon2id)` capability that the corresponding
field `CustomerSession.refresh_token_hash` declares
(`full-capsule.lzi:451`).

The Stage 1+2 inventory's recommendation was not to propose new auth
primitives until the lowering decision for the existing block is made.
Adding `service_account`, `passkeys`, `device_session` ahead of that
locks the new constructs into the same dead-on-arrival state. This
proposal resolves the lowering route first, then names the primitive
subset that justifies a design pass.

## Why `parse_feature_skeleton` is scoped to `agent`

The canonical-indent parser slice in `lazuli_syntax` was introduced as
Phase 1 of Cut A (commit `d2a6202` — *Cut A — Phase 1: AST + parser
slice for `agent`*). Its commit message states the intent plainly:

> Adds a hand-written canonical-indent slice to `lazuli_syntax` that
> parses `feature <name>` headers and the indented `agent <name>`
> blocks inside them. Every other feature child (resources, commands,
> queries, workflows, ...) is silently skipped — the legacy pest
> pipeline still owns those until later cuts migrate.

The slice landed deliberately narrow. Reasons recoverable from the
record:

1. **Bounded risk for a load-bearing surface.** Cut A introduced
   `agent`, `tools`, discriminated `output`, and `evals` simultaneously.
   Generalising the slice to all feature children at the same time
   would have entangled the agent design with a parser migration risk
   the proposal explicitly avoided
   (`docs/proposals/ai-primitives-v0-implementation.md`).
2. **Established text-pattern bridge.** When Cut A.9 (`approval`) and
   Cut A.8 (`agent_run` trace subscribers) needed cross-checks against
   constructs not in the slice (commands, event subscribers), they used
   text-pattern facts — `CommandApprovalFact`
   (`crates/lazuli_cli/src/doctor.rs:4025-4155`) — instead of extending
   the slice. `docs/design-decisions.md:405-410` documents the bridge:
   *"Captured via text-pattern facts (`CommandApprovalFact`) until the
   canonical-indent slice covers commands."*
3. **Phase L explicitly backlogged.** `docs/next-checklist.md:60` row
   24 names the migration: *"Phase L — canonical-indent slice covers
   commands/resources/queries/records. The Cut A parser slice covers
   `agent` only. Commands (used by Cut A.9 `approval`) + resources/
   records (used by Cut A discriminator records) live in text-pattern
   doctor facts until the slice generalises. No ETA; tracked as a
   tech-debt cut."*

The criterion documented for promotion is implicit: extend the slice
when text-pattern friction (multi-line block harvesting, indent-
sensitive children, cross-feature symbol tables) starts costing more
than the upfront parser work. No row in `docs/next-checklist.md`
currently triggers that promotion for `auth`.

## Routes A vs B

Two ways to close the auth lowering gap, both honouring the
Lazuli/Drusa boundary:

### Route A — extend canonical-indent slice to include `auth`

Add `auth` recognition to `parse_feature_skeleton`
(`crates/lazuli_syntax/src/parser.rs:1147-1173`) plus a `parse_auth`
function mirroring `parse_agent`. Add `auth: Option<Auth>` to
`FeatureSkeleton` (`crates/lazuli_syntax/src/ast.rs:218`), wire it
through `lower_feature_skeleton` into the existing
`crates/lazuli_ir/src/lib.rs:1797-1853` IR struct (which already
exists). Inspect projection is mechanical because the IR shape is
already serializable.

### Route B — text-pattern fact extraction (the `CommandApprovalFact` shape)

Add `collect_feature_auth_facts` in `crates/lazuli_cli/src/doctor.rs`
next to `collect_command_approvals`
(`crates/lazuli_cli/src/doctor.rs:4046-4155`), harvesting an
`AuthFact` with the same indent-walking idiom. Surface a
`registry_auth_defects: Vec<AuthFact>` slot on `DoctorPackage` (mirror
of `registry_tool_defects`) and emit cross-feature diagnostics from
the harvested facts. Inspect projection stays text-derived (or
unimplemented for now).

### Comparison

| Axis | Route A (slice) | Route B (text-pattern) |
|---|---|---|
| Upfront cost | ~2 cells of lowering (auth header + 4 child blocks). Smaller than the `agent` slice because the IR struct already exists and there are no nested predicate languages. | ~1 cell (one `collect_*` walker + diagnostic emission). Smallest possible patch. |
| Maintenance cost | One canonical home for the shape; any future kid (recovery_codes, webauthn) extends the `parse_auth` recogniser. | Each new child requires editing the regex/walker. Hot zone for silent drift if the surface evolves. |
| Cross-checks possible | All cross-checks doctor needs: `auth password algorithm` vs `@cap.Hashed(algorithm:...)` on referenced fields; `auth sessions resource <X>` vs an actual resource declaration in the same feature; OAuth adapter slot resolution against `registry.integrations`. Each runs against typed IR. | Same set of cross-checks, but bound to text patterns. Brittle for the OAuth adapter resolution because the registry side is already typed (`crates/lazuli_cli/src/app_manifest.rs`) — text-pattern bridges typed-on-one-side, untyped-on-other diagnostics, which is what Phase L is meant to eliminate. |
| LSP coverage | Slice produces typed AST; LSP gains hover, completion, and shape diagnostics for `auth` children automatically (same path as agent). | Stays at the current file-local text-walk level (`crates/lazuli_lsp/src/lib.rs:9424-9514`). No hover or completion on `auth` children. |
| Compat with Phase L | Aligned — promotes one more block out of text-pattern territory and shrinks the backlog row 24 surface area. | Misaligned — adds a third text-pattern fact family (after `CommandApprovalFact` and the `collect_feature_symbols` walker), increasing the eventual Phase L migration debt. |
| Risk | Localized to lowering; the existing IR struct constrains the shape. The slice's parse-then-skip-others contract is preserved (`auth` is added next to `agent` at the same indent). | Bridge code in doctor accumulates; small risk of drift between LSP's text-walk and doctor's text-walk if both grow. |

### Recommendation

**Route A.** The IR struct already exists, the parsing surface is
small (one new block, four named children with closed grammars), and
the cross-checks the auth bucket needs are doctor-cross-feature,
which is exactly what the slice was designed to enable. Route B
exists as the established bridge for constructs whose IR shapes are
not yet defined — that is not the case for `auth`. Choosing Route B
here would mean introducing text-pattern facts for a construct whose
IR has been defined since the original auth Phase 1e
(`crates/lazuli_ir/src/lib.rs:1789-1795` block comment), which is a
strict regression of the established direction.

Route A also matches the boundary discipline: every new `auth` child
the design phase introduces (passkeys, recovery codes, etc.) extends
typed IR directly, not the text-pattern harvester, keeping Drusa's
codegen consumer surface stable.

## Pilot-needed vs Speculative

The 10 "missing" constructs in the Stage 1 inventory map to roadmap
§1.8 (`docs/roadmap.md:115-122`). Classified against fixture evidence
and the §0 ciclo L0→L2 pilot bucket
(`docs/roadmap.md:23-45` — the auth/session bucket is bucket-piloto
#1):

### PILOT-NEEDED — exercised by the canonical fixture today

| Construct | Fixture evidence | Justification |
|---|---|---|
| `auth password` block | `full-capsule.lzi:507-511` | Already authored; needs lowering. The whole reason this proposal exists. |
| `auth oauth <provider>` | `full-capsule.lzi:513-514` | Authored with `adapter @adapter.google_oauth`. Needs lowering and cross-check against `registry.integrations` / extension adapter declarations. |
| `auth mfa totp` | `full-capsule.lzi:516-518` | Authored; references `@fn.enroll_customer_totp` and `@validator.verify_customer_totp`. Needs lowering plus cross-check that the named extensions exist. |
| `auth sessions` + bound resource | `full-capsule.lzi:520-523`, `full-capsule.lzi:449-455` | Authored with `resource CustomerSession`, `ttl "7 days"`, `refresh false`. Needs lowering plus cross-check that the named resource exists in the same feature and carries `@cap.Hashed` on its refresh-token field. |
| `auth identity <Resource.field>` | `full-capsule.lzi:505` | Authored; needs lowering + cross-check that the field exists, is unique-shaped, and carries `@semantic.Email` or a comparable identity-marker capability. |
| `auth password rate_limit` | `full-capsule.lzi:511` | Already LSP-checked file-local (`crates/lazuli_lsp/src/lib.rs:9492-9501`). The slice promotes this to doctor cross-feature. |

### SPECULATIVE — not in the fixture; defer until a real pilot exercises them

| Construct | Status | Why defer |
|---|---|---|
| `magic_link` flow as a kind | Fixture has it as an `AuthProvider` enum value (`full-capsule.lzi:447`) but the `auth` block does not declare a `magic_link` child. No flow shape, no token rotation, no rate-limit contract authored. | Pilot needed: a product that actually issues, validates, and expires magic-link tokens. Today's enum value is naming-only; promoting to a kind without pilot pressure invents shape. |
| `passkeys` / `webauthn` kind | Not in fixture; not in any proposal; only referenced in `docs/capability-layering.md:225` as language-light + pack territory. | Pilot needed: a product that authors a passkey enrolment flow. The protocol surface (authenticator attachment, attestation, RP id binding) is wide; designing it without pilot pressure produces speculative shape. |
| `saml` / `ldap` kind | Not in fixture; appears only in roadmap §1.8 and the framework-coverage audit. | Pilot needed: enterprise-tier pilot with directory federation. SAML especially is a metadata-driven contract that does not lower cleanly from indented syntax alone; the design needs evidence to settle. |
| `service_account` kind | Not in fixture; framework-coverage audit lists it among 1.8. | Pilot needed: a product authoring non-interactive credentials. Today's `@cap.Token` carries TTL/single-use/store contract; `service_account` would have to declare scope, rotation, and revocation semantics on top — not designable without pilot. |
| `api_token` kind (amplified `@cap.Token`) | `@cap.Token` exists as a capability decorator (`docs/invariants.md:408`) but no kind-level construct lists scopes / rotation / revocation as first-class. | Pilot needed: a product issuing user-scoped tokens with rotation policy. Promotion candidate, not pre-pilot. |
| `impersonation` kind | Not in fixture; only in roadmap §1.8 / §1.9. | Pilot needed: a product with admin impersonation flows. The audit edges (every command becoming `acting_as.actor_id`) need real product pressure to settle. |
| `device_session` kind | Not in fixture; only in audit and roadmap. | Pilot needed: a product modelling per-device session metadata. Today's `auth sessions resource <X>` plus a custom resource carries the same contract without a new kind. |
| `mfa recovery_codes` | Not in fixture; `auth mfa totp` is the only mfa method authored. | Pilot needed: a product that authors backup-code flows. Today's `auth mfa` shape would need an alt-method list, not just a single method. |
| `audit_log` kind | Roadmap §1.9. Today's `audit` child on commands plus `event.trace` covers the audit surface already. | Pilot needed: a product where the per-command `audit` declaration cannot express the contract (e.g., cross-actor or cross-feature audit aggregation). Until that pressure surfaces, the existing primitive is sufficient. |
| `auth_failed_redirect` (already shipped) | `examples/full-capsule/app.lzi:9` | Not missing — already typed in `crates/lazuli_ir/src/lib.rs:1084` and cross-checked at `crates/lazuli_cli/src/doctor.rs:1444`. Listed in inventory by mistake. |

The pilot-needed subset is exactly the children the existing `auth`
block already authors. Speculative additions wait for §0 bucket-piloto
auth/session to surface real authoring pressure.

## Closed-cycle criterion for the auth bucket

Adapted from `docs/roadmap.md:34-43` (8-item §0 checklist) to the
specific shape of bucket-piloto #1:

- [ ] **Fixture authors the full surface.** The canonical fixture
  exercises `auth password + oauth + mfa + sessions`
  (`full-capsule.lzi:504-523`). Already true. Any new primitive added
  by Stage 3 design must extend this without breaking it.
- [ ] **`lazuli check` accepts the syntax.** Already true — the legacy
  pipeline accepts it; the slice will too once Route A lands.
- [ ] **`lazuli inspect --expand=auth` projects the IR.** New projection;
  required deliverable. Uses the existing `Auth` IR struct
  (`crates/lazuli_ir/src/lib.rs:1797-1853`) directly.
- [ ] **`lazuli doctor` carries ≥3 cross-feature diagnostics for auth.**
  Concrete proposals:
  - `auth_password_algorithm_hash_mismatch_diagnostics` — `auth password
    algorithm <X>` does not match `@cap.Hashed(algorithm:<Y>)` on the
    `refresh_token_hash`-shaped field of the session resource.
  - `auth_sessions_resource_unknown_diagnostics` — `auth sessions
    resource <X>` refers to a resource that does not exist in the
    feature.
  - `auth_identity_field_unknown_diagnostics` — `auth identity
    <Resource.field>` does not resolve.
  - `auth_oauth_adapter_unbound_diagnostics` — `oauth <provider> adapter
    @adapter.<x>` does not resolve to a declared `extension adapter` or
    `registry.integrations` entry.
- [ ] **`lazuli generate` produces Go that compiles.** Runtime-team
  deliverable (parallel Drusa work). Consumed via stable IR JSON
  through `lazuli inspect --format=json --expand=auth`.
- [ ] **Drusa executes end-to-end login + mfa + oauth.** Runtime-team
  deliverable. Outside language scope.
- [ ] **`eval`/test coverage.** Either Lazuli-native `eval` cases on the
  `login` / `enable_mfa` commands (`full-capsule.lzi:525-545`) or Go
  integration tests in Drusa. Boundary point: if `eval` extends to
  auth flows, design that delta in a separate proposal.
- [ ] **LSP hover/completion on auth children.** Today the LSP carries
  shape-only diagnostics (`crates/lazuli_lsp/src/lib.rs:9424-9514`).
  Hover + completion on `auth password algorithm <X>` (closed catalog:
  `argon2id`, `bcrypt`) is a small additive deliverable once Route A
  lands.

The first four items are language-team Stage 3 deliverables. Items
5-7 are Drusa-team. Item 8 is language-team but small (LSP catalog
extension).

This list is attainable in a single Stage 3 design cut once Route A
lands; nothing on it depends on speculative primitives.

## Recommendation

1. **Take Route A** (extend the canonical-indent slice to cover the
   `auth` block). Estimated scope: ~2 cells of lowering, mechanical
   because the IR is pre-existing. Add `auth: Option<Auth>` to
   `FeatureSkeleton`, add `parse_auth` in `parser.rs` next to
   `parse_agent`, wire through lowering.
2. **Scope Stage 3 design to the PILOT-NEEDED subset only.** Six
   children — `identity`, `password`, `oauth`, `mfa totp`, `sessions`,
   `password rate_limit` — already in the fixture. Stage 3's job is to
   tighten the contract (closed catalogs, cross-checks, doctor
   diagnostics, LSP hover), not invent new kinds.
3. **Defer SPECULATIVE additions** until the bucket cycle surfaces real
   pilot pressure. `passkeys`, `saml`, `ldap`, `service_account`,
   `device_session`, `impersonation` are catalog noise without a pilot
   exercising them. The roadmap §1.8 list stays as is; this proposal
   does not promote any of those items.
4. **Run Stage 3 with the closed-cycle criterion above as the
   acceptance gate.** Anything that doesn't shrink the gate counts as
   speculative and goes to backlog.
5. **Update `docs/next-checklist.md` row 24** (`Phase L`) only after
   Route A lands, to reflect that `auth` joins `agent` in the slice's
   coverage. Do not edit row 24 as part of this proposal.

When Route A is implemented, Stage 3 (design-language) runs on the
shipped substrate and produces a focused proposal covering at most the
four doctor diagnostics named in the closed-cycle criterion plus the
`--expand=auth` projection. Stage 4 (Drusa codegen) then has a stable
IR JSON to consume.
