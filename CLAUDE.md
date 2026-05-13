# Lazuli — Working Rules for AI Agents

This file is the canonical operating manual for any AI agent (Claude, GPT/Codex, future models) working in this repo. It distills the lessons from a real incident on 2026-05-12 where a GPT-orchestrated batch of 375 commits introduced ~93k LOC of bloat that took an entire session to audit and revert. **Read this before doing anything substantive.**

Mirrored verbatim at `AGENTS.md` for tooling that loads `AGENTS.md` (Codex, Aider, etc.).

---

## The founding principle (NEVER violate)

**Lazuli is abstraction; the Lazuli Go runtime is *wire*.**

The runtime in `runtime/go/lazuli/<bucket>/` **does not reimplement** primitives that already exist in Go stdlib / extended / mature SDKs. Each adapter / bucket helper is **~10-50 LOC of `import` + `call`**, not 200-800 LOC of homegrown logic.

**Concrete examples of what NOT to do** (all from the 2026-05-12 incident, all reverted):

- `http_mtls.go` 587 LOC, zero external imports → should have been ~30 LOC wrapping `crypto/tls.Config` + maybe `caddyserver/certmagic`.
- `http_circuit_breaker.go` 311 LOC, zero external imports → should have been ~20 LOC wrapping `sony/gobreaker`.
- `event_versioning.go` 421 LOC, zero external imports → questionable feature, but if needed should wire a real library.
- `db_shard_router.go` 412 LOC, zero external imports → reimplementing sharding from scratch is way premature.
- `db_unit_of_work.go` — Java/Hibernate-ism nobody asked for in Go.
- `views/markdown.go` 1066 LOC reimplementing markdown when `gomarkdown/markdown` exists.
- `testkit/coverage.go` 1112 LOC reimplementing what `go test -cover` already does.
- `rpc/grpc.go` 657 LOC reimplementing gRPC when `google.golang.org/grpc` is the de-facto standard.

**Test for your own work before committing:** open the file you just created, count external imports (`github.com/...`, `golang.org/x/...`, `gopkg.in/...`, `cloud.google...`). If LOC > 100 and external imports == 0 and the feature exists in any well-known Go library, **you are violating this principle**. Either rewrite as wire, or delete and use the library directly in the user's code.

