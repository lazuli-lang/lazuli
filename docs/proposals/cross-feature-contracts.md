# Proposal — Cross-Feature Contracts (Boundary-Preserving References)

**Status:** L0 v0.2 DRAFT — 2026-05-16 (v0.1 graded 6.92/10 BLOCK via `lazuli-language-architect`; six structural blockers all about phantom anchors — `split_services`, `aggregate`, `external_call` surface syntax, brace `previously`, `coordinator:`, plus a runtime-mechanics boundary leak — fixed in v0.2)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Driver:** Cross-feature references lack contract metadata; the `monolito → microserviços é uma flag, não uma reescrita` promise is currently a half-truth.
**Honors:** `docs/architecture.md` §"What we learned from a previous attempt", `docs/design-principles.md` Rule Zero, `docs/invariants.md`, `docs/capability-layering.md` §Pipeline + §Decision Test (the language-vs-runtime split rule), `feedback_normative_not_narrative_2026-05-15`, `feedback_cement_over_ship_until_users_2026-05-15`.

---

## §1. Status & motivation

### §1.1 The gap

The closed `architecture_mode` enum at `docs/grammar.app.md:121-122` admits three values:

```
architecture_mode = "monolith" | "modular_monolith" | "microservices"
```

`workspace.lzi` already declares **event** boundaries explicitly:

```lazuli
workspace FullCapsule
  apps
    crm at "./app.lzi"
  boundaries
    crm publishes customer.*
    ai consumes customer.*
```

(`examples/full-capsule/workspace.lzi:8-10`)

`services` blocks in `app.lzi` declare per-service ownership + publish/consume contracts (`examples/full-capsule/app.lzi:53-69`).

But there is no equivalent for **types**, **query return shapes**, **command input shapes**, or **event payloads** that cross feature boundaries. When `account` declares `enum Gender` and `host.lzi` declares `field gender: Gender required` (resolved via `uses account`), the IR carries no record that `Gender` crosses a boundary. In `monolith` or `modular_monolith` mode this is harmless — `Gender` is one type in one binary. In `microservices` mode the implicit resolution becomes:

- **Shared package at build time:** consumer service imports the type from the origin's generated module. Both services share binary code. Mutating `Gender` in `account` forces a coordinated release. Boundary is dead.
- **Replicated schema:** consumer carries its own copy at a pinned version. Mutating in `account` requires explicit consumer migration. Boundary preserved at the cost of explicit coordination.

Lazuli today picks the shared-package path implicitly because codegen targets one binary. The implicit choice is what breaks the promise: any project that flips `architecture mode microservices` discovers, on its first cross-feature type reference, that the framework operated as a monolith disguised by `services` blocks.

### §1.2 What's already explicit

| Construct | Cross-feature explicit? | Where |
|---|---|---|
| `event <feature>.<event>` | Yes | `workspace.lzi boundaries`; `services publishes/consumes` (`examples/full-capsule/app.lzi:59-69`) |
| `services owns / exposes` | Yes | `examples/full-capsule/app.lzi:53-58` |
| `requires integration <slot>: <Capability>` | Yes (capability-typed adapter slot) | `docs/invariants.md:71-87` |

### §1.3 What's silently implicit (the inventory)

Five classes — narrowed from v0.1's eight after dropping items that either don't have surface syntax yet (aggregate) or fold into existing language mechanisms (cross-feature handlers via `requires integration`; cross-feature workflows via analyzer-detection only). §3 catalogs the five.

### §1.4 Why now

Hostpoint (active pilot per memory `project_strategic_pivot_2026-05-15`) runs in `modular_monolith`, so no immediate breakage. But every pilot inherits the implicit-boundary semantics. The first project to flip `architecture mode microservices` discovers the gap. Cementing the invariant now is cheap; retrofitting after three projects ship is expensive (per `feedback_cement_over_ship_until_users_2026-05-15`).

---

## §2. Scope

**This proposal is boundary-cementing, not boundary-moving.** It does not introduce a new architecture mode, deployment topology, or vocabulary class. It surfaces the language-level reference contract that today lives implicit-in-codegen.

### In scope

