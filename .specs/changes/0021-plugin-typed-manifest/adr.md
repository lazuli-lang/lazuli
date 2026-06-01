---
id: 0021
title: Kind-discriminated plugin manifest — inference rule + adapter schema
type: adr
track: evolve (plugin platform)
depends_on: [0019]
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_manifest plugin_manifest && cargo test --workspace"
agent: unassigned
---

# ADR — Kind-discriminated plugin manifest

## Context

`PluginManifest` (`crates/lazuli_manifest/src/plugin_manifest/types.rs:20-26`)
is `plugin: Option<PluginIdentity>` + `semantic_types: Vec<PluginSemanticTypeDecl>`,
everything `#[serde(default)]` so any manifest deserialises. It models the
**semantic** kind only. The 23 other real plugins are **adapters** whose
contract (`implements`, `[env]`, `[binds]`) is dropped on parse.

We must make the manifest **kind-discriminated and typed per kind** without
breaking a single one of the 24 existing manifests (none of which carry a
top-level `kind` key) and without touching 0019's semantic resolution path.

Ground truth from reading the real manifests:

- `scalars-br` — `[plugin]` + 4× `[[semantic_types]]`. (semantic)
- `mercadopago` — top-level `version`/`status`/`maintainer`/`implements =
  ["payments.PaymentGateway"]`, `[plugin]`, `[env].required`/`.optional`.
- `smtp` — `[plugin]` (with `kind = "notifications/email-sender"` and `module`),
  `[env]` (`required`, `required_for_auth`, `optional`, plus per-var sub-tables
  `[env.SMTP_TLS_MODE]` with `description`/`allowed`/`default`), `[binds]`
  (`interface` + `methods`).
- `object-store` — top-level `version`/`implements = ["storage.ObjectStore"]`,
  `[plugin]`, `[env].required`/`.optional` (multi-line array).
- Wider census: identity sometimes lives at top level (`name = "@lazuli/..."`)
  instead of `[plugin]`; go-module key is spelled `go_module` **or** `module`
  **or** `[plugin].module`; binding contract is spelled `[binds]` **or**
  `[contract].methods` **or** `[provides].go_interface`; non-contract catalog
  keys `[vendors]`/`[audit]`/`[faces]`/`runtime`/`license`/`bucket` appear ad hoc.

## Decision

### D1 — `kind` is INFERRED from present sections, with an explicit override

We add `kind: Option<PluginKind>` (`#[serde(default)]`). When **present**, it
wins. When **absent** (all 24 real manifests), an inference function derives it.
We do **not** make `kind` required — that would break every existing manifest
and force 24 external repo edits, violating the back-compat mandate.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    #[default]
    Semantic,
    Adapter,
    Capability,
    Design,
}
```

**Inference precedence ladder** (`PluginManifest::resolved_kind()`):

1. If an explicit top-level `kind` field is set → use it. *(Note: `smtp` carries
   `kind = "notifications/email-sender"` **inside `[plugin]`**, not top-level,
   and it is a free-form catalog string, not our enum. That stays a tolerated,
   unmodeled `PluginIdentity` field — see D4 — and does NOT feed inference.)*
2. Else if `semantic_types` is non-empty → `Semantic`. *(Preserves 0019: any
   manifest the semantic resolver cares about keeps classifying as semantic.)*
3. Else if any **adapter section** is present (`implements` non-empty, or
   `[env]`, or `[binds]`) → `Adapter`. *(Captures all 23 real adapters.)*
4. Else → `Semantic` (the historical default; an identity-only manifest stays
   on the legacy path, harmless because it contributes no aliases).

This ladder makes `kind` a derived view, never a parse requirement. The
**default** of the enum is `Semantic` so that `#[serde(default)]` and the
`#[default]` variant agree, keeping the historical no-op behaviour for bare
manifests.

**Why semantic wins over adapter (step 2 before 3):** a future plugin could be
*both* (contributes scalars AND binds an interface). For 0019 safety the
semantic classification must never be lost. The adapter-contract fields are
still parsed and readable regardless of inferred kind (they are plain optional
fields on the struct); `kind` only drives *which template/verify path* 0022/0023
take. Keeping semantic-first guarantees 0019's resolver never stops seeing a
manifest it used to see.

