---
id: 0021
title: Plugin typed manifest — kind discriminant + adapter schema
type: techspec
track: evolve (plugin platform)
depends_on: [0019]
parallel_safe: true
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_manifest plugin_manifest && cargo test --workspace"
agent: unassigned
---

# TechSpec — Plugin typed manifest

## Approach

Make `PluginManifest` kind-discriminated and adapter-aware **additively**: add a
`PluginKind` enum + optional per-kind sections (`implements`, `[env]`, `[binds]`
for adapter; thin `[capability]`/`[design]` stubs) as flat `#[serde(default)]`
fields, plus a `resolved_kind()` inference accessor. The `[[semantic_types]]`
path is byte-for-byte untouched so 0019's resolver is unaffected. No tagged
enum, no required field — every one of the 24 real manifests keeps
deserialising. The oracle is: every real manifest (scalars-br + 24
`lazuli-plugin-*`) round-trips and classifies to its expected kind; a malformed
adapter is structurally rejected. Pure schema crate addition — `lazuli_manifest`
+ `docs/` only.

## Surface

**Modify:**
- `crates/lazuli_manifest/src/plugin_manifest/types.rs` — add `PluginKind`,
  `PluginEnvContract`, `PluginBindsContract`, `PluginCapabilityStub`,
  `PluginDesignStub`; extend `PluginManifest` with `kind`, `implements`, `env`,
  `binds`, `capability`, `design` (all `#[serde(default)]`); add
  `PluginManifest::resolved_kind()`; add `PluginIdentity.kind` + `.module`
  tolerated fields + `effective_go_module()` accessor. `PluginSemanticTypeDecl`
  and `ResolvedPluginSemantic` UNCHANGED.
- `crates/lazuli_manifest/src/plugin_manifest/mod.rs` — extend the `pub use
  types::{...}` re-export with the new public types + `PluginKind`. Update the
  `## Sub-files` doc bullet for `types` to list the new structs.
- `docs/plugin-authoring.md` — add a "Manifest is typed per kind" section: the
  `kind` discriminant, the inference ladder, and the blessed adapter schema
  (`implements` / `[env]` / `[binds]`), with the deferral note for
  capability/design.

**Create:**
- `crates/lazuli_manifest/tests/fixtures/plugin_manifests/` — vendored copies of
  the 25 real manifests (scalars-br + 24 `lazuli-plugin-*`), one `.toml` each,
  named `<short>.toml` (e.g. `mercadopago.toml`, `smtp.toml`). Plus
  `malformed_adapter.toml` (a structurally-broken adapter). These are TEST DATA
  vendored into the repo so the test is hermetic (the real plugins live outside
  this repo at `c:\Users\lucas\dev\lazuli-plugin-*`).
- `crates/lazuli_manifest/tests/plugin_manifest_typed.rs` — the round-trip +
  kind-inference + malformed-rejection tests (TDD below). (May instead extend
  the existing `crates/lazuli_manifest/src/plugin_manifest/tests.rs` if the
  agent prefers an in-crate test module — pick one, keep the `plugin_manifest`
  test-name filter matching so `cargo test -p lazuli_manifest plugin_manifest`
  runs them.)

## Contracts