1. **The invariant** (§4) — in any capsule that may transition to `microservices`, cross-feature references must be expressible as contracts.
2. **`public contract` declaration on origin types** (§5) — one new compound keyword pair gating the cross-feature reference under `microservices` mode.
3. **Architecture-mode resolution rules** (§6) — how `uses account` translates per mode. Existing capsules under `monolith` / `modular_monolith` compile unchanged.
4. **Doctor diagnostics** (§7) — two rules, both gated on `architecture mode microservices`.
5. **Five-class inventory** (§3).
6. **Migration story** (§8).

### Non-goals

1. **No new `architecture_mode` value.** The closed enum at `docs/grammar.app.md:121-122` (`monolith | modular_monolith | microservices`) is honored; this proposal binds language semantics to the existing `microservices` value, not a new one.
2. **No schema registry implementation.** Whether contracts vendor via Buf / Confluent / Lazuli-native is a **runtime/adapter decision** per `docs/capability-layering.md` §Pipeline + §Decision Test (the language-vs-runtime split rule). Out of scope.
3. **No codegen specification.** This proposal cements the **doctor enforcement** as the load-bearing artifact. The runtime decides how vendored types travel (transport, codec, versioning at the wire) — that lives in a future runtime-side proposal.
4. **No deployment automation.**
5. **No breaking change to existing capsules.** Every project that compiles today under `monolith` / `modular_monolith` must continue to compile after this lands. The new vocabulary is **opt-in until `architecture mode microservices` is flipped**.
6. **No runtime resolution.** Cross-feature contract violations fire at **compile/doctor time**.
7. **No new IR top-level type.** Contract metadata lives as additive `Option<PublicContract>` fields on existing IR types (`EnumDecl`, `Resource`, `Record`, `Command`, `Query`). Additive, `#[serde(default)]`. Per `docs/invariants.md` closed-grammar discipline.
8. **No vocabulary for cross-app references.** `workspace.lzi boundaries` already handles app-level. This proposal addresses feature-to-feature within an app under `microservices`.
9. **No aggregate cross-feature rules.** `aggregate` isn't surface syntax in the current grammar (`grammar.lzi.md` has no `aggregate_block` production). When aggregates eventually ship as a primitive, their cross-feature semantics enter via that proposal.
10. **No workflow `coordinator:` keyword.** Workflows already declare `policy | emits | transition_decl` per `grammar.lzi.md:506-509`. Cross-feature workflow detection becomes an analyzer concern (walking `creates/updates/deletes` against feature ownership) without new syntax.
11. **No `external_call` surface keyword.** `ExternalCallRef` (`crates/lazuli_ir/src/lib.rs:1010, 3853`) is **analyzer-derived** from `requires integration` slots + registry bindings (`docs/invariants.md:71-87`). Cross-feature `@fn.<name>` resolution under `microservices` reuses that machinery — it does NOT add author-side syntax.

---

## §3. Inventory — five classes of cross-feature reference

Each class is audited from `docs/grammar.lzi.md` and the fixture at `examples/full-capsule/full-capsule.lzi`. Each carries the same shape under `microservices`: the **origin** site declares `public contract`; the **consumer** site is unchanged.

### §3.1 Type references — types/enums/records as field types

`field gender: Gender required` where `Gender` is declared in `account` resolves via `uses account` to `account.Gender`. Doctor under `microservices`: `Gender` in origin must carry `public contract`.

Examples in full-capsule: `customer.lzi:25` (`uses org, user, billing`); `field organization: Org required unique` (cross-feature ref to `Org`).

### §3.2 Query return shapes — typed returns crossing features

`host.query.recent_bookings returns Customer[]` where `Customer` lives in `customer` feature. The result crosses the wire under `microservices`. Doctor requires `Customer` to carry `public contract` in `customer`.

### §3.3 Command input shapes — typed inputs from another feature

`command create_booking { input { user: User required, listing: Listing required } }` where `User` lives in `account` and `Listing` in `host`. The input shape carries types from two features. Doctor requires every cross-feature input type to carry `public contract` in its origin.

### §3.4 Event payload references — typed payloads carrying foreign types

`event booking_created payload { user: User, booking: Booking }` carries types from `account` and `host`. Events already cross via `services publishes/consumes` (the *event itself* is contracted), but the **payload types** are not. Doctor requires payload types to carry `public contract`.

### §3.5 Policy atom data dependencies — `actor.*` field references

`@policy.user_owns_resource` resolves `actor.user_id` from session context. The session context is owned by `account` (auth). The policy executes in `host`. Under `microservices`, the runtime needs to know which session fields to propagate to `host_service` (an operational concern), but the language must declare the dependency: which `actor.*` fields the policy reads, and at what version.

