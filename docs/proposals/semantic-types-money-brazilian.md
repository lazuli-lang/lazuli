# Semantic Types: `Money` Surface + `@plugin/scalars-br` Locale Pack (v0.3)

**Status**: L0 design proposal v0.3 — addresses v0.2 architect review (5 blockers). v0.1 BLOCKED at 7.05/10, v0.2 BLOCKED at 7.56/10. Closes `WAR-VOCAB-SEMANTIC-01` (Money surface) + paths for `WAR-VOCAB-SEMANTIC-02` (`@plugin/scalars-br`, deferred to companion proposal) from the Hostpoint port audit.

**Audience**: Lazuli compiler team (`crates/lazuli_syntax`, `crates/lazuli_analyzer`, `crates/lazuli_codegen_go`, `crates/lazuli_codegen_ts`, `runtime/go/lazuli/`), DSL authors, AI agents emitting `.lzi`.

**Date**: 2026-05-16.

**Pilot bucket**: scalar / semantic types. This proposal does NOT widen the namespace catalog (`@plugin/...` and `@semantic.*` already exist). It binds shape + storage + codegen to the IR variants `SemanticMoney` + `SemanticCurrency` that already live at `crates/lazuli_ir/src/lib.rs:704-717`.

**First consumer**: Hostpoint. 6 Money-shaped columns across 3 features; 4 Brazilian-scalar fields across 4 features.

**Companion docs**:
- `docs/scope-discipline.md` §71, §117 — canonical name `@plugin/scalars-<locale>`.
- `docs/audit/hostpoint-port-workarounds-2026-05-16.md` `WAR-VOCAB-SEMANTIC-01..02` (storage = `NUMERIC(20,4)`; plugin = `@plugin/scalars-br`; types = `@semantic.BrazilianCPF` family).
- `docs/invariants.md` §"Builtin Types", §"Manifest scope".
- `crates/lazuli_ir/src/lib.rs:704-717` — `SemanticMoney` + `SemanticCurrency` variants.
- `runtime/go/lazuli/types.go:35-46` — existing `Money = int64`, `Currency = string` aliases.

**v0.2 → v0.3 changes** (architect review 2026-05-16, second cycle):
- **B1 — Decimal dependency named explicitly**: `shopspring/decimal` becomes a real new Lazuli runtime dep with wire-principle justification (the alternative `pgtype.Numeric` is rejected for ergonomic + cross-feature reasons documented below).
- **B2 — Bare `Money` ↔ `Decimal` polysemy named as a breaking change**: today `crates/lazuli_analyzer/src/lib.rs:1301` resolves `"Money"` to `BuiltinType::Decimal`. After this proposal it resolves to `SemanticMoney`. The analyzer ships an explicit doctor migration lint (`VOCAB-MONEY-002`) that warns on every existing `field: Money` declaration before the breaking flip lands, and the flip itself happens in a tagged release after one full advisory cycle.
- **B3 — Default literal semantics**: `"BRL:1990"` form **eliminated**. Only `"BRL:1990.00"` (explicit decimal) is accepted; bare integer fragments after `:` are rejected with a diagnostic citing the 100×-error trap.
- **B4 — `default_currency` slot moves under existing `locale` block**: composes with the existing `locale / default "pt-BR"` vocabulary at `examples/full-capsule/app.lzi:9-12` rather than opening a new top-level slot.
- **B5 — Single-slot Money, no auto-paired escape hatch**: `amount: Money` is the only authoring shape. Authors who want per-row currency override write `amount: Money` and override the currency at insert time via a domain-function or update command; the explicit-pair shorthand (`amount: Money; amount_currency: Currency`) is **rejected** by the analyzer.