### D2 — Model the ADAPTER kind FULLY, grounded in the real keys

The adapter contract is the big unmodeled category. We add three typed,
optional, blessed-shape fields to `PluginManifest`:

```rust
/// `implements = ["bucket.Interface", ...]` — the framework contract
/// bucket(s) this adapter satisfies (e.g. "payments.PaymentGateway",
/// "storage.ObjectStore"). 0022 verifies each against a real Go interface.
#[serde(default)]
pub implements: Vec<String>,

/// `[env]` — the environment-variable contract. 0022/doctor surface this;
/// the scaffolder (0023) seeds it.
#[serde(default)]
pub env: Option<PluginEnvContract>,

/// `[binds]` — the Go interface this adapter binds against + its methods.
#[serde(default)]
pub binds: Option<PluginBindsContract>,
```

```rust
/// `[env]` block. `required`/`optional` are the two blessed keys seen across
/// mercadopago, object-store, google-maps, smtp, etc. `required_for_auth`
/// (smtp) is tolerated as an extra conditional-required bucket. Per-variable
/// detail sub-tables (`[env.SMTP_TLS_MODE]`) are intentionally NOT modeled in
/// v1 — they are catalog-doc metadata, not contract; serde drops them
/// harmlessly (they live under a different TOML path than the arrays).
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginEnvContract {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    /// smtp-style "required only when auth is configured" bucket. Tolerated;
    /// 0022 may treat as a third (conditional) tier. Empty for most adapters.
    #[serde(default)]
    pub required_for_auth: Vec<String>,
}
```

```rust
/// `[binds]` block — the public Go surface this adapter exposes and the
/// interface it (will) bind to. Grounded in smtp's:
///   interface = "github.com/.../lazuli-plugin-smtp.EmailSender"
///   methods   = ["SendEmail", "SendEmailBatch"]
/// 0022 resolves `interface` to a real Go interface and checks `methods`
/// against it. `[contract].methods` / `[provides].go_interface` (the legacy
/// spellings on social-apple / social-google) are NOT auto-mapped here — the
/// blessed shape is `[binds]`; legacy plugins keep parsing (their keys land in
/// no modeled field and are dropped) and migrate to `[binds]` when 0023 lands.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginBindsContract {
    /// Fully-qualified Go interface path (`<module>.<Interface>`). Optional so
    /// a partially-authored manifest still parses; 0022 treats absent as
    /// "unbound — verify-skip with a note".
    #[serde(default)]
    pub interface: Option<String>,
    /// Method names the adapter exports that 0022 checks against `interface`.
    #[serde(default)]
    pub methods: Vec<String>,
}
```

**Why these exact fields:** they are the keys actually present in
mercadopago/smtp/object-store and the 13/17 wider adapters. We model the
contract-bearing keys (`implements`, `env.required/optional`, `binds.interface/
methods`) and deliberately stop there.

### D3 — Capability + Design kinds: thin typed stubs, full schema deferred

No real plugin in the census is a capability or design plugin (those are future
kinds named by the seam). We reserve them in the `PluginKind` enum (D1) so the
discriminant is complete and 0022/0023 can match exhaustively, but we model
**only** a thin marker:

```rust
/// `[capability]` — reserved. v1 models identity only; the full capability
/// contract (provided behaviours, activation guards) is DEFERRED to a later
/// spec. Present here so `kind = "capability"` round-trips and matches
/// exhaustively.
#[serde(default)]
pub capability: Option<PluginCapabilityStub>,

/// `[design]` — reserved. v1 models identity only; full design-emitter
/// contract (token namespaces, emitted surfaces) DEFERRED.
#[serde(default)]
pub design: Option<PluginDesignStub>,
```