The negative reference is **Aerocoding/Orion Studio** (the lead's prior attempt at the same problem) which became unmaintainable precisely because templates carried implementations rather than wiring a runtime. Lazuli exists to NOT repeat that.

See: `docs/architecture.md` lines 26-55 (founding principle + Aerocoding negative reference).

---

## Namespace policy (CHECK BEFORE EVERY NEW FILE)

Two namespaces, strict separation:

- **`@runtime/<name>`** — OSS commodity infrastructure. Postgres, Redis, S3-protocol signing, SMTP, Kafka, NATS, RabbitMQ, webpush (W3C). Lives in this repo at `runtime/go/lazuli/<bucket>/`. Public.

- **`@plugin/<name>`** — Proprietary or opinionated providers. **Vendor SaaS, paid APIs, or specific named tools/products** (even if open-source). Stripe, MercadoPago, Sendgrid, Mailgun, Twilio, Datadog, Sentry, LaunchDarkly, Algolia, Meilisearch, Discord, Slack, PagerDuty, Expo Push, Google Maps, Mapbox, FCM, MinIO client, Prometheus exporter, OpenFeature SDK, Atlas migrations, gh-ost-style migrators, etc. Lives in **separate (often private) repos** at `github.com/lazuli-lang/lazuli-plugin-<name>` (or under the user's own org for proprietary providers).

- **NEVER** `@plugin/<consumer-product>/<name>` (retired 2026-05-11). The adapter is named after the *provider*, not the consuming product. MercadoPago is `@plugin/mercadopago` (generic), not `@plugin/<app>/mercadopago` (product-scoped).

- **Plugins are multi-language by nature.** Most plugins have a Go server adapter (imported by `dist/go/main.go` via anonymous import + `init()` self-registration) plus optionally TS web (`web/`) and TS mobile (`mobile/`) sides for client-rendered widgets. Same repo, subdirs by target language — Stripe / Mapbox / MercadoPago all do this. Don't pre-create empty `web/`/`mobile/` dirs; ship each face when a product port needs it. **See `docs/plugin-authoring.md`** for the canonical repo shape + adapter patterns + scaffold pipeline.

**Before writing a new adapter file, ask: "is this commodity infrastructure (open spec or de-facto-OSS layer) or is it a specific named product/service?"** If it's a named product, **do not put it in `runtime/go/lazuli/`**. Either it belongs in a separate `@plugin/<name>` repo, OR the user should write it as a regular Go module in their app.

The 2026-05-12 incident shipped Stripe / MercadoPago / Sendgrid / Mailgun / Postmark / Resend / SES / Pagarme / PayPal / Pix / Google Maps / HERE / Mapbox / MapTiler / Nominatim / Datadog / Sentry / LaunchDarkly / GrowthBook / Unleash / Discord / Slack / Twilio / PagerDuty / Expo / Algolia / Meilisearch / Typesense / Opensearch — every single one of those was a namespace violation. ~25k LOC of vendor code in core. All extracted in Wave A/B/C cleanup. **Don't do this again.**

See: memory `project_plugin_namespace_policy.md`.

---

## Division of labor: Claude plans, Codex executes

Validated empirically across multiple parallel batches in 2026-05-11/12/13:

- **Claude (this profile)** — orchestrates. L0/L1 design, grading via `lazuli-language-architect`, cherry-pick, wire-up in shared files (`module.rs`, `mod.rs`, `types.rs`), commits, push. **Decisions**: scope, design, review, acceptance.

- **Codex** (via `codex exec`) — executes L2 mechanical codegen in parallel worktrees. Single-file output per cell. **Mechanical**: implement spec, ship tests, write a report.

**Anti-patterns** (all of these have caused real harm — do not repeat):

1. **Do NOT delegate orchestration to GPT/Codex.** Per the user explicitly on 2026-05-13: "ele é muito bom pra implementar, codificar em si, agora pra pensar, isso eu deixo com você". GPT orchestrating ~20 codex agents without supervision on 2026-05-12 produced 375 commits of bloat. Cost: ~35% of user's weekly Claude budget + a full session of audit/revert + a `git reset --hard` to a snapshot tag.

2. **Do NOT pedir design judgement do Codex.** The model is capable but produces results misaligned with Lazuli vocabulary, founding principles (wire-thin), and Rule Zero ("Vocabulary Over Mechanism"). Always have Claude grade Codex output before commit.

3. **Do NOT launch >5 Claude Agent tool calls in parallel** — wastes budget. The user is on a fixed weekly cap.

4. **Do NOT lance Codex agents to touch shared files** (`module.rs`, `mod.rs`, `types.rs`, `tests/emit_v1.rs`) — multiple agents writing to the same file cherry-pick-conflict catastrophically. Codex always writes ONE new isolated file or makes a small additive edit. The orchestrator (Claude) wires it up post-merge.

5. **Do NOT run `cargo test` from multiple Codex agents in parallel** — they fight over `target/`. Wire-up + cargo run happens in the orchestrator, post-merge.

See: memory `feedback_claude_plans_codex_executes.md`, `feedback_review_codex_batches.md`.

---

## Review-before-cherry-pick discipline

Before merging ANY Codex/agent batch:

1. **`git -C <worktree> diff --stat main..HEAD`** — eyeball the file paths. Scan for vendor SaaS names (sendgrid, stripe, datadog, fly, helm, terraform, launchdarkly, algolia, etc.). **Reject anything not commodity OSS infrastructure.**

2. **`git -C <worktree> log -1 --format='%s'`** — does the commit subject match the spec? If Codex went off-spec, the subject line drifts; that's a signal to read the diff carefully.

3. **`go -C runtime/go test ./lazuli/...`** in the orchestrator's main worktree after cherry-pick — fixtures must keep passing. Pre-existing doctor checks for `examples/full-capsule/`, `examples/auth-roundtrip/`, `examples/smoke-hello/`, `examples/marketplace-mini/`, `examples/auth-multi-tenant/`, `examples/binary-smoke/`, `examples/lazurite-multifrontend/` must remain green.

4. **`cargo check --all-targets`** — Rust crates must remain green.

5. **Tag snapshots** before any high-blast-radius surgery: `git tag -a <name>-pre-<wave>-YYYY-MM-DD -m "snapshot before X"`. Push the tag. Tags are cheap insurance; the 2026-05-13 reset to `runtime-pre-vendor-audit-2026-05-13` saved hours of recovery work.

See: memory `feedback_review_codex_batches.md`.

---

## Roadmap discipline

**`docs/roadmap.md` is bloated.** It derives from a "1400-feature framework audit" filtered to ~1140 items. Many of those items are **speculative**, not actual product needs.

When choosing what to build:

- **The current product-port checklist** (kept in the user's private workspace, not in this open-source repo) is the active source of truth for "what does the real product need?". Cross-reference against it before any new framework work.
- **Phase Prep** items only — finishing the runtime/codegen for the first downstream product port to start.
- **DO NOT** generate batches of "framework readiness" cells from the roadmap without checking that they (a) honor the wire principle, (b) are actually consumed by something, (c) aren't already covered by stdlib + an existing library.

If a roadmap item exists but the implementation would be 300+ LOC of stdlib-only code reimplementing a known library, the right move is **delete the roadmap item** or **rewrite it as wire-of-X** with the specific library named.

See: `docs/roadmap.md` (treat as advisory). The authoritative product-port checklist lives in the user's private workspace, not in this open-source repo.

---

## Grade-before-commit for proposals

Every design proposal (`docs/proposals/*.md`) goes through the **grade-then-fix loop** with `lazuli-language-architect` agent before commit. This is non-negotiable; the user specifically values this discipline.

Pattern:
1. Write the proposal draft.
2. Invoke `Agent` with `subagent_type: lazuli-language-architect`. Give the agent the proposal path + grading rubric context + explicit ask for blockers vs polish.
3. Apply ALL blocker-level fixes; track polish items as future cells.
4. Re-grade. Target ≥ 9.0; gate at ≥ 8.5 with no individual dimension < 7.
5. Then commit + push + kick implementation cells.

The 2026-05-13 Lazurite scaffold proposal hit BLOCK at 7.6/10 on v0.1 (two boundary leaks), reached PASS at 8.68/10 on v0.2, and PASS at 9.19/10 on v0.3. Every iteration was 30 minutes of work that prevented hours of downstream churn from leaks.

See: memory `feedback_grade_before_commit.md`.

---

## Memory discipline

The orchestrator has a persistent memory system at `~/.claude/projects/c--Users-lucas-lazuli/memory/` (Windows: `C:\Users\lucas\.claude\projects\c--Users-lucas-lazuli\memory\`). Other agents (Codex, etc.) do not share this memory.

When information should outlive the current session:
- **User profile facts** (role, expertise) → `user_*.md`
- **Feedback on how to work** (this user wants X, avoid Y, because reason Z) → `feedback_*.md`
- **Project state that's not in git** (ongoing initiatives, deadlines, decisions made out-of-band) → `project_*.md`
- **External system references** (Linear project IDs, Grafana URLs, Slack channels) → `reference_*.md`

**Always include `Why:` and `How to apply:`** in feedback/project memories. Reading "rule X" without rationale produces brittle rule-following; reading "rule X because incident Y; apply when Z" produces judgment.

Index entries in `MEMORY.md` are ≤ 1 line, ≤ 150 chars. Update the index whenever you write/remove a memory file.

---

## Folder conventions (current)

### Authored sources (commit these)

```
app.lzi                   # Top-level app declaration (envs, urls, uses)
registry.lzi              # Integrations + plugin bindings
profiles.lzi              # (optional) env-specific overlays
workspace.lzi             # (optional) distributed-system root

features/<feature>/
  <feature>.lzi           # DSL surface — domain/policy/commands/queries/...
  <feature>.lzx           # abstract experience (optional, UI features only)
  <feature>.web.lzx       # web platform projection
  <feature>.mobile.lzx    # mobile platform projection
  handlers/<fn>.go        # @fn.* / @validator.* / @hook.* extension code
  domain/<fn>.go          # domain function extensions
  queries/<name>.sql      # raw SQL files referenced via query.sql @file.<name>
  jobs/<name>.go          # job handler extensions
  integrations/<name>.go  # webhook verifiers, adapter handlers
  templates/<name>.<locale>.tmpl  # email/notif templates
  i18n/<name>.<locale>.json       # feature-local catalogs

contracts/<service>.lzi   # External service contracts
i18n/common.<locale>.json # App-wide translation catalogs
lazurite.toml             # Workspace manifest (distros use distro-named TOML)
```

### Generated (gitignored unless committed deliberately)

```
dist/go/                  # Generated Go (regen-only)
dist/ts-<frontend>/       # Generated TS SDK per frontend (audience-scoped)
.lazuli/                  # Internal cache (graph, source-map, manifest)
```

**Convention rules:**
- Filenames inside `handlers/`, `domain/`, etc. **must match** the DSL reference. `@fn.verify_password` → `handlers/verify_password.go` with `func VerifyPassword(...)`. Doctor enforces.
- `.tmpl` files in scaffold templates use `{{app_name}}` / `{{module}}` placeholders; codegen uses Go `text/template` `{{.Field}}` syntax for runtime templates.
- `dist/` is never user-edited. Regen overwrites; do not commit edits.

See: `docs/project-structure.md`, `docs/proposals/lazurite-scaffold.md` §3 + §3.3.

---

## Lazuli vs Lazurite vocabulary

- **Lazuli** = the framework. Language (`.lzi`/`.lzx`) + IR + compiler (Rust crates in `crates/`) + Go runtime lib (`runtime/go/lazuli/`) + CLI (`lazuli` binary).
- **Lazurite** = the opinionated distribution on top of Lazuli. Folder conventions + `lazurite.toml` manifest + `lazuli new` template body. **One distro currently shipped** but the design space supports others.
- **NOT** "Drusa" — retired vocabulary (pre-2026-05-11). Old commits may reference it; ignore.
- **NOT** "Aerocoding" — the lead's prior project; negative reference (template-driven full codegen that became unmaintainable). Mentioned only as a "don't do this" comparison.

A future distro (Lazonyx for ERP, Lazpipe for automation, etc.) **cannot add language mechanisms**. New `@-namespace`, new `kind` keyword, new escape-hatch → must enter Lazuli language first, then distros adopt. Same rule that prevents Nuxt modules from extending the Vue compiler.

See: `docs/architecture.md` §"Lazuli vs Lazurite", `docs/proposals/lazurite-scaffold.md` §3.3.

---

## Codex parallel-dispatch reference

When orchestrating Codex agents (this section is operational, not philosophical):

### Setup per cell

```bash
# 1. Write the prompt to a temp file (heredocs break with backticks/quotes inside prompts)
#    Use the Write tool to write to /c/tmp/codex-<id>-prompt.md

# 2. Create an isolated worktree (the -b flag is mandatory; main is already checked out)
git worktree add -b codex-<id>-branch .claude/worktrees/codex-<id> main

# 3. Launch (medium effort by default; the user is on a fixed ChatGPT subscription)
codex exec --dangerously-bypass-approvals-and-sandbox \
  -C .claude/worktrees/codex-<id> \
  -c model_reasoning_effort=medium \
  "$(cat /c/tmp/codex-<id>-prompt.md)" 2>&1 | tee /c/tmp/codex-<id>-output.log
```

Use Bash `run_in_background: true` to fire multiple agents simultaneously. The harness sends a `<task-notification>` when each completes; do NOT poll, do NOT sleep, do NOT proactively check progress.

### Gotchas

1. **`--sandbox workspace-write` is broken on Windows** (`CreateProcessWithLogonW failed: 1326/1909`). Use `--dangerously-bypass-approvals-and-sandbox`. Worktree isolation already protects the main tree.

2. **`-b <branch>` flag on `git worktree add` is mandatory** — without it, `main` is already-checked-out and the command errors.

3. **Heredocs with backticks/quotes break** — write prompts via the Write tool to `/c/tmp/codex-<id>-prompt.md` and `$(cat ...)` them.

4. **Reasoning effort**: `low` / `medium` / `high` / `xhigh`. `medium` is the default cost-conscious choice. `xhigh` torches ChatGPT subscription (~90K tokens/agent). Reserve for genuinely hard cells.

5. **Codex returns exit 0 even when the agent did nothing** (sandbox failure, early abort). **Always verify** (a) output log content, (b) `git -C <worktree> log` shows the expected commit.

6. **Cherry-pick chain conflicts**: `git cherry-pick --abort` rewinds the ENTIRE chain. Use `--skip` for empty cherry-picks, manual conflict resolution + `--continue` for content conflicts. Recover via `git reflog` + `git reset --hard <last_good_sha>` if you fully lose the chain.

7. **ChatGPT plan throttle** at ~10-20 simultaneous agents. Split into waves if needed.

### Codex prompt template

Every Codex cell prompt should include:
- **Task** — what to build (1-3 sentences).
- **Read first** — exact file paths + sections (Codex doesn't have orchestrator context).
- **Spec / outline** — pseudo-code or signature stub showing the desired shape.
- **Constraints** — what NOT to do (touch shared files, add deps, etc.).
- **Tests** — inline tests required.
- **Commit message** — exact subject line + Co-Authored-By trailer.
- **Report** — path to write a `/tmp/codex-<id>-report.txt` summary.

See: memory `reference_codex_cli_parallel.md`.

---

## Decision log: what's already been decided

Avoid relitigating these. Each has a memory entry with the rationale.

| Decision | Resolution | Source |
|---|---|---|
| DSL syntax style | Indentation-based, PascalCase kinds (NOT Ruby do/end + :symbols) | `project_lzi_syntax.md` |
| GeoPoint representation | `@semantic.GeoPoint { lat, lng }` — single semantic type | memory `project_product_decisions_2026-05-11.md` (private) |
| Geospatial search backend | PostGIS in Postgres (NOT Algolia) | same |
| Maps provider | Google Maps direct (NOT Nominatim fallback) | same |
| Payment provider | `@plugin/mercadopago` (separate repo, generic — adapter named after the provider, not any consumer product) | `project_plugin_namespace_policy.md` |
| Push notifications | Expo Push (NOT FCM direct) | same |
| DB driver | `pgx/v5` + `RowToStructByName[T]` (NOT sqlx, NOT pgxscan/scany) | `project_db_driver_choice.md` |
| HTTP routing | `net/http` Go 1.22+ enhanced ServeMux (NOT chi) | runtime/go/lazuli/http.go usage |
| Migrations | `atlas` (declarative diff) | docs/architecture.md technology picks |
| Background jobs | `river` (Postgres-backed) | same |
| Event bus | In-process for v0 (Go channels) + River for durable | same |
| Manifest format | TOML (`lazurite.toml`), NOT `.lzi` (distro-named per Nuxt analogy) | `docs/proposals/lazurite-scaffold.md` §4 |
| Manifest scope | Framework version pin, plugins, codegen, frontend topology. Does NOT duplicate `app.lzi` envs/urls/deploy | same + `docs/invariants.md:14-15` |
| Generated output path | `dist/` (NOT `.lazuli/generated/`). `.lazuli/` is internal cache only | `docs/proposals/lazurite-scaffold.md` §6 |
| `dist/go/go.mod` | Sub-module by default; `go.work` at root | same §6.1 |
| Deploy in framework | OUT of scope. `runtime/go/lazuli/deploy/` was deleted in Wave B 2026-05-13 | same §13.3 |

---

## When in doubt

- Read `docs/architecture.md` end-to-end.
- Read `docs/invariants.md` for the closed grammar/IR constraints.
- Read `docs/design-principles.md` — Rule Zero ("Vocabulary Over Mechanism") is the most-cited principle in design decisions.
- Read the relevant `docs/proposals/<x>.md` if working on the corresponding subsystem.
- Search the memory directory for prior decisions before relitigating.
- Ask the user (you, Lucas) rather than guessing. The cost of a clarifying question is much lower than the cost of a wrong batch.
