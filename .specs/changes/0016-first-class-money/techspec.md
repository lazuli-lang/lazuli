---
id: 0016
title: first-class Money — amount + currency + scale as one typed unit
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
track: evolve/ship
test_gate: "cargo test -p lazuli_syntax money_type && cargo test -p lazuli_codegen_go money && lazuli check + doctor + go build clean in BOTH pilots"
agent: unassigned
---

# TechSpec — first-class Money type

## Approach
Add a first-class money type to the grammar/IR/codegen carrying `(amount, currency, scale)` as one typed unit with an enforced currency. Lower it to representation-preserving storage (cents-integer + currency-text) so the migration off the three existing encodings produces no DB drift. Add ONE doctor rule covering all three drift shapes; subsume the existing string-tagged-only `VOCAB-MONEY-MULTI-CURRENCY-001`. Migrate both pilots. Currency code stays app data (no baked-in locale).

## Surface
**Modify (language):**
- `crates/lazuli_keywords/src/registry/sections/` — register the `Money` field type (next to other field types/`@semantic.*`), with completion/hover copy.
- `crates/lazuli_syntax/src/parser/lzi/...` (field-type parsing) — parse `Money` with its currency binding + optional scale, e.g. `amount: Money(currency: BRL, scale: 2)` or `amount: Money currency: <field>` (final surface chosen in BUILD; the contract is amount+currency+scale inseparable).
- `crates/lazuli_syntax/src/ast/` — AST node for the money type (amount, currency-binding, scale).
- `crates/lazuli_ir/src/nodes/` — IR: money as a typed unit (amount + currency + scale), NOT three loose fields.
- `crates/lazuli_codegen_go/src/emitter/types/` + `emitter/resource/struct_emit_p1.rs` — emit Go representation (amount minor-units integer + currency string, or a Money value type) and `emitter/migration_ddl/create_table.rs` DDL columns matching the storage shape.