Doctor requires policies that touch `actor.*` fields to be reachable from the origin feature's `auth` declaration — when `actor.user_id` is referenced, the origin's `auth` declaration must carry `public contract identity as v<N>` (the identity surface is the version-pinned contract; individual `actor.*` fields are members of it).

### §3.6 Out of inventory (deliberately)

- **Cross-app references** — already explicit via `workspace.lzi boundaries`.
- **Aggregates** — not surface syntax in the current grammar (no `aggregate_block` production at `grammar.lzi.md`). When aggregates ship as a primitive, their cross-feature rules enter via that proposal.
- **Workflows** — `workflow_body` admits `policy | emits | transition_decl` only (`grammar.lzi.md:506-509`); cross-feature workflow detection is an analyzer concern walking `creates/updates/deletes` against feature ownership, no new syntax. The `microservices`-mode rule "workflow spans multiple features" emits a doctor `warning` (not error) without new keywords.
- **`@fn.<name>` cross-feature handler references** — already covered by the existing `requires integration <slot>: <Capability>` + binding mechanism (`docs/invariants.md:71-87`). Cross-feature handler calls under `microservices` route through capability slots; the analyzer derives `ExternalCallRef` from that. No new author syntax.
- **Migrations** — per-service schema scoping under `microservices` is a runtime/atlas concern, not a language concern. Out of scope.
- **Caches/Redis/Locks** — operational shared state, not language reference. Out of scope.

---

## §4. The invariant

> **In any capsule that may transition to `architecture mode microservices`, cross-feature references must be expressible as contracts.** Types, query return shapes, command input shapes, event payloads, and policy `actor.*` references that cross feature boundaries must be marked as `public contract <Symbol> as v<N>` in their origin feature. The doctor enforces this only under `microservices` mode; capsules under `monolith` / `modular_monolith` compile unchanged. Cross-feature contract violations surface at compile time, not at runtime.

This is the cement layer that makes the framework's `monolito → microserviços é uma flag` claim honest. The capsule shape stays identical between modes; the semantics adapt to the architecture mode.

---

## §5. Cross-feature contract declaration

### §5.1 Origin feature — `public contract` annotation

A feature publishes contracts via `public contract <Symbol> as v<N>` adjacent to the declared symbol:

```lazuli
feature account
  domain
    public contract Gender as v1
    enum Gender
      female = 1
      male = 2
      non_binary = 3
      prefer_not_to_say = 4

    public contract User as v2
    resource User
      field email: @semantic.Email @pii.contact required unique
      field gender: Gender required
      field display_name: Text required
      previously alias name_field
```

The compound `public contract` enters the closed reserved-word set (`grammar.lzi.md:88-108`). `public` matches only as the leading word of the compound; it has no other use in the grammar. The version `v<N>` is monotonic per symbol; gaps are not allowed.

### §5.1.bis Site placement vs feature-level `contracts` block

Considered: place all contract declarations in a single feature-level `contracts` block (sibling to `domain`, `policies`, `commands`). Rejected because:

1. **Locality.** The contract is metadata about ONE symbol; the version + intent + history belong adjacent to the symbol's site. A reader scanning `enum Gender` sees the contract version on the previous line without scrolling.
2. **Composition with `previously`.** Rename history already lives at the symbol site (`previously alias <name>`); the contract version lives next to it.
3. **Pattern consistency.** Other metadata-on-symbol concerns (e.g. `@pii.*` on fields, `audit default` on commands) live at the site, not in feature-level blocks.

The site placement is the canonical form. A `contracts` block remains structurally possible if a future proposal finds load-bearing reasons for the indirection.

### §5.2 Consumer feature — no syntactic change

`uses account` stays unchanged. `field gender: Gender required` stays unchanged. The compiler computes whether the resolution crosses a feature boundary; if yes AND `architecture mode == microservices`, doctor checks the origin's `public contract`. If absent, error.

Ergonomic property: **the consumer's reference site doesn't change when the architecture mode changes.** A team starts in `modular_monolith`, ships, decides to split, flips `architecture mode microservices`, and doctor produces the exact per-symbol list of `public contract` annotations the lead must add to **origin** features. Consumer call sites stay byte-identical.

### §5.3 Per-class declaration form

