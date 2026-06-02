<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: crates/lazuli_keywords (REFERENCE_NAMESPACES / SCALAR_TYPES /
     SEMANTIC_TYPES / SCALAR_ALIASES). Regenerate with:
     cargo run -p xtask -- gen-catalog-reference
     Freshness is gated by tools/xtask/tests/catalog_reference_fresh.rs. -->

# Lazuli Closed Catalogs

The single source of truth for the language's closed catalogs. The LSP, doctor, analyzer, and parser all derive from `lazuli_keywords`; this document is generated from the same source and gated for freshness, so it can never drift (the bug it replaces: three docs publishing 8 / 17 / 23 reference namespaces).

## `@`-reference namespaces

A reference `@<ns>.<target>` is valid only when `<ns>` is one of:

- `@role`
- `@scope`
- `@actor`
- `@policy`
- `@semantic`
- `@cap`
- `@pii`
- `@key`
- `@fn`
- `@hook`
- `@validator`
- `@adapter`
- `@client`
- `@query_modifier`
- `@anchor`
- `@llm`
- `@tool`
- `@trace`
- `@translation`
- `@feature`
- `@file`
- `@audience`
- `@doctor`

## Scalar types

Canonical bare PascalCase scalar type names:

- `ID`
- `Text`
- `Boolean`
- `Integer`
- `Decimal`
- `Date`
- `DateTime`
- `JSON`

## Semantic-scalar types

Validated/formatted scalars (spelled `@semantic.<X>` today; bare after the `@`-off-types cut):

- `Email`
- `Phone`
- `Url`
- `Uuid`
- `Currency`
- `GeoPoint`
- `HexColor`
- `Percentage`
- `PositiveDecimal`
- `NonNegativeInt`
- `Money`

## Non-canonical scalar aliases

These resolve for back-compat but are NOT canonical: `lazuli fmt` normalizes them and `VOCAB-SCALAR-ALIAS-001` flags them (reject + autocorrect, never silently tolerate).

| alias | canonical |
|---|---|
| `Id` | `ID` |
| `String` | `Text` |
| `Bool` | `Boolean` |
| `Int` | `Integer` |
| `Float` | `Decimal` |
| `Json` | `JSON` |