**v0.1 → v0.2 changes** (kept; first review cycle):
- Renamed `@plugin/locale-br` → `@plugin/scalars-br` (canon per `docs/scope-discipline.md`).
- Renamed `@br.CPF` → `@semantic.BrazilianCPF` (extends closed `@semantic.*` catalog, no new namespace).
- Storage layout reconciled with audit: `NUMERIC(20,4)`.
- Preserved `Money = int64` alias; added `MoneyValue` struct on top.
- Removed `Lazurite.toml [i18n]` (violated manifest invariants).
- Generalized plugin-type contribution mechanism — no hardcoded `scalars-br` in the analyzer.
- Migration story specified.
- Sub-cent precision settled in v1 (`NUMERIC(20,4)`).

---

## Problem

(Unchanged from v0.1 — the cents+currency pair drift across 6 columns and the regex-validator duplication across 9 handler sites are well-documented in the audit doc. See `WAR-VOCAB-SEMANTIC-01..02` for the full case.)

The novel observation v0.2 surfaces: the IR **already has** `SemanticMoney` and `SemanticCurrency` (`crates/lazuli_ir/src/lib.rs:704-717`). The grammar parses neither today. This proposal binds a grammar surface to the existing IR variants — it is a **plumbing fix**, not a vocabulary invention.

---

## Proposal — two tiers, two separate proposals

> **Note**: v0.1 bundled Money (core) + Brazilian-scalars (plugin) in one proposal. Per the architect review they are separate blast radii: Money is universal core grammar; Brazilian-scalars exercises a generic plugin-type-contribution mechanism that itself is new. **This proposal ships only Tier 1 (Money) end-to-end.** Tier 2 (`@plugin/scalars-br`) is summarized at the bottom and tracked as a separate proposal (`semantic-types-plugin-locales.md`) once the generic plugin-type mechanism it depends on is graded.

### Tier 1: `Money` surface — binds existing IR variants to authorable grammar

#### Grammar surface

```lzi
domain
  resource Charge
    amount: Money required = "BRL:0"
    platform_fee: Money required = "BRL:0"
    net_to_host: Money required = "BRL:0"
```

`Money` is the field-type name. It resolves to `BuiltinType::SemanticMoney` in the IR.

**One canonical default-value form**: `"<ISO 4217>:<decimal>"` where `<decimal>` MUST contain a decimal point. Examples:
- `"BRL:0.00"` — 0.0000 BRL
- `"USD:19.99"` — 19.9900 USD
- `"BRL:19.90"` — 19.9000 BRL
- `"JPY:0."` — 0 JPY (zero-decimal currency; trailing dot still required for unambiguity)

**Rejected forms** (closes B3 from v0.2 review — silent 100× errors):
- `"BRL:0"` (no decimal point — author could have meant 0 cents or 0 reais; analyzer rejects with `expected decimal point in Money literal`).
- `"BRL:1990"` (same; integer-without-decimal ambiguity).
- `"R$ 0"`, `"$10"` (symbol-prefixed — too many locale dialects).
- `"0"`, `"19.99"` (no ISO 4217 prefix — except when a `locale / default_currency` is declared and the analyzer infers the currency; see "Capsule-level default currency" below).

One form, parsed left-to-right by `ISO:Decimal` shape.

#### Storage

Per audit's removal criterion (`docs/audit/hostpoint-port-workarounds-2026-05-16.md:134`):

```sql
amount NUMERIC(20,4) NOT NULL DEFAULT 0,
amount_currency TEXT NOT NULL DEFAULT 'BRL'
CHECK (length(amount_currency) = 3 AND amount_currency = upper(amount_currency))
```

Two columns: a NUMERIC for the amount (20 digits, 4 decimal places — covers atomic crypto satoshi 1e-8 with room, also FX rates) and a TEXT for the ISO 4217 code with a length+case check constraint.

**Why NUMERIC(20,4) over BIGINT cents**:
- Sub-cent precision (FX rates, gold, fractional shares) without re-storage migration.
- No integer-overflow ceiling at ~$92 quadrillion for the few products that ever hit it.
- Arithmetic is exact under Postgres NUMERIC semantics.
- Matches the audit's already-graded removal criterion.

