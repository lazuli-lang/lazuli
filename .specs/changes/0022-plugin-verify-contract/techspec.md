---
id: 0022
title: Plugin verify contract — end-to-end wiring proof + compile-time adapter contract check
type: techspec
track: prove/ship (plugin platform)
depends_on: [0020, 0021]
parallel_safe: false
status: ready
created: 2026-06-01
test_gate: "cargo test -p lazuli_cli plugin_verify && cargo test -p lazuli_doctor plugin_contract && cargo test --workspace"
agent: unassigned
---

# TechSpec — Plugin verify contract

## Approach
Two halves over the same shared contract logic. (1) A `lazuli plugin verify [--plugin <ns>]` subcommand that loads the project through the **authoritative resolver** (0020) + **typed manifest loader** (0021) — the same path real codegen walks — and emits a per-plugin, per-link PASS/FAIL report (`--json` + human), exiting non-zero on any FAIL. (2) A `lazuli_doctor` rule `PLUGIN-CONTRACT-001` (error) that makes the DECLARED adapter contract fail at `lazuli check` time: it fires when an adapter's `implements`/`[binds]` interface is not a known bucket interface, or when its declared capability has no registry binding to this plugin. The contract logic (known-bucket catalog + "is this plugin bound for capability X?") is ONE function both surfaces call. Honest static limit, stated everywhere: we verify the declared contract + the wiring graph; the Go method-set proof stays the plugin's runtime `var _ Interface = (*Adapter)(nil)` assertion under `go build`. The oracle is `lazuli plugin verify` on hostpoint reporting its 8 plugins' real wiring status.

> **Dependency note.** 0020 (single authoritative resolver doctor + codegen share) and 0021 (typed multi-kind manifest with `[binds]`/`implements`/`[env]`) are this spec's frozen contracts. At authoring time their techspecs are not yet written; 0022 references them by id/intent. The executing agent MUST land after 0020/0021 (`parallel_safe: false`) and consume their public surfaces: 0020's authoritative resolver entry point (the single `resolve_module_plugins`-derived path doctor + codegen share) and 0021's typed `PluginManifest` fields (`plugin.implements: Vec<String>`, `[binds]`, `[env]` as parsed structs on `crates/lazuli_manifest/src/plugin_manifest/types.rs::PluginManifest`). If a field name differs at integration time, bind to 0021's actual field, not the name guessed here.

## Surface
**Create:**
- `crates/lazuli_cli/src/commands/plugin/verify.rs` — the `plugin verify` command: `run_plugin_verify(project_root: &Path, only: Option<&str>, json: bool) -> anyhow::Result<i32>`. Loads via the authoritative resolver + typed manifest, builds a `Vec<PluginVerifyReport>`, renders human/JSON, returns the process exit code (non-zero on any FAIL).
- `crates/lazuli_cli/src/commands/plugin/contract.rs` — the SHARED contract logic, depended on by both the CLI and the doctor rule (re-exported so `lazuli_doctor` can call it, or duplicated as a tiny pure fn if the dep direction forbids it — see Plan step 2): `KNOWN_BUCKET_INTERFACES: &[&str]`, `fn classify_adapter_contract(manifest: &PluginManifest, plugin_ref: &str, registry: &RegistryView) -> ContractStatus` returning `Ok | UnknownInterface { declared, nearest } | UnboundCapability { capability }`.
- `crates/lazuli_doctor/src/correctness/plugin_contract_001.rs` — the `PLUGIN-CONTRACT-001` rule. Co-located `pub const CODE: &str = "PLUGIN-CONTRACT-001";`, a `//!` header carrying severity + a concrete `fires when` trigger cue (module_headers test), and `pub fn check_plugin_contract(...) -> Vec<DoctorDiagnostic>`.
- `crates/lazuli_cli/tests/plugin_verify.rs` — the verify CLI tests (TDD below): PASS on hostpoint's resolvable plugins, FAIL on a misdeclared-adapter fixture with the exact broken-link message.
- `crates/lazuli_doctor/tests/plugin_contract.rs` — the contract-rule unit tests (unknown-interface fires, unbound-capability fires, semantic-only plugin is `n/a`, well-bound adapter is clean).
- `crates/lazuli_cli/tests/fixtures/plugin-verify/` — fixtures: `ok-adapter/` (a tiny adapter plugin whose `implements` names a real bucket + an app that binds it) and `bad-adapter/` (same shape but `implements = ["payments.PaymentGatway"]` typo / no registry binding).

