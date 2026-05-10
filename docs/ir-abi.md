# Lazuli IR ABI

The IR is the canonical machine representation of Lazuli source. `app.lzi`
lowers to an optional operational `AppManifest` on `Module`, feature `.lzi`
source lowers to domain/capability `Module` IR, and `.lzx` lowers to
experience/surface `ExperienceModule` IR. All are derived from DSL, never
authored, and exposed to consumers as stable, versioned data shapes.

## Audience

The IR is for toolmakers: backend code generators, planners, LSP servers, MCP servers, semantic diff tools, visualizers, third-party linters. It is not for end-users. If a human or agent needs to read IR to understand a feature, the DSL or the `explain` output has failed.

## Source Of Truth

DSL is the source of truth. IR is `lower(parse(source))`. The IR has no edit API: there is no public mutator on `lazuli_ir`, no builder factory outside `lazuli_analyzer`, no MCP endpoint that accepts IR patches. Re-authoring means rewriting `.lzi`/`.lzx`.

The lifecycle is:

```txt
authored app.lzi manifest -> AppManifest IR -> inspect JSON / doctor / Drusa
authored .lzi capsule -> AST -> Module IR -> inspect JSON / codegen / planner / MCP
authored .lzx experience source -> AST -> ExperienceModule IR -> inspect JSON / UI codegen / MCP
```

In this repository, "capsule" means the authored `.lzi` source that contains
one or more feature blocks. Code generators consume derived IR, not the source
text and not `lazuli inspect` projections. `lazuli inspect --format=json` is a
stable read model for tools and agents, but it is not the IR ABI and should not
become a write target.

Round-trip `IR → DSL → IR` is not preserved. Comments, blank lines, and formatting live in the AST (`lazuli_syntax`) and die at lowering. If a consumer needs to display authored intent, it reads AST. If it needs to reason about semantics, it reads IR.

## Versioning

The IR schema has its own version, independent from the DSL language version:

- `LZI_LANG`: DSL syntax version (parser/grammar).
- `LZIR_SCHEMA`: IR shape version (this document).

Most lowering changes do not bump `LZI_LANG`. Most syntax changes bump both. There is no implicit coupling. The compatibility matrix is published below.

### Bump Rules

- **Patch** (`0.1.0 → 0.1.1`): documentation, internal renames, bug fixes that produce identical IR for valid inputs.
- **Minor** (`0.1.0 → 0.2.0`): new optional field, new node variant with default, new derived layer entry. Older consumers ignore unknown fields with a warning.
- **Major** (`0.1.0 → 1.0.0`): rename of an existing field, removal, semantic change, type change, or required field added.

### Unknown Fields Policy

When a consumer reads IR newer than its compiled-in `LZIR_SCHEMA`:

- Default: emit a warning, continue. Unknown fields are ignored.
- `--strict`: fail with a non-zero exit and a list of unknown fields. CI runs strict.

This applies to consumers reading IR. Producing IR from a DSL older than the lowering pipeline still fails fast on missing required fields; that is not the unknown-field case.

### No IR Migrator

Major bumps invalidate older IR. There is no migration tool. Re-lower from `.lzi`:

```sh
cargo run -p lazuli_cli -- compile examples/crm.lzi --out generated/crm
```

Backends and tooling never read IR of an incompatible major version. The compatibility matrix tells consumers which IR versions they can accept.

### Compatibility Matrix

| `LZI_LANG` | `LZIR_SCHEMA` | Notes   |
|------------|---------------|---------|
| 0.1.0      | 0.1.0         | initial domain/capability IR |
| 0.2.0      | 0.2.0         | adds `.lzx` `ExperienceModule` IR |
| 0.3.0      | 0.3.0         | adds optional app operational manifest shape |

New rows are appended as versions ship. Removing a row is a major bump on both sides.

## Determinism

`lower(parse(source))` is a pure function. The same input must produce byte-identical IR JSON.

- Maps use `BTreeMap` or `IndexMap` with documented ordering. `HashMap` is forbidden in any IR struct; CI enforces.
- Field ordering in JSON output is sorted alphabetically.
- No timestamps, absolute paths, or non-reproducible hashes inside IR.
- No floating-point fields. If a numeric field is needed, it is integer or rational.

Snapshot tests in `crates/lazuli_analyzer/tests/` lock the JSON shape of every `examples/*.lzi`. CI fails on snapshot diff without explicit acceptance.

## Spans Are Debug, Not ABI

Each IR node carries `span_ref: Option<SpanId>` pointing back to the AST. Spans serve LSP, error reporting, and debugging. They are **not** part of the published JSON ABI.

- Default JSON dump strips spans.
- `--with-spans` includes them; consumers must opt in.
- Span format is not versioned independently; treat it as best-effort.

If a consumer relies on spans to build features (LSP, IDE highlighting), it must accept that spans may change without a major bump.

