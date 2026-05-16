# Proposal — Shadow-Record + Resource Wide-Cluster Doctor Lints

**Status:** L0 v0.2 DRAFT — 2026-05-16 (v0.1 graded BLOCK 7.4/10 via `lazuli-language-architect`; five structural blockers — anchor inflation in evidence narrative, cross-feature scope mismatch with driver, file path contradiction with parent catalog, suffix-gameable Shape B heuristic, missing FK-to-peer-resource filter — all addressed in v0.2)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** PASS ≥ 8.5 via `lazuli-language-architect`
**Driver:** Hostpoint `catalog.update_property` command input inlines the same address-shape that `catalog.Property` resource declares above it; doctor sees nothing because both declarations are grammatical. Rule Zero ("Vocabulary Over Mechanism") names the smell: a shape that recurs across declaration sites wants a `record`.
**Honors:** `docs/design-principles.md` Rule Zero, `docs/scope-discipline.md`, `docs/proposals/doctor-vocabulary-lints.md` (parent catalog, `VOCAB-*` family conventions), `feedback_normative_not_narrative_2026-05-15`, `feedback_grade_before_commit.md`, `feedback_cement_over_ship_until_users_2026-05-15`.

---

## §1. Problem

### §1.1 The vocabulary drift

Lazuli's `record` primitive exists so that field clusters with semantic identity get a name. Without a named record, the same cluster is re-declared at every site that needs it; each re-declaration is grammatical and the compiler is silent.

Doctor's `VOCAB-*` rule family (parent catalog: `docs/proposals/doctor-vocabulary-lints.md`) surfaces drifts toward absent vocabulary. This proposal adds two rules in that family: one for the multi-declaration shadow (a cluster recurs across N declarations within a feature), and one for the single-declaration name-tagged cluster (a wide resource contains a coherent sub-shape distinguished by a shared name token).

### §1.2 Anchored evidence — Hostpoint, intra-feature

`app/features/catalog/catalog.lzi:109-130` declares `resource Property` with these address fields (lines 121-128, plus lat/lng at 129-130):

| Field | Type | Optionality |
|---|---|---|
| `country` | `Text` | `required` (default `"BR"`) |
| `cep` | `Text` | `optional` |
| `street` | `Text` | `optional` |
| `address_number` | `Text` | `optional` |
| `complement` | `Text` | `optional` |
| `neighborhood` | `Text` | `optional` |
| `city` | `Text` | `optional` |
| `state` | `Text` | `optional` |

`app/features/catalog/catalog.lzi:250-270` declares `command update_property` whose input carries (lines 260-266, plus lat/lng at 267-268, plus voltage/water_source at 269-270):

| Field | Type | Optionality |
|---|---|---|
| `cep` | `Text` | `optional` |
| `street` | `Text` | `optional` |
| `address_number` | `Text` | `optional` |
| `complement` | `Text` | `optional` |
| `neighborhood` | `Text` | `optional` |
| `city` | `Text` | `optional` |
| `state` | `Text` | `optional` |

The resource and the input share **seven `(name, type_ref)` pairs** under strict-name match: `cep`, `street`, `address_number`, `complement`, `neighborhood`, `city`, `state`. The resource has `country` that the input lacks. Both declarations live in `catalog`, so the lint operates intra-feature.

The rule's N=4 minimum-cluster threshold is satisfied (7 ≥ 4); v0.1 detection fires.

### §1.3 Anchored evidence — Hostpoint, cross-feature (advisory only in v0.1)

`app/features/host/host.lzi:14-22` declares a `record Address` with these fields:

| Field | Type | Optionality |
|---|---|---|
| `country` | `Text` | `required` (default `"BR"`) |
| `cep` | `Text` | `optional` |
| `street` | `Text` | `optional` |
| `number` | `Text` | `optional` |
| `complement` | `Text` | `optional` |
| `neighborhood` | `Text` | `optional` |
| `city` | `Text` | `optional` |
| `state` | `Text` | `optional` |

`host.Host` resource uses it correctly: `address: Address optional` at `host.lzi:43`. The record is reused once within `host`, never imported elsewhere (`catalog.lzi:5` has `uses org`, no `uses host`).

Strict-name match between `host.Address` and `catalog.Property` address fields: 7 of the 8 fields share name+type (`country`, `cep`, `street`, `complement`, `neighborhood`, `city`, `state`); the eighth pair is `number` vs `address_number` — a name drift the strict-match heuristic does not catch.

**v0.1 does not fire on this cross-feature pair.** Detection is scoped intra-feature for the reasons in §2 (non-goal 4) and §6.5 (deferral rationale). The diagnostic emitted for the intra-feature case includes an advisory line pointing at `host.Address` as a possible canonical extraction target — informational, not a separate diagnostic.