**`PluginKind`** (FROZEN — 0022/0023 match on this):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind { #[default] Semantic, Adapter, Capability, Design }
```

**`PluginManifest`** (extended; new fields only, all `#[serde(default)]`):
```rust
pub struct PluginManifest {
    #[serde(default)] pub plugin: Option<PluginIdentity>,
    #[serde(default)] pub semantic_types: Vec<PluginSemanticTypeDecl>, // UNCHANGED
    #[serde(default)] pub kind: Option<PluginKind>,        // explicit override
    #[serde(default)] pub implements: Vec<String>,         // adapter: bucket.Interface
    #[serde(default)] pub env: Option<PluginEnvContract>,  // adapter: [env]
    #[serde(default)] pub binds: Option<PluginBindsContract>, // adapter: [binds]
    #[serde(default)] pub capability: Option<PluginCapabilityStub>, // stub
    #[serde(default)] pub design: Option<PluginDesignStub>,         // stub
}
```

**`PluginManifest::resolved_kind(&self) -> PluginKind`** (FROZEN inference;
precedence ladder from ADR D1):
1. `if let Some(k) = self.kind { return k; }` — explicit top-level override wins.
2. `if !self.semantic_types.is_empty() { return Semantic; }` — preserves 0019.
3. `if !self.implements.is_empty() || self.env.is_some() || self.binds.is_some()
   { return Adapter; }` — captures all 23 real adapters.
4. `Semantic` — historical default for identity-only manifests.

   *(Capability/Design are never inferred in v1 — they only arise from an
   explicit `kind`. Documented in the accessor's doc comment.)*

**`PluginEnvContract`** (`[env]`): `required: Vec<String>`, `optional:
Vec<String>`, `required_for_auth: Vec<String>` — all `#[serde(default)]`.
Grounded in mercadopago `required = ["MERCADOPAGO_ACCESS_TOKEN",
"MERCADOPAGO_WEBHOOK_SECRET"]` / `optional = []`; object-store multi-line
`optional`; smtp `required_for_auth = ["SMTP_USERNAME", "SMTP_PASSWORD"]`.
Per-var sub-tables (`[env.SMTP_TLS_MODE]`) NOT modeled — serde drops them.

**`PluginBindsContract`** (`[binds]`): `interface: Option<String>`, `methods:
Vec<String>` — `#[serde(default)]`. Grounded in smtp `interface =
"github.com/lazuli-lang/lazuli-plugin-smtp.EmailSender"`, `methods =
["SendEmail", "SendEmailBatch"]`.

**`PluginCapabilityStub`** / **`PluginDesignStub`** (thin, deferred): `provides:
Vec<String>` / `emits: Vec<String>`, `#[serde(default)]`. Doc comment marks full
schema DEFERRED.

**`PluginIdentity`** (extended; tolerated fields only):
- `#[serde(default)] pub kind: Option<String>` — absorbs smtp's `[plugin].kind`
  free-form string; does NOT feed inference.
- `#[serde(default)] pub module: Option<String>` — legacy alias for `go_module`.
- `effective_go_module(&self) -> Option<&str>` — `go_module` then `module`.

**Back-compat invariant (the gate):** for each vendored real manifest, `toml::
from_str::<PluginManifest>(...)` is `Ok`, and `resolved_kind()` equals the
expected kind: `scalars-br → Semantic`; all 24 `lazuli-plugin-*` adapters →
`Adapter` (they all carry `implements` and/or `[env]` and/or `[binds]`).

## Plan — for the executing agent

1. Read `crates/lazuli_manifest/src/plugin_manifest/types.rs` (full),
   `mod.rs` (re-exports), `tests.rs` (existing test style), and
   `docs/plugin-authoring.md` (doc style + where the manifest is described).
2. Add the new types + extend `PluginManifest` + `PluginIdentity` in `types.rs`.
   Add `resolved_kind()` + `effective_go_module()`. Do NOT touch
   `PluginSemanticTypeDecl` / `ResolvedPluginSemantic`.
3. Extend the `pub use types::{...}` in `mod.rs` to export `PluginKind`,
   `PluginEnvContract`, `PluginBindsContract`, `PluginCapabilityStub`,
   `PluginDesignStub`. Update the `## Sub-files` doc bullet.
4. Vendor fixtures: copy each real `manifest.toml` from
   `c:\Users\lucas\dev\lazuli-plugin-*\manifest.toml` (24 dirs) +
   `lazuli-plugin-scalars-br` into
   `crates/lazuli_manifest/tests/fixtures/plugin_manifests/<short>.toml`. Author
   `malformed_adapter.toml` (e.g. `implements = "payments.PaymentGateway"` as a
   scalar string instead of an array, or `[binds]` declared as `binds =
   "EmailSender"` scalar) — something serde structurally rejects.
5. Write the tests FIRST (TDD below), then make them pass.
6. TEACH: add the "Manifest is typed per kind" section to
   `docs/plugin-authoring.md`.
7. Run the gate (below).
8. Commit framework on `loop-serial` (or the active loop branch).

## Tests first (TDD)

Put these under `cargo test -p lazuli_manifest plugin_manifest` (filename
`plugin_manifest_typed.rs` or an in-crate `plugin_manifest::tests` module —
either keeps the `plugin_manifest` filter matching):

- [ ] `all_real_manifests_deserialize` — iterate every `.toml` in
  `tests/fixtures/plugin_manifests/` except `malformed_adapter.toml`;
  `toml::from_str::<PluginManifest>` must be `Ok` for each (the 24-adapter +
  scalars-br back-compat gate). Assert the count is ≥ 25 so a missing fixture
  fails loudly.
- [ ] `scalars_br_infers_semantic` — `scalars-br.toml` →
  `resolved_kind() == Semantic` AND `semantic_types.len() == 4` (0019 path
  intact — the 4th type `BrazilianPhone` present).
- [ ] `mercadopago_infers_adapter_with_contract` — `mercadopago.toml` →
  `resolved_kind() == Adapter`; `implements == ["payments.PaymentGateway"]`;
  `env.required == ["MERCADOPAGO_ACCESS_TOKEN", "MERCADOPAGO_WEBHOOK_SECRET"]`.
- [ ] `smtp_models_binds_and_env` — `smtp.toml` → `resolved_kind() == Adapter`;
  `binds.interface == Some(".../lazuli-plugin-smtp.EmailSender")`;
  `binds.methods == ["SendEmail", "SendEmailBatch"]`;
  `env.required_for_auth == ["SMTP_USERNAME", "SMTP_PASSWORD"]`;
  `plugin.kind == Some("notifications/email-sender")` (tolerated free-form);
  `effective_go_module()` falls back to `module` (smtp has no `go_module`).
- [ ] `object_store_optional_env_multiline` — `object-store.toml` →
  `env.optional` contains `"S3_ENDPOINT"` and length 5 (the multi-line array
  parses).
- [ ] `explicit_kind_overrides_inference` — an inline manifest with
  `kind = "capability"` and a `[capability]` block →
  `resolved_kind() == Capability` even with no semantic/adapter sections; and an
  inline manifest with `kind = "adapter"` + `[[semantic_types]]` →
  `resolved_kind() == Adapter` (explicit wins over the semantic-present rule).
- [ ] `identity_only_defaults_semantic` — `viacep.toml` (only `[plugin]`, no
  sections) → `resolved_kind() == Semantic` (historical default; harmless — no
  aliases contributed).
- [ ] `malformed_adapter_rejected` — `toml::from_str::<PluginManifest>` on
  `malformed_adapter.toml` is `Err` (proves the schema is load-bearing).
- [ ] `semantic_decl_struct_unchanged` — a focused deserialize of one
  `[[semantic_types]]` entry still yields all 0019 fields
  (`name`/`alias`/`carrier_type`/`validator`/`formatter`/`error_code`/
  `message_key`/`ts_validator`) — drift guard that the semantic struct didn't
  regress.

## Gate

### Definition of Done (Lazuli Plugin Platform gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep, not
   per-crate)**.
2. PILOT: every real plugin manifest (scalars-br + the 24
   `c:\Users\lucas\dev\lazuli-plugin-*`) deserialises and classifies to its
   expected kind — proven by the vendored-fixture
   `all_real_manifests_deserialize` test (this spec's "pilot" is the real
   manifest corpus, since it is a schema-only change with no codegen surface).
3. TEACH: `docs/plugin-authoring.md` documents the `kind` discriminant + the
   typed adapter schema (`implements` / `[env]` / `[binds]`) + the
   capability/design deferral note.
4. ENFORCE: the round-trip + kind-inference + `malformed_adapter_rejected` tests
   prevent regression of the frozen contract.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_manifest plugin_manifest` (all TDD green) +
   `cargo test --workspace` 0 failures + `cargo build --workspace` clean. No new
   diagnostic code added (malformed rejection is structural serde failure, not a
   doctor code) — if you DO add one, register it in facets + bridge.
2. **PILOT** — `all_real_manifests_deserialize` green over ≥ 25 vendored
   fixtures; `resolved_kind()` matches expected for scalars-br (Semantic) + each
   adapter (Adapter). Report the fixture count.
3. **TEACH** — plugin-authoring.md "Manifest is typed per kind" section landed.
4. **ENFORCE** — round-trip + inference + malformed-rejection + semantic-drift-
   guard tests green.

## Risks & rollback

- **A new field name collides with an existing TOML key and silently changes
  parsing** (e.g. a real manifest has a top-level `kind` we'd now interpret as
  the enum) → mitigation: census shows only smtp uses `kind`, and only **inside
  `[plugin]`** (free-form string, absorbed by `PluginIdentity.kind`), never
  top-level. The `all_real_manifests_deserialize` test catches any
  misclassification. If a future manifest sets a top-level `kind` to a non-enum
  string, serde rejects it — caught by the corpus test at vendor time.
- **Inference ladder misclassifies a semantic+adapter hybrid** → mitigation:
  semantic-present wins (step 2 before 3), so 0019's resolver never loses a
  manifest; the `explicit_kind_overrides_inference` test pins the override path.
- **The frozen shape is wrong for 0022/0023** → mitigation: the field set is
  grounded verbatim in the real mercadopago/smtp/object-store keys; the ADR
  records why each field exists. If 0022 needs more, it extends additively
  (new `#[serde(default)]` field), not a rename.
- **Vendored fixtures drift from the real plugin repos** → accepted: the
  fixtures are a frozen snapshot of the contract-bearing keys as of 2026-06-01;
  their job is back-compat regression, not live mirroring. A note in the
  fixtures dir records the source paths + snapshot date.

**Rollback:** `git revert` — the change is purely additive struct fields + an
accessor + tests + a doc section. Absent it, `PluginManifest` is exactly today's
two-field struct and every manifest parses as before (the new fields are all
`#[serde(default)]`, so reverting drops them with no behavioural change to the
semantic path). No pilot/runtime file is committed.