## Authored vs Derived

Two layers, kept distinct by type, never merged for convenience:

- **Authored:** what the DSL says. Direct projection. No inference.
- **Derived:** what the lowering pass computes. Effective scope per query, operation table, resolved policy bindings, resolved extension paths, resource graph, required-input checklist per command.

Consumers that reason about author intent read the authored layer. Consumers that generate code or check invariants read the derived layer. A derived value never overwrites an authored one.

`storage_value` on enum variants is authored only. If the DSL does not declare it, IR carries `storage_value: None` and codegen picks per-target locally without writing back to IR.

## Identity And Renames

Every IR node has a stable nominal ID derived from its qualified path: `feature.customer.command.create`, `feature.customer.resource.Customer.field.email`. IDs are the unit of semantic diff, MCP indexing, and error addressing.

Renames break the ID by design. Rename is a semantic event, not a layout detail. Two mechanisms handle continuity:

1. **Planner heuristic.** When comparing IR to a previously stored IR, the planner reports probable renames (structural similarity plus name distance) as suggestions, not hard links.
2. **Author override.** The DSL has `previously` to declare continuity explicitly:

   ```lazuli
   command register previously migrated create
     creates Customer
     ...
   ```

   The IR carries `previous_names: Vec<String>` on the renamed node. Planner, MCP, and semantic diff respect it.

`previously migrated <old_name>` is the canonical way to claim identity across a rename. `previously alias <old_name>` is reserved for temporary compatibility aliases that generated surfaces still accept. Bare `previously <old_name>` is legacy authoring syntax.

## What Never Enters IR

- Codegen-chosen storage values (target-specific, computed locally per generator).
- Comments, blank lines, formatting.
- Configuration that varies between environments (production DB URL, secrets).
- Runtime concerns (logging adapters, metric sinks).
- Editor metadata ("show this collapsed", "color this red").
- Unverified input from agents or GUIs that did not flow through the parser.

If a field's only justification is "an editor needs it later," reject the field.

## App Manifest IR

`app.lzi` lowers into an optional `AppManifest` attached to `Module` and reused
by inspect/doctor. The manifest is provider-neutral: targets, environments,
URLs, env schema, capabilities, logical service boundaries, communication
intent, runtime units, and deploy gates enter IR; provider-specific details
such as AWS accounts, Kubernetes namespaces, Fly app ids, bucket names, gRPC
implementations, Kafka/NATS/SQS brokers, or Terraform settings stay in Drusa
adapter configuration.

`AppEnvVar` entries are still keyed by explicit env variable name. Optional
`group` metadata preserves authoring organization such as `customer_import`,
`mercadopago`, or `public_clients`, but it does not create a namespace for
`env.NAME` references. Optional `environments` metadata carries declarations
such as `required in production`; values and provider-specific secret storage
never enter IR.

## Experience IR

`.lzx` lowers into `ExperienceModule`, separate from `.lzi` `Module`. The
split is intentional:

- `.lzi` domain/capability IR is compilable without UI source.
- abstract `.lzx` `Experience` nodes import domain features and declare product
  view-model intent.
- concrete `.web.lzx`/`.mobile.lzx` `PlatformSurface` nodes project an
  abstract experience for `audience`/`tenant` variants.

Experience IR has the same no-edit rule as feature IR. Consumers may compose
`Module` and `ExperienceModule` for UI codegen, inspect output, or MCP context,
but neither IR writes back into the other.

## Producers

IR producers are owned by the compiler/tooling pipeline:

- `lazuli_analyzer::lower_document(&Document) -> Result<Module, AnalyzeError>`
- `lazuli_analyzer::lower_lzx_document(&LzxDocument) -> ExperienceModule`
- `lazuli_cli` app-manifest lowering for `app.lzi` until the canonical `.lzi`
  parser owns the manifest AST

There is no public IR producer outside the Lazuli compiler/tooling crates.
Tempting cases that must be refused:

- Builder fluent API for tests. Tests parse `.lzi` strings through the real pipeline.
- Migration tool that rewrites IR. Migration rewrites `.lzi`.
- External schema importer (Drizzle, Prisma, OpenAPI) that produces IR. The importer produces `.lzi`.

A second producer is a regression in design.

## Consumers

All consumers are read-only:

- `lazuli_codegen_go`, `lazuli_codegen_ts`: read IR, write code.
- `lazuli_planner`: reads current IR plus stored previous IR, produces `Plan` with `steps` and `risk`.
- `lazuli_lsp`: reads IR for hover, go-to-definition, semantic diagnostics.
- `lazuli_mcp`: reads IR; see `mcp-abi.md` for the full surface.
- `lazuli_cli explain`: dumps human-readable derived IR for debugging.

External visualizers, linters, and diff tools read the JSON. They are encouraged. None of them write.
