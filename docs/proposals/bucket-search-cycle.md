# Bucket Cycle: Search (L0→L1, design-only)

**Status**: design proposal. Stages 3–9 of the `bucket=search`
pipeline. **Implementation is Cut search gated** — runtime/codegen
stages are out of scope; this proposal stops at L1 (parser+IR+
inspect+doctor+LSP).

**Audience**: language team (Lazuli core). Lazuli Go runtime team is
informed but not on the hook.

**Date**: 2026-05-11.

## Contexto

The canonical fixture authors text matching twice
(`examples/full-capsule/full-capsule.lzi:97-98` and `:120-121`):

```
      search params.search over name, email
        mode contains
```

That clause sits inside `query.list customer.list` and
`query.list customer.global_search`. The legacy pest pipeline
parses it (`crates/lazuli_syntax/src/parser.rs:1012-1028`) into
`Query.search: Vec<String>`, but `lower_query`
(`crates/lazuli_analyzer/src/lib.rs:452-482`) **drops it on the
floor**. The comment at line 454-456 is explicit: *"Search
currently has no canonical home and is dropped on the floor; it
will return as a typed query construct in a later phase."*
`ListQuery` IR (`crates/lazuli_ir/src/lib.rs:647-668`) has no
search field; `lazuli inspect` emits zero `search` keys; doctor has
no cross-check between the searched fields and the resource's
field set; LSP has no hover/completion on `over`/`mode`. The
capability kind `search` exists in the closed catalog
(`crates/lazuli_lsp/src/lib.rs:8711`) but the fixture does not
author a `search <name>` capability — it inherits no L0 evidence
for the registry side.

The lowering route was decided in
`docs/proposals/bucket-search-scope.md` (canonical input for this
run): **Surface A + Route B** — keep the inline
`search params.<X> over <fields> mode <m>` clause as the canonical
authoring form, extend the legacy pest grammar to carry the
parsed fields, and lift to typed IR (`SearchSpec` on `ListQuery`)
through `lower_query`. Defer the speculative `index` / `facet` /
`ranking` top-level kinds (roadmap §1.27) until Cut search
opens with pilot evidence. Additive children `ranking <field>
<weight>` and `facets <fields>` join the inline clause in this
cut to satisfy DL=4 from audit §29.

The closed-cycle criterion (4 new doctor diagnostics,
`--expand=search` projection, LSP hover/completion on
`search`/`over`/`mode`/`ranking`/`facets`, no runtime work) is the
acceptance gate.

## Baseline (Stages 1-2 inventory)

| Layer | Status | Anchor |
|---|---|---|
| Surface syntax (`.lzi`) — `search params.X over <fields>` | authored, 2 use sites | `examples/full-capsule/full-capsule.lzi:97`, `:120` |
| Surface syntax — `mode <contains\|prefix\|fulltext>` | authored, 2 use sites | `examples/full-capsule/full-capsule.lzi:98`, `:121` |
| Registry capability `search <name>` | listed in LSP catalog; not authored in fixture | `crates/lazuli_lsp/src/lib.rs:8711`; `examples/full-capsule/registry.lzi` declares 7 other kinds, not search |
| Grammar (`docs/grammar.lzi.md:567,584`) | documents `search "by" ident_list` + `mode <contains\|prefix\|fulltext>` — **mismatch** with authored form (`search params.X over <fields>`) | both grammar.lzi.md and the pest grammar are stale |
| Pest grammar (`crates/lazuli_syntax/src/grammar.pest:23`) | `search_stmt = { "search" ~ ident_list }` — matches the legacy curly-brace form, not the canonical-indent form | vestigial |
| Legacy parser (`crates/lazuli_syntax/src/parser.rs:1012-1028`) | populates `Query.search: Vec<String>` from `search_stmt` | source plumbing exists |
| Analyzer lowering | **drops `Query.search` on the floor**; emits no IR | `crates/lazuli_analyzer/src/lib.rs:452-482` |
| Canonical-indent slice (`parse_feature_skeleton`) | does not cover `query` at all — Phase L Tier 4 outstanding | `crates/lazuli_syntax/src/parser.rs:1147-1173`; `docs/next-checklist.md` row 24 |
| IR (`crates/lazuli_ir`) | `ListQuery` has no `search` field | `crates/lazuli_ir/src/lib.rs:647-668` |
| LSP (file-local text walk) | one diagnostic only: rejects `field = params.search` (suggests `search params.search over ...`) | `crates/lazuli_lsp/src/lib.rs:1593`, `:15424` |
| Doctor cross-feature | none — zero `search`-aware diagnostics | confirmed via grep |
| Inspect projection | none — `lazuli inspect --format=json` emits zero `search` keys | confirmed via probe |
| Codegen | none — `crates/lazuli_codegen_go` references `search` only in views/queries equality-filter paths | confirmed via grep |
| Runtime (Lazuli Go) | none — Cut search gated | `runtime/go/lazuli/` has zero search helpers |
| Highlighting | `search` keyword colored generically; `over`/`mode`/`ranking`/`facets` not specially highlighted | `editors/vscode/syntaxes/lazuli.tmLanguage.json` |
| Adapter slot | `search` named in `is_allowed_capability_kind` closed set | `crates/lazuli_lsp/src/lib.rs:8711` |
| Capability layering | "Language declares `search params.q over ...`; runtime/adapters implement engines" | `docs/capability-layering.md:249` |
| Invariants | "Text matching uses `search params.<name> over <fields...>`; do not encode a contains search as `field = params.search`." | `docs/invariants.md:344-345` |

