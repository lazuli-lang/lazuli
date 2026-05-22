# Cell A.5 Status

Commit hash: `e4be3bb` (base HEAD for this working tree).

## IR Struct Deltas

- `lazuli_ir::EnumVariant` now carries optional opaque metadata:
  `label_key: Option<String>`, `hint_key: Option<String>`,
  `icon_key: Option<String>`.
- `lazuli_syntax::EnumVariantDecl` mirrors the same optional metadata so
  parser AST -> analyzer IR is value-preserving.
- Metadata is additive and serde-optional on IR; existing enum JSON without
  metadata deserializes with `None`.

## Parser Tests

- `enum_metadata_parses_label_hint_icon_combinations`
- `enum_metadata_preserves_bare_variants_and_storage_values`
- `enum_metadata_rejects_hint_or_icon_without_label`

## Analyzer / IR Tests

- `enum_metadata_lowers_to_ir_variant_fields`
- `enum_variant_metadata_round_trips_as_optional_strings`
- `enum_variant_metadata_absent_fields_stay_omitted`

## Golden / Codegen Tests

- TS: `enum_metadata_options_golden_emits_typed_literal`
- TS: `enum_without_metadata_golden_omits_options`
- Go: `metadata_enum_emits_options_struct_and_values`
- Go: `metadata_free_enum_omits_options_struct`

## Generated `*_OPTIONS` Sample

From `examples/full-capsule` regen:

```ts
export const CUSTOMER_TIER_OPTIONS: ReadonlyArray<{
  value: CustomerTier;
  labelKey: string;
  hintKey?: string;
  iconKey?: string;
}> = [
  { value: "free", labelKey: "customer_tier_free", iconKey: "gift" },
  { value: "pro", labelKey: "customer_tier_pro", hintKey: "customer_tier_pro_hint", iconKey: "briefcase" },
  { value: "enterprise", labelKey: "customer_tier_enterprise", hintKey: "customer_tier_enterprise_hint", iconKey: "building-2" },
];
```

## Verification

- `cargo test -p lazuli_ir -p lazuli_syntax -p lazuli_analyzer -p lazuli_cli`
  passed.
- `cargo test -p lazuli_codegen_go -p lazuli_doctor` passed.
- `cargo run -p lazuli_cli -- generate ts examples/marketplace-mini-mobile`
  regenerated the mobile pilot SDK.
- `pnpm exec tsc -p tmp.enum-sdk-tsconfig.json` passed for the regenerated
  marketplace mobile SDK probe.
- `cargo run -p lazuli_cli -- generate ts examples/full-capsule --output target/full-capsule-ts-probe`
  produced the sample block above.