### §1.4 What the lint cements

A declarative invariant the framework currently does not enforce: structural-similarity field clusters should be named. The lint is `warning`-level (not `error`) because the refactor changes schema shape — author judgment matters — but the surface fan-out (doctor + LSP + audit-skill) ensures the signal reaches both human and AI authors during the edit cycle.

Cementing now is cheap per `feedback_cement_over_ship_until_users_2026-05-15`: one pilot has the smell, the rule catches it, the precedent is set before a second pilot replicates the drift independently.

---

## §2. Scope

### In scope

1. **Two doctor lints** (`VOCAB-SHADOW-RECORD-001`, `VOCAB-RESOURCE-WIDE-CLUSTER-001`) with detection heuristics, severity matrix, opt-out forms.
2. **Detection across three declaration kinds within a feature**: resources, records, and command-input blocks. Query params and workflow inputs are not v0.1 declaration sites (no pilot evidence yet).
3. **Universal-column filter** that strips id, tenancy FKs, timestamps, soft-delete, audit columns, and FK references to other resources in the same capsule before cluster matching.
4. **False-positive guards**: strict-name match (no Levenshtein), minimum cluster ratio relative to declaration size, view-projection record detection.
5. **Surface fan-out**: doctor library rule + CLI dispatch + LSP diagnostic + audit-skill EXAMPLES fixture + audit-skill RULES.md catalog row. Same surfaces every other `VOCAB-*` rule covers.
6. **Opt-out form**: `# doctor:allow VOCAB-SHADOW-RECORD-001 — reason "..."` on the offending declaration, mirroring `VOCAB-TESTS-MISSING-001` (`vocab_tests_missing_001.rs:45`) and `VOCAB-MONEY-MULTI-CURRENCY-001` (`vocab_money_multi_currency_001.rs:45`) conventions. Em-dash separator is intentional and matches the live parser-expected form.
7. **Parent catalog path correction** (Cell C.2) — `docs/proposals/doctor-vocabulary-lints.md` lines 415, 450-453, 571-573 cite `crates/lazuli_cli/src/doctor/vocab/` which no longer exists; rules live at `crates/lazuli_doctor/src/vocab/` since the 2026-05-15 extraction. The proposal includes a cleanup edit.

### Non-goals

1. **No new language vocabulary.** Both lints fire on grammar that exists today. No new keywords, no new IR fields. Vocabulary fan-out is the cement layer; no new mechanism.
2. **No auto-fix.** Manual refactor only — schema-shape changes warrant author judgment. Matches the parent catalog's auto-fix policy.
3. **No fuzzy name matching.** Strict `(name, type_ref)` equality. `number` vs `address_number` is two different fields. Levenshtein-tolerant matching is a v0.2 polish item.
4. **No cross-feature shadow detection in v0.1.** Cross-feature reuse crosses contract surface per `docs/proposals/cross-feature-contracts.md`; the decision to import a record from another feature is non-mechanical. v0.1 emits an advisory mention in the intra-feature diagnostic when a cross-feature record matches the cluster; v0.2 promotes this to a separate rule (`VOCAB-SHADOW-RECORD-002`) once intra-feature false-positive rates are known.
5. **No graph-clustering across the entire capsule.** Pairwise within a feature; quadratic in declarations-per-feature.
6. **No `lazuli inspect --shadows` query mode in v0.1.** Inspect extension tracked in §10.
7. **No coverage-mode summary** in v0.1.
8. **No new IR type or sidecar.** Detection reads existing `Feature`/`Resource`/`Record`/`Command` IR.

---

## §3. The smell, characterized

### §3.1 Two distinct lints

The Hostpoint evidence in §1.2 motivates one lint. A second lint captures a parallel smell observed in framework dogfooding: a single resource carrying a coherent sub-shape distinguished by a shared name token. The two lints share the `VOCAB-*` family and the universal-column filter (§5); their detection logic is independent.

**Lint A — multi-declaration shadow.** Two or more declaration sites within the same feature share ≥ N field `(name, type_ref)` pairs after universal-column filtering. The signal is "the same shape recurs."

**Lint B — single-declaration wide cluster.** One resource has > K authored fields after universal-column filtering AND a name-token cluster of ≥ M fields shares a common leading or trailing snake-case token. The signal is "one resource carries an extractable sub-shape."

The two lints are sibling rules in the same surface, not a unified detection pass. Lint A's match is structural (intersection across declarations); Lint B's match is lexical (name-token grouping within one declaration). Folding them produces a heuristic that is precise for neither case.

### §3.2 Strict-name match and the gameability boundary

