---
id: 0016
title: first-class Money — amount + currency + scale as one typed unit
type: prd
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
track: evolve/ship
---

# PRD — first-class Money type

## Problem
Money is modeled **three incompatible ways**, and the split runs *inside the canonical pilot itself*:
1. **`@semantic.Money` string-tagged + sibling `@semantic.Currency` string** — hostpoint `payments/payments.lzi:61` `amount: @semantic.Money required = "BRL:0.00"` + `:70 amount_currency: @semantic.Currency required = "BRL"`. The currency is a separate, string-encoded field the author must remember to add (and they wrote a 9-line comment explaining why, `payments.lzi:62-69`).
2. **Raw `_cents: Integer` + `_currency: Text`** — hostpoint `catalog/catalog.lzi:205-206` `price_amount_cents: Integer required = 0` + `price_currency: Text required = "BRL"` (7 such pairs in catalog; more in `operations.lzi:51,103,125`).
3. **Bare `Decimal`, NO currency sibling at all** — pauta `media_price_tables.lzi:89,106` (`price_per_cm_col`, `price`), `hoxo_financial_integration.lzi:51,128` (`amount: Decimal`). Currency is implied-BRL by comment, never typed.

There is no single typed unit binding amount + currency + scale, so the currency sibling is optional-by-discipline and silently drifts or vanishes. `VOCAB-MONEY-MULTI-CURRENCY-001` already scans for a missing `<money>_currency` sibling on the string-tagged form, but it can't see the `_cents:Integer + _currency:Text` or bare-`Decimal` encodings.

## Who hurts
- **hostpoint** — two encodings inside one app (`@semantic.Money` in payments vs `_cents:Integer` in catalog/operations); cross-feature money math crosses representation boundaries.
- **pauta** — bare `Decimal` with no currency typed anywhere; correctness rests on a comment.
- **anyone reading IR** — money isn't a unit; you must reassemble amount+currency by naming convention.

## What we ship
1. **A first-class money type** where amount + currency + scale travel as one typed unit, with an **enforced currency sibling** (you cannot declare a money amount without its currency). Currency CODE stays app data (locale-neutral — no baked-in BRL).
2. **A doctor rule** that flags: (a) a money-amount field with no currency sibling, (b) the `_cents:Integer + _currency:Text` drift, (c) the bare-`Decimal`-as-money drift.
3. **Migrate** all three encodings across both pilots onto the type.
4. **Teach** `docs/lazuli_way/money.md` (stub from 0001).

## Out of scope
- Multi-currency arithmetic / FX conversion semantics (existing `money_arithmetic_001` / `money_compare_001` rules already police same-currency math; this spec is about the *type*, not the algebra).
- Locale-specific currency formatting (apps own display; locale discipline).

## Success
Both pilots model money with the first-class type; the 3 encodings are gone; the new doctor rule is green (no missing-currency, no `_cents`/`_currency`, no bare-`Decimal` money drift); both pilots `lazuli check` + `doctor` + `go build` clean.
