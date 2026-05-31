---
id: 0010
title: Escape-hatch visibility rules — ESC-RAWSQL-IN-HANDLER / ESC-SQL-TENANCY-CONTRACT / ESC-SCOPE-OVERRIDE-UNGUARDED
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: true
track: ship/prove
test_gate: "cargo test -p lazuli_doctor esc_rawsql_in_handler && cargo test -p lazuli_doctor esc_sql_tenancy && cargo test -p lazuli_doctor esc_scope_override && lazuli check . && lazuli doctor ."
agent: unassigned
---

# TechSpec — Escape-hatch visibility rules

## Approach
Three new doctor rules in a new `escape_hatch` category in `crates/lazuli_doctor`. `ESC-RAWSQL-IN-HANDLER-001` reuses the existing Go handler walker (`handler_walker.rs`) to AST-scan `handlers/*.go` for `lazuli.DB().Query(` / `QueryRow(` call sites carrying a multi-line string literal, then cross-references the feature `.lzi` IR to see whether the effect is declared only as an opaque `fn ...: Function[...]`. The other two parse `query.sql` files and their `.lzi` `query.sql` blocks: one checks binding-style consistency + declared params; one checks for a tenant predicate or an `@actor.<privileged>` policy. No grammar/codegen/runtime change. The idiom doc (filled by 0001) gains the three rule codes in its "Enforced by" lines.

## Surface
**Create:**
- `crates/lazuli_doctor/src/escape_hatch/mod.rs` — category module.
- `crates/lazuli_doctor/src/escape_hatch/rawsql_in_handler_001.rs` — Go-AST scan + `.lzi` cross-ref.
- `crates/lazuli_doctor/src/escape_hatch/sql_tenancy_contract_001.rs` — binding-style + declared-param check.
- `crates/lazuli_doctor/src/escape_hatch/scope_override_unguarded_001.rs` — tenant-predicate / `@actor` guard check.
- `crates/lazuli_doctor/src/escape_hatch/preset.rs` — per-preset severities.
- `crates/lazuli_doctor/tests/esc_rawsql_in_handler.rs`
- `crates/lazuli_doctor/tests/esc_sql_tenancy.rs`
- `crates/lazuli_doctor/tests/esc_scope_override.rs`
- `crates/lazuli_doctor/tests/fixtures/escape_hatch/` — fixtures: `hidden_sql_handler.go` + paired `.lzi` (opaque fn); `declared_query_sql.go` + `.lzi` (the well-declared negative); `mixed_binding.sql`; `undeclared_param.sql`; `positional_clean.sql`; `no_tenant_no_guard.sql`; `no_tenant_with_actor.sql` (the `list_all_agencies` shape, must pass).

