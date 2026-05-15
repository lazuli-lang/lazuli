# Wave C — Deferred CL Integration Catalog

**Date**: 2026-05-15
**Status**: 4 Claude-authored branches preserved in worktrees; pending surgical re-integration into `main`.

## Context

Wave C dispatched 10 parallel workers (5 Codex + 5 Claude subagents), each in an isolated worktree off `5b279f0`. The 5 Codex commits and 1 Claude commit (CL.C.4) cherry-picked cleanly into `main`. The remaining 4 Claude commits each ran `cargo fmt` over the workspace, producing line-ending/import-sort churn across 60-90 unrelated files that surfaces as merge conflicts during cherry-pick. Substantive content remains valid and is worth ~8-10 hours of focused integration work.

## Branches preserved

Each branch sits unpushed on its own commit in `.claude/worktrees/wave-c-<id>/`. Branch names:

| Worker | Branch | Commit | Substantive files | Spec |
|---|---|---|---|---|
| CL.C.1 | `wave-c-cl1-branch` | `56e96a9` | 9 substantive + 1 new (`expand_http.rs`) | `/c/tmp/wave-c-cl1-prompt.md` + `/c/tmp/wave-c-cl1-report.md` |
| CL.C.2 | `wave-c-cl2-branch` | `fea4d5f` | ~10 substantive + 3 new doctor diagnostic files | `/c/tmp/wave-c-cl2-prompt.md` + `/c/tmp/wave-c-cl2-report.md` |
| CL.C.3 | `wave-c-cl3-branch` | `826ba5e` | 9 substantive + 3 fixture .lzi | `/c/tmp/wave-c-cl3-prompt.md` + `/c/tmp/wave-c-cl3-report.md` |
| CL.C.5 | `wave-c-cl5-branch` | `043891d` | ~12 substantive + new `security_duration.rs` module | `/c/tmp/wave-c-cl5-prompt.md` + `/c/tmp/wave-c-cl5-report.md` |

## What each branch ships

### CL.C.1 — HTTP cookie / proxy / limits app-level blocks (roadmap §1.2)

- New IR types: `AppCookie { profiles }`, `AppProxy { trusted, real_ip_header, ... }`, `AppLimits { body_size, header_size, ... }`.
- `AppManifest.cookie/proxy/limits: Option<...>` slots (additive).
- Parser: 3 `parse_app_<kind>_block` functions under `app_manifest.rs`.
- Doctor: `app-cookie-contract`, `app-proxy-contract`, `app-limits-contract`.
- LSP: hovers for each block + closed-catalog completion for children.
- Inspect: `--expand=http` projection in new `expand_http.rs` (200 LOC, **no overlap with existing code**).
- Fixture: `examples/full-capsule/app.lzi` extended with the new blocks.

### CL.C.2 — DB resource-level decorators (roadmap §1.5)

- New IR types: `LockSpec { Optimistic { version_field } / Pessimistic / RowLevel }`, `CompositeKey { fields, primary }`.
- Field gains `slug: bool` and `full_text: bool` — note `slug` already lives in `main` from CL.C.4; **dedupe at integration time**.
- Resource gains `lock: Option<LockSpec>`, `composite_key: Option<CompositeKey>`.
- Parser: closed-catalog dispatch for the new decorators + depth-aware `@full_text` extractor.
- Doctor: 3 new files under `crates/lazuli_cli/src/doctor/correctness/`:
  - `resource_lock_contract_001.rs`
  - `composite_key_contract_001.rs`
  - `full_text_type_001.rs`
- LSP: 4 new hovers + `RESOURCE_LOCK_STRATEGY_VALUES` completion.
- DDL emission in `migration_ddl.rs`: `composite_key primary true` → `PRIMARY KEY (<fields>)`; `@full_text` → `CREATE INDEX … USING GIN(to_tsvector(...))`.

### CL.C.3 — Feature-level `cache <name>` kind (roadmap §1.15)

- New IR type: `CacheProfile { name, key, ttl, namespace, tags, stale_while_revalidate, coalesce, sliding }`.
- `Feature.caches: Vec<CacheProfile>` (additive).
- `QueryCache.profile_ref: Option<String>` (additive — existing inline form preserved).
- Analyzer: profile lowering + reference resolution copies the profile body into `QueryCache` so codegen/runtime never re-lookup.
- Parser: feature-level `cache <name>` block with 7 closed-catalog children + `cache <profile>` query reference + mutual-exclusion guard.
- Doctor: `cache-profile-unknown`, `cache-tag-unknown`, `cache-ttl-contract`.
- LSP: 3 new hovers (`stale_while_revalidate`, `coalesce`, `sliding`) + closed-catalog completion.
- Inspect: `--expand=caches` projection sibling.
- Fixtures: 3 new `.lzi` files under `crates/lazuli_cli/tests/fixtures/cache/`.
- **Runtime untouched** per spec (`runtime/go/lazuli/cache/` stays hand-written).