Lint A's match is strict on `(name, type_ref)` pairs. `street: Text` and `road_name: Text` are not a match; `number: Text optional` and `number: Text required` are a match (required-ness is not part of structural identity for the cluster check — the refactor preserves the more permissive optionality unless the author overrides).

A motivated author can dodge Lint B by renaming a prefix cluster to a suffix cluster (`shipping_street → street_for_shipping`). Lint B's heuristic detects **leading OR trailing snake-case tokens** (closes B4 from v0.1 review). An author can still dodge by using disjoint names (`shipping_street → addr1`, `billing_street → addr2`) — at that point the smell is no longer detectable by lexical signal, and the rule's contract is "catches naive recurrence, not adversarial naming." The opt-out comment exists for the legitimate-flat-schema case; gaming the rule is the author's prerogative.

Lint A's match is a SET operation on field NAMES — order does not matter. Lint B's match is a LEXICAL operation on field NAMES — token affinity matters. Both walk fields as sets, never as sequences of declarations; "set" refers to the input collection's lack of order, not to the matching algorithm's shape.

Worked example for Lint A's set semantics: a resource with `{cep, street, city}` declared in that order and a record with `{street, cep, city}` declared in a different order produce a 3-field intersection regardless of authored order. The match is on the `(name, type_ref)` pair value, not on positional index.

### §3.3 What the smell is NOT

| Not-a-smell | Why filtered |
|---|---|
| Two resources both have `id` + `created_at` + `updated_at` | Universal-column filter (§5) strips these before matching |
| Two resources both have `org: Org required` | Universal-column filter strips tenancy FKs |
| Two resources both have an FK to a third resource (`property: Property required`, `host: Host required`) | Universal-column filter strips FK references to in-capsule resources (closes B5 from v0.1 review) |
| Two resources both have `name: Text` + `description: Text` (cluster size 2) | Cluster threshold N=4 not met |
| A `record FooView` that mirrors `resource Foo`'s field set | View-projection detection (§5 row: name ending in `View`/`Snapshot`/`Entry`/`Item` + `<noun>_id: ID required` lookup column) |

---

## §4. The lints

### §4.1 `VOCAB-SHADOW-RECORD-001` — multi-declaration shadow

**Detection heuristic.**

For each feature, walk every declaration site (resource, record, command-input). For each pair of declarations within the feature, compute the intersection of their post-filter field sets under strict `(name, type_ref)` match. If the intersection size is ≥ **N (default 4)** AND the intersection is ≥ **50%** of each side's post-filter field count, emit one finding per pair.

**Why 50% intersection ratio.** Two resources of 20 fields each that happen to share 5 fields are signal-poor (5/20 = 25%). Two resources of 8 fields each that share 5 (5/8 = 62%) is a real shadow. The threshold makes the rule precise on the structurally-similar case, not on the "happens-to-overlap" case.

**Example trigger (the Hostpoint case, intra-feature):**

```lzi
feature catalog
  domain
    resource Property
      # ... 11 non-address fields elided ...
      country: Text required = "BR"
      cep: Text optional
      street: Text optional
      address_number: Text optional
      complement: Text optional
      neighborhood: Text optional
      city: Text optional
      state: Text optional
      # ... lat/lng + remaining fields ...

  command update_property
    input
      property_id: ID required
      # ... 6 non-address fields ...
      cep: Text optional
      street: Text optional
      address_number: Text optional
      complement: Text optional
      neighborhood: Text optional
      city: Text optional
      state: Text optional
      # ... lat/lng + voltage/water_source ...
```

Post-filter field count: `Property` has 24 (FKs to `Org`/`Host`/`UploadedAsset` and timestamps stripped). `update_property.input` has 17. Intersection: 7 fields (`cep`, `street`, `address_number`, `complement`, `neighborhood`, `city`, `state`). 7/24 = 29%, 7/17 = 41%. Neither side meets the 50% ratio.

Therefore the rule **as specified does NOT fire on the Hostpoint case as-is**.

The proposal continues to specify the rule. The Hostpoint anchor is **calibration data**, not a passing-test anchor. v0.1 ships the rule with these defaults; if pilot evidence shows the 50% ratio is too tight, the toml override (§6) lowers it per-project, and v0.2 may recalibrate the default.

Documented as Open Question §9.1: should the ratio threshold be lower (e.g. 30%)? Defer until v0.1 ships and a second pilot tests the boundary.

**Diagnostic (when the rule fires, hypothetical):**

