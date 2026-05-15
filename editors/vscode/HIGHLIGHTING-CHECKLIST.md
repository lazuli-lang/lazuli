# Lazuli VS Code Extension — Syntax Highlighting Checklist

End-to-end roadmap to "perfect" highlighting for `.lzi` + `.lzx` + `Lazurite.toml`. Each item is independently dispatchable to a subagent. Phased by ROI: ship Phase 1 first (high-impact, low-cost), iterate.

Authoritative source for keyword catalog: `crates/lazuli_lsp/src/lib.rs` `keyword_description()` + `KEYWORDS` const + per-context `is_canonical_*_block` checks.

Status legend: ☐ pending · ◐ partial · ✅ done · ⊘ deferred (out of scope or low ROI)

---

## Phase 1 — high-ROI polish (visible in 5 minutes of usage)

### 1.1 Decorator namespace coloring ☐
**Problem:** `@policy.member`, `@scope.workspace_member`, `@fn.X`, `@cap.X`, `@semantic.X`, `@pii.X`, `@key.X`, `@anchor.X`, `@actor.X`, `@role.X`, `@runtime.X`, `@plugin.X`, `@validator.X`, `@hook.X`, `@client.X`, `@translation.X`, `@adapter.X`, `@query_modifier.X`, `@tool.X`, `@llm.X`, `@slug.X`, `@full_text.X` are all rendering plain because `entity.name.reference.decorator.lazuli` isn't in most themes.
**Fix:** change to `entity.name.tag.lazuli` (HTML-tag-style, universally colored) OR `support.type.decorator.lazuli` (less standard but specific). Test against Default Dark+ + Material Icon Theme.
**Acceptance:** `@policy.X` shows distinct color (likely orange/coral) in any default theme.

### 1.2 HTTP method constants ☐
**Problem:** `GET`/`POST`/`PUT`/`PATCH`/`DELETE`/`HEAD`/`OPTIONS` after `method` keyword need to read as constants (not as identifiers).
**Fix:** add pattern `\b(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\b` → `constant.language.http-method.lazuli` inside api-block.
**Acceptance:** `method GET` shows `GET` in constant color (typically blue/cyan).

### 1.3 Closed-catalog enum-like values ☐
**Problem:** values like `cascade`, `restrict`, `nullify`, `lax`, `strict`, `none` (cookie same_site), `DENY`, `SAMEORIGIN` (frame options), `manual`, `kms_managed` (rotation), `argon2id`, `bcrypt`, `scrypt` (hash algorithms), `aes_256_gcm` (encryption), `lazy`, `eager`, `merge`, `append` (digest strategy), `nosniff` show plain.
**Fix:** add `constant.language.closed-catalog.lazuli` patterns scoped to their respective blocks (cookie/encryption/headers/auth/digest/etc.) for each known value list.
**Acceptance:** values in closed catalogs show as constants (color match `true`/`false`/numbers).

### 1.4 Audit `@decorator` argument names ☐
**Problem:** inside `@cap.File(max_size:..., accept:..., visibility:..., signed_ttl:...)` — the keys (`max_size`, `accept`, `visibility`, `signed_ttl`, `algorithm`, etc.) are arg names but currently render plain.
**Fix:** add pattern inside the decorator-paren content: match `[a-z_]+\s*:` as `variable.parameter.decorator-arg.lazuli`.
**Acceptance:** `@cap.File(max_size:"10mb")` shows `max_size` distinctly.

### 1.5 cron strings inside `trigger schedule "..."` ☐
**Problem:** cron expressions inside the quoted string render as plain string content; for power users, validating cron visually helps.
**Fix:** OPTIONAL polish — add inline pattern `^"((?:[*0-9,\-/]+\s+){4}[*0-9,\-/]+)"$` to detect cron literals and color the fields. Defer if not high impact.
**Acceptance:** decision call — skip unless explicitly asked.
**Status:** ⊘ deferred unless requested

---

## Phase 2 — context disambiguation (kill remaining over/under-matching)

