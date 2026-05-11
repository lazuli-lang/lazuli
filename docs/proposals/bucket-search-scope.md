# Search Lowering Scope (pre-design)

**Status**: pre-design investigation. Resolves a side-quest blocker
discovered during the `bucket=search` pipeline Stage 1+2 inventory,
so Stage 3 (design-language) runs against the correct scope.

**Audience**: language team, runtime team, anyone touching the search
bucket cycle.

**Date**: 2026-05-11.

## Context

The `bucket=search` inventory cataloged every search-related
construct in the canonical fixture, IR, analyzer, doctor, LSP,
codegen, and runtime. The picture is **L0 for two distinct surfaces,
L1 for neither, and L2 for none**:

1. **Surface A — inline `search` clause inside `query.list`**. The
   fixture authors
   `search params.search over name, email` followed by
   `mode contains` at
   `examples/full-capsule/full-capsule.lzi:97-98` and again at
   `:120-121` (the `global_search` admin query). The grammar
   documents it at `docs/grammar.lzi.md:567`
   (`"search" "by" ident_list ( "mode" search_mode )?`) — though the
   authored form is `search params.<name> over <fields>` rather than
   the grammar's `search by <fields>`, which is itself a documented
   inconsistency.
2. **Surface B — `search` as a capability kind in `registry.lzi`**.
   The LSP's closed catalog
   (`crates/lazuli_lsp/src/lib.rs:8711`) lists `search` alongside
   `database`, `queue`, `object_storage`, `mailer`, `event_bus`,
   `tracing`, `cache`. The canonical fixture does **not** author this
   today (`examples/full-capsule/registry.lzi:13-19` declares the
   other six, not `search`) — the LSP accepts it but the fixture has
   no L0 evidence.
