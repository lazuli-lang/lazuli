# Curated Examples — Inclusion Policy

Every example in `examples/curated/` exists to be loaded into AI context
(via `lazuli examples --bundle`) when authoring new `.lzi`. The bundle
is the AI's reference library for "how to write idiomatic Lazuli."

## Inclusion criteria

An example MUST meet ALL of:

1. **Real-world provenance**: covers a pattern that has appeared in at
   least two production pilots OR illustrates an anti-pattern from a
   real bug report.

2. **CI-tested**: validates against the current `LZIR_SCHEMA`. The
   `lazuli examples --validate` smoke runs:
   - `lazuli check <name>.lzi` — passes.
   - `lazuli inspect <name>.lzi` — produces the frozen IR snapshot at
     `expected_ir.json`. Mismatches block PRs.

3. **Decay rotation**: if the justifying pilot(s) no longer exist,
   the example rotates to `docs/recipes/` at the next MINOR review.
   Maintained membership is not historical privilege.

## Layout

```
examples/curated/
  <name>/
    <name>.lzi           # source
    manifest.toml        # intent + common_errors + pilot_provenance
    expected_ir.json     # frozen IR snapshot
```

`manifest.toml`:

```toml
[example]
name = "command_with_safety"
intent = "create command guarded by a safety validator"

[provenance]
pilots = ["<pilot-id-1>", "<pilot-id-2>"]
last_validated = "2026-05-13"

[common_errors]
codes = ["safety_unbound", "validator_pii_class_mismatch"]
```

## Anti-bundle

Examples that don't meet criteria 1-2 live under `docs/recipes/`.
They're searchable but NOT in the AI bundle.