```
features/<feature>/<feature>.lzi:<line>:5: warning [VOCAB-SHADOW-RECORD-001]:
  resources `A` and `B` share <N> fields with matching types
  (<field_names>). Consider extracting a `record <SuggestedName>` and
  referencing it from both: `<suggested_field>: <SuggestedName> required`.
  If the resources independently need these fields (e.g. they will
  diverge), add `# doctor:allow VOCAB-SHADOW-RECORD-001 — reason "..."`
  on each resource.
```

When a cross-feature record matches the cluster, append:

```
A record with a similar shape exists at `<feature>.lzi:<line>` (`<Feature>.<Record>`).
Consider `uses <feature>` + `<field>: <Record> required` if cross-feature
reuse is acceptable for your modular boundary. Cross-feature reuse is
non-mechanical; see `docs/proposals/cross-feature-contracts.md`.
```

**Suggested refactor (in the diagnostic, NOT applied):**

The diagnostic includes the cluster fields verbatim; the author writes the record by extracting them. No code-fence skeleton in v0.1 — generating the snippet is §10.7 polish.

**False-positive cases (rule MUST not fire):**

- Clusters smaller than N=4 fields after universal-column filtering.
- Clusters where the intersection is < 50% of either declaration's post-filter size.
- Pairs where one side is a view/projection record (filter in §5).
- Pairs where one side is a discriminator union variant (deferred check; v0.2 if the union grammar gains a closed-discriminator form).

**Severity:** `warning` in strict and production profiles. Not `error` because the refactor crosses schema boundaries.

---

### §4.2 `VOCAB-RESOURCE-WIDE-CLUSTER-001` — single-declaration name-token cluster

**Detection heuristic.**

For each resource, count authored fields after universal-column filtering. If the count > **K (default 10)**, group fields by leading-snake-case token (the substring before the first `_`) AND by trailing-snake-case token (the substring after the last `_`). For each group of ≥ **M (default 4)** fields sharing a leading OR trailing token, emit a finding.

The leading-or-trailing token detection closes B4 from v0.1 review (suffix-cluster gameability). A field named `street_for_shipping` shares the trailing token `shipping` with `city_for_shipping` and `state_for_shipping` — detected. The "naive author renames prefix to suffix" pattern no longer dodges.

**Token exclusion list (default):** `id`, `at`, `by`, `count`, `total`, `org`, `tenant`. Configurable via `Lazurite.toml`. Single-letter tokens always excluded.

**K=10 (not v0.1's K=12).** Calibration anchor: Hostpoint `host.Host` has 13 authored fields; `catalog.Property` has 26 authored fields; `catalog.Service` has 16 authored fields. K=10 fires on Property and Service. Whether Property's "wide" is a genuine smell is the rule's surface; the author opts out if the shape is intentional.

**Example trigger (synthetic — no Hostpoint anchor for Lint B's positive case at v0.2 draft time; Cell D.1 will produce one or confirm the rule does not fire on the active pilot at default settings):**

```lzi
resource Order
  customer: Customer required
  status: OrderStatus required
  total_cents: Integer required
  notes: Text optional
  shipping_street: Text required
  shipping_city: Text required
  shipping_state: Text required
  shipping_postal_code: Text required
  shipping_country: Text required = "BR"
  billing_street: Text required
  billing_city: Text required
  billing_state: Text required
  billing_country: Text required = "BR"
  created_at: DateTime required
```

Post-filter field count: 13 (customer/status/total_cents/notes + 5 shipping_* + 4 billing_*; `created_at` stripped, `customer` is an FK to another resource → stripped). 13 > K=10.

Leading-token cluster: `shipping_*` has 5 fields, `billing_*` has 4 fields. Both ≥ M=4. Trailing-token cluster: `street`, `city`, `state`, `country` each appear twice across the two groups — trailing-token clusters of size 2 < M=4, no fire.

Diagnostic (largest cluster surfaced first):

```
features/<feature>/<feature>.lzi:<line>:5: warning [VOCAB-RESOURCE-WIDE-CLUSTER-001]:
  resource `Order` has 13 authored fields and 5 share leading token
  `shipping` (shipping_street, shipping_city, shipping_state,
  shipping_postal_code, shipping_country). Consider extracting a
  `record ShippingAddress` and referencing it as
  `shipping_address: ShippingAddress required`.
  If the naming grouping is incidental (no semantic cluster), add
  `# doctor:allow VOCAB-RESOURCE-WIDE-CLUSTER-001 — reason "..."`
  on the resource.