### 2.1 Word-as-identifier vs word-as-keyword ☐
**Problem:** several words double as keywords in one context and identifiers in another. Current grammar uses `(?=\s+\S)` lookahead to skip bare cases, but some still leak.

Concrete cases to verify:

- `default` — modifier in field decl (`Integer default 0`) vs decorator arg key (`@cap.File(default:...)`) vs enum value
- `from` — `from creates` modifier vs `payload from webhook_events.X` ref vs `(select id from slug)` SQL
- `to` — route target (`to customer.view.X`) vs general preposition
- `at` — `route X at "/path"` vs `audit at line 42`
- `by` — `paginate by 50` vs `idempotency by ctx.X` vs `query.lookup by_X`
- `tags`, `prompt`, `contract`, `integration`, `deprecated` — already fixed via lookahead, but verify with a regression test

**Fix:** for each, audit current grammar matches against canonical examples; refine patterns with stricter context (require preceding kind keyword, require specific position in line).
**Acceptance:** open `examples/full-capsule/full-capsule.lzi` + Pleiades — visually verify each word colors correctly per context.

### 2.2 Block-opener vs statement keyword unification ◐
**Status:** mostly done in last pass (unified to `keyword.control.section.lazuli` + `keyword.control.statement.lazuli`).
**Remaining:** spot-check that all bare block-openers (e.g., `tools` opening agent.tools subblock vs `tools` as command-statement) still color consistently.

### 2.3 Field-decl type pass-through ✅
**Status:** done. Field type now falls through to `#types` for primitive/UI/extension/domain coloring.

### 2.4 Reference paths (`input.X`, `ctx.X`, `output.X`, `payload.X`) ☐
**Problem:** `input.q`, `ctx.tenant.id`, `output.score`, `payload.customer_id` — currently matched by `entity.name.reference.semantic.lazuli` (or `references` pattern). Themes color `entity.name` but the LEAF (after the dot) might lose distinct treatment.
**Fix:** consider splitting: the ROOT (`input`/`ctx`/`output`/`payload`) → `support.variable.context.lazuli`; the LEAF chain → `variable.other.member.lazuli`.
**Acceptance:** `ctx.tenant.id` shows `ctx` in distinct color, `tenant.id` as members.

### 2.5 Resource model paths (`User.email`, `Item.tags`) ☐
**Problem:** `User.email` matches references but theme treats it identically to lowercase `ctx.X`.
**Fix:** add specific pattern for capitalized-root path → `support.type.lazuli` (root) + `variable.other.member.lazuli` (leaf).

---

## Phase 3 — full block-scoping audit