**Cross-cutting fact**: the search clause is the only sub-language
construct documented in `invariants.md` whose IR shape is
documented as missing in the same file
(`invariants.md:342-343`: "Declarative `search` ... do not derive
indexes" — index derivation skips it precisely *because* it has no
IR shape to read).

## Linguagem (Stage 3)

Surface is canonical for the existing `search params.X over <fields>`
+ `mode <m>` — already authored, already in invariants. Stage 3 is
**documentation + two additive children** (`ranking` + `facets`)
to satisfy DL=4 from audit §29, plus alignment of the grammar
documentation with the authored form.

### Formal grammar (EBNF, draft for `docs/grammar.lzi.md` §12)

Replaces the line at `docs/grammar.lzi.md:567`
(`"search" "by" ident_list ( "mode" search_mode )?`) — the
authored form has never been `search by ...`; this is a doc fix.

```ebnf
query_list_body   = ( ...
                    | search_clause
                    | ... )+ ;

search_clause     = "search" "params." IDENT_LOWER "over" field_list NEWLINE
                    INDENT search_child* DEDENT ;

search_child      = "mode" search_mode NEWLINE
                  | "ranking" ranking_list NEWLINE
                  | "facets" field_list NEWLINE ;

search_mode       = "contains" | "prefix" | "fulltext" ;

ranking_list      = ranking_entry ( "," ranking_entry )* ;
ranking_entry     = IDENT_LOWER "weight" INTEGER_POSITIVE ;

field_list        = IDENT_LOWER ( "," IDENT_LOWER )* ;
```

### Slot inventory (required/optional + type + closed catalog)

| Slot | Required | Type | Closed catalog | Fixture anchor |
|---|---|---|---|---|
| `search params.<X>` | yes (the param the clause reads from) | `params.` prefix + identifier matching a slot in `params` block | n/a — must resolve to a `Text`-typed param declared in same `query.list` | `full-capsule.lzi:90` declares `search: Text optional`; `:97` references `params.search` |
| `over <fields>` | yes (at least one field) | identifier list | n/a — each ident must resolve to a field on the resource backing this `query.list` | `full-capsule.lzi:97`: `over name, email` |
| `mode <m>` | optional (defaults to `contains`) | identifier | **closed**: `contains`, `prefix`, `fulltext` | `full-capsule.lzi:98`: `mode contains` |
| `ranking <field> weight <n>, ...` | **new — optional** | comma list of `<field> weight <positive int>` | weight ∈ ℤ⁺; field ∈ `over` list | not in fixture; Stage 3 adds to `customer.list` |
| `facets <fields>` | **new — optional** | identifier list | each ident must resolve to a field on the resource | not in fixture; Stage 3 adds to `customer.list` |

### Closed-catalog rationale

