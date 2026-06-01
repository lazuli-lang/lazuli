---
id: 0021
title: Plugin manifest — typed, kind-discriminated, compiler-readable
type: prd
track: evolve (plugin platform)
depends_on: [0019]
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_manifest plugin_manifest && cargo test --workspace"
agent: unassigned
---

# PRD — Plugin typed manifest

## Problem

A Lazuli plugin's `manifest.toml` is the contract between an external adapter
and the compiler. Today the compiler models **one of three real plugin kinds**.
`PluginManifest` (`crates/lazuli_manifest/src/plugin_manifest/types.rs:20-26`)
is exactly two fields:

```rust
pub struct PluginManifest {
    pub plugin: Option<PluginIdentity>,
    pub semantic_types: Vec<PluginSemanticTypeDecl>,
}
```

Everything is `#[serde(default)]` + optional **on purpose** — so a manifest
authored against a different sibling contract still deserialises cleanly
instead of erroring. The cost of that leniency: the compiler **silently drops
every key it doesn't model**. Those dropped keys are not noise — they are the
actual adapter contract.

A census of the 24 real plugins under `c:\Users\lucas\dev\lazuli-plugin-*`
shows that exactly **one** (`scalars-br`) uses the modeled
`[[semantic_types]]` path. The other 23 are **adapters** whose entire contract
lives in keys the compiler never reads:

| Key (real, observed)                | Plugins using it                                | Modeled today? |
|-------------------------------------|-------------------------------------------------|----------------|
| `implements = ["bucket.Interface"]` | mercadopago, object-store, audit-sink, chromadb, google-maps, mapbox, waf, captcha, expo-push, openai-embeddings, secret-vault, breach-watch, ip-reputation, mtls, ratelimit-redis, csp-builder, pii-scan (~17) | **No** |
| `[env].required` / `.optional`      | mercadopago, smtp, object-store, google-maps, mapbox, waf, chromadb, openai-embeddings, secret-vault, audit-sink, expo-push, sms-twilio, verify-twilio (~13) | **No** |
| `[binds].interface` + `.methods`    | smtp                                            | **No** |
| top-level `version` / `status`      | ~20 plugins                                     | **No** |

These keys are validated by **nobody**. The compiler ignores them; they are
consumed only by external catalog tooling. So a payment adapter can declare
`implements = ["payments.PaymentGateway"]` and `[env].required =
["MERCADOPAGO_ACCESS_TOKEN"]` and **nothing in the Lazuli pipeline ever reads,
checks, or surfaces those facts** — not doctor, not codegen, not the wiring
that decides whether a `Lazurite.toml [plugins]` activation is even coherent.

This is **Seam 2** of the plugin platform: the schema models 1 of 3 kinds.
Spec 0019 unified *how* the one modeled kind is resolved; this spec makes the
schema **know what the other kinds are** so later specs can act on them.

## Who it's for

- **The compiler / future verify pass (0022).** Today there is nothing typed to
  verify an adapter's `implements`/`[binds]` against the Go interfaces it
  claims. 0022 needs a typed, readable adapter contract to verify. This spec
  produces exactly that data structure — and freezes it.
- **The scaffolder (0023).** `lazuli plugin new <kind>` needs to know what a
  well-formed manifest of each kind looks like. The `kind` discriminant + typed
  per-kind schema is that template source of truth.
- **Plugin authors.** Right now an author guesses the adapter manifest shape by
  copying a neighbour (which is why `module`/`go_module`, `[binds]`/`[contract]`/
  `[provides]` all coexist as ad-hoc spellings). A typed schema + authoring doc
  gives one blessed shape per kind.
- **Doctor / catalog surfaces (downstream).** Once the keys are typed and read,
  doctor can surface the env-var contract and the implemented buckets instead
  of treating the manifest as opaque prose.

## Why now

0019 just landed the single mandatory resolution pipeline for the **semantic**
kind. The adapter kind is the **largest unmodeled category** (23 of 24 real
plugins) and the **next two specs depend on it being typed**: 0022 (verify the
adapter contract against Go interfaces) and 0023 (scaffold new plugins) both
build directly against this schema. If the typed shape is not frozen first,
0022 and 0023 either invent their own ad-hoc structs (drift) or block.

