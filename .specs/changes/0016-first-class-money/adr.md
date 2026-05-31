---
id: 0016
title: first-class Money — amount + currency + scale as one typed unit
type: adr
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
track: evolve/ship
---

# ADR — Money as a first-class type

## Context
`@semantic.Money` exists today only as a string-tagged convention (`hostpoint payments/payments.lzi:61` `= "BRL:0.00"`), paired by hand with a string `@semantic.Currency` sibling (`:70`). The existing doctor rule `VOCAB-MONEY-MULTI-CURRENCY-001` (`crates/lazuli_doctor/src/vocab/vocab_money_multi_currency_001.rs`) scans for that sibling and skips the resource on first hit — so the contract is "remember to add a string field." Meanwhile catalog uses `_cents:Integer + _currency:Text` (`catalog.lzi:205-206`, 7 pairs) and pauta uses bare `Decimal` with no currency (`hoxo_financial_integration.lzi:51`, `media_price_tables.lzi:89,106`). Three encodings; the canonical pilot disagrees with itself.

## Decision
1. **Promote money to a first-class type** carrying `(amount, currency, scale)` as one typed unit. A field typed money REQUIRES a currency — there is no "money amount without currency" state. Scale (minor-unit precision) is part of the type, not reconstructed from a `_cents` naming hack.
2. **Currency code is app data.** The type enforces *that* a currency is present and typed; it does NOT enumerate or default to a locale's currency (no baked-in BRL — locale discipline). Apps supply the currency value.
3. **Replace, don't co-exist long-term.** The string-tagged `@semantic.Money` form is migrated onto the first-class type; `_cents:Integer + _currency:Text` and bare `Decimal` are migrated too. Post-migration the only money representation in either pilot is the first-class type.
4. **One doctor rule, three drift shapes.** A new rule flags missing-currency, the `_cents`/`_currency` pair, and bare-`Decimal`-as-money. The existing `VOCAB-MONEY-MULTI-CURRENCY-001` (string-tagged missing sibling) is subsumed/retargeted, not duplicated.

## Alternatives considered
- **Keep `@semantic.Money` string-tagged, just harden the linter** — rejected: the string `"BRL:0.00"` form can't carry scale as a type and still lets the currency sibling be a separate optional field; the canonical-pilot self-split persists.
- **Standardize everyone on `_cents:Integer + _currency:Text`** — rejected: it's the lowest-abstraction encoding, leaks storage representation into the surface, and gives the agent no type to reason about.
- **Bake a default currency (BRL) into the type** — rejected: violates locale discipline (framework is locale-agnostic; currency is app/tenant data).
- **Multi-currency arithmetic in the type** — out of scope; existing `money_arithmetic_001`/`money_compare_001` already police same-currency math.

## Consequences
- **Positive**: one money representation across both pilots; currency can't silently vanish (type-enforced); scale is explicit; IR carries money as a unit; doctor sees all three drift shapes.
- **Negative / cost**: grammar + IR + Go codegen all gain a money type; migration touches `payments.lzi`, `catalog.lzi`, `operations.lzi` (hostpoint) and `media_price_tables.lzi`, `hoxo_financial_integration.lzi` (pauta); `_cents`-keyed Go handler SQL (`catalog`/`operations`/`payments` `.go`) and `.lzx` columns (`payments.web.lzx`, `operations.web.lzx`) reference `*_cents`/`*_currency` and must be re-pointed.
- **Storage compatibility**: the type must lower to columns that match each pilot's live DB (cents-integer + currency-text is the safe storage shape) so migration is representation-preserving where possible — verified in the migrate gate.