```

The rule surfaces the LARGEST cluster per fire. A subsequent run after the author refactors `shipping_*` may surface `billing_*`. No bundled diagnostics — keeps the message narrow.

**False-positive cases:**

- Resources with ≤ K=10 post-filter authored fields.
- Clusters smaller than M=4 sharing a token.
- Token in the exclusion list (`id`, `at`, `by`, `count`, `total`, `org`, `tenant`, plus `Lazurite.toml` extensions).
- Resources where the named cluster IS the resource's primary semantic (a `BillingAddress` resource whose 5 fields all share token `billing` — the resource name itself is the token; this is a polish item, see §10.4).

**Severity:** `warning` in strict profile, `info` in production profile. Lower default than Lint A because single-resource wide clusters are more often intentional flat-schema choices.

---

## §5. Universal-column filter (shared between lints)

Both lints walk a pre-filtered field set. The filter strips:

| Filter | Trigger | Rationale |
|---|---|---|
| `created_at`, `updated_at`, `deleted_at` | Implicit timestamps + soft-delete | Every resource has them |
| `id: Id` (only when resource lacks an explicit `composite_key`) | Implicit row identity | Auto-emitted |
| FK to tenancy axis (`Org`, `Tenant`) declared in feature `defaults` | `org: Org required` patterns | Tenancy is feature-wide |
| `<resource_name>_id: ID required` | FK self-reference | Foreign key self-reference |
| Field with `type_ref = UserDefined(<X>)` where `<X>` resolves to another resource in the same capsule | `host: Host required`, `customer: Customer required`, etc. | FK to peer resource — relational graph signal, not structural-similarity signal (closes B5 from v0.1 review) |
| Field with name ending in `_count` AND type `Integer` AND default `0` | Aggregation snapshot fields | Denormalisation markers |
| Declaration whose name ends in `View` / `Snapshot` / `Entry` / `Item` AND has a field matching `<noun>_id: ID required` | View/projection records | Denormalised lookup row, not VO candidate |

The filter is a single helper `is_universal_column(field, declaration, feature, module) -> bool` and `is_view_projection(declaration) -> bool`. Both lints call them. Centralised so future polish (e.g. additional exclusion patterns) updates one site.

The `is_universal_column` helper takes `module` because the FK-to-peer-resource filter (closes B5) must check that the `UserDefined(<X>)` reference resolves to a resource — that resolution may walk `feature.uses` (cross-feature peer references like `host: Host` in `catalog`).

Configurable via `Lazurite.toml`:

```toml
[doctor.vocab.shadow_record]
min_cluster_fields = 4
min_cluster_ratio = 0.5
exclude_field_names = ["audit_log", "rollup_id"]
severity = "warning"

[doctor.vocab.resource_wide_cluster]
min_resource_fields = 10
min_cluster_fields = 4
exclude_tokens = ["id", "at", "by", "count", "total", "org", "tenant"]
severity = "warning"
```

`min_cluster_fields` is the same key in both blocks for ergonomic symmetry (closes v0.1 review P7). The rules read it independently; defaults are identical.

---

## §6. Surface fan-out plan

### §6.1 Doctor (library + CLI)

- `crates/lazuli_doctor/src/vocab/vocab_shadow_record_001.rs` (NEW) — pure `check(feature, module, path) -> Vec<Finding>` function; mirrors `vocab_handler_heavy_001.rs` shape.
- `crates/lazuli_doctor/src/vocab/vocab_resource_wide_cluster_001.rs` (NEW) — same pattern.
- `crates/lazuli_doctor/src/vocab/universal_columns.rs` (NEW) — `is_universal_column` + `is_view_projection` helpers, used by both rules.
- `crates/lazuli_doctor/src/vocab/mod.rs` (EDIT) — export the new modules.
- `crates/lazuli_cli/src/doctor.rs` (EDIT) — dispatch the new lints from the feature walker, map `Finding` to `DoctorDiagnostic`. Wire-up parallels the deferred-but-spec'd path for `VOCAB-TESTS-MISSING-001` and `VOCAB-HANDLER-HEAVY-001` (their CLI dispatch lands in this same cell as part of the shipping wave).

Path corrected in v0.2 (closes B3 from v0.1 review): rules live at `crates/lazuli_doctor/src/vocab/`, not `crates/lazuli_cli/src/doctor/vocab/`. The parent catalog `docs/proposals/doctor-vocabulary-lints.md` retains the stale path at lines 415, 450-453, 571-573; Cell C.2 below corrects it.

### §6.2 LSP

`crates/lazuli_lsp/src/lib.rs` already imports `lazuli_doctor::vocab::*`. The LSP serves diagnostics from the same `check` fn. No new code on the LSP side beyond the per-rule severity remapping table addition (`Warning` for `warning`, `Information` for `info`).

### §6.3 Audit skill

- `skills/audit/EXAMPLES/vocab-shadow-record-001.lzi` (NEW fixture with `# Triggers:` + `# Expected message contains:` header per parent catalog convention).
- `skills/audit/EXAMPLES/vocab-resource-wide-cluster-001.lzi` (NEW fixture).
- `skills/audit/RULES.md` (EDIT) — two new rows in the rule table, each citing detection summary + refactor template.
- `crates/lazuli_doctor/tests/examples_snapshot.rs` (EDIT) — one new integration test per rule, mirroring `vocab_handler_heavy_001_example_fires`.