3. **Surface C — invariant prose**.
   `docs/invariants.md:342-345` documents the inline form as the
   canonical text-matching mechanism ("text matching uses
   `search params.<name> over <fields...>`; do not encode a contains
   search as `field = params.search`") and explicitly excludes
   declarative `search` from index derivation
   (`invariants.md:342-343`).

The defining gap:

- `lower_query` (`crates/lazuli_analyzer/src/lib.rs:452-482`) drops
  `search` on the floor. The comment at line 454-456 is unambiguous:
  *"Search currently has no canonical home and is dropped on the
  floor; it will return as a typed query construct in a later
  phase."* The legacy `Query` syntax struct
  (`crates/lazuli_syntax/src/parser.rs:1012-1028`) parses `search`
  via the pest `search_stmt` rule
  (`crates/lazuli_syntax/src/grammar.pest:23`) into a
  `search: Vec<String>` field — but the analyzer's lowering
  discards it. Net result: `ListQuery` in IR
  (`crates/lazuli_ir/src/lib.rs:647-668`) has **no search field at
  all**.
- `lazuli inspect --format=json examples/full-capsule/full-capsule.lzi`
  emits **zero** `search` keys for `query.list customer.list` or
  `customer.global_search` (confirmed by probe). The clause is
  invisible to inspect, doctor, codegen, and runtime.
- The canonical-indent slice
  (`crates/lazuli_syntax/src/parser.rs:1147-1173`) covers `agent`
  (Cut A), `auth` (Phase L Tier 1), `@cap.File(args)` (Tier 2), and
  `job`/`webhook`/`notification`/`event_group` (Tier 3). **Queries
  are still in Tier 4** (row 24 of `docs/next-checklist.md`, status
  *prerequisite (Tier 4 outstanding)*). Until Tier 4 lifts
  `parse_query`, any search lowering work must either ride the
  legacy pest pipeline (the search clause is dropped after parsing)
  or wait on Tier 4.
- Roadmap §1.27 (`docs/roadmap.md:265-269`) lists three speculative
  kinds: `index`, `facet`, `ranking`. None of them are authored in
  the fixture; none of them have IR; none of them are referenced
  anywhere outside the roadmap and the framework-coverage audit.
- Audit §29 (`docs/audit/framework-coverage-1400.md:354-360`)
  classifies search as **F (Cut search gated)**: language is
  "nenhum" today, DL is 4 (`index` kind + filters/ranking/facets
  declarable + tenant-scoped + permission-scoped), DF is 4, DA is
  10 with Meilisearch primary.

The Cut search gate is what makes this scope investigation
necessary: the search bucket is **design only** (mode=design); the
language team must decide which of the two L0 surfaces (inline
`search` clause vs `index` kind) is the canonical home before any
implementation run can proceed.

## The two competing surfaces

### Surface A — inline `search params.<X> over <fields>` (status quo)

The authored form sits inside `query.list <name>` next to `filters`
/ `scope` / `params` / `paginate`. It reuses the existing query
read-path machinery: `params` declare the typed input, `policy`
gates the read, `scope`/`scope override` define tenancy, `paginate`
caps results.

**Strengths**:

- Already authored in the fixture (`full-capsule.lzi:97`, `:120`)
  and documented as invariant (`invariants.md:342-345`).
- Inherits tenant scope, policy, and pagination from `query.list`
  for free — no separate plumbing for cross-tenant safety.
- One canonical authoring path for "I want a list of X filtered
  and text-matched."
- Tenant-scoped by default (matches §0 audit's tenant_from
  invariant) because `query.list` is.

**Weaknesses**:

- The text clause `search params.X over a, b` is **parsed but not
  lowered** — IR shape is missing entirely.
- `mode <contains|prefix|fulltext>` (`grammar.lzi.md:584`) implies
  three different engines under the hood (SQL `ILIKE`, SQL prefix
  index, full-text index). No way today to declare *which engine*
  backs which mode — provider mechanics leak into the language if
  this widens.
- No place to declare facets, ranking, weights, synonyms — those
  are intrinsic to declarative search, but the inline clause has
  no shape for them.
- The clause is one-shot per query — a feature with five
  searchable queries restates the field list five times.

### Surface B — `index <name>` kind in resource

A new top-level kind under `feature.<F>.domain.resource.<R>`:

```
resource Customer
  index search_index
    fields name, email, company
    facets tier, lifecycle_stage
    ranking name weight 2, email weight 1
    language en
```

Queries reference the index by name:
`search params.q against index.search_index`.

**Strengths**:

- One declaration site per resource — five queries can share one
  index without restating fields.
- Facets and ranking have a natural home (fields and ordering are
  intrinsic to the index, not to the query).
- Maps cleanly to Meilisearch/Typesense/ES schemas: an index
  resource and its facets/ranking are the typical primary
  authoring axis in those backends.
- Doctor gets a single source of truth for "what fields are
  searchable on this resource."
- Pilot-gated kinds `index` / `facet` / `ranking` from roadmap
  §1.27 land naturally as children of this kind.

**Weaknesses**:

- Authoring cost — the inline form is "one line" today; this
  surface requires a separate block.
- Tenant scope and policy are intrinsic to the **query**, not the
  index. A `search params.q against index.search_index` still
  needs to inherit tenant scope from `query.list`'s parent context.
- Risk of duplicating `filters` (which already declare equality
  facet-ish behaviour on `query.list`).
- Tenant scope on the index itself has no obvious axis — is the
  index tenant-scoped (one index per tenant, like a Meilisearch
  multi-index pattern) or tenant-filtered (one index, filter by
  tenant on read)? Both are real product patterns, both are
  adapter-flavoured.

## Comparison

| Axis | Surface A (inline clause) | Surface B (`index` kind) |
|---|---|---|
| Already in fixture | yes (`full-capsule.lzi:97`, `:120`) | no — speculative |
| Already in invariants | yes (`invariants.md:344-345`) | no |
| Already in grammar | yes (`grammar.lzi.md:567`) | no |
| IR shape today | none (`crates/lazuli_analyzer/src/lib.rs:454-456` drops it) | none |
| Pilot evidence | `customer.list` + `customer.global_search` exercise it | none |
| Roadmap §1.27 mapping | `search params.q ...` is the read clause; no kind | `index`/`facet`/`ranking` map directly |
| Audit §29 DL mapping | partially: "search filters/ranking/facets declaráveis, tenant-scoped via tenant_from, permission-scoped via policy" — the inline clause covers the first two only loosely | full: a typed kind expresses all four DL items |
| Tenant scope | inherits from `query.list` (correct by default) | needs explicit decision (per-tenant vs filtered) |
| Policy scope | inherits from `query.list` (correct by default) | requires re-declaration on each consuming query |
| Boundary discipline | low risk: no provider names; mode catalog `{contains, prefix, fulltext}` is closed | low risk: capability binding (`search <name>` in registry) selects engine, adapter resolves to Meilisearch/Typesense/ES |
| LLM authoring difficulty | low: it reads as "this query searches X over Y, Z" | medium: requires understanding indices as separate first-class objects |
| LLM reading difficulty | low: the clause is self-explanatory in context | medium: query references an index by name; reader has to find the index declaration |

## Recommendation

**Take Surface A as the canonical home, defer Surface B as
pilot-gated.**

The reasoning:

1. **Fixture pressure** is on Surface A. The canonical exercise
   authors `search params.X over <fields>` twice, in the two
   queries that motivated the search feature in the first place
   (`customer.list` + `customer.global_search`). Surface B exists
   nowhere except the roadmap. Cut search is gated precisely
   because no fixture evidence justifies the `index` kind yet.
2. **Tenant + policy inheritance is correct by default** on
   Surface A. Putting the search clause inside `query.list` means
   it inherits the same `scope`, `scope override`, and `policy`
   that the rest of the query carries. Doctor's existing
   cross-tenant rules apply transparently. Surface B would
   introduce a second axis where tenant scope can drift
   independently of the query that reads it — that's exactly the
   kind of contract gap the search bucket cycle is supposed to
   close.
3. **The DL=4 audit list is satisfiable on Surface A.** The four
   declarative requirements are: search filters, ranking, facets,
   tenant-scoped, permission-scoped. Three of those (tenant,
   permission, filters) are already inherited; ranking and facets
   can ride as **additive children** of the inline `search` clause
   (`ranking <field> <weight>`, `facets <fields>`) without
   inventing a new top-level kind.
4. **Pilot-gating Surface B is consistent with Cut admin / Cut
   media / Cut billing** — kinds that wait for product pressure
   before promoting from speculative roadmap entries to typed
   primitives.
5. **Phase L Tier 4 dependency is mitigated.** Surface A's
   lowering needs `parse_query` in the canonical-indent slice,
   which Tier 4 owns. Until Tier 4 lands, Route C — extending the
   legacy pest pipeline to **carry** the parsed `search`/`over`/
   `mode` instead of dropping it — buys IR coverage now, with the
   slice promotion landing when Tier 4 fires. (See routes below.)

## Routes A vs B vs C

Three ways to close the search lowering gap; all honour the
language/runtime boundary:

### Route A — wait for Phase L Tier 4, then lower in the canonical-indent slice

Add `auth`-style children (`parse_query_search_clause`) inside
`parse_query` once Tier 4 lands. IR `SearchSpec` (proposed Stage 4)
lives on `ListQuery`. Inspect/doctor pick it up.

### Route B — write a new pest rule for `search params.X over Y mode Z`, lower it from the legacy pipeline

Extend the pest grammar
(`crates/lazuli_syntax/src/grammar.pest`) with a richer
`search_stmt` matching the authored form. Extend
`crates/lazuli_syntax/src/parser.rs:1012-1028` `parse_query` to
populate a new `Query.search_spec: Option<SearchSpec>`. Extend
`lower_query` (`crates/lazuli_analyzer/src/lib.rs:452-482`) to
emit `ListQuery.search`. Doctor + inspect consume the typed shape.

### Route C — text-pattern facts (the `CommandApprovalFact` shape)

Extract a `QuerySearchFact` walker in doctor (next to
`CommandApprovalFact` at `crates/lazuli_cli/src/doctor.rs:4046-4155`)
that harvests `search params.X over fields ... mode <m>` by
indent-walking the source. Drive doctor and inspect from the
fact. IR stays empty.

### Comparison

| Axis | Route A (slice) | Route B (legacy pest) | Route C (text-pattern) |
|---|---|---|---|
| Upfront cost | depends on Tier 4 timing; ~1 cell of additional lowering on top | ~2 cells (pest rule + parser fix + lower_query extension) | ~1 cell (one walker + diagnostic emission) |
| Maintenance | one canonical home in the slice; no drift | two homes (pest + slice) until Tier 4 retires legacy; drift risk | drift risk against the LSP's text-walk + new bucket-specific text-walks |
| Cross-checks possible | all — typed IR feeds doctor cross-feature | all — typed IR feeds doctor cross-feature | same set; brittle |
| LSP coverage | typed AST → hover/completion for `over`, `mode`, ranking weight syntax | same | text-walk only |
| Compat with Phase L | aligned: the slice gains queries with search already lifted | misaligned: Tier 4 then needs to re-lift the search clause | misaligned: adds a third text-pattern fact family (after `CommandApprovalFact` and the storage walker) |
| Time-to-IR | gated on Tier 4 (no ETA today) | available now | available now |
| Risk | clean | adds legacy-pest debt that Tier 4 has to retire | adds doctor-walker debt |

### Recommendation: Route B

The Stage 3 design proposal (`bucket-search-cycle.md`) writes
against **Route B**: extend the pest rule and `lower_query`. The
search clause is already parsed; the only missing step is carrying
the parsed fields into IR. Route A is blocked on Tier 4; Route C
re-introduces text-pattern facts for a construct whose IR can be
defined today.

The legacy-pest debt Route B incurs is bounded — when Tier 4 lifts
`parse_query` into the canonical-indent slice, the search clause
moves with it; `SearchSpec` on `ListQuery` stays as-is.

## Pilot-needed vs Speculative

The DL=4 audit list maps as follows:

### PILOT-NEEDED — exercised by the canonical fixture today

| Construct | Fixture evidence | Justification |
|---|---|---|
| `search params.<X> over <fields>` | `full-capsule.lzi:97`, `:120` | Already authored; needs lowering. The whole reason this bucket exists. |
| `mode <contains \| prefix \| fulltext>` | `full-capsule.lzi:98`, `:121` | Already authored; closed catalog already in `grammar.lzi.md:584`. |
| Tenant-scoped search (via `tenant_from` on the parent query) | Inherited at `full-capsule.lzi:83` (`query.list list` under `customer` resource with default tenancy from `app.lzi`) | Already correct by inheritance; needs doctor check that tenant scope is **not** dropped by a `scope override` on a search-bearing query. |
| Permission-scoped search (via `policy` on the parent query) | `full-capsule.lzi:111` (`policy @policy.global_read` on `global_search`) | Already correct; needs doctor check that a search query with `scope override` declares explicit `policy`. |

### PILOT-NEEDED-EXTENSION — additive children to the inline clause

| Construct | Justification |
|---|---|
| `ranking <field> <weight>` | Closed shape: integer weight per field. Pilot evidence is the asymmetric importance of `name` vs `email` in `customer.list`'s `search` clause. Cheap addition; no new kind. |
| `facets <fields>` | Mirror of `filters`-as-faceting. Already authored implicitly via `filters` blocks; the explicit `facets` child makes the search-side semantics typed (lazy filters are equality; facets are aggregation buckets returned by the search engine). |

### SPECULATIVE — defer pending pilot pressure

| Construct | Status | Why defer |
|---|---|---|
| `index <name>` top-level kind | Roadmap §1.27. Not in fixture. | Surface B above. No product evidence that the cost of a separate authoring axis pays off; inline + ranking + facets covers DL=4. |
| `facet` as a decorator on a `resource` field | Roadmap §1.27 | Conflicts with `filters` semantics (which already covers field-level equality). Promote when a fixture queries against an external search engine where field-level facetability is a per-field property. |
| `synonyms` | Audit §29 F | Engine-specific (Meilisearch vs ES treat synonyms differently); deferred to Cut search. |
| `multilingual` / `language <lang>` on the index | Audit §29 F | Speculative until a product authors multilingual content. |
| `highlighting` | Audit §29 F | Adapter-side concern; the language declares the contract via mode, not the rendering of matches. |
| `analytics` (search analytics) | Audit §29 DF | Runtime/adapter concern; the language declares the contract via `event.trace search_run` (Tier-3-style built-in trace event). |
| `async indexing` job | Audit §29 DF | Runtime concern — declarative job dispatching from event subscribers; no language addition needed. |
| `reindex` CLI | Audit §29 DF | CLI/admin concern. Not language. |

The pilot-needed subset is exactly the clause the fixture already
authors. Speculative additions wait for a real pilot exercising
them (e.g., a product whose search behaviour can't be expressed
with `search params.X over fields mode <m>` + ranking + facets).

## Closed-cycle criterion for the search bucket

Adapted from `docs/roadmap.md:44-53` (the 8-item §0 checklist) to
the specific shape of the search bucket — **language-side only**;
runtime is gated on Cut search:

- [ ] **Fixture authors the full surface.** The canonical fixture
  exercises `search params.X over fields mode contains`
  (`full-capsule.lzi:97-98`, `:120-121`). Already true. Stage 3
  design extends this with `ranking` + `facets` children — must
  not regress existing authoring.
- [ ] **`lazuli check` accepts the syntax.** Already true (legacy
  pipeline accepts; cycle adds the additive children).
- [ ] **`lazuli inspect --expand=search` projects the IR.** New
  projection; required deliverable. Uses the new `SearchSpec` IR
  struct (Stage 4 of `bucket-search-cycle.md`).
- [ ] **`lazuli doctor` carries ≥3 cross-feature diagnostics for
  search.** Concrete proposals (named in cycle.md §Doctor):
  - `search_field_unknown_diagnostics` — `search ... over <X>`
    references a field that does not exist on the resource backing
    the `query.list`.
  - `search_scope_override_missing_policy_diagnostics` — a
    search-bearing `query.list` with `scope override` does not
    declare explicit `policy`. (Extension of the existing
    `scope override` invariant `invariants.md:350`.)
  - `search_capability_unbound_diagnostics` — a feature with a
    search-bearing query references no `search` capability in
    app/registry. (Mirror of `APP-CAP-001` for storage.)
  - `search_mode_unknown_diagnostics` — `mode <X>` is not in the
    closed catalog `{contains, prefix, fulltext}`. (Upgrade of
    `grammar.lzi.md:584`.)
- [ ] **`lazuli generate` produces Go that compiles.** Cut search
  gated — out of scope for this design.
- [ ] **Lazuli Go executes end-to-end search.** Cut search gated — out
  of scope.
- [ ] **`eval`/test coverage.** Doctor fixture coverage only at
  this stage. Runtime tests gated on Cut search.
- [ ] **LSP hover/completion on search children.** Hover for
  `search`, `over`, `mode`, `ranking`, `facets`;
  closed-catalog completion for `mode` (`contains` / `prefix` /
  `fulltext`).

The first four items + last item are language-team Stage 3
deliverables. Items 5-7 are runtime-team **after Cut search
opens** — explicitly out of scope for this proposal.

This list is attainable in a single Stage 3 design cut once the
Route B lowering lands; nothing on it depends on speculative
primitives.

## Recommendation

1. **Take Surface A** (inline `search params.<X> over <fields>` +
   additive `ranking` + `facets` children). Defer Surface B
   (`index` top-level kind) to Cut search pilot evidence.
2. **Take Route B** (extend the legacy pest pipeline) for now.
   Promote the lowering to the canonical-indent slice when Phase L
   Tier 4 (`parse_query`) lands.
3. **Scope Stage 3 design to the PILOT-NEEDED subset only.** Four
   children — the existing `params.X / over / mode` + the additive
   `ranking <field> <weight>` + `facets <fields>`. Stage 3's job
   is to tighten the contract (typed IR, closed catalogs, doctor
   diagnostics, LSP hover), not invent new kinds.
4. **Defer SPECULATIVE additions** until Cut search pilot evidence
   surfaces. `index` kind, `facet`/`ranking` as decorators,
   `synonyms`, multilingual, highlighting, search analytics stay
   in roadmap §1.27 + audit §29 F until pilot pressure justifies
   promotion. This proposal does not promote any of those items.
5. **Run Stage 3 with the closed-cycle criterion above as the
   acceptance gate.** Anything that doesn't shrink the gate counts
   as speculative and goes to backlog. **Implementation is Cut
   search gated** — Stage 3 is *design only*.
6. **Update `docs/next-checklist.md` row 24** (`Phase L`) only
   after Tier 4 lands and the search clause promotes from the
   legacy pipeline into the slice. Do not edit row 24 as part of
   this proposal.

When Route B is implemented, the search bucket cycle ships:
typed `SearchSpec` IR on `ListQuery`, `--expand=search`
projection, 4 doctor diagnostics, 5 LSP hovers, 1 closed-catalog
completion. Stage 4 (Lazuli Go codegen) waits on Cut search opening.