Trade-off: NUMERIC arithmetic is slower than BIGINT. Lazuli explicitly chooses correctness over hot-path speed for money — the few money-arithmetic hot paths (high-frequency trading, etc.) belong outside the Lazuli scope per `docs/scope-discipline.md`.

#### Capsule-level default currency

Authors avoid repeating `"BRL:0.00"` 50 times by declaring a default under the existing `locale` block in `app.lzi` (closes B4 from v0.2 review):

```lzi
# examples/full-capsule/app.lzi already has:
app Hostpoint
  locale
    default "pt-BR"
    supported "pt-BR", "en-US"
    fallback "en-US" -> "pt-BR"
    default_currency BRL    # ← this proposal adds this line
```

`default_currency` is a sub-key of `locale`, composing with the existing `locale / default` + `supported` + `fallback` vocabulary. Doctor enforces that the currency is a valid ISO 4217 code via the same closed catalog used in field defaults.

Then field defaults shorten:

```lzi
amount: Money required = "0.00"          # uses Hostpoint locale default = BRL
amount: Money required = "USD:0.00"      # opts into another currency, full ISO:Decimal form
```

The `"0.00"` (no ISO prefix) form is the ONLY way to skip the ISO prefix and is permitted ONLY when `locale / default_currency` is declared. Otherwise the parser rejects: "Money default missing ISO 4217 prefix; declare `locale / default_currency` in app.lzi or use `<ISO>:<decimal>`".

`default_currency` lives under `locale` in `app.lzi`, NOT in `Lazurite.toml` — `docs/invariants.md:560-565` forbids locale-aware settings in the manifest. The composition with existing `locale` vocabulary is Rule Zero: no new top-level slot is opened.

#### IR + analyzer changes

```
crates/lazuli_ir/src/lib.rs:
  - SemanticMoney + SemanticCurrency variants exist (no change).
  - Add MoneyDefault struct: { currency: String, units: Decimal } parsed from "<ISO>:<decimal>".

crates/lazuli_syntax:
  - Recognize `Money` as a field-type name.
  - Parse default literal `"<ISO 4217>:<decimal>"`.
  - Reject symbol-prefix forms; emit diagnostic suggesting the canonical form.

crates/lazuli_analyzer:
  - Resolve `Money` → BuiltinType::SemanticMoney.
  - Validate default currency against ISO 4217 (closed list bundled with lazuli — 180 codes).
  - Validate default value parses as Decimal.
  - When a field is Money typed and the resource has no other Money-currency column, AUTO-EMIT the paired SemanticCurrency column at codegen time (it's an implementation detail, not authored).
```

#### Codegen output

```go
// generated dist/go/payments/resource.gen.go:
import "github.com/shopspring/decimal"
import "lazuli.dev/runtime/lazuli"

type Charge struct {
    ID            lazuli.ID            `db:"id"             json:"id"`
    Amount        decimal.Decimal      `db:"amount"         json:"amount"`
    AmountCurrency lazuli.Currency     `db:"amount_currency" json:"amount_currency"`
    // ... (paired column auto-emitted; authored as single `amount: Money`)
}
```

**Decimal dep — explicit new runtime requirement** (closes B1 from v0.2 review):

`shopspring/decimal` becomes a new direct dependency of `runtime/go/lazuli/`. It is NOT transitively available via `pgx/v5` (verified by `go list -m all` in `runtime/go/`; v0.2 claimed otherwise — corrected here).