```rust
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginCapabilityStub {
    /// Free-form capability name(s); typed shape deferred. Captured so an
    /// early author isn't blocked and the section round-trips.
    #[serde(default)]
    pub provides: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct PluginDesignStub {
    /// Free-form emitted-surface name(s); typed shape deferred.
    #[serde(default)]
    pub emits: Vec<String>,
}
```

Each stub carries a doc comment + a deferral note. This satisfies "minimally
modeled (identity + typed-but-thin shape) with a note that full per-kind schema
is deferred" while keeping the discriminant exhaustive.

### D4 — `[[semantic_types]]` is UNTOUCHED; identity gains a tolerated `kind` field

`semantic_types: Vec<PluginSemanticTypeDecl>` and `PluginSemanticTypeDecl` are
**byte-for-byte unchanged** — 0019's resolver and the BR-scalar path depend on
them. We add nothing to the semantic path.

`PluginIdentity` gains one tolerated optional field, `kind: Option<String>`, to
absorb smtp's `[plugin].kind = "notifications/email-sender"` (a free-form
catalog string, distinct from our `PluginKind` enum). It does **not** feed
inference (D1 step 1 is the *top-level* `kind`). Without this, smtp's
`[plugin].kind` is already silently dropped (serde tolerates it today); we model
it explicitly so it stops being a surprise and is surfaced in the catalog. We
also add `module: Option<String>` as a tolerated alias for the `module` spelling
of `go_module` seen on smtp/captcha/waf/etc., with an accessor
`effective_go_module()` preferring `go_module` then `module`.

### D5 — No new error variants for v1; malformed = serde/structural rejection

The "rejects a malformed adapter contract" gate is satisfied **structurally**,
not via a new `PluginManifestError` variant. A malformed adapter is one where a
field that must be a string array is a scalar, or `[binds]` is a string instead
of a table — serde rejects these at parse with a typed error. We do **not** add
semantic validation (e.g. "`implements` entry must be `bucket.Interface`
shaped") in this spec — that crosses into 0022's verify territory. The test
asserts a structurally-malformed adapter manifest fails to deserialise.

*(If a future round wants a lint like "adapter declares `[binds]` but empty
`methods`", it registers a real diagnostic code in facets + bridge per the
plugin-platform discipline — out of scope here.)*

## Alternatives considered

- **Required `kind` field (serde-tagged enum, `#[serde(tag = "kind")]`).**
  Rejected: breaks all 24 manifests (none carry `kind`), forces edits to 24
  external repos, and a tagged enum makes *every* field kind-exclusive — but the
  real manifests are loose (a manifest could carry both semantic + adapter
  sections). Inference + flat optional fields is strictly more back-compatible.
- **Separate structs per kind, parsed by a dispatcher.** Rejected for v1: more
  surface, and the kinds share `[plugin]` identity. A single flat struct with
  optional per-kind sections + a `resolved_kind()` view is smaller and keeps
  0019's struct intact. 0022/0023 match on `resolved_kind()`.
- **Canonicalise the legacy spellings (`module`→`go_module`, `[contract]`→
  `[binds]`) by rewriting the 24 repos.** Rejected: out of this repo's scope and
  out of this spec's non-goals. We tolerate via optional alias fields + an
  accessor; migration is 0023's scaffolder concern.
- **Model `[env.<VAR>]` per-variable sub-tables now.** Rejected (Pareto): they
  are catalog/doc metadata on one plugin (smtp), not contract the compiler acts
  on. Deferred; serde drops them harmlessly.

## Consequences

- **Frozen contract.** `PluginManifest`'s new fields + `PluginKind` +
  `PluginEnvContract` + `PluginBindsContract` are the committed shape 0022
  (verify) and 0023 (scaffold) build against. Field renames after this lands are
  breaking changes to those specs.
- **0019 untouched.** Semantic path byte-identical; `resolved_kind()` returns
  `Semantic` for every manifest the resolver used to handle.
- **Readable, not yet verified.** The compiler can now *read* an adapter's
  contract. Acting on it (verify) is 0022; scaffolding it is 0023. This is the
  enabling schema layer only.
- **24/24 manifests still parse.** Guaranteed by the fixture test; the
  inference ladder classifies each to the expected kind.