**Modify:**
- `crates/lazuli_cli/src/commands/plugin/mod.rs` (or the existing `plugin` command module) — declare `mod verify; mod contract;` and route the `verify` subcommand.
- `crates/lazuli_cli/src/cli_args/mod.rs` — add the `plugin verify [--plugin <ns>] [--json]` arg shape to the `plugin` subcommand enum.
- `crates/lazuli_doctor/src/correctness/mod.rs` — `mod plugin_contract_001;` + export `check_plugin_contract` / `CODE`.
- `crates/lazuli_keywords/src/registry/facets.rs` — add `P_PLUGIN_CONTRACT: &[DiagnosticFacet] = &[df("PLUGIN-CONTRACT-001", "error", "correctness")];` and attach it to the plugin/manifest capability row's `produces` (mirror how `P_AGGREGATE` etc. attach).
- `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/mod.rs` — extend `lazurite_manifest_diagnostics` to call the bridged contract check (new `plugins_contract` sub-module that adapts the shared logic to `DoctorPackage`), next to the existing `plugins_semantic` / `plugins_manifest` calls.
- `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugins_contract.rs` — **create** the thin aggregator bridge: `pub(super) fn check_plugin_contract(manifest, package) -> Vec<DoctorDiagnostic>` that builds the typed manifest + registry view from the `DoctorPackage` and delegates to the shared `classify_adapter_contract`.
- `docs/plugin-authoring.md` — add a "Verify your plugin wiring" section (`lazuli plugin verify` + the 5 links) and a "Contract check (`PLUGIN-CONTRACT-001`)" section (the two-part static+runtime story; keep the `var _ Interface = (*Adapter)(nil)` snippet).

## Contracts