**Create (enforcement):**
- `crates/lazuli_doctor/src/vocab/money_field_shape_001.rs` — `VOCAB-MONEY-SHAPE-001`: fires on (a) a money-amount field with no currency, (b) `<x>_cents: Integer` + `<x>_currency: Text` pair, (c) bare `Decimal` named like money (`amount`/`price`/`*_price`/`total`) with no currency sibling. Subsumes the string-tagged-only check in `vocab_money_multi_currency_001.rs` (retarget/merge, don't duplicate the missing-sibling logic).

**Modify (teaching):**
- `docs/lazuli_way/money.md` — fill the stub (0001 idiom shape).
- `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — money idiom bullet ("Reach for `Money`, not `@semantic.Money` string / `_cents:Integer+_currency:Text` / bare `Decimal`").
- `docs/keyword-reference.md` / `docs/grammar.lzi.md` — document the `Money` type.

**Migrate (BOTH pilots):**
- hostpoint `payments/payments.lzi:61,70,71,72` (`amount`/`amount_currency`/`platform_fee`/`net_to_host`) + command/result money fields `:134-136,144-146,164`; re-point `payments.web.lzx:7,12` (`amount_cents`, `platform_fee_cents`) and `payments/handlers/*.go` SQL (`create_checkout_preference.go:75` `amount, amount_currency`).
- hostpoint `catalog/catalog.lzi:205-206` (`price_amount_cents`/`price_currency`, 7 pairs) + `operations/operations.lzi:51,103,125` (`total_amount_cents`); re-point `operations.web.lzx:11` and the `_cents`-keyed handler SQL across `catalog`/`operations`/`payments` `.go`.
- pauta `media_price_tables.lzi:89,106` (`price_per_cm_col`, `price`) + `hoxo_financial_integration.lzi:51,128` (`amount`).
- Remove the now-redundant "implied BRL" / currency-override comments (e.g. `payments.lzi:62-69`).

## Contracts
- **Type contract**: a money field carries amount + currency + scale; currency is REQUIRED (no money-without-currency state). Currency value is app data (no framework default/enum of currencies).
- **Storage contract**: lowers to representation-preserving columns (minor-units integer + currency text) so migrating `_cents:Integer + _currency:Text` is a rename/re-type with **zero** value drift; migrating string-tagged `"BRL:0.00"` and bare `Decimal` preserves stored value/scale.
- **Drift rule**: `VOCAB-MONEY-SHAPE-001` is the single source of truth for money-shape; `VOCAB-MONEY-MULTI-CURRENCY-001` no longer fires independently for the string-tagged form (merged).
- **DoD block (embedded verbatim — see Gate).**
- **Idiom-doc shape (from 0001):** `# Money` / `## Reach for this` / `## Before (hand-rolled) / After (idiomatic)` (cite `catalog.lzi:205-206` + `payments.lzi:61,70` + `hoxo_financial_integration.lzi:51` before; `Money` after) / `## Enforced by VOCAB-MONEY-SHAPE-001`.

## Plan — for the executing agent
1. **BUILD-lang**: add the `Money` field type (registry + parser + AST). Decide the currency-binding surface (inline `currency:`/scale). Test `money_type` in `lazuli_syntax`.
2. **BUILD-ir/codegen**: IR money-as-unit; Go struct + DDL representation-preserving storage. Test `money` in `lazuli_codegen_go`.
3. **ENFORCE**: write `VOCAB-MONEY-SHAPE-001` (3 drift cases) + tests; merge the string-tagged check out of `vocab_money_multi_currency_001.rs`.
4. **MIGRATE-hostpoint**: convert payments + catalog + operations money fields; re-point `.lzx` columns and `_cents`-keyed handler SQL; `lazuli check && doctor && go build ./...` clean.
5. **MIGRATE-pauta**: convert media_price_tables + hoxo money fields; clean.
6. **TEACH**: fill `docs/lazuli_way/money.md`; add CLAUDE.md.tmpl + AGENTS.md.tmpl bullet; update keyword-reference + grammar docs.

## Tests first (TDD)
- [ ] `money_type` (lazuli_syntax) — `Money` parses with required currency + scale; a money field without currency is a parse/analyze error.
- [ ] `money` (lazuli_codegen_go) — Money lowers to minor-units integer + currency string columns; round-trips a value.
- [ ] `vocab_money_shape_001` — fires on missing-currency, `_cents:Integer+_currency:Text`, and bare-`Decimal`-as-money; silent on `Money`.
- [ ] `money_multi_currency_subsumed` — the old string-tagged missing-sibling case is now reported by `VOCAB-MONEY-SHAPE-001`, not a separate rule.
- [ ] `pilots_money_zero_value_drift` (gate) — both pilots' generated money columns preserve stored value/scale vs pre-migration.

## Gate

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**4 concrete gates:**
1. `cargo test -p lazuli_syntax money_type && cargo test -p lazuli_codegen_go money` green.
2. `lazuli check && lazuli doctor && go build ./...` clean in BOTH hostpoint and pauta-web; generated money columns show zero value/scale drift.
3. `docs/lazuli_way/money.md` filled per the 0001 shape; CLAUDE.md.tmpl + AGENTS.md.tmpl bullet present.
4. `VOCAB-MONEY-SHAPE-001` fires on all 3 pre-migration encodings and is silent after; its code is named in the idiom doc.

## Risks & rollback
- **Storage drift** (re-typing `_cents`/`Decimal`) corrupts stored values → mitigation: representation-preserving lowering + `pilots_money_zero_value_drift` gate; migrate `_cents` as a no-op column re-type, not a value transform.
- **Handler SQL references `*_cents`/`*_currency` by name** (`create_checkout_preference.go`, `get_service_dashboard.go`, etc.) → mitigation: enumerate every `_cents`/`_currency` SQL site in the migrate cell; re-point before `go build`.
- **Scope creep into FX/arithmetic** → OUT; existing arithmetic rules unchanged.
- **Rollback**: `git revert`. Language change is additive (new type); pilot migration revert restores the three encodings.
