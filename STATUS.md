# Cell A.4 Status

Code commit: `bd95792272a07fa74a596e13191db001428894c7`

## Edit sites

- `crates/lazuli_cli/src/main.rs`: rich Zod lowering for enums, core semantic string carriers, plugin semantic Brazilian CPF/CNPJ patterns, and fallback comments for unhandled plugin semantic carriers.
- `crates/lazuli_cli/src/doctor.rs`: wires `SCHEMA-RICH-001` into doctor diagnostics.
- `crates/lazuli_cli/src/doctor/schema_rich_001.rs`: scans generated Zod command schemas for `z.unknown()` on now-handled typed slots.

## Golden tests

- `rich_zod_base_emits_enum_catalog`
- `rich_zod_base_emits_core_semantic_validators`
- `rich_zod_base_emits_plugin_semantic_digit_patterns`
- `feature_zod_emits_enum_and_semantic_command_schema`

## Doctor tests

- `schema_rich_001_fires_for_generated_unknown_enum_slot`
- `schema_rich_001_accepts_generated_enum_schema`

## Sample regen diff

Representative full-capsule command schema improvement:

```diff
 export const captureCustomerLeadInputSchema = z.object({
   name: z.string(),
-  email: z.unknown(),
+  email: z.string().email(),
 });

 export const createCustomerInputSchema = z.object({
   name: z.string(),
-  email: z.unknown(),
-  tier: z.unknown(),
-  source: z.unknown(),
+  email: z.string().email(),
+  tier: z.enum(["free", "pro", "enterprise"]),
+  source: z.enum(["manual", "import", "api", "webhook"]),
 });
```

## Verification

- `cargo test -p lazuli_cli rich_zod_base -- --nocapture`: passed.
- `cargo test -p lazuli_cli schema_rich_001 -- --nocapture`: passed.
- `cargo test -p lazuli_cli feature_zod_emits_enum_and_semantic_command_schema -- --nocapture`: passed.
- `cargo run -q -p lazuli_cli --bin lazuli -- generate ts examples/full-capsule`: passed; `z.enum(` and `z.string().email()` both found under `examples/full-capsule/dist/ts-web`.
- `pnpm install --frozen-lockfile`: passed to install repo-pinned TypeScript.
- `pnpm exec tsc --noEmit -p target/a4-tscheck/tsconfig.json`: passed for generated `*.zod.ts`.
- `cargo test -p lazuli_cli`: passed.

Note: the broader existing smoke test `cargo test -p lazuli_codegen_ts --features smoke full_capsule_typechecks_under_tsc` is blocked before typechecking generated output because it still points at `runtime/web/lazuli/src/index.ts`; this checkout uses `runtime/ts/lazuli/src/index.ts`.