| Class | Origin-side declaration |
|---|---|
| Enum (`§3.1`) | `public contract <EnumName> as v<N>` adjacent to `enum <EnumName>` |
| Resource (`§3.1, §3.3`) | `public contract <ResourceName> as v<N>` adjacent to `resource <ResourceName>` |
| Record (`§3.1, §3.3`) | `public contract <RecordName> as v<N>` adjacent to `record <RecordName>` |
| Query return (`§3.2`) | `public contract query.<kind>.<name> as v<N>` adjacent to the query declaration (the underlying return type itself also needs its own contract if it crosses) |
| Command input (`§3.3`) | `public contract command.<name> as v<N>` adjacent to the command declaration |
| Event payload (`§3.4`) | `public contract event.<name> as v<N>` adjacent to the event declaration |
| Identity / policy (`§3.5`) | `public contract identity as v<N>` adjacent to `auth identity <Resource>.<field>` |

### §5.4 Version bumps — explicit semantic categories

| Category | Trigger | Action |
|---|---|---|
| **Patch** | Comments, docs, internal renames that produce identical IR | No bump |
| **Compatible** | Add an optional field; add an enum variant; relax a constraint | Bump `v<N>` → `v<N+1>`. Consumers on `v<N>` still work; new variants/fields are unknown-but-not-present in their schema. |
| **Breaking** | Rename a required field; remove an enum variant; tighten a constraint; change a type | Bump `v<N>` → `v<N+1>`. **Consumer feature must explicitly migrate.** Doctor `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` flags consumer sites still on the old version. |

**Ambiguous cases resolved:** renaming an *optional* field is **breaking** (consumers that DO use the field break); renaming an *unused* field is **compatible** (no consumer can break). The doctor walks consumers to decide. Removing an enum variant a consumer didn't reference is still **breaking** at the contract level (the consumer's switch statements may need updating to honor exhaustiveness), but the doctor downgrades to warning when no consumer references the variant.

### §5.5 `previously` integration

Existing canonical `previously alias <name>` and `previously migrated <name>` clauses (`grammar.lzi.md:235`) carry rename history on the symbol. Under contracts, the rename trail composes with the contract version:

```lazuli
public contract User as v3
resource User
  field email: @semantic.Email required
  field display_name: Text required
  previously alias name_field
```

The IR's `previous_names: Vec<String>` on each resource/field already exists (`crates/lazuli_ir/src/lib.rs` `Resource`/`Field`). The contract version cross-references this list when computing version-diff semantics. No new vocabulary needed.

---

## §6. Architecture-mode resolution

### §6.1 `monolith` mode

`uses account` → in-process Rust/Go module import. `Gender` resolves to a single type. **No contract check.** Backward compatible with every capsule that compiles today.

`public contract` declarations are syntactically valid but informational only — they document intent for future-proofing and feed tooling (audit-skill, `lazuli inspect`, LSP).

### §6.2 `modular_monolith` mode

Same as `monolith` for language-level concerns: in-process module import; no contract enforcement. The mode difference (modular packaging, build-time service boundary tracking) is operational; the language behavior is identical to `monolith` for cross-feature references.

### §6.3 `microservices` mode

This proposal's enforcement triggers on `architecture mode microservices`. Per `grammar.app.md:282-283`, the doctor already cross-checks that `microservices` mode implies `enforce_service_boundaries true` — the two flags coexist by construction; a `microservices` capsule with the boundary flag false fails compile via an existing doctor rule, not by this proposal. The new enforcement here triggers as soon as `microservices` is set:

For each cross-feature symbol reference site, the analyzer checks the origin feature for a `public contract`:

- **Origin has `public contract`** → analyzer records contract version on the consumer's reference. Compiled output gates per the (deferred) runtime proposal.
- **Origin lacks `public contract`** → doctor fires `CROSS-FEATURE-CONTRACT-MISSING-001`. Hard error.

What the **language layer** does NOT specify (per `docs/capability-layering.md` §Pipeline + §Decision Test (the language-vs-runtime split rule)):

- How types are vendored at the consumer (copy in repo, fetch from registry, embed in binary).
- The wire codec (Protocol Buffers, JSON Schema, Avro, custom Lazuli).
- The transport (RPC, gRPC, REST).
- The schema-registry choice (Buf, Confluent, Lazuli-native).