### CL.C.5 — `app.headers` block + `secret_rotation` policy kind (roadmap §1.10)

- New module: `crates/lazuli_ir/src/security_duration.rs` (shared duration parsing helper).
- New IR types: `AppHeaders { csp, hsts, x_frame_options, x_content_type_options, referrer_policy, permissions_policy }`, `AppHsts { max_age, include_subdomains, preload }`, `SecretRotation { name, cadence, overlap, auto_rollback }`.
- `EncryptionBinding.rotation_profile: Option<String>` (additive — ties `@key.<scope>` to a rotation profile).
- Parser: indent-4 `app.headers` block (inline CSP and block form for HSTS) + indent-2 named `registry.secret_rotation <name>` blocks.
- Doctor: `headers-contract` (production-grade slot coverage), `secret-rotation-overlap-contract`, `secret-rotation-binding-unknown`.
- LSP: 14 new hovers + completion for the new blocks.
- Inspect: new `app_security` projection surfaces under `--expand=security`.

## Why cherry-pick fails today

Each branch's commit has two concerns intermixed:

1. **Substantive additions** — the new types, parser functions, doctor diagnostics, LSP entries. These are small (10s-100s of LOC) and **conflict-free** with each other if applied surgically.
2. **`cargo fmt` cascade** — running `cargo fmt --check` after the mechanical fan-out (adding `headers: None` / `cookie: None` / `lock: None` / etc. to ~80 sibling test fixtures across the workspace) reformatted unrelated files. Each branch shipped this normalize as part of the ONE final commit per spec.

When two branches add a field to `AppManifest` (e.g., CL.C.1 adds `cookie/proxy/limits`; CL.C.5 adds `headers`) AND both reformatted the same test fixtures, every fixture becomes a 3-way conflict even though the conceptual additions don't overlap.

## Recommended integration sequence (next session)

Work in **isolation order** (least cross-cutting first):

1. **CL.C.1** — most isolated. Copy `expand_http.rs` directly (zero conflicts). Manually port `AppCookie/AppProxy/AppLimits` to `crates/lazuli_ir/src/lib.rs`. Add 3 `parse_app_<kind>_block` to `app_manifest.rs`. Wire `--expand=http` flag to `main.rs`. Mechanical fan-out via sed/grep for `headers: None` style: `find . -name '*.rs' -exec sed -i ... \;` after IR is in. Commit.

2. **CL.C.5** — close cousin of CL.C.1. Port `security_duration.rs` module, `AppHeaders`/`AppHsts`/`SecretRotation` types, 3 doctor diagnostics. Same mechanical pattern.

3. **CL.C.3** — adds `CacheProfile` + `Feature.caches`. Slightly more invasive because `QueryCache.profile_ref` touches the existing query slot. Resolve by copying the analyzer's resolution-copies-into-QueryCache pattern verbatim.

4. **CL.C.2** — last because it adds two `Field` flags (`slug` already exists from CL.C.4; need to dedupe) + a new `LockSpec` enum. The dedup means CL.C.2's `Field.slug: bool` line MUST be removed before applying CL.C.2's IR diff.

After each branch lands:
- `cargo build -p lazuli_cli` first (fast feedback on missing test-literal updates).
- Use `sed` or a small Python helper to add the new struct field (default to `None`/`vec![]`/`false`) across every test-literal site. Pattern: `field_above: vec![],` → `field_above: vec![],\n            new_field: <default>,`.
- `cargo test --workspace` to confirm.
- Commit with the worker's original commit message, prefixed by `(re-integrated)`.

## Process learning for Wave D+

The orchestrator should do one of:

1. **Sequential commits between waves** — dispatch 3-5 workers, cherry-pick all + push to main, THEN dispatch next 3-5. Each subsequent wave forks from a more up-to-date `main`. Slower wall-clock but eliminates 90% of conflicts.
2. **Forbid `cargo fmt` in worker prompts** — explicitly tell Codex/Claude subagents NOT to run `cargo fmt` before commit. They handle mechanical fan-out via targeted edits only. The orchestrator runs `cargo fmt` once after the wave merges.
3. **Per-worker rebase before commit** — each worker rebases its branch onto the current `main` right before committing. Slower per worker but cleaner merges.

Pattern 2 is the lightest-touch change with the biggest payoff. Wave D prompts should add a constraint line: **"DO NOT run `cargo fmt` on the workspace; rely on targeted edits to keep formatting consistent."**