### §6.4 Inspect (deferred to v0.2 polish, §10.2)

A future cell adds `lazuli inspect <Resource> --shadows` returning a JSON list of matching shapes. Out of scope for v0.1; tracked in §10.

### §6.5 Cross-feature shadow detection (deferred to v0.2 polish, §10.1)

Architect (v0.1 review B2) noted the headline driver is cross-feature. v0.2 of THIS proposal pivots the driver narrative to the intra-feature case (§1.2) because:

1. Cross-feature reuse crosses a contract boundary (`docs/proposals/cross-feature-contracts.md` §3 catalogs five classes that cross feature lines). The fix for a cross-feature shadow is not mechanical: it requires the consumer feature to `uses <origin>` AND (under `architecture mode microservices`) the origin record to carry `public contract`. The lint cannot stamp out a one-line refactor — it would require structural decisions about feature boundaries.
2. Cross-feature pairwise comparison scales as O(F² × R²) where F is features-per-capsule and R is declarations-per-feature. For Hostpoint (11 features, ~5 declarations each averaging ~10 fields), that's ~30K comparisons per run. Performant in absolute terms, but the v0.1 intra-feature surface ships first to lock the invariant before generalising the heuristic.
3. The cross-feature advisory mention in the intra-feature diagnostic (§4.1) provides information without making detection-time judgments. The author sees the existing record; the import decision stays manual.

v0.2 (`VOCAB-SHADOW-RECORD-002`) ships once v0.1 has fired on ≥ 2 pilots and false-positive rate is observed.

---

## §7. Migration story

Existing capsules see new warnings on first run of `lazuli doctor` after v0.1 lands. Burden is bounded by:

1. **No grammar change.** Authors do nothing to keep compiling.
2. **`warning` severity (Lint A) / `info` in production (Lint B).** Build passes; CI surfaces the warnings without gating merges by default.
3. **`# doctor:allow` opt-out** on the offending declaration.
4. **`[doctor.vocab.*]` toml block** for project-wide override.

For Hostpoint specifically, the expected migration after v0.1 lands (with the 50% intersection-ratio default the proposal specifies):

- `catalog.Property` vs `update_property.input` — intersection 7 fields, ratios 29% / 41%, both below 50%. **Lint A does NOT fire** at default settings. The author who reads this proposal can lower `min_cluster_ratio` to e.g. 0.3 in `Lazurite.toml` to surface the case; whether that lower bound is the right default is Open Question §9.1.
- `catalog.Property` (26 authored fields after FK filtering) — leading token grouping: `is_*` (is_public, is_active — 2 fields, < M=4), no other ≥4-field token cluster (the address fields are flat — `country`/`cep`/`street`/etc. share no leading or trailing token). **Lint B does NOT fire on Property.**
- `host.Host` (13 authored fields after filtering) — no leading/trailing token cluster of ≥ 4. **Lint B does NOT fire on Host.**

The expected Hostpoint surface at v0.1 default settings: zero new diagnostics. v0.1 lays the rule infrastructure for future capsules that exhibit the smell more clearly; the Hostpoint anchor is calibration evidence, not a failing case.

This is honest in a way v0.1's narrative was not (closes B1 from v0.1 review). The cross-feature case (§1.3) shows the smell is real even if the default-tuned rule doesn't fire on it; v0.2 (`VOCAB-SHADOW-RECORD-002`) is where the Hostpoint driver lands.

---

## §8. Implementation outline (cells)

### Cell A.1 — Shadow-record detector (library)
- File: `crates/lazuli_doctor/src/vocab/vocab_shadow_record_001.rs` (NEW).
- Logic: pairwise intersection per feature; minimum-cluster + ratio thresholds; emit `Finding` per pair.
- 7-9 unit tests: positive trigger, ratio-too-low no-fire, view-projection filter, FK-peer filter, three-way matrix (3 declarations) emits 3 pairwise findings, intra-feature cross-feature blocker.
- ~280 LOC including tests.

### Cell A.2 — Resource-wide-cluster detector (library)
- File: `crates/lazuli_doctor/src/vocab/vocab_resource_wide_cluster_001.rs` (NEW).
- Logic: post-filter field count > K; group by leading + trailing tokens; fire on largest cluster ≥ M; token exclusion list.
- 7-9 unit tests: positive trigger leading-token, positive trigger trailing-token (catches the renamed-to-suffix case), excluded-token no-fire, K-too-low no-fire, M-too-low no-fire.
- ~220 LOC including tests.