For each named-block kind, verify the begin/end captures + inner pattern set is **complete** (every keyword the LSP recognizes inside it gets highlighted) and **isolated** (sibling blocks don't bleed scopes into each other). The end-pattern indent-backref fix (`^(?!\1\s)(?=\s*\S)`) is in place; this audit confirms each block's PATTERNS list covers everything.

For each:
- Compare against `crates/lazuli_lsp/src/lib.rs` per-context catalog
- Open a real example (Pleiades feature OR full-capsule) and eyeball every token
- List missing keywords; add them; repackage

| Block | Keywords to verify | Status |
|---|---|---|
| `feature X` | uses, purpose, context, defaults, non_goals, delegated_to, domain, policies, auth, errors, extensions, tests, cache | ☐ |
| `app X` | uses, targets, environments, urls, env, integrations, capabilities, architecture, services, communication, runtime, deploy, encryption, headers, cookie, proxy, limits, locale, observability, tracing, logging | ◐ |
| `workspace X` | apps, shared_registry, boundaries, gateway | ☐ |
| `registry` | env, integrations, capabilities, packs, tools, webhook_events, secret_rotation | ☐ |
| `profile X` | urls, bindings, integrations, deploy | ☐ |
| `contract X` | operations, errors, fields, events, records | ☐ |
| `experience X` | imports, view (declarations) | ☐ |
| `surface X <platform>` | uses experience, audience | ☐ |
| `route X` (top-level) | path, to, surface, audience, lazy, prerender, route (sub-slot) | ☐ |
| `resource X` | fields (typed), constraints, derived, has_many, validators, validate, validates, unique, index, on_delete, retention, soft_delete, timestamps, no_timestamps, tenancy, lock, composite_key, slug, full_text, previously, migrated, alias | ☐ |
| `record X` | fields (typed), composite | ☐ |
| `enum X` | bare identifiers only — NO statement keywords | ✅ |
| `query.list X` | params, filters, order, paginate, cache, returns, search, over, mode, modifier, key, ttl, policy, rate_limit, audit | ☐ |
| `query.lookup X` | params, filters, by, returns, cache | ☐ |
| `query.sql X` | sql, returns, modifier | ☐ |
| `command X` | route, input, output, let, target, policy, policy_for, requires, integration, calls, audit, rate_limit, validate, deprecated, gate, idempotency, approval, handler, emit_to, write_window, expose, error/errors, creates, updates, deletes, returns, target, emits | ◐ |
| `api X` | method, path, route, output, policy, handler, rate_limit, deprecated, expose, gate | ◐ |
| `view X <type>` | source, columns, fields, sections, actions, filter, search, opens, submit, block, slot, anchor, item, title, badges, by, panel, cells, drawer, filters, sort, selection, bulk_actions, settings, action, extends, lazy, prerender, audience | ☐ |
| `event_group X` | patterns, includes | ☐ |
| `event.trace X` | conditions | ☐ |
| `events` (registry) | event_X declarations | ☐ |
| `webhook X` | path, payload, payload_from, verify, secret, header, tenant_from, idempotency, retry, backoff, replay, dlq, handler, emits, policy, rate_limit, audit, from | ✅ |
| `job X` | trigger, schedule, queue, fanout, idempotency, handler, retry, backoff, max_attempts | ☐ |
| `agent X` | model, prompt, tools, safety, output, output stream, output discriminator, evals, rate_limit, temperature, max_tokens, top_p, seed | ☐ |
| `notification X` | channels, template, digest, throttle, audience, every, group_by, max_size, template_strategy, max_per, per_recipient, per_channel, burst | ☐ |
| `poller X` | trigger, schedule, eligible_when, max_attempts, backoff, terminal_status_field, terminal_result_field, tick, resolve, fixed, linear, exponential | ☐ |
| `report X` | source, fields, filter, schedule | ☐ |
| `channel X` | transport, provider, template | ☐ |
| `aggregate X` | root, contains, invariants, invariant, when | ☐ |
| `tenant_migration X` | handler, axes, lock | ☐ |
| `cache X` (feature-level) | key, ttl, namespace, tags, stale_while_revalidate, coalesce, sliding | ☐ |
| `secret_rotation X` | cadence, overlap, auto_rollback | ✅ |
| `permission X` | grants, grants_all, inherits | ☐ |
| `role X` | inherits, grants, has_permission, has_role | ☐ |
| `plan X` | trial, features, limits, gate, behind, quota, then, unlimited, subscription | ☐ |
| `extends @anchor.X` | slot, before, after | ☐ |
| `auth` block | identity, password, oauth, mfa, sessions, hash, algorithm, rate_limit, ttl, refresh, adapter, credentials, enroll | ☐ |
| `policies` block | (policy name): @scope.X mappings + read/write/create/update/delete fields | ☐ |
| `errors` block | error X status N as Y | ☐ |
| `audit` block | emit_to, fields, level | ☐ |
| `expose` block | method, path | ☐ |
| `headers` block | csp, hsts, x_frame_options, x_content_type_options, referrer_policy, permissions_policy + nested hsts (max_age, include_subdomains, preload) | ✅ |
| `cookie` block | profile names + signed, secure, http_only, same_site, max_age | ☐ |
| `proxy` block | trusted, real_ip_header, forwarded_proto_header, forwarded_host_header | ☐ |
| `limits` block | body_size, header_size, upload_size, timeout | ☐ |
| `encryption` block | key @key.X + source, algorithm, rotation, rotation_profile | ☐ |
| `locale` block | default, supported, fallback (with `->`), locale_negotiate (source/strategy) | ☐ |
| `observability` / `logging` / `tracing` block | level, format, redact, sample_rate, propagate, exporter | ☐ |
| `services` block | service blocks with owns, exposes, publishes, consumes, internal, async, sync, propagate, timeout | ☐ |
| `communication` block | internal sync, external http, async event_bus, propagate | ☐ |
| `runtime` block | unit X with serves, runs, healthcheck, readiness | ☐ |
| `deploy` block | migrations, migration_lock, destructive_migrations, rollback, topology, environment, environments | ☐ |
| `urls` block | api/web/mobile env "URL" | ☐ |
| `env` block | groups + var typed declarations (server, client, build) | ☐ |
| `integrations` block | name: KIND with adapter, environments, credentials, data_classification | ☐ |
| `capabilities` block | name: KIND mappings | ☐ |
| `packs` block | pack names + provides | ☐ |
| `bindings` block | name: source/value | ☐ |
| `defaults` block | tenancy, audit_default, soft_delete | ☐ |
| `non_goals` block | out_of_scope, delegated_to | ☐ |

---

## Phase 4 — `.lzx` (experience + surface) coverage

### 4.1 `experience X` declarations ☐
**Vocabulary:** `imports`, `view <name>`, `source`, `cells`, `block`, `slot`, `extends`, `lazy`, `prerender`, `defaults`, `audience`
**Status:** partial — agent's revisit covered some of this. Verify against canonical example `full-capsule.lzx`.

### 4.2 `surface X <platform>` declarations ☐
**Vocabulary:** `uses experience`, `audience <name>`, `view <name> <Component>`
**Status:** partial.

### 4.3 View-body keywords (L0 #6 grammar) ◐
**Vocabulary:** `cells`, `drawer`, `filters`, `search segmented`, `sort` (with `by`/`default`), `selection` (with `multi`/`single`), `bulk_actions`, `settings` (with `persist local|server`), `actions`, `columns`, `fields`, `sections`, `submit`, `route`, `at`
**Status:** L0 #6 is in the parser but the LSP file-local diagnostics don't fully cover it; grammar should still try. 
**Note:** Pleiades currently uses simplified view forms because of the LSP gap; richer forms will land later.

### 4.4 Component types (UI primitive catalog) ◐
**Catalog:** `Form`, `AuthForm`, `Mutation`, `Transition`, `Table`, `List`, `CardList`, `Screen`, `SidePanel`, `Sheet`, `Terminal`, `Drawer`, `Dashboard`, `Wizard`, `Modal`, `Stepper`
**Status:** in `support.type.ui.lazuli` — verify list completeness against runtime/lazuli/views catalog.

### 4.5 Audience policy mappings ☐
Inside `audience X`, lines like `requires @scope.X` (legacy, may come back) — currently rejected by parser. If/when re-added, grammar needs to handle.

---

## Phase 5 — `Lazurite.toml` (manifest grammar) ◐

### 5.1 Reuse standard TOML grammar ✅
**Status:** done. `lazurite-manifest.tmLanguage.json` includes `source.toml`.

### 5.2 Lazurite-specific keys ☐
**Polish:** highlight known top-level tables (`[lazuli]`, `[lazurite]`, `[plugins]`, `[generate.go]`, `[generate.ts]`, `[frontends.X]`, `[migrations]`, `[seeds]`) with a distinct injection scope so they pop vs arbitrary TOML tables.
**Acceptance:** `[lazuli]` reads as a known table; arbitrary `[my-stuff]` reads as plain TOML.

---

## Phase 6 — embedded language injections (advanced)

### 6.1 SQL embedding inside `query.sql X` body ☐
**Approach:** TextMate `injection` rule — embed `source.sql` inside the `sql` block content.
**ROI:** moderate — power-user feature; useful for long SQL queries. ~30min work.

### 6.2 Cron embedding inside `trigger schedule "..."` ⊘ deferred (1.5)

### 6.3 Markdown inside `prompt "..."` and `description "..."` (post-Phase-1.5 vocabulary) ☐
If the comments-are-vocabulary-smell proposal lands and we get `description "..."` slots that often hold markdown, embed `text.html.markdown` for them. Requires the vocabulary first.

### 6.4 Regex inside `pattern "..."` validators ☐
For `validate pattern "..."` — embed `source.regexp` — niche but nice.

---

## Phase 7 — non-grammar polish

### 7.1 Code folding ☐
**Add:** `language-configuration.json` `folding.markers` for `^.*\b(feature|app|workspace|registry|profile|contract|experience|route|resource|record|enum|query\.|command|api|view|event|job|webhook|agent|notification|poller|report|channel|cache|secret_rotation|aggregate|permission|role|plan)\b` start markers + indent-based close.
**Acceptance:** click the gutter triangle next to `feature item` to fold the whole feature.

### 7.2 Bracket pair colorization ✅ (VS Code default)

### 7.3 Indent guides ✅ (VS Code default)

### 7.4 Snippets for common patterns ☐
**Targets:** `feature`, `command`, `query.list`, `api`, `resource`, `enum`, `policies`, `webhook`, `job`, `agent`, `notification`, `route`. Each snippet expands a skeleton ready to fill. Lives in `editors/vscode/snippets/lazuli.code-snippets`.

### 7.5 Auto-pairs / surrounding pairs ✅ (already configured)

---

## Phase 8 — theme compatibility audit

### 8.1 Test against major themes ☐
Open Pleiades `item.lzi` in each:
- Default Dark+ / Default Light+
- Dark Modern / Light Modern  
- Dracula
- Material Theme (Darker / Palenight / Default)
- One Dark Pro
- GitHub Theme (Dark / Light)
- Monokai
- Solarized

For each, screenshot a 30-line slice with mixed token types. Note any token that disappears or overlaps badly. File issues per theme.

### 8.2 Document scope-name → semantic role mapping ☐
**Output:** `editors/vscode/SCOPES.md` listing every TextMate scope used by the grammar + which role it represents + suggested theme color guidance.

---

## Phase 9 — regression test fixture

### 9.1 Snapshot test fixture ☐
**Approach:** create a `.lzi` file that exercises EVERY keyword/scope from this checklist. Run a tokenizer (vsce / vscode-tmgrammar-test) and snapshot the scope assignments. Future grammar changes diff against the snapshot.
**Acceptance:** `npm test` in `editors/vscode/` validates the snapshot.

---

## Multi-agent dispatch hints

When delegating a Phase to a subagent, include in the prompt:

1. **Source of truth**: `crates/lazuli_lsp/src/lib.rs` (`keyword_description` + `KEYWORDS`)
2. **Real-world test files**: 
   - `c:/Users/lucas/lazuli/examples/full-capsule/full-capsule.lzi`
   - `c:/Users/lucas/dev/pleiades/app/features/item/item.lzi`
   - `c:/Users/lucas/dev/pleiades/app/features/account/account.lzi`
3. **Grammar file**: `editors/vscode/syntaxes/lazuli.tmLanguage.json` (~2273 lines)
4. **Scope name discipline**: stay within `keyword.control.declaration.structural.lazuli` / `keyword.control.section.lazuli` / `keyword.control.statement.lazuli` for keyword tokens. Use `support.type.lazuli` family for types. Use `entity.name.tag.lazuli` for `@decorators` (Phase 1.1 target).
5. **After every change**: validate JSON, repackage `.vsix`, reinstall via `code.cmd --install-extension`. Provide eyeball-test steps in the report.

Independent agent slices that won't conflict:
- Agent A: Phase 1.1 + 1.2 + 1.3 (decorator + HTTP method + closed-catalog values)
- Agent B: Phase 3 first half (feature/app/workspace/registry/contract/profile blocks)
- Agent C: Phase 3 second half (resource/record/query/command/api/view blocks)
- Agent D: Phase 4 + 5.2 (.lzx + Lazurite.toml polish)
- Agent E: Phase 6 (embedded languages — SQL/markdown)
- Agent F: Phase 7.1 + 7.4 (folding + snippets)
- Agent G: Phase 8 + 9 (theme audit + snapshot tests)

Dispatch Phase 1 first; iterate before Phase 2+ to avoid merge conflicts on the grammar JSON.