Alternatives considered:
1. **`pgtype.Numeric` from pgx/v5**: rejected because (a) the type carries a `Status` (Null/Present/Undefined) field that leaks pgx semantics into every field reference, (b) it has no arithmetic methods — every consumer site re-implements `Add`/`Sub`/`Mul` against `big.Int + scale`, (c) the JSON encoding is non-standard (`pgtype.Numeric` doesn't round-trip cleanly through `encoding/json` without a custom marshaller). Net: forces 100+ LOC of boilerplate per consumer for the win of "no new dep."
2. **`math/big.Float`**: rejected — base-2 float is the wrong storage for money. Rounding behavior is `RoundToNearestEven` by default, breaks every test that expects banker's rounding off-by-one cents.
3. **Roll our own**: explicit Lazuli anti-pattern per `CLAUDE.md` "Wire principle" — `shopspring/decimal` is 100+ KLOC of edge-case-handled arbitrary-precision arithmetic with `5.5M downloads/mo` (https://pkg.go.dev/github.com/shopspring/decimal). Reimplementing is the textbook trap.

Wire-principle compliance: this **is** a new dep. The runtime go.mod gains one line. The Lazuli `MoneyValue` type wraps `decimal.Decimal` plus `Currency` with thin convenience methods (`BRL(units string) MoneyValue`, `Format(locale string) string`); zero arithmetic logic owned by Lazuli — `Add`/`Sub`/etc. delegate to `decimal.Decimal` directly. Total Lazuli-side LOC: ~80 (Format delegates to `golang.org/x/text/currency`, also a new dep but already used elsewhere in the runtime — verified at `runtime/go/lazuli/i18n.go` if present, else added with the same explicit-dep stance).

```ts
// generated dist/ts-web/payments/payments.gen.ts:
import type { Money } from "@lazuli/runtime";

export interface Charge {
  id: ID;
  amount: Money;  // { amount: string (decimal); currency: string }
}
```

**TS Money interface**:
```ts
export interface Money {
  amount: string;   // decimal as string for wire safety; JS number loses precision at 2^53
  currency: string; // ISO 4217 3-letter uppercase
}

export function formatMoney(m: Money, locale = "pt-BR"): string {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: m.currency,
  }).format(Number(m.amount));
}
```

`Intl.NumberFormat` handles minor-unit count per ISO 4217 — JPY/KRW format without decimals, BHD with 3, BRL/USD with 2. Closes v0.1 polish item #5.

#### Backward compatibility — two distinct concerns

**Runtime alias (preserved)**: `runtime/go/lazuli/types.go:42` `type Money = int64` stays. The new struct lives at `lazuli.MoneyValue`. Code calling `var m lazuli.Money = 1990` continues to compile. Schedule the alias rename to `lazuli.MoneyMinorUnits` or deletion in a tagged breaking release once no consumer references the alias.

**DSL bare-`Money` polysemy (named breaking change, closes B2 from v0.2 review)**:

Today `crates/lazuli_analyzer/src/lib.rs:1301` resolves the bare DSL keyword `"Money"` to `BuiltinType::Decimal`. After this proposal lands it resolves to `BuiltinType::SemanticMoney`. Any existing `.lzi` author who wrote `price: Money` to mean "decimal scalar" silently changes:
- Column shape: `NUMERIC(20,6)` (Decimal) → `NUMERIC(20,4) + amount_currency TEXT` (Money pair).
- Go type: `decimal.Decimal` (Decimal) → `lazuli.MoneyValue` (Money struct).
- TS type: `number` (Decimal) → `Money` interface (`{ amount, currency }`).

**Migration cycle** (not silent):

1. **Release N (this proposal lands)**: analyzer continues to resolve `"Money"` → `BuiltinType::Decimal`, BUT doctor emits new lint `VOCAB-MONEY-002` (warn) on every `field: Money` declaration with message: "The bare `Money` keyword will resolve to `SemanticMoney` (currency-aware) in release N+1. If you meant a plain decimal scalar, rename to `field: Decimal`. If you meant currency-aware money, declare the field in app.lzi's locale block + add `# doctor:allow VOCAB-MONEY-002` once migrated."
2. **Release N+1 (one minor cycle later)**: analyzer flips the resolution. `field: Money` now means `SemanticMoney`. Authors who ignored the lint get a louder error (`VOCAB-MONEY-002` upgrades to `BLOCK`) plus the schema-shape change is now real.
3. **Hostpoint pilots `field: Money` from release N** — the audit doc closes `WAR-VOCAB-SEMANTIC-01` upon Hostpoint migration; the audit explicitly notes the one-release advisory cycle.

This is the only acceptable path for a breaking-semantic-flip of a closed-grammar keyword. Hard-flipping in release N silently changes every downstream DDL — unacceptable. Forking the keyword (`Money2` / `MoneyAware`) is the textbook Rule Zero violation — accepted as the discipline cost.

#### Migration of existing `cents: Integer` columns

Per `docs/proposals/migrations.md` (when authored — for now, follows the existing `lazuli plan` workflow):

1. Author edits `Charge.amount_cents Integer` → `Charge.amount Money` in `.lzi`.
2. Codegen emits two new columns (`amount NUMERIC(20,4)`, `amount_currency TEXT`) in the next `CREATE TABLE IF NOT EXISTS` block.
3. Author writes a hand-rolled `NN_charge_money.sql` migration:
   ```sql
   ALTER TABLE charge ADD COLUMN amount NUMERIC(20,4) NOT NULL DEFAULT 0;
   ALTER TABLE charge ADD COLUMN amount_currency TEXT NOT NULL DEFAULT 'BRL';
   UPDATE charge SET amount = amount_cents::numeric / 100, amount_currency = 'BRL';
   ALTER TABLE charge DROP COLUMN amount_cents;
   ```
4. Until `lazuli plan diff` lands (separate proposal), hand-rolled is the explicit, audit-visible path.

#### Doctor lints

```
VOCAB-MONEY-001 (warn): resource has `*_cents: Integer` paired with `*_currency: Text`
  in the same resource block AND the currency column default is an ISO 4217 string.
  Suggestion: replace with `<field>: Money`. Opt-out via
  `# doctor:allow VOCAB-MONEY-001` comment on the field (analytics counters etc.).

VOCAB-MONEY-002 (warn in release N, BLOCK in release N+1): field declared as `Money`
  while bare-`Money` still resolves to `BuiltinType::Decimal` (current behavior). On
  release N+1 the resolution flips to `SemanticMoney`. Suggestion: if author meant
  decimal scalar, rename to `Decimal`; if currency-aware, declare locale block default
  + add `# doctor:allow VOCAB-MONEY-002` once migration is verified.
```

#### Inspect / LSP / highlighting coverage (acceptance criteria)

- `lazuli inspect --expand=security` shows Money fields under the `audit` cross-cut.
- LSP completion for default literals: typing `"` inside a `Money required = ` slot triggers ISO 4217 code completion.
- Syntax highlighting: `tree-sitter-lazuli` highlights the `Money` keyword as a builtin type.
- `lazuli check` rejects invalid currency codes with location-anchored diagnostic.

---

### Tier 2: `@plugin/scalars-br` — deferred to separate proposal

Per architect blocker #7, the analyzer should not hardcode one plugin's name. Tier 2 needs:

1. A **generic plugin-type-contribution mechanism**: when a capsule has `uses @plugin/scalars-br` declared, the analyzer accepts the plugin's contributed semantic types (the plugin manifest publishes `@semantic.BrazilianCPF`, `@semantic.BrazilianCNPJ`, `@semantic.BrazilianCEP`, `@semantic.BrazilianPhone`) without baking the name into core.
2. The plugin itself, packaging the Go + TS validators + the `lookup_cep` query.

This is tracked as a separate proposal `semantic-types-plugin-locales.md` (to be authored after this Money proposal grades PASS). The `WAR-VOCAB-SEMANTIC-02` removal criterion (`docs/audit/hostpoint-port-workarounds-2026-05-16.md:143`) already specifies the type names; the plugin-types mechanism is the remaining piece.

For the Hostpoint port, until Tier 2 lands: handler-side validators stay (current state). The Money tier 1 closure alone removes ~6 fields × repeated currency-formatting boilerplate across Hostpoint, which is the bulk of the win.

---

## Non-goals

Same as v0.1:
- Universal FX conversion (separate `@plugin/exchange-rates`).
- Tax-aware Money (separate locale-pack tax layers).
- Other locale packs (US, UK, EU) — pattern generalizes but each is its own plugin cycle.

---

## Resolved design choices (no longer open questions — closes B5 from v0.2 review)

1. **Money is single-slot, no escape hatch**: `amount: Money` is the only authoring shape. The analyzer auto-emits the paired SemanticCurrency column. The explicit pair shorthand (`amount: Money; amount_currency: Currency`) is **rejected** as a redundant authoring of the same shape — diagnostic: `redundant currency column; Money implies a paired currency column emitted by codegen`. Authors who genuinely need per-row currency override write `amount: Money` and update via a command-level domain function (Lazuli already supports `@fn.*` extensions for that). Two-ways-to-say-one-thing is an AI-first kill; this proposal rejects it.

2. **Currency column visibility in TS**: nested. The TS `Money` interface is `{ amount: string, currency: string }`. The Go side returns `lazuli.MoneyValue` with two fields. This matches the single-slot DSL declaration semantically and keeps Intl.NumberFormat happy.

## Open questions (genuinely open, deferred)

1. **Crypto sub-cent precision** (`NUMERIC(40,18)` for Wei): defer to a future `MoneyPrecise` semantic type once a crypto pilot lands. v1 ships `NUMERIC(20,4)` which covers 99% of cases.

---

## Acceptance criteria

- `lazuli check examples/full-capsule/` accepts `Money` field declarations + the `"BRL:0"` literal form, rejects symbol-prefixed defaults.
- `lazuli generate go` emits `decimal.Decimal` Go fields + paired `lazuli.Currency` columns + correct NUMERIC(20,4) DDL.
- `lazuli generate ts` emits `Money` interface + import from `@lazuli/runtime`.
- `runtime/web/lazuli/src/types.ts` exports `Money` interface + `formatMoney(m, locale)` helper using `Intl.NumberFormat`.
- `runtime/go/lazuli/money.go` ships with a `MoneyValue` struct + `Format(locale)` delegating to `golang.org/x/text/currency`. Existing `Money = int64` alias preserved.
- Hostpoint migration: `Charge.amount_cents Integer` → `Charge.amount Money` produces correct codegen + a hand-rolled ALTER TABLE migration; e2e suite stays 82/82.
- Doctor `VOCAB-MONEY-001` flags the legacy `*_cents + *_currency` pair with the `# doctor:allow` opt-out.
- `cargo fmt --check && cargo test -p lazuli_ir && cargo test -p lazuli_codegen_go && cargo test -p lazuli_codegen_ts && cargo run -p lazuli_cli -- doctor examples/full-capsule` all green.
- `inspect --expand=security` shows Money fields with their audit metadata.
- LSP completion for ISO 4217 codes in default literals.
- Audit doc updates `WAR-VOCAB-SEMANTIC-01` status to **closed**; `WAR-VOCAB-SEMANTIC-02` referenced as deferred to a separate proposal.

---

## Cost estimate (revised per architect polish #8)

- Grammar + IR adapter: ~150 LOC `lazuli_syntax` + ~80 LOC `lazuli_ir` (no new variant, just MoneyDefault struct).
- Analyzer + ISO 4217 catalog: ~100 LOC.
- Codegen Go: ~120 LOC (NUMERIC DDL emit + `decimal.Decimal` field type).
- Codegen TS: ~50 LOC (interface emit).
- Runtime Go `money.go`: ~100 LOC (`MoneyValue` struct + Format).
- Runtime TS `types.ts` extension: ~30 LOC.
- Doctor lint: ~60 LOC + fixture.
- LSP completion for ISO 4217: ~80 LOC.
- Hostpoint migration: ~50 .lzi edits + 1 hand-rolled ALTER TABLE + ~20 handler-file sed-rewrites of formatBRL → formatMoney.

Total: ~770 LOC across 8 crates + ~1.5 days for a Claude-class agent. Tier 2 (`@plugin/scalars-br`) is a separate ~3-day cycle gated on the plugin-types mechanism.
