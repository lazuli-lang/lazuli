# Money

## Reach for this

When a field holds an amount of money, type it `Money` — the first-class type
that carries **amount + currency + scale as one typed unit**. The currency is
part of the type: codegen emits an enforced `<field>_currency` column (pinned by
a `CHECK` to the declared ISO), so a `Money` field can never lose its currency
the way a hand-rolled amount silently does. Scale (minor-unit precision) is the
type's, not reconstructed from a `_cents` naming hack.

```
amount: Money(currency: BRL) required
```

Bare `Money` is the single-currency shorthand; `Money(currency: <ISO>)` pins a
specific ISO 4217 code (`USD`, `EUR`, `GBP`, `JPY`, `CHF`, `BRL`). The currency
**value** is app data — the framework is locale-agnostic and bakes in no default
currency catalog; it only enforces *that* a currency travels with every amount.

Money math is policed separately: `MONEY-COMPARE-001` /
`MONEY-ARITHMETIC-001` reject mixed-currency comparison/arithmetic at analyse
time (that is the *algebra*, not the *type* — out of scope for this idiom).

## Before (hand-rolled) / After (idiomatic)

Money is hand-rolled **three incompatible ways**, and the split runs *inside one
pilot*. Each loses something the type carries for free:

**Before (a)** — string-tagged amount with a separate, easy-to-forget currency
string the author has to remember to add (hostpoint
`features/payments/payments.lzi:61,70`, with a 9-line comment at `:62-69`
explaining why the override exists):

```
# hostpoint features/payments/payments.lzi:61,70
amount: @semantic.Money required = "BRL:0.00"
# … 9 lines of comment explaining the currency override …
amount_currency: @semantic.Currency required = "BRL"
```

**Before (b)** — minor-units `Integer` + currency `Text` pair; the storage
representation leaks into the surface and the agent gets no type to reason about
(hostpoint `features/catalog/catalog.lzi:205-206`, **7 such pairs**; more in
`operations.lzi:51,103,125`):

```
# hostpoint features/catalog/catalog.lzi:205-206
price_amount_cents: Integer required = 0
price_currency: Text required validate utf8_safe = "BRL"
```

**Before (c)** — bare `Decimal`, **no currency typed anywhere**; correctness
rests on a comment (pauta `features/hoxo_financial_integration/…:51`,
`media_price_tables.lzi:89,106`):

```
# pauta hoxo_financial_integration.lzi:51
amount: Decimal required   # implied BRL — by comment only
```

**After** — one typed unit, currency enforced, no separate sibling to forget, no
storage hack, no comment standing in for a type:

```
resource Charge
  amount: Money(currency: BRL) required
  platform_fee: Money(currency: BRL) required
```

### Storage is representation-preserving

`Money` lowers to the **same columns** the hand-rolled forms already store, so
migrating off the three encodings is a re-type with no value/scale drift:

```sql
-- amount: Money(currency: BRL)
amount NUMERIC(20,4) NOT NULL,
amount_currency TEXT NOT NULL CHECK (amount_currency = 'BRL') DEFAULT 'BRL'
```

- The amount column is minor-units `NUMERIC(20,4)` — migrating a `_cents:Integer`
  is a no-op column re-type, not a value transform.
- The `<field>_currency` column is **emitted by codegen** — the author never
  declares it; it cannot drift out of sync with the amount.
- The `CHECK`/`DEFAULT` are pinned to the **declared** ISO. `Money(currency: USD)`
  emits a `'USD'` check — there is no baked-in `'BRL'`.
- The Go field type is the rich `lazuli.MoneyValue` (decimal + currency), not a
  bare `int64`.

## Enforced by

- `VOCAB-MONEY-SHAPE-001`
  (`crates/lazuli_doctor/src/vocab/money_field_shape_001.rs`) — the single source
  of truth for money-**shape** drift. Fires (`warning`, suppressible with
  `# doctor:allow VOCAB-MONEY-SHAPE-001`) on all three hand-rolled encodings:
  - **(a)** a string-tagged `Money` amount with no `<field>_currency` sibling
    (the currency can silently vanish),
  - **(b)** a `<x>_cents: Integer` (+ optional `<x>_currency: Text`) minor-units
    pair (storage shape leaked into the surface),
  - **(c)** a bare `Decimal` named like money (`amount`, `price`, `*_price`,
    `*_amount`, `total`, `*_fee`, …) with no currency sibling.

  Geo/ratio decimals (`latitude`, `avg_rating`) are deliberately **not** matched —
  only money-shaped names fire. Each finding names `Money` as the fix and points
  back at this doc. Wired into `lazuli doctor` via the vocab aggregator
  (`crates/lazuli_doctor_run/src/doctor/aggregators/vocab.rs`).

The *algebra* rules are separate and unchanged:
`MONEY-COMPARE-001` (`vocab/money_compare_001.rs`),
`MONEY-ARITHMETIC-001` (`vocab/money_arithmetic_001.rs`), and the multi-currency
shared-column nudge `VOCAB-MONEY-MULTI-CURRENCY-001`
(`vocab/vocab_money_multi_currency_001.rs`).

See also the spec at `.specs/changes/0016-first-class-money/`.