**`lazuli plugin verify [--plugin <ns>] [--json]`** (the frozen CLI surface):
- Resolves the project root by upward search (reuse 0019's `find_project_root`). No root / no `Lazurite.toml` → exit non-zero with `not a Lazuli project (no Lazurite.toml found)`. Empty `[plugins]` → human note "no plugins declared", exit 0.
- Loads the module + manifest through the **0020 authoritative resolver** (NOT a private re-parse) so the alias map + import-emission view match real codegen exactly.
- Produces one `PluginVerifyReport { plugin: String, links: Vec<Link>, overall: Pass|Fail }` per declared plugin (filtered to `--plugin <ns>` when set; an unknown `<ns>` → exit non-zero, `plugin '<ns>' not declared in [plugins]`).
- **Link chain (ordered; a broken link short-circuits the meaning of deeper links → render them `skipped`):**
  - `L1 manifest` — `Pass` iff `manifest.toml` exists at the resolved plugin root AND parses into the 0021 typed shape with a `[plugin]` block. `Fail` detail names the resolved path + the parse error (mirrors `PLUGIN-MANIFEST-MISSING` / `PLUGIN-MANIFEST-SCHEMA-LEGACY`).
  - `L2 semantic` — for each `@semantic.*` the plugin declares, `Pass` iff it resolves in the authoritative alias map. `n/a` when the plugin declares no semantic types. `Fail` names the unresolved alias (mirrors `SEMANTIC-PLUGIN-001`).
  - `L3 contract` — `n/a` when the plugin declares no `implements`/`[binds]`. Otherwise delegates to `classify_adapter_contract`: `Pass` on `Ok`; `Fail` on `UnknownInterface` (detail: `implements '<x>' is not a known bucket interface (did you mean '<nearest>'?)`) or `UnboundCapability` (detail: `declares capability '<cap>' but the app registry binds no adapter to it — bind it in the registry or remove the declaration`). **Tail every L3 line with the honest-limit note.**
  - `L4 import` — `Pass` iff the plugin survives into a `LazuritePlugin { go_module: Some(_) }` (i.e. `read_plugin_go_module` resolved a module from `go.mod`, so `main_go.rs:273-308` WILL emit `_ "<go_module>"`). `Fail` detail: `go_module unresolved — no 'module ...' directive in <path>/go.mod; the side-effect import will NOT emit and the adapter will be ErrAdapterMissing at runtime`.
  - `L5 env` — for each var in the manifest's required `[env]` set, `Pass` iff present in the app's env contract. `Fail` lists the missing vars. `n/a` when `[env]` declares no required vars.
- **Exit code:** non-zero iff ANY plugin's `overall` is `Fail`. The `--json` shape is `{ "plugins": [ { "plugin", "overall", "links": [ { "id", "status", "detail" } ] } ], "ok": bool }`.

**`classify_adapter_contract(manifest, plugin_ref, registry) -> ContractStatus`** (the SHARED logic, single source of truth for both CLI L3 and the doctor rule):
- `KNOWN_BUCKET_INTERFACES` (v1 closed list, mirrors `runtime/go/lazuli/`): `payments.PaymentGateway`, `storage.ObjectStore`, `maps.Geocoder`, `notifications.EmailSender`, `auth/social.Provider`. (Extend by one line when a runtime bucket ships; pinned by a fixture test.)
- Collect the plugin's declared interfaces from the 0021 typed fields (`plugin.implements` + `[binds].interface`, normalised to the `<pkg>.<Interface>` short form — strip any `lazuli.dev/runtime/lazuli/` module prefix and the `.../lazuli-plugin-*` self-references).
- If the plugin declares NO interface → `Ok` with an `n/a`-equivalent marker (caller renders L3 `n/a`).
- For each declared interface NOT in `KNOWN_BUCKET_INTERFACES` → `UnknownInterface { declared, nearest }` (`nearest` = smallest edit-distance known interface, for the "did you mean" hint).
- For each known interface whose capability the app does NOT bind to `plugin_ref` in the registry view → `UnboundCapability { capability }`. (Binding read: the app's registry binding map keyed by capability → adapter ref; "bound" = there exists an entry whose adapter ref equals `plugin_ref`. When the registry expresses no bindings at all — common pre-binding pilot — treat as `Ok` to avoid false FAILs, consistent with `PLUGIN-UNUSED-001` being a warning; the unbound-capability FAIL fires only when the registry DOES bind the capability, to a DIFFERENT ref.)

**`PLUGIN-CONTRACT-001`** (the frozen doctor code): error severity, category `correctness`. Anchored at the plugin's `manifest.toml` (interface case) or `Lazurite.toml` (binding case). Message names the plugin, the offending interface/capability, the nearest known interface on a near-miss, and the fix — tail with the honest-limit note: `(method-set conformance is verified at runtime by 'var _ <Interface> = (*Adapter)(nil)' in the plugin's adapter.go under go build)`. Registered in `lazuli_keywords` facets, carries a `//!` header with a `fires when` trigger cue, bridged into `lazurite_manifest_diagnostics`.

**Invariant test contract (drift guard):** a test asserts the CLI L3 and the doctor `PLUGIN-CONTRACT-001` agree on the SAME fixture — both call `classify_adapter_contract`, so a `bad-adapter` fixture that the doctor flags MUST also be an L3 `Fail` in `verify`, and an `ok-adapter` clean in both. This prevents the two surfaces drifting.

## Plan — for the executing agent
1. Read 0020's landed techspec for the authoritative resolver entry point + 0021's landed `PluginManifest` typed fields (`implements`/`[binds]`/`[env]`). Read `crates/lazuli_cli/src/lazurite_codegen.rs` (`codegen_lazurite_manifest` + `read_plugin_go_module` — the `go_module` resolution that L4 mirrors), `crates/lazuli_codegen_go/src/emitter/root/main_go.rs:273-308` (the side-effect import emission L4 predicts), `crates/lazuli_doctor_run/src/doctor/aggregators/lazurite_manifest/plugins_semantic.rs` + `plugins_manifest.rs` (the diagnostic shape + `resolve_plugin_root`/`build_alias_map` calls to reuse), and the existing `plugin` CLI command module (find it via the `plugins` / `plugin` subcommand routing in `cli_args`).
2. Create `commands/plugin/contract.rs` with `KNOWN_BUCKET_INTERFACES` + `classify_adapter_contract` + `ContractStatus`. Decide the dep direction: if `lazuli_doctor` cannot depend on `lazuli_cli`, put the pure contract fn + catalog in a crate `lazuli_doctor` already depends on (e.g. `lazuli_manifest`) and have BOTH the CLI and the doctor rule call it — the catalog must have exactly one home. Write its unit tests FIRST.
3. Create `lazuli_doctor/src/correctness/plugin_contract_001.rs` (`CODE` const + `//!` header with a `fires when` cue + `check_plugin_contract`). Register `mod` + export in `correctness/mod.rs`.
4. Register the facet: add `P_PLUGIN_CONTRACT` in `lazuli_keywords/src/registry/facets.rs` and attach to the plugin/manifest capability `produces`. Run `cargo test -p lazuli_diagnostics_registry` (the bridge) + `cargo test -p lazuli_doctor module_headers` and confirm both green — they FAIL loudly if the code isn't registered / the header lacks a trigger cue.
5. Bridge into the run aggregator: create `plugins_contract.rs` under `aggregators/lazurite_manifest/`, add the `diagnostics.extend(plugins_contract::check_plugin_contract(manifest, package))` line in `mod.rs`.
6. Create `commands/plugin/verify.rs` (`run_plugin_verify`) loading via the authoritative resolver, building `PluginVerifyReport`s across L1-L5, rendering human + `--json`, returning the exit code. Route it in `commands/plugin/mod.rs` + `cli_args`.
7. Write the fixtures (`ok-adapter/`, `bad-adapter/`) + `tests/plugin_verify.rs` and `tests/plugin_contract.rs` (TDD — write the assertions FIRST, against the fixtures).
8. Run the gate (below). The CRITICAL acceptance is step-9 live hostpoint.
9. LIVE PROOF (read-only on hostpoint): `cargo build -p lazuli_cli`, then from `C:\Users\lucas\hostpoint`: `target\debug\lazuli.exe plugin verify`. Confirm it reports all **8** declared plugins (mercadopago, google-maps, scalars-br, object-store, smtp, sms-twilio, social-google, social-apple) with per-link status. Capture the output. The PILOT gate is "reports its 8 plugins' real wiring status" — some links MAY legitimately FAIL (e.g. an adapter not yet bound in the registry); report which and why. DO NOT commit hostpoint changes.
10. TEACH: `docs/plugin-authoring.md` — the verify + contract-check sections.
11. Commit framework on the loop branch.

## Tests first (TDD)
- [ ] `classify_unknown_interface_flags_typo` (contract unit) — `implements = ["payments.PaymentGatway"]` → `UnknownInterface { nearest: "payments.PaymentGateway" }`.
- [ ] `classify_known_interface_unbound_capability` — adapter declares a known interface, registry binds the capability to a DIFFERENT ref → `UnboundCapability`.
- [ ] `classify_known_interface_bound_is_ok` — adapter declares a known interface, registry binds the capability to THIS plugin → `Ok`.
- [ ] `classify_no_registry_bindings_is_ok` — adapter declares a known interface, registry expresses no bindings at all → `Ok` (no false FAIL, mirrors `PLUGIN-UNUSED-001` warning stance).
- [ ] `classify_semantic_only_plugin_is_na` — plugin declares no `implements`/`[binds]` → contract `n/a`, never a FAIL.
- [ ] `plugin_contract_001_fires_on_unknown_interface` (doctor) — `bad-adapter` fixture → exactly one `PLUGIN-CONTRACT-001` at error severity, message names the plugin + nearest interface + the honest-limit tail.
- [ ] `plugin_contract_001_clean_on_ok_adapter` (doctor) — `ok-adapter` fixture → zero `PLUGIN-CONTRACT-001`.
- [ ] `plugin_verify_passes_on_ok_adapter` (CLI) — `verify` on `ok-adapter` → exit 0, every link `Pass`/`n/a`.
- [ ] `plugin_verify_fails_on_bad_adapter_with_broken_link` (CLI) — `verify` on `bad-adapter` → non-zero exit, L3 `Fail` with the unknown-interface detail (assert the exact broken-link substring).
- [ ] `plugin_verify_json_shape` (CLI) — `--json` emits `{ "plugins": [...], "ok": false }` with per-link `id`/`status`/`detail`; assert parseable + the failing link present.
- [ ] `plugin_verify_scopes_to_single_plugin` (CLI) — `--plugin <ns>` reports only that plugin; unknown ns → non-zero exit.
- [ ] `verify_and_doctor_agree_drift_guard` (CLI/doctor invariant) — the `bad-adapter` fixture is BOTH an L3 `Fail` in `verify` AND a `PLUGIN-CONTRACT-001` in doctor; `ok-adapter` is clean in both (proves both call the shared `classify_adapter_contract`).
- [ ] `bucket_catalog_pinned` (contract unit) — `KNOWN_BUCKET_INTERFACES` equals the expected v1 set (drift reminder for when a runtime bucket ships).

## Gate

### Definition of Done (Lazuli Plugin Platform gate)
1. BUILD: implemented; **`cargo test --workspace` green (FULL sweep, not per-crate)**.
2. PILOT: `lazuli plugin verify` on hostpoint reports its **8 plugins' real wiring status** (captured output; legitimate FAILs reported with reason).
3. TEACH: `docs/plugin-authoring.md` documents `lazuli plugin verify` + the `PLUGIN-CONTRACT-001` contract check (static/runtime split).
4. ENFORCE: the misdeclared-adapter fixture tests (`verify` FAILs with the right broken-link message; `PLUGIN-CONTRACT-001` fires) + the verify/doctor drift-guard prevent regression.

**Four concrete gates:**
1. **BUILD** — `cargo test -p lazuli_cli plugin_verify` (all CLI TDD) + `cargo test -p lazuli_doctor plugin_contract` (all doctor TDD) + `cargo test --workspace` 0 failures + `cargo build --workspace` clean. The new diagnostic IS registered: `cargo test -p lazuli_diagnostics_registry` (bridge) + `cargo test -p lazuli_doctor module_headers` + `cargo test -p lazuli_keywords` all green.
2. **PILOT** — hostpoint `lazuli plugin verify` reports 8 plugins with per-link status (report the actual PASS/FAIL breakdown; e.g. "8 plugins, 6 fully PASS, 2 with L3 unbound-capability — reasons captured").
3. **TEACH** — plugin-authoring.md verify + contract sections landed.
4. **ENFORCE** — `plugin_verify_fails_on_bad_adapter_with_broken_link` + `plugin_contract_001_fires_on_unknown_interface` + `verify_and_doctor_agree_drift_guard` green.

## Risks & rollback
- **0020/0021 not landed when this starts** (`parallel_safe: false`) → mitigation: this spec is sequenced AFTER them; the executing agent binds to their ACTUAL surfaces (authoritative resolver entry point + typed `PluginManifest` fields). If a field name differs, bind to the real one — the contract logic is field-shape-agnostic beyond "give me the declared interfaces + required env".
- **`lazuli_doctor` cannot depend on `lazuli_cli`** (so the shared contract fn can't live in `commands/plugin/contract.rs` as the single home) → mitigation: Plan step 2 — put `classify_adapter_contract` + `KNOWN_BUCKET_INTERFACES` in `lazuli_manifest` (which `lazuli_doctor` already depends on) and have both surfaces call it. One home for the catalog is non-negotiable; the drift-guard test enforces agreement.
- **L3 false FAIL on a legitimately-unbound adapter** (pilot declares an adapter before binding its capability) → mitigation: the `classify_no_registry_bindings_is_ok` rule — `UnboundCapability` fires ONLY when the registry binds the capability to a DIFFERENT ref, never when it binds nothing. hostpoint's pre-binding plugins report L3 `Pass`/`n/a`, not FAIL.
- **Heterogeneous legacy manifests** (sms-twilio `[provides]` with no interface, social-google `[provides].go_interface`) → mitigation: 0021 normalises these into typed `implements`/`[binds]`; a plugin that yields NO declared interface is L3 `n/a`, never a FAIL. The contract check reads ONLY 0021's typed fields, not the legacy prose shapes.
- **`verify` PASS misread as a method-set guarantee** → mitigation: the honest-limit note on every L3 line + the doctor message tail + the docs section; the runtime `var _ Interface = (*Adapter)(nil)` is named as the actual proof.
- **Bucket catalog goes stale** when a new runtime bucket ships → mitigation: `bucket_catalog_pinned` test fails on drift and points at the one-line edit.

**Rollback:** `git revert` — the change is one CLI subcommand + one doctor rule + one shared fn + facet/aggregator registration + docs. Absent it, behaviour is exactly today's (adapter contracts unverified at compile time; no `plugin verify`). No pilot file is committed.