**Modify:**
- `crates/lazuli_doctor/src/lib.rs` — register the `escape_hatch` category.
- `crates/lazuli_doctor/src/rule_category.rs` — add the three codes + summaries.
- `crates/lazuli_diagnostics_registry/...` — register codes, titles, help one-liners.
- `docs/lazuli_way/escape-hatch-decision-tree.md` — add the three rule codes to the "Enforced by" lines (the doc was created by 0001 naming `ESC-RAWSQL-IN-HANDLER-001` as "incoming"; flip it to shipped + add the other two).
- `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — reference the three codes next to the decision-tree link (byte-identical edit in both).

## Contracts

**`ESC-RAWSQL-IN-HANDLER-001` (Warn; waivable-to-convert, not waivable-to-silence):**
- Detect: in any `features/<f>/handlers/*.go`, an AST call to `lazuli.DB().Query(` or `.QueryRow(` whose first SQL argument is a multi-line string literal.
- Fire: when the feature `<f>`'s `.lzi` declares the matching effect only as `fn <name>: Function[...]` with **no** `query.sql` and **no** `returns` shape — i.e. the read is invisible to inspect/exposure/LSP.
- Canonical fire: `trust/handlers/list_property_reviews.go:42-71` (6-table JOIN) ∩ `trust.lzi` declaring it as opaque `fn`.
- Body: names the file:line and states "convert this read to a declared `query.sql`/`query.compose` (see `docs/lazuli_way/escape-hatch-decision-tree.md`); a bare `# doctor:allow` records debt, it does not resolve the finding."
- `# doctor:allow` honored mechanically (uniform allow-comment contract); enforcement-to-convert is by message + grader convention.

**`ESC-SQL-TENANCY-CONTRACT-001` (Warn):**
- Fire when a `query.sql` body mixes `:named` and `$N` markers, OR references a param (`:x` or `$N`) not declared in the `.lzi` `query.sql` block's `params`.
- Canonical fire: `notifications/queries/unread_count.sql` (`:user_id`/`:org_id`) in a project whose prevailing contract is positional `$N` ("tenancy NOT auto-injected", per `admin_panel.lzi:55-57` / `dashboard.lzi:47`).
- The "prevailing contract" is read from the project's other `query.sql` blocks, not hardcoded, so a future auto-inject build doesn't make the rule lie.

**`ESC-SCOPE-OVERRIDE-UNGUARDED-001` (Warn):**
- Fire when a `query.sql`'s SQL has no tenant predicate in WHERE (no `org_id`/tenant-axis filter) AND the query carries no `@actor.<privileged>` policy.
- Canonical **pass**: `admin_panel/list_all_agencies.sql` — no tenant filter, but guarded by `@actor.super_admin` on the query → does NOT fire (a comment alone would).
- Canonical fire: a sibling cross-tenant query with the guard removed.

**Idiom-doc "Enforced by" lines (in `escape-hatch-decision-tree.md`):**
```
## Enforced by
ESC-RAWSQL-IN-HANDLER-001 — raw SQL in a @fn Go handler with no declared query.sql/returns
ESC-SQL-TENANCY-CONTRACT-001 — query.sql mixing :named/$N or referencing an undeclared param
ESC-SCOPE-OVERRIDE-UNGUARDED-001 — query.sql with no tenant predicate and no @actor.<privileged> guard
```

## Plan — for the executing agent
1. Stand up `escape_hatch/mod.rs` + register the category in `lib.rs` / `rule_category.rs` / diagnostics registry.
2. Build `rawsql_in_handler_001.rs` on top of `handler_walker.rs`: find `DB().Query(`/`QueryRow(` call sites with multi-line string args; cross-ref the feature `.lzi` for an opaque-`fn`-only declaration; emit with the convert-not-silence body.
3. Build `sql_tenancy_contract_001.rs`: scan `query.sql` for binding markers; derive the prevailing contract from sibling `query.sql` blocks; fire on mix or undeclared param.
4. Build `scope_override_unguarded_001.rs`: detect missing tenant predicate + missing `@actor.<privileged>` policy.
5. Add fixtures (positive + the two must-pass negatives: `declared_query_sql` and `no_tenant_with_actor`).
6. Write the three test files (TDD list below).
7. Update `escape-hatch-decision-tree.md` "Enforced by" lines + the two scaffold templates.
8. Run `lazuli doctor .` on hostpoint + pauta-web to confirm the three canonical fires fire and the two negatives stay silent.

## Tests first (TDD)
- [ ] `rawsql_hidden_in_handler_fires` — `hidden_sql_handler.go` + opaque-`fn` `.lzi` → `ESC-RAWSQL-IN-HANDLER-001` fires; body cites file:line + "convert, don't silence."
- [ ] `declared_query_sql_silent` — a handler whose read IS a declared `query.sql`/`returns` does NOT fire.
- [ ] `rawsql_allow_records_debt_not_fix` — a `# doctor:allow ESC-RAWSQL-IN-HANDLER-001` suppresses the finding but the test asserts the body marks it as debt-to-convert (message contract).
- [ ] `mixed_binding_fires` — `mixed_binding.sql` (`:x` + `$1`) → `ESC-SQL-TENANCY-CONTRACT-001`.
- [ ] `undeclared_param_fires` — `.sql` references `:org_id` not in the `.lzi` `params` → fires.
- [ ] `positional_clean_silent` — a `$N`-only query with all params declared → silent.
- [ ] `no_tenant_no_guard_fires` — `no_tenant_no_guard.sql` → `ESC-SCOPE-OVERRIDE-UNGUARDED-001`.
- [ ] `no_tenant_with_actor_silent` — the `list_all_agencies` shape (`@actor.super_admin`, no tenant filter) → silent.

## Gate

### Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_doctor esc_rawsql_in_handler && esc_sql_tenancy && esc_scope_override` green (all 8 TDD cases).
2. **MIGRATE** — `lazuli doctor .` confirms the three canonical fires on the pilots (hostpoint trust `list_*`; pauta `unread_count.sql`; a guard-removed `list_all_agencies` sibling) and the two negatives (`declared_query_sql`, `list_all_agencies` with `@actor.super_admin`) stay silent. (Pilot *fixes* are 0011/0013; this gate proves the rules are correct.)
3. **TEACH** — `docs/lazuli_way/escape-hatch-decision-tree.md` "Enforced by" lines carry the three rule codes (the `ESC-RAWSQL-IN-HANDLER-001` "incoming" note from 0001 flipped to shipped); both scaffold templates reference them.
4. **ENFORCE** — each rule fires on its fixture and is named in the idiom doc.

## Risks & rollback
- **`ESC-RAWSQL` false-positives on a non-read DB call** (a vendor-required imperative statement) → mitigation: require the matcher to target `Query`/`QueryRow` returning rows + a multi-line literal; the `declared_query_sql_silent` fixture guards the boundary. Tighten the matcher, don't relax to default-silence.
- **Tenancy "prevailing contract" detection mis-reads a single-file project** (no siblings to compare) → mitigation: when there is only one `query.sql`, fall back to the positional-`$N` default and only fire on the *mix* / undeclared-param sub-checks, not on a contract inference.
- **Scope-override rule can't see a tenant predicate expressed unusually** (e.g. a JOIN-implied scope) → mitigation: the `@actor.<privileged>` guard branch passes those; the rule only fires when BOTH the predicate AND the guard are absent.

**Rollback:** `git revert` — three additive doctor rules + one doc edit + two template edits; no pilot is modified here, so reverting leaves the pilots exactly as found.