- `mode ∈ {contains, prefix, fulltext}` — these are the three
  distinct read-side contracts a text query can have:
  - `contains`: substring match (engine choice: SQL `ILIKE` /
    `position` / engine equivalent). The cheapest mode; no index
    required.
  - `prefix`: prefix match (engine choice: SQL prefix index /
    trigram / engine prefix tokenizer).
  - `fulltext`: tokenized fulltext (engine choice: PostgreSQL
    tsvector / Meilisearch / ES). Triggers the capability binding
    `search <name>` in registry.
  These three modes already in `grammar.lzi.md:584`; this cut
  promotes them from doc-only to IR-typed.
- `ranking` weights are integers ≥ 1 — the absolute value doesn't
  matter to the language (engine decides normalisation); only the
  relative ordering matters. Doctor warns if the same field
  appears twice with different weights.
- `facets` field list — each entry must resolve to a field on the
  resource backing the parent `query.list`. Field types are not
  constrained at the language level (engines differ on what's
  facetable: ES requires `keyword` type, Meilisearch is more
  permissive); doctor warns when a `facets` field is also a
  `derived` or `has_many` (those are not facetable in any
  engine).

### Example expansion in the fixture

Stage 3 extends `full-capsule.lzi:97-98` to make ranking and
facets explicit:

```lazuli
      search params.search over name, email
        mode contains
        ranking name weight 2, email weight 1
        facets tier, lifecycle_stage
```

And documents the `global_search` query at `:120-121` as the
admin counterpart — same clause, no ranking (admin search is
exhaustive, not relevance-ordered), no facets (admin shouldn't
facet across tenants):

```lazuli
      search params.search over name, email
        mode contains
```

The two new children are **additive** — every existing `search ...
over` clause without them keeps parsing.

## IR (Stage 4)

The IR shape needs a new struct carrying the parsed options.
Today's `ListQuery` (`crates/lazuli_ir/src/lib.rs:647-668`) has no
search field at all.

### IR additions

Two additive types. Recommended placement: next to `Filter` /
`OrderBy` / `KeyClause` at
`crates/lazuli_ir/src/lib.rs:706-729`.

```rust
// crates/lazuli_ir/src/lib.rs — additive

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchSpec {
    /// Name of the slot declared in the parent query's `params`
    /// block. Analyzer cross-checks that it exists and is
    /// `Text`-typed.
    pub param: String,
    /// At least one entry. Each entry must resolve to a field on
    /// the resource backing the parent `query.list`.
    pub over: Vec<String>,
    /// `None` parses as `contains` (default); analyzer normalises
    /// to `Some` after lowering so doctor reads from one axis.
    pub mode: Option<SearchMode>,
    /// `None` when not authored. Doctor warns on duplicate fields
    /// with different weights; analyzer dedups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ranking: Vec<RankingEntry>,
    /// `None` when not authored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Contains,
    Prefix,
    Fulltext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankingEntry {
    pub field: String,
    pub weight: u32,
}

// Existing ListQuery gains an additive field:
pub struct ListQuery {
    // ...existing fields unchanged...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchSpec>,
}
```

Schema-additive — every consumer that reads `ListQuery` without
the new field keeps working (Rust `#[serde(default)]` plus
`Option::is_none` skip).

### Surface → IR mapping

| Surface | IR field | Notes |
|---|---|---|
| `search params.search over name, email` on `query.list customer.list` | `ListQuery.search = Some(SearchSpec { param: "search", over: ["name", "email"], mode: Some(Contains), ranking: [], facets: [] })` | `mode` populated to default `Contains` by analyzer. |
| `mode contains` (child of `search`) | `SearchSpec.mode = Some(Contains)` | parsed; analyzer normalises absence to `Some(Contains)`. |
| `ranking name weight 2, email weight 1` | `SearchSpec.ranking = [RankingEntry { field: "name", weight: 2 }, RankingEntry { field: "email", weight: 1 }]` | order preserved; doctor warns on weight collisions. |
| `facets tier, lifecycle_stage` | `SearchSpec.facets = ["tier", "lifecycle_stage"]` | order preserved. |

### Inspect JSON shape (`lazuli inspect --format=json --expand=search`)

New top-level `--expand=search` flag in `ExpandSet`
(`crates/lazuli_cli/src/main.rs:98-118`, sibling to `storage`,
`auth`, `jobs`, etc.). Projection:

```json
{
  "features": [
    {
      "name": "customer",
      "search": {
        "queries": [
          {
            "query": "list",
            "search": {
              "param": "search",
              "over": ["name", "email"],
              "mode": "contains",
              "ranking": [
                { "field": "name", "weight": 2 },
                { "field": "email", "weight": 1 }
              ],
              "facets": ["tier", "lifecycle_stage"]
            },
            "origin": "examples/full-capsule/full-capsule.lzi:97"
          },
          {
            "query": "global_search",
            "search": {
              "param": "search",
              "over": ["name", "email"],
              "mode": "contains",
              "ranking": [],
              "facets": []
            },
            "origin": "examples/full-capsule/full-capsule.lzi:120"
          }
        ]
      }
    }
  ]
}
```

Normalisation rules:

- `mode` is always emitted (default `contains` is materialised at
  lowering so consumers don't have to know the default).
- `ranking` and `facets` are arrays, possibly empty; emitted only
  when non-empty (mirrors the `agent.tools` / `oauth` convention).
- Features without any search-bearing query have `search` omitted
  from the projection.
- Without `--expand=search` the `search` key is omitted entirely.

### Cross-refs the analyzer must register

| Edge | Source | Target | Resolution scope |
|---|---|---|---|
| `search.param` ↔ `params.<X>` | `SearchSpec.param` on a `ListQuery` | the parent query's `params` block must declare a slot with this name | query-local |
| `search.over.<field>` ↔ resource field | each `SearchSpec.over` entry | a field on the resource backing the parent `query.list` (resolved via the `domain.resource` chain) | feature-local |
| `search.facets.<field>` ↔ resource field | each `SearchSpec.facets` entry | same as above; additionally must not be a `derived` or `has_many` field | feature-local |
| `search.ranking.<field>` ↔ `search.over.<field>` | each `RankingEntry.field` | must be present in the `over` list | query-local |
| `ListQuery.search.mode = fulltext` ↔ `search <name>` capability | any `ListQuery` with `mode: Fulltext` | the app/registry must declare a `search <name>` capability | package-wide (mirror of `APP-CAP-001` for storage) |
| `ListQuery.search` + `ListQuery.scope_override` ↔ `policy` | any search-bearing query with `scope_override = true` | must declare explicit `policy @policy.*` (extension of the existing `scope override` invariant) | query-local; today the invariant is documented but not search-aware |

## Codegen (Stage 5)

**Cut search gated** — out of scope. The cycle stops at L1.
When Cut search opens, codegen and runtime work proceeds against
the now-stable IR JSON.

For traceability, the expected shape (informational only):

- A `dist/go/<feature>/search.gen.go` file per feature with a
  search-bearing query, mounting a typed `SearchContract`
  (mirroring `FileContract` from the storage bucket) consumed by
  the runtime `search` capability.
- A `runtime/go/lazuli/search/` package with `contract.go` +
  `engine.go` + adapter contract (`Engine` interface with
  `Query`, `Index`, `Reindex` methods).
- Adapters (`@runtime/postgres-fts`, `@plugin/meilisearch`,
  `@plugin/typesense`, `@plugin/elastic`) implementing `Engine`.

The language stops at typed IR; the engine choice is registry-
adapter territory. Language never names Meilisearch / ES /
PostgreSQL.

## Runtime (Stage 6)

**Cut search gated** — out of scope. No new files in
`runtime/go/lazuli/`. Runtime work waits on Cut search opening.

For traceability, the expected adapter contract (informational
only):

```go
// runtime/go/lazuli/search/adapter.go (NOT codegen-generated,
// NOT shipped in this cut)
type Engine interface {
    Query(ctx context.Context, idx IndexKey, q string, opts QueryOpts) (Results, error)
    Index(ctx context.Context, idx IndexKey, doc Document) error
    Delete(ctx context.Context, idx IndexKey, id string) error
    Reindex(ctx context.Context, idx IndexKey) error
}
```

Adapters live in their own packages
(`@runtime/postgres-fts`, `@plugin/meilisearch`, ...). The
`search <name>` capability binding selects one at boot from
`registry.lzi`. Lazuli core never names a provider.

## Evals/Testes (Stage 7)

### Doctor fixture — `over` references unknown field

`crates/lazuli_cli/tests/fixtures/search/over_field_unknown.lzi`:

```lzi
feature x
  domain
    resource Customer
      id: ID required
      name: Text required

    query.list list
      params
        q: Text optional

      search params.q over name, nonexistent
        mode contains
```

Asserts that doctor emits **exactly one**
`search_field_unknown_diagnostics` at the `over name, nonexistent`
line, naming `nonexistent`.

### Doctor fixture — `scope override` without explicit `policy`

`crates/lazuli_cli/tests/fixtures/search/scope_override_no_policy.lzi`:
authors a `query.list` with `scope override` + `search params.q
over name` but no `policy`. Asserts
`search_scope_override_missing_policy_diagnostics` fires.

### Doctor fixture — `mode fulltext` without `search` capability

`crates/lazuli_cli/tests/fixtures/search/fulltext_no_capability.lzi`:
authors a `query.list` with `search params.q over name mode
fulltext` but the package's `registry.lzi` / `app.lzi`
capabilities declare no `search <name>`. Asserts
`search_capability_unbound_diagnostics` fires.

### Doctor fixture — unknown mode

`crates/lazuli_cli/tests/fixtures/search/mode_unknown.lzi`:
authors `mode trigram`. Asserts `search_mode_unknown_diagnostics`
fires.

### LSP test — hover + completion on `search` clause

`crates/lazuli_lsp/tests/search.rs`:

- Hover on `search` keyword inside `query.list` shows
  "Declarative text search; reads `params.<X>` over `<fields>`
  with `mode <contains|prefix|fulltext>`."
- Hover on `over` shows "Comma-separated fields the search reads
  against; each must be a field on the resource backing the
  query."
- Hover on `mode` shows the closed catalog.
- Hover on `ranking` shows "Per-field relative weights;
  positive integers. Each field must appear in `over`."
- Hover on `facets` shows "Aggregation buckets returned alongside
  results; each field must be a resource field (not `derived` or
  `has_many`)."
- Completion at `mode |` offers exactly `contains`, `prefix`,
  `fulltext`.

### Inspect contract test

`crates/lazuli_cli/tests/inspect_search.rs`: runs
`lazuli inspect --format=json --expand=search examples/full-capsule`
and asserts the `search` projection matches the JSON shape in
Stage 4 (typed args, normalisation rules, omission of features
without search-bearing queries).

### Roundtrip test on the canonical fixture

`crates/lazuli_cli/tests/search_roundtrip.rs`: asserts the fixture
parses, lowers, and projects without warnings (`lazuli doctor
examples/full-capsule` stays clean after the cut).

### Go integration test

**Cut search gated** — no runtime test in this cut. When Cut search
opens, a `runtime/go/lazuli/search/search_test.go` mirrors the
storage `testing/synctest` pattern.

## Doctor/LSP (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `search_field_unknown_diagnostics` | error | "`search params.<X> over <Y>` references field `<Y>` that does not exist on resource `<R>` backing query `<Q>`." | typed IR shows a `SearchSpec.over` entry not in the resource's field set | `over_field_unknown.lzi` above |
| `search_scope_override_missing_policy_diagnostics` | error | "`query.list <Q>` declares `scope override` and `search params.<X>` but no explicit `policy`; cross-tenant search must declare its policy explicitly." | typed IR shows `ListQuery.scope_override = true` + `ListQuery.search.is_some()` + no `policy` on the query | `scope_override_no_policy.lzi` above |
| `search_capability_unbound_diagnostics` | error | "feature `<F>` has search-bearing query `<Q>` with `mode fulltext` but app/registry declares no `search <name>` capability." | typed IR shows `SearchSpec.mode = Fulltext` and the package's `AppManifest.capabilities` has no `search` entry | minimal `.lzi` per fixture above |
| `search_mode_unknown_diagnostics` (typed promotion) | error | "`search ... mode <X>` is not in the closed catalog; expected `contains`, `prefix`, or `fulltext`." | analyzer rejects the mode literal | `mode_unknown.lzi` above |
| `search_facets_field_invalid_diagnostics` | warning | "`search ... facets <field>` references `<field>` which is a `derived`/`has_many` field; engines cannot facet on non-stored fields." | typed IR shows a `SearchSpec.facets` entry whose resolved field is `derived` or `has_many` | minimal `.lzi` with `facets <derived_field>` |
| `search_ranking_field_not_in_over_diagnostics` | error | "`search ... ranking <field> weight <n>` references `<field>` which is not in the `over` list." | typed IR shows a `RankingEntry.field` not in `SearchSpec.over` | minimal `.lzi` with mismatched ranking |

The four headline diagnostics (the first four) satisfy the
closed-cycle criterion "doctor carries ≥3 cross-feature
diagnostics". The two warnings are additive polish.

All six codes register under
`is_security_enforcement_code`
(`crates/lazuli_lsp/src/lib.rs:9527`) only for the
`search_scope_override_missing_policy_diagnostics` (the security-
relevant one). The other five are correctness diagnostics,
unrelated to the strict/production profile escalation.

### Diagnostic anchors (where to add)

- `search_field_unknown_diagnostics` — `crates/lazuli_cli/src/doctor.rs`
  cross-feature pass; reads `ListQuery.search.over` and the
  resource's field set (already harvested by `ResourceFact` at
  `crates/lazuli_cli/src/doctor.rs:629-640`).