Those decisions belong to a future runtime-side proposal. The language surfaces the **contract metadata** in the IR; the runtime selects mechanics.

---

## §7. Doctor diagnostics

Two rules, both gated on `architecture mode microservices`. Existing capsules under `monolith` / `modular_monolith` see no new diagnostics.

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `CROSS-FEATURE-CONTRACT-MISSING-001` | A cross-feature reference (type/enum/record in field decl, query return, command input, event payload, or identity reference) resolves to a symbol in the origin feature that lacks `public contract`. | error (under `microservices` only) | Add `public contract <Symbol> as v1` adjacent to the symbol's site in the origin feature. |
| `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` | Consumer feature references `<feature>.<Symbol>` at one version while the origin's current contract is a different version. | error (under `microservices` only) | Migrate the consumer to the new version (and adjust call sites if breaking) or pin explicitly. Doctor lists each consumer site. |

Both rules follow the existing catalog style (`docs/error-contract.md`) with `code / trigger / severity / fix / example` tuples. Module placement: `crates/lazuli_cli/src/doctor/cross_feature/` (new directory; orchestrator-owned `mod.rs`).

A third rule **`CROSS-FEATURE-WORKFLOW-SPAN-001` is a warning** (not error) under `microservices` only, emitted when a workflow's body touches resources owned by multiple features. No new syntax; the rule walks the existing `workflow_body` against the feature ownership map and surfaces "this workflow spans features X, Y, Z — consider saga decomposition." Authors decide; the warning catalogs the concern.

---

## §8. Migration story

### §8.1 Existing capsules in `monolith` / `modular_monolith` mode

**No-op.** Hostpoint, Pleiades v2, full-capsule fixture all compile today without contract annotations. After this proposal lands, they continue to compile — the new doctor rules are gated on `architecture mode microservices`.

### §8.2 Opt-in `public contract` under non-`microservices` modes

A capsule lead can author `public contract` annotations now, while still under `modular_monolith`. The annotations are informational under non-`microservices` modes (no doctor enforcement) but become load-bearing on the day the project flips to `microservices`. The mid-migration capsule reads cleanly: "this symbol is intended to be a boundary contract."

The audit-skill MVP (`docs/proposals/audit-skill-mvp.md`) can surface a recommendation: "feature X has N symbols referenced cross-feature; consider declaring them as `public contract` for future-proofing."

### §8.3 Migration from `modular_monolith` to `microservices`

One-line change to `app.lzi`:

```diff
  architecture
-   mode modular_monolith
+   mode microservices
    service_ready true
    enforce_service_boundaries true
```

After the flip:
1. Doctor runs and produces N `CROSS-FEATURE-CONTRACT-MISSING-001` errors — one per cross-feature symbol reference that lacks a contract. **Doctor enumerates the per-symbol list** (test fixture in D.2 pins this behavior).
2. For each error, the lead adds `public contract <Symbol> as v1` to the origin feature. Mechanical.
3. Workflows spanning features produce `CROSS-FEATURE-WORKFLOW-SPAN-001` warnings; the lead decides per-workflow whether the saga decomposition is necessary now.
4. Once doctor is green, runtime + codegen (per a future runtime proposal) produce per-service binaries with vendored contracted schemas. **Not in this proposal.**

The work is **catalogued, not hidden.** The team sees the full migration surface before deploying. Per the founding principle: language failures must surface before deploy.

### §8.4 What v0.1 of this proposal actually ships

After implementation:

- Capsules under `monolith` / `modular_monolith`: observable behavior unchanged.
- Capsules under `microservices`: doctor refuses to compile until cross-feature references are contracted; warnings catalog workflow spans.
- The full-capsule fixture in this repo (currently `modular_monolith`) gets `public contract` annotations added (`§9` cell D.1) as a worked example, exercising the new doctor rules under a switched-mode test fixture.

No codegen change. No runtime change. The cement lives in the analyzer + doctor.

---

## §9. Implementation cells