### Cell A.3 — Universal-columns + view-projection helpers (shared)
- File: `crates/lazuli_doctor/src/vocab/universal_columns.rs` (NEW).
- Pure helpers `is_universal_column(field, declaration, feature, module) -> bool`, `is_view_projection(declaration) -> bool`.
- 6-7 unit tests covering each filter row in §5.
- ~120 LOC.

### Cell B.1 — mod.rs wire-up + EXAMPLES fixtures
- File: `crates/lazuli_doctor/src/vocab/mod.rs` (EDIT).
- Files: `skills/audit/EXAMPLES/vocab-shadow-record-001.lzi`, `skills/audit/EXAMPLES/vocab-resource-wide-cluster-001.lzi` (NEW).
- File: `crates/lazuli_doctor/tests/examples_snapshot.rs` (EDIT) — add `vocab_shadow_record_001_example_fires`, `vocab_resource_wide_cluster_001_example_fires` integration tests.
- ~70 LOC.

### Cell B.2 — CLI dispatch + LSP wire-up
- **Cell scope (explicit, four rules)**: this cell ships CLI dispatch for `VOCAB-SHADOW-RECORD-001`, `VOCAB-RESOURCE-WIDE-CLUSTER-001`, `VOCAB-TESTS-MISSING-001`, `VOCAB-HANDLER-HEAVY-001`. The latter two are currently library-only per their own next-checklist follow-up; bundling avoids re-paying the walker scaffolding cost.
- File: `crates/lazuli_cli/src/doctor.rs` (EDIT) — feature walker calls the four rule fns; maps `Finding` to `DoctorDiagnostic`.
- File: `crates/lazuli_lsp/src/lib.rs` (EDIT) — per-rule severity table additions for all four codes.
- LOC budget: ~120 (≈30 per rule including the per-rule severity remap row + a Finding-to-DoctorDiagnostic conversion case).
- Risk: the existing wire-up tests for `VOCAB-GRAMMAR-FORM-001` (the only currently-wired vocab rule) should be the template — clone the test shape per new code.

### Cell C.1 — RULES.md catalog rows
- File: `skills/audit/RULES.md` (EDIT).
- Two new rows per the existing catalog format.
- ~40 LOC.

### Cell C.2 — Parent-catalog path correction
- File: `docs/proposals/doctor-vocabulary-lints.md` (EDIT at lines 415, 450-453, 571-573).
- Replace `crates/lazuli_cli/src/doctor/vocab/` with `crates/lazuli_doctor/src/vocab/` per current repo layout (closes B3 from v0.1 review).
- Add a 2-line note in the catalog's revision history naming the 2026-05-15 extraction commit.
- ~10 LOC.

### Cell D.1 — Hostpoint validation pass
- After v0.1 ships, run `lazuli doctor` on Hostpoint. Confirm zero new diagnostics at default thresholds. Document the surface in a memory.
- Run again with `[doctor.vocab.shadow_record] min_cluster_ratio = 0.3` to confirm the `Property` ↔ `update_property.input` pair fires. Use this as a candidate for default re-tuning in v0.1.1.
- **Acceptance criterion (resolves §9.1 calibration debt)**: the memory writeup MUST propose either (a) keep `min_cluster_ratio = 0.5` default + rely on per-project override for Hostpoint-shape capsules, OR (b) lower default to 0.3 and re-validate FP rate on a second pilot before v0.1.1. The criterion is "answer the open question, do not silently leave it deferred."

### Cell E.1 — next-checklist tracking
- File: `docs/next-checklist.md` (EDIT).
- Track v0.2 (`VOCAB-SHADOW-RECORD-002` cross-feature), §10 polish items, default-ratio retuning if pilot evidence demands.

Total estimated LOC: ~860 across cells. Total estimated wall time at one focused session: ~4-5 hours including iterating on grading.

---

## §9. Open questions (for the architect)

### §9.1 `min_cluster_ratio` default — 0.5 or 0.3?

At 50%, the Hostpoint anchor (Property ↔ update_property.input) does NOT fire because the intersection is 29-41% of either side. At 30%, it fires. 50% catches only the strong-similarity case; 30% catches more drift with higher false-positive risk. Architect to weigh in. Defer to v0.1.1 if no pilot evidence yet.

### §9.2 `min_cluster_fields` default — N=4 or N=5?

N=4 catches the Hostpoint cluster (7 fields). N=5 still catches it. The trade-off is on small clusters: `name + description + slug + is_active` has 4 fields that could legitimately recur across resources. N=5 reduces noise on those. Probably N=5 if the architect wants tighter; defer otherwise.

