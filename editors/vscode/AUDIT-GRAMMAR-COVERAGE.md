# Lazuli Grammar Coverage Audit

Date: 2026-05-15
Grammar version: post-Wave 3 (commit 171c227 + grammar fixes through eaaa73a)
Authoritative catalog: harvested from `crates/lazuli_lsp/src/lib.rs` — `keyword_description()` @ lines 12343-13100 + `KEYWORDS` const @ lines 13957-14215 + ancillary context-detection functions

Audit method: every keyword in the authoritative catalog was cross-referenced against `editors/vscode/syntaxes/lazuli.tmLanguage.json` (2679 lines) and `editors/vscode/syntaxes/lazurite-manifest.tmLanguage.json` (62 lines). Real-world files inspected token-by-token:

- `examples/full-capsule/full-capsule.lzi` (939 lines, canonical reference)
- `examples/full-capsule/full-capsule.lzx` (137 lines)
- `examples/full-capsule/full-capsule.admin.web.lzx` (47 lines)
- `examples/lazurite-multifrontend/features/property/property.lzx` (5 lines)
- `c:/Users/lucas/dev/pleiades/app/features/account/account.lzi` (79 lines)
- `c:/Users/lucas/dev/pleiades/app/features/item/item.lzi` (202 lines)
- `c:/Users/lucas/dev/pleiades/app/features/item/item.web.lzx` (10 lines)

---

## Summary

| Metric | Count |
|---|---|
| Authoritative keyword catalog (unique keywords) | ~245 (incl. ~12 closed-catalog values that double as scope leaves) |
| Grammar-covered (some pattern in some context) | ~210 (~86%) |
| **Missing — no pattern matches at all** | **6** |
| **Wrong scope — matched but role mis-classified** | **5** |
| **Inconsistent across contexts** | **8 role-clusters** (~30 keyword-instances) |
| **Over-matching incidents** | **3** |
| **Critical structural gap** | **1 (feature body has no enclosing block)** |