- `search_scope_override_missing_policy_diagnostics` — same pass;
  reads `ListQuery.scope_override` + `ListQuery.search` + the
  query's `policy` axis.
- `search_capability_unbound_diagnostics` — same pass; mirror of
  `APP-CAP-001` (`crates/lazuli_cli/src/doctor.rs:1328-1336`).
- `search_mode_unknown_diagnostics` — file-local in LSP (typed
  shape rule) and cross-feature in doctor (same check to catch
  packaged code that bypassed LSP).
- `search_facets_field_invalid_diagnostics` — cross-feature in
  doctor; reads resource field `kind` (derived/has_many/normal).
- `search_ranking_field_not_in_over_diagnostics` — file-local in
  LSP (typed shape rule); also cross-feature in doctor.

### LSP hovers (new entries)

Add to `KEYWORD_HOVER` in `crates/lazuli_lsp/src/lib.rs`:

| Keyword (in `query.list` search-clause context) | Hover summary |
|---|---|
| `search` (inside `query.list`) | "Declarative text search. Shape: `search params.<X> over <fields>` with optional `mode <contains\|prefix\|fulltext>`, `ranking <field> weight <n>, ...`, `facets <fields>`. Inherits tenant scope and policy from the parent query." |
| `over` (after `search params.X`) | "Comma-separated fields the search reads against; each must be a field on the resource backing the query." |
| `mode` (under `search`) | "Search mode. Closed catalog: `contains` (substring), `prefix` (prefix match), `fulltext` (tokenized; requires `search <name>` capability)." |
| `ranking` (under `search`) | "Per-field relative weights; positive integers. Each field must appear in `over`. Engines decide normalisation; only relative ordering matters." |
| `facets` (under `search`) | "Aggregation buckets returned alongside results; each field must be a resource field (not `derived` or `has_many`)." |