### §9.3 Should the cross-feature advisory line (§4.1) fire as a DIAGNOSTIC instead of a hint?

v0.1 keeps it as an in-message hint to avoid double-counting (the intra-feature diagnostic fires AND a cross-feature advisory would fire). Promoting to a separate diagnostic is v0.2 territory; v0.1's hint is sufficient.

### §9.4 Should `VOCAB-RESOURCE-WIDE-CLUSTER-001` exclude resources matching its own name prefix?

A `BillingAddress` resource whose fields all share the `billing` token is structurally circular — the resource's name IS the token. v0.1 fires; the author opts out. v0.2 polish could detect this case and skip. See §10.4.

### §9.5 Should FK fields count toward K in Lint B?

v0.1 strips FKs to in-capsule resources via the universal filter (§5). FKs to scalar types (`Id`, `Text`) — like `provider_external_reference: Text required unique` — count. This matches the intent: an FK-heavy junction table with 8 FKs + 2 data fields = 2 post-filter, which is below K=10. The rule does not fire on junction tables. Acceptable.

### §9.6 Naming — `VOCAB-RESOURCE-WIDE-CLUSTER-001` vs `VOCAB-RESOURCE-FAT-001`?

v0.1 used `FAT`. Architect's review (P10) suggested either is acceptable per the `VOCAB-HANDLER-HEAVY-001` precedent. v0.2 uses `WIDE-CLUSTER` because the rule's actual signal is "wide resource AND name-cluster present" — "fat" alone misses the cluster axis. `VOCAB-HANDLER-HEAVY` is single-axis (handler ratio); this rule is two-axis (width AND clustering).

---

## §10. Polish items (post-PASS, tracked separately in `docs/next-checklist.md`)

- §10.1 — `VOCAB-SHADOW-RECORD-002` for cross-feature detection (gated on v0.1 pilot evidence).
- §10.2 — `lazuli inspect <Resource> --shadows` query mode.
- §10.3 — Coverage-mode summary (`lazuli doctor --vocab-coverage`) listing all shadow clusters in a capsule.
- §10.4 — Skip `VOCAB-RESOURCE-WIDE-CLUSTER-001` when the cluster token matches the resource's own name prefix (BillingAddress case).
- §10.5 — Levenshtein-tolerant name matching (`number` <-> `address_number` would match with edit distance ≤ 2).
- §10.6 — Optional-required widening tolerance: treat `Text optional` as matching `Text required` for shadow detection.
- §10.7 — Generate a refactor-skeleton code fence in the diagnostic message.
- §10.8 — Promote `VOCAB-RESOURCE-WIDE-CLUSTER-001` to `error` in production after two pilots confirm signal-to-noise ratio.

---

## §11. Revision history

- **v0.2 (2026-05-16)** — addressed five blockers from v0.1 grading (BLOCK 7.4/10): (B1) corrected Hostpoint anchors — `update_property.input` carries 7 not 8 address fields, missing `country`; cross-feature `host.Address` ↔ `catalog.Property` strict-name match is 7 of 8, name-drifted on `number`/`address_number`. §7 now describes honest expected surface at default thresholds (zero new diagnostics). (B2) Driver narrative pivoted to intra-feature case at §1.2; cross-feature deferred to §10.1 with structural rationale at §6.5. (B3) File paths corrected to `crates/lazuli_doctor/src/vocab/`; parent-catalog cleanup added as Cell C.2. (B4) Lint B detection extended to leading OR trailing snake-case tokens; §3.2 reconciles set-vs-sequence language. (B5) Universal-column filter §5 now strips FKs to peer resources in the capsule. Polish: P1 (narrative procedence moved to §11); P3 (cross-feature moved from §4.3 to §10); P5 (Shape A/B design history moved to §11); P7 (toml field names symmetric); P8 (effective-surface-table dropped — §6.1-§6.4 prose suffices); P9 (FK count clarified at §9.5); P10 (renamed `FAT` → `WIDE-CLUSTER` because the signal is two-axis: width AND clustering). Architect-noted polish P2 (default-K calibration) and P4 (Property post-filter count) folded into Cell A.2 tests + §7 worked examples. Polish P6 (cross-feature spec) tracked at §10.1.

- **v0.1 (2026-05-16)** — initial draft. Surfaced by Hostpoint `catalog.lzi` vs `host.lzi` address-shape duplication. Two rules, intra-feature only, advisory cross-feature line. Graded BLOCK 7.4/10 by `lazuli-language-architect`: five structural blockers (anchor inflation, scope-driver mismatch, file path conflict, suffix gameability, missing peer-FK filter), nine polish items. All addressed in v0.2.