| Cell | Owner | Scope | Risk |
|---|---|---|---|
| **A.1** | Codex | `crates/lazuli_syntax/` — add `public_contract_clause` to the closed grammar. Reserved words `public` + `contract` added as a compound. Tests cover 7 placements (enum, resource, record, command, query, event, identity). | Low (additive syntax) |
| **A.2** | Codex | `crates/lazuli_ir/src/lib.rs` — add `public_contract: Option<PublicContract>` field to `EnumDecl`, `Resource`, `Record`, `Command`, `Query`, `Event`, plus an analogous field on the auth/identity surface. New struct `PublicContract { version: u16, span_ref: Option<SpanRef> }`. All additive, `#[serde(default)]`. | Medium (7 IR structs touched, additive) |
| **A.3** | Claude | `crates/lazuli_analyzer/src/lib.rs` — lowering for the new clause + cross-feature reference walker. Extends `SymbolOriginIndex` from `lsp-symbol-origin` proposal to record `contract_version: Option<u16>` per symbol. | Medium |
| **B.1** | Codex | `crates/lazuli_cli/src/doctor/cross_feature/contract_missing_001.rs` — implements `CROSS-FEATURE-CONTRACT-MISSING-001`. Gated on `architecture mode microservices`. | Low |
| **B.2** | Codex | `crates/lazuli_cli/src/doctor/cross_feature/version_drift_001.rs` — `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001`. | Low |
| **B.3** | Codex | `crates/lazuli_cli/src/doctor/cross_feature/workflow_span_001.rs` — `CROSS-FEATURE-WORKFLOW-SPAN-001` (warning; no new syntax). | Low |
| **B.4** | Claude | `crates/lazuli_cli/src/doctor/cross_feature/mod.rs` + wire-up. | Low |
| **D.1** | Claude | `examples/full-capsule/` — add `public contract` annotations for the cross-feature symbols (`User`, `Org`, `Customer`, …) to exercise the new doctor rules. | Low |
| **D.2** | Claude | `crates/lazuli_cli/tests/cross_feature_contracts.rs` — integration tests covering (a) `monolith`/`modular_monolith` no-op behavior; (b) `microservices` mode catching N missing contracts; (c) version-drift detection; (d) workflow-span warning. **Test format:** for each case, assert the exact set of doctor finding codes + per-finding `feature` + `symbol` fields (set-equality, not count-equality), following the existing pattern at `crates/lazuli_cli/src/doctor.rs:15012-15026` (e.g., `assert!(surfaced.contains("CROSS-FEATURE-CONTRACT-MISSING-001"))`). Snapshot diffs not used; the IDs are the load-bearing contract. | Medium |
| **F.1** | Claude | `docs/grammar.lzi.md` — add `public_contract_clause` to the keyword catalog + grammar production. | Low |
| **F.2** | Claude | `docs/invariants.md` — record the invariant ("every cross-feature reference is a potential cross-service reference [under `microservices`]"). | Low |
| **F.3** | Claude | `docs/error-contract.md` — catalog the 3 new doctor codes. | Low |

**Wave layout:** A.1 → A.2 → A.3 sequential (each depends on the previous). B.1+B.2+B.3 in parallel after A.3. D.1+D.2+F.* in a final wave. **No codegen cell** — runtime mechanics for `microservices` (vendored types, wire codec, transport) belong to a future runtime-side proposal per `docs/capability-layering.md` §Pipeline + §Decision Test (the language-vs-runtime split rule).

---

## §10. References

- `docs/architecture.md` §"What we learned from a previous attempt" — Aerocoding/Orion Studio template-driven sprawl; preserving boundaries early is the lesson.
- `docs/design-principles.md` Rule Zero — Vocabulary Over Mechanism. `public contract` is vocabulary (closed semantics, static checks, IR-projected, doctor-enforced), not a mechanism for arbitrary projection.
- `docs/capability-layering.md` §Pipeline + §Decision Test (the language-vs-runtime split rule) — the boundary this proposal honors: language declares contract metadata; runtime selects mechanics (codec, transport, registry).
- `docs/invariants.md` §"Identity And Renames" (`previously alias / migrated`); §"Capabilities And Adapter Wiring" lines 71-87 (`requires integration`).
- `docs/grammar.lzi.md` §`uses_block`, §`previously_clause` (line 235), §`qualified_*_ref`.
- `docs/grammar.app.md:118-122` — closed `architecture_mode` enum; `:282-283` — `microservices` requires `enforce_service_boundaries true`.
- `docs/proposals/lsp-symbol-origin.md` — `SymbolOriginIndex` is the infrastructure this proposal extends with version metadata.
- `examples/full-capsule/workspace.lzi:8-10` — existing `boundaries` block for events.
- `examples/full-capsule/app.lzi:48-69` — `architecture` + `services` + `publishes/consumes`.
- `crates/lazuli_ir/src/lib.rs:1010, 3853` — `ExternalCallRef` (analyzer-derived, not surface).
- `crates/lazuli_codegen_go/src/emitter/cross_feature.rs` — existing codegen infrastructure that future runtime-side proposal will extend.
- Memory: `project_strategic_pivot_2026-05-15`, `feedback_cement_over_ship_until_users_2026-05-15`, `feedback_normative_not_narrative_2026-05-15`.