Closed-catalog completions to add:

- `mode |` → `contains`, `prefix`, `fulltext`.
- (no completion for `over` / `ranking` / `facets` — fields are
  resource-specific, handled by the existing field-completion
  path for `filters`).

### Namespaces (`is_allowed_reference_namespace`)

No new namespace required. `params.<X>` already in the closed
catalog (`crates/lazuli_lsp/src/lib.rs:2114-2135` and
`docs/invariants.md:233-249`). `search` capability kind already in
the closed set (`crates/lazuli_lsp/src/lib.rs:8711`).

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

`search` already colored as a generic keyword. Add `over`,
`ranking`, `facets`, `weight`, `mode` (in search context),
`contains`, `prefix`, `fulltext` to the query-clause scope.

### Grammar doc fix

`docs/grammar.lzi.md:567` documents `search "by" ident_list` but
the authored form is `search params.<X> over <fields>`. Stage 8
fixes the EBNF to match the authored form (per Stage 3 grammar
above). This is a documentation-only edit; no parser code
changes (the legacy pest grammar is wrong too — `grammar.pest:23`
will be extended under Route B, see Stage 4).

## Critério de "ciclo fechado"

- [ ] Fixture exercises typed `search` with `ranking` + `facets`
  on `customer.list` (Stage 3 extends `full-capsule.lzi:97-98`
  per the inline examples above).
- [ ] `lazuli check examples/full-capsule` accepts the syntax
  after Route B lands (no regression on existing pre-typed
  `search` clauses — additive children only).