The Pareto call is explicit: **model the adapter kind fully** (it carries 6+
real distinct contracts: env, binds, implements), **stub capability/design**
(identity + a thin typed marker, full per-kind schema deferred). This spec is
the **frozen contract** 0022/0023 lock against — so it must be precise, not
exhaustive.

## What success looks like

1. A plugin manifest carries an explicit `kind` (`semantic` | `adapter` |
   `capability` | `design`), and the compiler can **read the adapter contract**
   (`implements`, `[env]` required/optional, `[binds]` interface+methods) as
   typed Rust, not dropped prose.
2. `kind` is **inferred, not mandatory**: existing manifests that omit it
   (every one of the 24) keep deserialising and resolve to the right kind from
   the sections present. No author edits any existing `manifest.toml`.
3. **Zero regression** to 0019's semantic path: `[[semantic_types]]` is byte-
   for-byte the same struct, parsed the same way; scalars-br + the BR-scalar
   resolution still work identically.
4. A test deserialises **every real manifest** (scalars-br + all 24
   `lazuli-plugin-*` dirs, vendored as fixtures) and asserts each one parses to
   the expected inferred kind without error.
5. Authors have one doc — `docs/plugin-authoring.md` — that documents the `kind`
   discriminant + the typed adapter schema, with the blessed adapter shape.
6. A malformed adapter contract (e.g. `[binds]` with `interface` but the
   well-formedness invariant violated) is **rejected** by a schema test, proving
   the schema is now load-bearing, not decorative.

## Scope

**In:**
- A `PluginKind` discriminant + kind inference rule (decided in the ADR).
- Full typed model of the **adapter** kind, grounded in the real
  mercadopago/smtp/object-store keys: `implements: Vec<String>`,
  `[env]` (`required`/`optional`), `[binds]` (`interface` + `methods`).
- Minimal typed stubs for **capability** and **design** kinds (identity + thin
  marker; full schema deferred with an explicit note).
- Back-compat: every existing manifest deserialises; a fixture test proves it.
- `docs/plugin-authoring.md` update (the `kind` discriminant + adapter schema).
- A schema round-trip + malformed-rejection test.

**Out (non-goals):**
- **Verifying** the adapter contract against real Go interfaces — that is 0022.
  This spec makes the contract *typed + readable*, full stop. No interface
  resolution, no Go AST, no cross-check.
- The scaffolder / `lazuli plugin new` — that is 0023.
- Any change to the semantic resolver or alias-map (0019 owns that surface).
- Normalising the ad-hoc spellings in the *real* manifests (`module` vs
  `go_module`, `[contract]` vs `[binds]`). We model the **blessed** keys and
  tolerate the legacy spellings via optional fallback fields; we do **not**
  rewrite the 24 plugin repos (they live outside this repo).
- Modeling every observed key (`[vendors]`, `[audit]`, `maintainer`,
  `[faces]`, `runtime`, `license`, `bucket`). Those stay tolerated-but-
  unmodeled catalog metadata; only the **contract-bearing** keys are typed.

## Constraints

- **Framework-only, pure schema crate addition.** Touches only
  `crates/lazuli_manifest` + `docs/`. `parallel_safe: true`.
- **Additive + back-compat mandatory.** Every field stays `#[serde(default)]`.
  No existing manifest may break. The 24-manifest fixture test is the gate.
- **Freeze the adapter shape.** 0022 and 0023 build against it; field names and
  nesting are a committed contract once this lands. The ADR records the field
  set verbatim against the real keys.
- **No semantic-resolver change.** `cargo test --workspace` (incl. 0019's
  plugin_resolution suite) must stay green.

## Open questions (resolved in the ADR)

- Is `kind` a required field or inferred from present sections? → ADR decides
  (inferred, with explicit `kind` as an override).
- How is the adapter `kind` distinguished from `semantic` when a manifest has
  neither `[[semantic_types]]` nor adapter sections? → ADR decides the
  precedence ladder.
- Do we tolerate the legacy `module` (vs `go_module`) and `[contract]`/
  `[provides]` spellings, or canonicalise? → ADR: tolerate via optional alias
  fields, blessed shape is `go_module` + `[binds]`.