---

## §11. Acceptance criteria

L0 PASS condition: this proposal answers, deterministically, the following 14 questions from the proposal text alone.

1. **What is the invariant being cemented?** → §4. In any capsule that may transition to `architecture mode microservices`, cross-feature references must be expressible as contracts.
2. **What `architecture_mode` value(s) trigger contract enforcement?** → §6.3. Only `microservices` (existing closed-enum value). `monolith` and `modular_monolith` see no new enforcement.
3. **What capsule changes does this require for existing projects in `monolith` / `modular_monolith` mode?** → §8.1. **None.** Existing capsules compile unchanged.
4. **What new keyword(s) enter the closed grammar?** → §5.1. `public contract` (compound; both words enter the closed reserved word set as a single grammar production; `public` has no other use).
5. **What classes of cross-feature reference does this cover?** → §3 inventory: five classes (types/enums/records, query returns, command inputs, event payloads, policy `actor.*` references).
6. **What classes are deliberately out of inventory?** → §3.6. Cross-app refs (already explicit), aggregates (no surface syntax yet), workflow `coordinator:` (no new keyword; analyzer-only detection), `@fn` cross-feature handlers (covered by existing `requires integration`), migrations (runtime concern), caches (operational state).
7. **What new IR types/fields are added?** → §9 A.2. `PublicContract { version: u16, span_ref }` + `public_contract: Option<PublicContract>` field on 7 existing IR structs. Additive, `#[serde(default)]`. No new top-level vocabulary.
8. **What new doctor diagnostics?** → §7. Two errors (`CROSS-FEATURE-CONTRACT-MISSING-001`, `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001`) + one warning (`CROSS-FEATURE-WORKFLOW-SPAN-001`). All gated on `architecture mode microservices`.
9. **What's the migration story?** → §8. No-op for non-`microservices` capsules. Opt-in for forward-looking projects. The flip to `microservices` is one line in `app.lzi`; doctor catalogs all required contract additions per-symbol.
10. **How does `previously` (canonical `alias`/`migrated`) integrate?** → §5.5. The existing canonical clause carries rename history on the symbol; contract version cross-references the existing `previous_names: Vec<String>` in IR. No new vocabulary.
11. **What's NOT in v0.1?** → §6.3 closing paragraph + §9 wave layout. **No codegen cell.** Runtime mechanics for `microservices` (vendored types, wire codec, transport, registry) belong to a future runtime-side proposal per `docs/capability-layering.md`. v0.1 ships the doctor enforcement as the cement layer.
12. **Why is this target-cementing, not boundary-moving?** → §2 preamble + §1.1. Existing `architecture_mode` + `services` + `workspace.lzi boundaries` already declare deployment topology; this proposal makes the language-level reference contract explicit. The promise "monolito → microserviços é uma flag" requires this cement to be honest.
13. **Why aren't `aggregate`, `coordinator:`, and `external_call` invented as new surface keywords?** → §2 non-goals 9, 10, 11. `aggregate` has no current grammar production (deferred to whatever proposal eventually ships aggregates). `coordinator:` would expand `workflow_body` admission; replaced by analyzer-only warning. `external_call` is analyzer-derived from `requires integration` slots, not author syntax — cross-feature `@fn.<name>` resolution under `microservices` reuses the existing capability/binding mechanism.
14. **What composes with the `lsp-symbol-origin` proposal?** → §9 A.3. `SymbolOriginIndex` gains a `contract_version: Option<u16>` field per symbol; the existing `SymbolOrigin` struct (`crates/lazuli_ir/src/lib.rs` per `lsp-symbol-origin` proposal §6.2) receives this addition. The two proposals compose: `lsp-symbol-origin` surfaces where a symbol is defined; this proposal surfaces what version a consumer is allowed to use.

If all 14 answers are mechanical from the proposal text, L0 passes.