- [ ] `lazuli inspect --format=json --expand=search
  examples/full-capsule` shows the IR shape described in Stage 4
  for `customer.list` and `customer.global_search`.
- [ ] `lazuli doctor` emits the 4 headline diagnostics (`field_unknown`,
  `scope_override_missing_policy`, `capability_unbound`,
  `mode_unknown`) on the matching fixtures.
- [ ] LSP hovers + completion cover the 5 keywords + 1 closed
  catalog from Stage 8.
- [ ] `docs/grammar.lzi.md:567` aligns with the authored form.
- [ ] `docs/invariants.md` updated to note that the search IR
  shape has landed (replaces the existing
  `invariants.md:342-345` note that the clause exists but is
  not lowered).
- [ ] **Cut search gated** — no codegen, no runtime, no Lazuli Go
  work. Items 5-7 of `docs/roadmap.md:44-53` stay unchecked for
  the search bucket until Cut search opens.

## Próximo passo

Human approval of this proposal **and** the scope proposal
(`docs/proposals/bucket-search-scope.md`) + a separate
`mode=implement` run that lands Route B:

1. Extend `crates/lazuli_syntax/src/grammar.pest:21-23` with a
   richer `search_stmt` matching the authored form
   (`"search" "params." ident "over" ident_list (NEWLINE INDENT
   ("mode" mode | "ranking" ranking_list | "facets" ident_list)*
   DEDENT)?`).
2. Extend `crates/lazuli_syntax/src/parser.rs:1006-1029`
   `parse_query` to populate a new `Query.search_spec:
   Option<SearchSpec>`.
3. Add `SearchSpec` / `SearchMode` / `RankingEntry` to
   `crates/lazuli_ir/src/lib.rs` next to `Filter` / `OrderBy`.
4. Add `ListQuery.search: Option<SearchSpec>` (additive).
5. Extend `crates/lazuli_analyzer/src/lib.rs:452-482`
   `lower_query` to emit `ListQuery.search` (and remove the
   "dropped on the floor" comment).
6. Add `ExpandSet.search`
   (`crates/lazuli_cli/src/main.rs:98-118`) and the inspect
   projection per Stage 4.
7. Ship the 4 headline doctor diagnostics + 2 polish warnings +
   LSP hovers per Stage 8.
8. Update `docs/grammar.lzi.md:567` per Stage 3.
9. Update `docs/invariants.md:342-345` per closed-cycle criterion.

Runtime team has **no deliverable** for this cycle — Cut search
gated.

When Phase L Tier 4 (`parse_query` in canonical-indent slice)
lands, the search clause promotes from the legacy pest pipeline
into the slice without IR changes — `SearchSpec` stays
unchanged.

## Rows sugeridas para `docs/next-checklist.md`

Two additions, formatted to match the existing table:

```
| 38 | Search bucket cycle Route B — typed `search` clause lowering | planned | Surface A (inline `search params.X over <fields> mode <m>`) + additive `ranking` + `facets` children. Add `SearchSpec`/`SearchMode`/`RankingEntry` to `crates/lazuli_ir/src/lib.rs`. Extend `crates/lazuli_syntax/src/grammar.pest` + `parser.rs:1006-1029` to carry parsed fields. Replace `lower_query`'s "dropped on the floor" with typed emission. New `--expand=search` projection. Implementation **Cut search gated** — design only. See `docs/proposals/bucket-search-cycle.md` §Linguagem/§IR + `docs/proposals/bucket-search-scope.md`. |
| 39 | Search bucket cycle — 4 doctor diagnostics + LSP coverage | planned | `search_field_unknown`, `search_scope_override_missing_policy`, `search_capability_unbound`, `search_mode_unknown` (typed promotion) + 2 polish warnings (`search_facets_field_invalid`, `search_ranking_field_not_in_over`). LSP hovers for 5 keywords (`search`, `over`, `mode`, `ranking`, `facets`) + closed-catalog completion for `mode`. Grammar doc fix at `docs/grammar.lzi.md:567`. Depends on row 38. See `docs/proposals/bucket-search-cycle.md` §Doctor/LSP. |
```

No row 40 needed — runtime/codegen rows are **Cut search gated**
and will be added when the cut opens with pilot evidence.
