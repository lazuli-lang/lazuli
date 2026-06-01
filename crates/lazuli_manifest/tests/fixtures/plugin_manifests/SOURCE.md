# Vendored plugin-manifest fixtures (spec 0021)

These `.toml` files are a **frozen snapshot** of the real plugin
`manifest.toml` files for the back-compat regression test
(`all_real_manifests_deserialize`). Their job is to prove every real
manifest keeps deserialising under the kind-discriminated schema — not to
mirror the live repos.

- Source: `c:\Users\lucas\dev\lazuli-plugin-*\manifest.toml`
- Snapshot date: 2026-06-01
- Count: 24 real manifests (each `lazuli-plugin-<short>` → `<short>.toml`,
  including `scalars-br` which is the lone `semantic`-kind manifest).
- `malformed_adapter.toml` is hand-authored (NOT a real plugin) — a
  structurally-broken adapter the schema must reject.

Do not edit these to track upstream drift; if a real plugin's contract
changes meaningfully, re-snapshot deliberately.