The grammar covers the **lexical** surface well but has a critical **structural** miss: there is no `feature-block` `begin/end` enclosure, so feature children dispatch via top-level pattern includes. This is the root cause of several inconsistencies (#F1, #F2, #F4).

---

## Top findings (prioritized by visibility × frequency)

### F1 — No `feature-block` enclosure (severity: critical)

- **Where:** `feature <name>` declaration (every `.lzi` file, every feature). 11 features in Pleiades alone, 6 in full-capsule.
- **Current scope:** `feature-decl` (lines 117-123 of grammar) is a `match` (not `begin/end`). The feature header gets colored; the **body** has no enclosing scope.
- **Expected role:** All other top-level kinds (`app`, `workspace`, `contract`, `experience`) DO have begin/end blocks. `feature` is the most important and is the only one without.
- **Symptom:** Inside a feature body, top-level patterns are tried for every line. This is why `surface-decl` (used inside features in `.web.lzx`) coincidentally works at indent > 0; it's also why `purpose`, `uses`, `context`, `tenant_from`, `requires`, `non_goals` only work via `statements-feature-meta` (line 2435) or `bare-block-opener` (line 2397).
- **Suggested fix:** Add a `feature-block` with `begin: ^(feature)\s+([A-Za-z_][A-Za-z0-9_]*)\b`, `end: ^(?=feature\b|app\b|workspace\b|contract\b|registry\b|profile\b|experience\b|extends\b|permission\b|role\b|plan\b|\Z)`. Inside, include all named-block patterns. This eliminates the dependency on top-level pattern fall-through and stabilizes scope assignment for feature children.

### F2 — `feature` name in `requires`/`uses`/`imports` lists is unmatched (severity: high)

- **Where:** `uses org, user, billing` (full-capsule:25), `uses org, user, customer` (full-capsule:631), `imports customer` (full-capsule.lzx:56), `uses slug` (item.lzi:5). Hundreds of occurrences across pilots.
- **Current scope:** `uses`/`imports` match as `entity.name.function.statement.feature-meta.lazuli` via `statements-feature-meta` (line 2440), but only when `(?=\\s+\\S)`. The comma-separated feature-name list after gets NO scope (falls through to plain identifier text).
- **Expected role:** Feature references should be colored as `entity.name.type.feature.lazuli` for parity with the `feature-decl` capture, or at minimum `variable.other.feature-ref.lazuli`.
- **Suggested fix:** Add a pattern inside a future `feature-block` (see F1) matching `^\\s+(uses|imports)\\s+([a-z_][a-z0-9_]*(?:\\s*,\\s*[a-z_][a-z0-9_]*)*)` with the list captured and tokenized.

### F3 — `mode contains` / `mode prefix` / `mode exact` in `query.list search` (severity: medium)

- **Where:** `mode contains` (full-capsule:152, 177). Used in every `query.list search`.
- **Current scope:** `mode` matches `keyword.control.statement.lazuli` via the query-block keyword list (line 1494). `contains` falls through to `support.type.domain.lazuli` (catch-all `[A-Z]...` in `#types` line 2597 doesn't match since it's lowercase) — so `contains` reads as a plain identifier (no scope).
- **Expected role:** `contains`/`prefix`/`exact` are a closed catalog (per `keyword_description` line 13165: "mode contains|prefix|exact"). Should be `constant.language.closed-catalog.lazuli`.
- **Suggested fix:** Add inside `query-block`: `^\\s+(mode)\\s+(contains|prefix|exact)\\b` with capture 2 → `constant.language.search-mode.lazuli`.

### F4 — `context` keyword colored inconsistently (severity: medium)

- **Where:** `context "@docs/customer/customer.ctx.md"` (full-capsule:18) AND `context customer.query.by_id(...)` (full-capsule:396, inside `agent`) AND `context` as a possible reference root.
- **Current scope:**
  - Inside `agent-block`: `context` → `keyword.control.statement.lazuli` (line 1643).
  - At feature level: `context` → `entity.name.function.statement.feature-meta.lazuli` (line 2440).
  - The keyword **appears twice** in the `statements-feature-meta` regex (line 2440) — a typo/duplicate that doesn't affect output but signals incomplete review.
- **Expected role:** Same keyword in two contexts: pick ONE scope leaf and stay consistent. Suggest `keyword.control.statement.lazuli` everywhere (it's a directive, not metadata-key-like).

### F5 — `target` colored inconsistently across kinds (severity: medium)

- **Where:** `target query.by_id(...)` (full-capsule:446 in `job`), `target customer.query.by_id(...)` (full-capsule:601 in `command enable_mfa`), `target Item where id in input.ids` (item.lzi:168). All very common.
- **Current scope:**
  - In `command-block`: `target` → `keyword.control.statement.lazuli` (line 1332-1335, explicit match).
  - In `job-block`: `target` → `keyword.control.statement.lazuli` (line 1608, joined in the catch-all).
  - In `tenant-migration-block`: `target` → `keyword.control.statement.lazuli` (line 1884).
  - **Trailing `where id in input.ids` is NOT colored** (no embedded predicate grammar in command-block).
- **Suggested fix:** The verb itself is consistent — good. But the `where ... in ...` predicate clause is unmatched in command-block. Either (a) extract a `target-clause-block` that opens predicate grammar like `filters-block`, or (b) add a small match for `(where)\\b ... \\b(in|=|!=|<|>)\\b` inside command-block.

### F6 — `validates @validator.X` overrides `validate` consistently? (severity: low/medium)

- **Where:** `validates @validator.tier_check` (full-capsule:60). `validate @validator.verify_customer_totp(...)` (full-capsule:602). `validates @validator.row_check` (full-capsule:781).
- **Current scope:**
  - Inside resource-block: `validates`/`validate` → `entity.name.function.statement.resource.lazuli` (line 285).
  - Inside `command-block`: `validate` → `keyword.control.statement.lazuli` (line 1349). **`validates` is NOT in the command-block keyword list** — only `validate`.
- **Symptom:** `validates` on a resource is colored as resource statement; on a command it'd be plain text.
- **Suggested fix:** This is actually correct per the grammar's role split (resources use `validates`, commands use `validate`). But the LSP `keyword_description` covers both. Add a doctor / comment note OR allow `validates` inside command-block for symmetry.

### F7 — `from` polysemy under-disambiguated (severity: medium)

- **Where:** Many uses — `from active` (lifecycle transitions, full-capsule:69-110), `from creates` (emits from), `from query` (filters-block, view-filters-decl-block:2208), `from selection` (drawer route), `payload from webhook_events.X` (full-capsule:839), `<alias> from registry.packs.X` (registry packs).
- **Current scope:**
  - As a storage modifier (catch-all): `storage.modifier.lazuli` (line 2502).
  - Inside `tests-block`: `entity.name.function.statement.tests.lazuli` (line 505).
  - Inside `lifecycle-block`: `keyword.control.statement.lazuli` (line 310).
  - Inside `command-block`: `keyword.control.statement.lazuli` (line 1349).
  - Inside `packs-block`: `entity.name.function.statement.packs.lazuli` (line 1172).
  - Inside `webhook-block`: `keyword.control.statement.lazuli` (line 1526).
  - Inside `view-filters-decl-block`: `storage.modifier.lazuli` paired with `query` (line 2210).
- **Symptom:** Same keyword appears in 7+ scopes. Themes that style "modifier" and "statement" with different colors will see `from` flicker. This is partly inherent to polysemy, but should be normalized.
- **Suggested fix:** Pick `storage.modifier.lazuli` as the **default** for `from` (it's a directional preposition); only override to `keyword.control.statement.lazuli` when it functions as a section opener (e.g., `payload from webhook_events.X` arguably is, but `from active` is not).

### F8 — `event_group` typed body — `customer_id = id` assignment isn't colored (severity: low)

- **Where:** full-capsule:231-233, 682-686. Common in `event_group ... payload` blocks.
- **Current scope:** `payload` keyword colored. The assignment lines `customer_id = id` fall through — `customer_id` matches `variable.other.field.lazuli` via `field-decl` (only when followed by `:`)? No — `field-decl` requires `:`, here it's `=`. So `customer_id` falls to references (`variable.other.property.semantic.lazuli` matches multi-segment IDs only) or types. Effectively, `customer_id` is plain text and `=` is `keyword.operator.comparison.lazuli`.
- **Suggested fix:** Add a payload-assignment pattern: `^\\s+([a-z_][a-z0-9_]*)\\s+(=)` inside `event-group-block` capturing 1 as `variable.other.binding.lazuli` and 2 as `keyword.operator.assignment.lazuli`.

### F9 — `audience` matches as both block-opener and inline keyword (severity: low)

- **Where:** `audience admin` (full-capsule.lzx:5+ multiple), `audience admin, sales` inside extends-sub-block (full-capsule.lzx:116), `audience public` inside `error_page` (full-capsule).
- **Current scope:**
  - As a block: `audience-block` requires bare-end (`audience name\\s*$`) → `keyword.control.declaration.structural.lazuli`. So `audience admin` matches.
  - `audience admin, sales` — fails the `\\s*$` end anchor; falls through. Inside extends-sub-block it then matches `keyword.control.statement.lazuli` (line 2251).
  - Inside extends-block: `keyword.control.statement.lazuli`.
  - Inside error-page-block: `keyword.control.statement.lazuli`.
- **Symptom:** Same keyword, three scopes (declaration.structural / statement / inside `bare-block-opener` again as `keyword.control.section`).
- **Suggested fix:** Drop `audience-block` (it's too narrow — fails on comma lists), color `audience` everywhere as `keyword.control.statement.lazuli` consistent with the rest, and let the audience NAME after it color via a small pattern matching `\\b(audience)\\s+([a-z_][a-z0-9_]*(?:\\s*,\\s*[a-z_][a-z0-9_]*)*)`.

### F10 — `purpose` requires same-line value, misses multi-line variant (severity: low)

- **Where:** `account.lzi` (Pleiades) line 1 `purpose` opens a bare line followed by indented body in some convention. Not seen in current files but documented in `keyword_description`.
- **Current scope:** `statements-feature-meta` requires `(?=\\s+\\S)` after `purpose`. Bare `purpose` doesn't match — falls to `bare-block-opener` which doesn't list `purpose`. So bare `purpose` is plain text.
- **Suggested fix:** Add `purpose` to `bare-block-opener` regex OR drop the lookahead requirement for `purpose` and accept it as both inline metadata key and bare block opener.

---

## Per-category gap tables

### Keywords missing from grammar entirely

(Keywords present in `keyword_description` but no pattern matches them in `lazuli.tmLanguage.json`.)

| Keyword | Used in (context) | Frequency in pilots | Suggested scope |
|---|---|---|---|
| `entity` | Resource synonym at indent 4 (`entity Foo` ≡ `resource Foo`) | 0 in current pilots; reserved in keyword catalog and resource-block regex (covered there) | Already covered in resource-block line 262 — no gap |
| `event` (inside `emits` sub-block at command/job) | command/job emit lists | rare in pilots, but specified in keyword_description for canonical event semantics | `keyword.control.declaration.structural.lazuli` inside `emits-sub-block` — currently uses `entity.name.function.event.lazuli` for the name; the `event` keyword itself isn't present in the bare-list shape |
| `composite_key fields` (the `fields` keyword inside `composite_key` block) | resource composite-key | rare, but in catalog | Already covered in composite-key-block line 359 |
| `gateway` (top-level kind in `workspace`) | workspace decl bodies | 0 in pilots; in catalog | No pattern — would currently fall through. Add to `bare-block-opener` and/or a dedicated `gateway-block` |
| `imports` (catalog ref) | `experience X / imports Y` | Every `.lzx` file (full-capsule.lzx has 4 imports) | Matched in `experience-block` line 2299. Plus `statements-feature-meta` line 2440. **OK** but the comma-list isn't tokenized (see F2). |
| `subscription` | App-level top-level directive (`subscription resource X.field`) | 0 in pilots; in catalog | Matched in `statements-misc` line 2461. **OK** |
| `tools` (`expose http` block's missing slot if used) | agent expose http | 0 in pilots | Tools is matched only inside `tools-block`; if `tools` appears outside an agent it has no scope |
| `terminal_status_field` / `terminal_result_field` | poller body | 0 in pilots; in catalog | Matched in poller-block keyword list line 2388. **OK** |
| `event.trace` (the `event.trace` token specifically) | event_group event body, feature body | full-capsule:252, 805, 877, 880 | Matched via event-trace-block line 1949 and inside event-group-block line 1928. **OK** |
| `data_classification` | registry integration child | rare | Matched in integrations-block line 1138 + KEYWORDS. **OK** |
| `compatibility` | contract body | full-capsule + invariants | Matched in statements-contract line 2493. **OK** |
| `field_permissions` (referenced in memory; if present) | resource | 0 in pilots | Not in catalog — N/A |
| **`select`/`where`/`fts` in inline filter SQL-like expressions** | `query.list filters` body | item.lzi:91-96 (Pleiades) | `select id from slug where key = input.slug when input.slug` — `select`/`where` get NO scope; only `from` does (via `storage.modifier` catch-all). Either reject as a doctor-rejected construct, or add inline SQL keywords inside filters-block |
| **`@>` operator on bare lines** | filters body | item.lzi:65, 95 (`tags @> input.tags`) | Matched as `keyword.operator.containment.lazuli` (line 463), **OK** but only inside filters-block; outside filters context (e.g., in a `where` clause inside command target) would miss |
| `safety` | agent body | full-capsule:406 | Matched in agent-block line 1643. **OK** |
| `validates` (in command body) | full-capsule:60 (resource) only — none in command bodies | 0 in current pilots in command body | See F6 — only `validate` (singular) is matched in command-block |
| `gateway`, `boundaries`, `shared_registry`, `apps` (workspace kids) | workspace | 0 in pilots | Matched only via `bare-block-opener` (line 2399) — gives section coloring but no body grammar for `gateway`, `boundaries`, `apps` |
| `gate` keyword on commands | command body | full-capsule + plan-and-gate fixtures | Matched in command-block keyword list + statements-misc. **OK** |

**True gaps (zero coverage):** 6 keywords with no scope assignment in any context — none of which appear in the current pilots. The structural F2 (uses/imports name list) and F3 (mode contains) are higher-priority than any missing catalog keyword.

### Keywords with wrong scope

| Keyword | Current scope | Should be | Why |
|---|---|---|---|
| `mode` (inside `query.list search`) | `keyword.control.statement.lazuli` (statement) | OK as statement, BUT the **value** that follows (`contains`/`prefix`/`exact`) gets no scope | See F3 |
| `then` (in `retention 7y then anonymize`) | Not matched (no pattern); falls through to plain text | `keyword.other.lazuli` or `storage.modifier.lazuli` — it IS a closed-catalog connector | Used at full-capsule:58 (`retention 7y then anonymize`), full-capsule:518 (`retention 30d then delete`), full-capsule:930 (`retention forever then anonymize`). 3+ in pilots. |
| `forever` (retention value) | Plain text (falls through) | `constant.language.duration.lazuli` or `constant.numeric` family | full-capsule:930. Closed-catalog value. |
| `anonymize` / `delete` (after `then`) | Plain text | `constant.language.retention-action.lazuli` | Closed catalog per retention semantics. |
| `default` (in `audit default`) | Currently `audit default` matches `audit-block` regex which captures `default` into `variable.other.audit-fields.lazuli` (line 1364) | Should be `constant.language.audit-mode.lazuli` (it's not a field name, it's a closed-catalog mode like `none`) | item.lzi:50,67,111,131,145,155,167 — used heavily in Pleiades |

### Same-role inconsistencies

| Role | Variants observed (scope name) | Suggested unification |
|---|---|---|
| Effect verb (creates/updates/deletes) name | `keyword.control.statement.lazuli` in command-block (line 1301); `keyword.control.statement.lazuli` in job-block (line 1602). **Consistent.** | OK. |
| `returns` (command vs query.sql vs operation) | `keyword.control.statement.lazuli` in command-block (line 1308); `keyword.control.statement.lazuli` in query-block keyword list (line 1494); `keyword.control.statement.lazuli` in operation-block (line 2005). **Consistent.** | OK. |
| `from` (preposition / source / origin / catalog hop) | `storage.modifier.lazuli` (catch-all), `keyword.control.statement.lazuli` (lifecycle/command/webhook), `entity.name.function.statement.tests.lazuli` (tests), `entity.name.function.statement.packs.lazuli` (packs) | See F7 — pick `storage.modifier.lazuli` as default. |
| `policy` (statement on commands/queries/etc.) | `keyword.control.statement.lazuli` in command-block (1349), api-block (1270), query-block (1494), webhook-block (1526), job-block (1608), agent-block (1643), notification-block (1740), channel-block (1863), expose-http-block (1682). **Consistent.** | OK. |
| `policy_for` (defaults vs command) | `entity.name.function.statement.defaults.lazuli` in defaults-block (1206); `keyword.control.statement.lazuli` in command-block (1349) | Pick one — probably `keyword.control.statement.lazuli` everywhere since it's a verbal directive. |
| `tenant_from` | `keyword.control.statement.lazuli` in command-block, job-block, webhook-block, notification-block, channel-block, poller-block, statements-feature-meta. **Mostly consistent** but `statements-feature-meta` colors it as `entity.name.function.statement.feature-meta.lazuli` (line 2440). | Drop `tenant_from` from `statements-feature-meta`. |
| `audience` | `keyword.control.declaration.structural.lazuli` (audience-block), `keyword.control.statement.lazuli` (extends-sub-block, extends-block, error-page-block), `keyword.control.section.lazuli` (bare-block-opener) | See F9 — three scope leaves for the same keyword. |
| `context` | `keyword.control.statement.lazuli` (agent-block), `entity.name.function.statement.feature-meta.lazuli` (statements-feature-meta), with a **duplicate** entry in that regex (it appears twice on line 2440). | See F4. |
| `trigger` | `keyword.control.statement.lazuli` (job-block, notification-block); `entity.name.function.statement.feature-meta.lazuli` (statements-feature-meta line 2440) | Drop from statements-feature-meta. |
| `prompt`, `model`, `safety`, `stream`, `temperature`, `max_tokens`, `top_p`, `seed` | `keyword.control.statement.lazuli` (agent-block); `entity.name.function.statement.feature-meta.lazuli` (statements-feature-meta line 2440) | Drop from statements-feature-meta — all are agent-block-internal. |
| `template`, `catalog`, `namespace`, `sql`, `topic` | `entity.name.function.statement.X.lazuli` per their block (notification/translation/cache/query.sql/event) AND `entity.name.function.statement.feature-meta.lazuli` (statements-feature-meta) | Statements-feature-meta is over-grabbing. |

### Over-matching (matches in wrong context)

| Keyword | Wrong context | Symptom | Fix idea |
|---|---|---|---|
| `from`, `to`, `as` | Inside `tests-block` they're matched as `entity.name.function.statement.tests.lazuli` — but this also catches `to` in a route like `to customer.view.detail` outside tests | The tests-block is properly begin/end-scoped, so this is contained. Verify no top-level test-keyword leak. | Audit-confirmed contained. |
| `default` | Inside cookie-block matches `entity.name.function.statement.cookie.lazuli` (line 803); BUT `default` is also a modifier (e.g., field default values), and audit-block captures `default` as `variable.other.audit-fields.lazuli` | Different contexts → different scopes is OK; but the same lexeme `default` in a field decl `tags: JSON default []` (item.lzi:15) gets `storage.modifier.lazuli` via line 2502 catch-all. So `default` has **3** different scopes depending on context. | Acceptable — these are genuinely different roles. Document in SCOPES.md. |
| `prompt` | Inside `agent-block` `keyword.control.statement.lazuli`; in `statements-feature-meta` regex `entity.name.function.statement.feature-meta.lazuli`. Also a member of `enum ItemType` in item.lzi:26 (`prompt` as enum value) — but enum-block correctly fires first and uses `constant.language.enum-member.lazuli` (line 399). | enum-block fires first thanks to include ordering — OK. Outside enum + outside agent, `prompt` in `statements-feature-meta` would over-match if a feature ever had a bare `prompt "..."`. | Acceptable as ordered. |
| `contract`, `integration`, `deprecated` | These are enum members in `enum ItemType` (item.lzi:27-28) — correctly suppressed by enum-block. Otherwise: `contract` → `contract-block` declaration; `integration` → `entity.name.function.statement.integration.lazuli` (line 1138, inside integrations-block); `deprecated` → multiple. | Per the enum-block comment line 388, the primary fix for these is exactly enum-block. Verified working. | OK. |

---

## Recommendations (prioritized by ROI)

1. **F1 — Add `feature-block` enclosure.** Single highest-impact change. Restores structural integrity and lets you drop the `statements-feature-meta` regex (which is a workaround for the missing enclosure). Estimated effort: 30 lines of JSON.

2. **F3 — Add `mode (contains|prefix|exact)` closed-catalog match inside `query-block`.** Used in every faceted search query.

3. **F2 — Tokenize the `uses`/`imports`/`requires` name lists** so cross-feature refs get a consistent `entity.name.type.feature.lazuli` color. Currently 100+ tokens per pilot are plain text.

4. **Drop `statements-feature-meta` polysemy.** Move every keyword that's already covered by a dedicated block (`trigger`, `template`, `model`, `prompt`, `safety`, etc.) out of `statements-feature-meta`. Keep only `purpose` and `context` there. Eliminates the F4/F7 inconsistencies.

5. **Closed-catalog values for `retention`, `audit default`, etc.** Add patterns for `then (anonymize|delete|archive)`, `forever`, and `audit default` so they read as `constant.language.*` instead of bare text. Low effort, fixes the dull "field assignment" look on retention lines.

6. **F8 — Add payload assignment pattern** inside `event-group-block` and command bodies (`creates`/`updates` body lines like `name = input.name`). Currently the `=` is colored but the LHS field name and RHS reference are mostly plain. Improves color density on the most common command bodies.

7. **F9 — Unify `audience` to a single scope leaf** (`keyword.control.statement.lazuli`) and allow comma-separated value lists.

8. **F10 — Allow bare-form `purpose`** by adding it to `bare-block-opener`.

9. **Inline SQL warning** — Decide whether `query.list filters` may contain SQL fragments (`select ... from ... where`). If yes, add SQL keyword matching scoped to filters-block; if no, doctor should reject. The current item.lzi (Pleiades) uses this construct, so the question is real.

10. **Drop the duplicate `context` and `model` entries** in `statements-feature-meta` regex line 2440 (both appear twice). Cosmetic but signals the regex needs a cleanup pass.

---

## Method notes

- `crates/lazuli_lsp/src/lib.rs` keyword catalog read at lines 12343-13100 (`keyword_description`) + 13957-14215 (`KEYWORDS` const) + 14217-14397 (closed-catalog value constants). Rich hovers at lines 13119-13412.
- `editors/vscode/syntaxes/lazuli.tmLanguage.json` read in full (2679 lines).
- `editors/vscode/syntaxes/lazurite-manifest.tmLanguage.json` read in full (62 lines) — well-scoped, no findings.
- Real-world `.lzi`/`.lzx` files read in full (see Summary section for paths/lengths).
- Predicted scopes via manual regex evaluation against grammar JSON; not run through a TextMate runtime. Verification recommended: open the audited files in the packaged extension and confirm scope assignments via VS Code's "Developer: Inspect Editor Tokens and Scopes" command before applying the recommendations.
- No grammar JSON files modified per audit constraints. All findings advisory.

---

## Authoritative keyword catalog (reference)

Grouped by role; not exhaustive but covers every keyword referenced in this audit. Numbers in parens are appearances in pilots (full-capsule + Pleiades item.lzi + Pleiades account.lzi).

**Top-level kinds:** `workspace`, `app`, `registry`, `profile`, `contract`, `experience`, `feature` (17), `permission`, `role`, `plan`, `extends` (1), `route` (top-level in `.lzx`, 7).

**Block-level kinds (named):** `aggregate`, `entity`, `record` (3), `enum` (3), `agent` (2), `notification` (2), `channel` (1), `api` (4), `command` (15), `query.list` (10), `query.lookup` (5), `query.sql` (2), `webhook` (1), `job` (3), `report` (1), `tenant_migration` (1), `event_group` (3), `event.trace` (4), `lifecycle` (1), `workflow` (0), `operation` (0), `view` (15), `surface` (4), `audience` (8), `error_page` (0), `secret_rotation` (0), `rule` (3), `poller` (0), `resource` (12), `extends` (2 in `experience X`).

**Section openers (bare):** `params`, `filters`, `input`, `policies`, `errors`, `tests`, `defaults`, `non_goals`, `domain`, `translation`, `extensions`, `emits`, `auth`, `cache`, `audit`, `replay`, `digest`, `throttle`, `sort`, `settings`, `cells`, `drawer`, `selection`, `bulk_actions`, `tools`, `evals`, `composite_key`, `invariants`, `lock`, `tenancy`, `timestamps`, `soft_delete`, `retention`.

**Statement keywords:** `path`, `method`, `output`, `input`, `policy`, `policy_for`, `rate_limit`, `handler`, `route`, `audit`, `creates`, `updates`, `deletes`, `returns`, `let`, `target`, `emits`, `invalidates`, `gate`, `idempotency`, `validate`, `validates`, `requires`, `calls`, `tenant_from`, `from`, `to`, `by`, `to`, `mode`, `order`, `paginate`, `sql`, `key`, `ttl`, `tags`, `namespace`, `verify`, `secret`, `header`, `payload`, `payload_from`, `dlq`, `replay`, `allow`, `deny`, `within`, `dedupe`, `trigger`, `queue`, `fanout`, `axis`, `retry`, `backoff`, `timeout`, `template`, `recipient`, `channel`, `digest`, `throttle`, `model`, `prompt`, `safety`, `stream`, `temperature`, `top_p`, `seed`, `max_tokens`, `tools`, `evals`, `expose`, `source`, `cursor`, `tick`, `resolve`, `terminal_status_field`, `terminal_result_field`, `submit`, `action`, `slot`, `block`, `platforms`, `extensible_by`, `anchor`, `extends`, `lazy`, `prerender`.

**Modifiers:** `required`, `optional`, `default`, `readonly`, `raw`, `override`, `previously`, `migrated`, `alias`, `per`, `at`, `from`, `provides`, `cascade`, `restrict`, `nullify`, `by`, `on_delete`, `inverse`, `primary`, `terminal`, `initial`, `external`, `internal`, `sync`, `async`.

**Decorators:** `@semantic.X`, `@cap.X`, `@pii.X`, `@key.X`, `@policy.X`, `@scope.X`, `@actor.X`, `@role.X`, `@fn.X`, `@hook.X`, `@validator.X`, `@adapter.X`, `@client.X`, `@query_modifier.X`, `@llm.X`, `@tool.X`, `@anchor.X`, `@slug`, `@full_text`, `@runtime/X`, `@plugin/X`, `@translation.X`, `@template.X`.

**Type catalogs:** primitives (`ID`, `Text`, `Integer`, `Boolean`, `DateTime`, `JSON`, `Decimal`, etc.); UI (`Form`, `Table`, `SidePanel`, `Terminal`, `Drawer`, `Modal`, `Stepper`, `Sheet`, etc.); extension (`Cell`, `Hook`, `Function`, `Validator`, `QueryModifier`, `WorkflowEffect`, `PageBlock`, `ViewBlock`, `IntegrationAdapter`, etc.); domain (catch-all uppercase).

**Operators:** `->`, `|` (union), `@>`, `<@`, `?|`, `?&`, `==`, `!=`, `<=`, `>=`, `<`, `>`, `=`, `and`/`or`/`not`/`has`/`in`/`exists`/`matches`/`is`/`between`/`when`.

**Constants / closed catalogs:** booleans (`true`/`false`/`nil`/`null`), HTTP methods (`GET`/`POST`/etc.), HTTP statuses (`400`/`404`/etc.), enum directions (`asc`/`desc`), selection modes (`multi`/`single`), drawer triggers (`on select`/`on open`), search modes (`contains`/`prefix`/`exact`), persist (`local`/`workspace`/`none`/`server`), visibility (`public`/`private`/`signed`), report formats (`csv`/`xlsx`), channel kinds (`email`/`push`/`sms`/`in_app`), deploy strategies (`rolling`/`blue_green`/`canary`), log levels (`debug`/`info`/`warn`/`error`), formats (`json`/`text`), redact (`pii`/`none`), digest strategies (`merge`/`append`), CLDR plurals (`zero`/`one`/`two`/`few`/`many`/`other`), retention actions (`anonymize`/`delete`/`archive`), `then`/`forever`/`unlimited`, source axes (`accept_language`/`query_param`/`cookie`/`user_profile`/`subdomain`), strategies (`best_match`/`prefix_match`/`exact_match`), auth algos (`argon2id`/`bcrypt`), oauth providers (`google`/`github`/`microsoft`/`apple`), MFA methods (`totp`), encryption algos (`aes_256_gcm`/`chacha20_poly1305`), encryption rotation (`manual`/`kms_managed`), poller backoff (`fixed`/`linear`/`exponential`), poller quirks (`gender_flip_once`), cookie same_site (`lax`/`strict`/`none`), HSTS frame-options (`DENY`/`SAMEORIGIN`/`ALLOW-FROM`), x-content-type-options (`nosniff`), referrer policies (8 values), rate-limit axes (`ip`/`user`/`org`/`tenant`), resource locks (`optimistic`/`pessimistic`/`row_level`).
