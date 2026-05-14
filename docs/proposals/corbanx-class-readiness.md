# Corbanx-Class Readiness — Roadmap Meta-Doc

**Status**: meta-roadmap; tracker for the multi-wave initiative to make Lazuli
welcoming for apps in the shape of a real client backend (Express + Drizzle +
BullMQ + Supabase Auth + 20+ vendor SaaS adapters + multi-tenant + reports).

**Audience**: orchestrator (Claude), language-architect agents, Codex
implementers.

**Date**: 2026-05-14.

**Trigger**: gap audit of `c:/Users/lucas/dev-trabalho/corbanx` against current
Lazuli surface. Goal is **not** to port corbanx — goal is to land the framework
features such that *any* corbanx-class app can adopt Lazuli without falling back
to hand-written Go for the load-bearing pieces.

## Non-goals

- Porting corbanx itself (deferred until framework is corbanx-class).
- Shipping bank adapters in core (`@plugin/bankerize`, `@plugin/v8`, etc. — those
  are handlers in the *consumer's* repo, not Lazuli plugins; see
  [`feedback_plugin_vs_handler_boundary` discussion 2026-05-14]).
- Vendor SaaS in `runtime/go/lazuli/` — namespace policy still applies.

## The 22 gaps, status, dispatch

Status legend: ⬜ no proposal · 🟡 proposal exists, not implemented ·
🟢 implemented · ⛔ deferred / out-of-scope.

| # | Gap | Layer touched | Status | Owner / cell |
|---|---|---|---|---|
| 1 | Storage / blob / signed URL | surface + IR + analyzer + doctor + LSP + codegen + runtime | 🟡 [`bucket-storage-scope.md`](bucket-storage-scope.md) + [`bucket-storage-cycle.md`](bucket-storage-cycle.md) | Codex: `runtime/go/lazuli/storage/{contract,s3,test}.go` |
| 2 | Field-level encryption / secret at-rest | surface (scalar/cap) + runtime | ⬜ | Claude subagent: draft `encryption-vocab.md`. Codex: `runtime/go/lazuli/encryption/{aes_gcm,aes_gcm_test}.go` |
| 3 | CSV/XLSX export | surface (`kind report` or `query.export`) + runtime | ⬜ | Claude subagent: draft `report-vocab.md`. Codex: `runtime/go/lazuli/report/{csv,xlsx}.go` |
| 4 | CSV upload + parse | surface (`command.accepts file: csv`) + runtime | ⬜ | Folded into #3 + storage (#1) |
| 5 | Async polling with cursor (`kind poller`) | surface + IR + analyzer + runtime | ⬜ | Claude subagent: draft `poller-vocab.md` |
| 6 | Cron / scheduled jobs | covered by [`bucket-jobs-cycle.md`](bucket-jobs-cycle.md) | 🟡 | Out of wave 1 (scope clear) |
| 7 | Worker process topology in `app.lzi` / `workspace.lzi` | surface + codegen + Lazurite scaffold | ⬜ | Wave 2 — depends on #5 |
| 8 | RBAC catalog (`role`/`permission` declarative) | surface + analyzer + doctor + codegen | ⬜ (auth proposals cover session/tenant only) | Claude subagent: draft `rbac-catalog-vocab.md` |
| 9 | Subscription / plan + feature gating | surface + analyzer + doctor + codegen | ⬜ | Claude subagent: draft `plan-and-gate-vocab.md` |
| 10 | Idempotent enqueue guard | doctor lint OR command directive | ⬜ | Wave 2 — small, after #9 lands shape |
| 11 | Aggregate / materialized view (`kind aggregate`) | surface + analyzer + codegen | ⬜ | Wave 2 — needs design |
| 12 | Locale-aware scalars (pt-BR) | plugin (`@plugin/scalars-pt-BR`) | ⛔ deferred to post-pilot ([`project_validation_strategy_2026-05-14`](../../C:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_validation_strategy_2026-05-14.md)) | — |
| 13 | Webhooks-receive into cache w/ TTL | runtime | 🟢 [`webhooks/`](../../runtime/go/lazuli/webhooks) + [`cache.go`](../../runtime/go/lazuli/cache.go) cover it | — |
| 14 | Token-manager / cross-bank infra | handler in consumer repo | — | Not Lazuli's problem |
| 15 | Multi-tenant w/ RLS-style row scoping | partial: [`examples/auth-multi-tenant/`](../../examples/auth-multi-tenant) | 🟢 mostly | Verify after #8 |
| 16 | Drizzle/Supabase importer (`lazuli import`) | CLI | ⬜ | Wave 3 — port tooling, blocks no greenfield |
| 17 | `lazuli dev` multi-frontend + workers | CLI | ⬜ ([`project_lazuli_dev_build_lifecycle_planned`](../../C:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_lazuli_dev_build_lifecycle_planned.md)) | Wave 2 |
| 18 | Sentry / Datadog / Prometheus exporters | plugin (`@plugin/sentry` etc.) | ⬜ | Wave 3 — separate repos |
| 19 | Supabase Auth provider | plugin (`@plugin/supabase-auth`) | ⬜ | Wave 3 — separate repo |
| 20 | Distro: Next.js + Chakra UI v3 | Lazurite distro target | ⛔ Chakra is plugin; Tailwind/Untitled-UI is default | — |
| 21 | Vendor adapter shape canon | docs + 1 reference plugin | ⬜ | Wave 3 — first plugin defines it |
| 22 | Doctor BR-specific lints | folds into #12 + #8 | ⛔ deferred | — |

## Wave 1 — this dispatch (2026-05-14)

### 5 Claude proposal drafts (parallel `general-purpose` subagents)

Each subagent writes `docs/proposals/<name>.md` v0.1. Output is **drafted**, not
graded. Grading wave follows with `lazuli-language-architect` agents.

| Cell | Proposal | Source gaps |
|---|---|---|
| `prop-encryption` | `encryption-vocab.md` — surface for field-level encryption (`@cap.Secret`? `field foo encrypted true`?) + key management + runtime contract | #2 |
| `prop-report` | `report-vocab.md` — surface for tabular exports (`query … export csv,xlsx` OR `kind report`?) + parse on input | #3, #4 |
| `prop-poller` | `poller-vocab.md` — `kind poller` for async resolution loops (cursor field, retry, final_status, gender_retry-style retries) | #5 |
| `prop-plan-gate` | `plan-and-gate-vocab.md` — `kind plan` + `gate` directive on commands/queries/features | #9 |
| `prop-rbac` | `rbac-catalog-vocab.md` — `role` + `permission` catalogs in `.lzi`; builds on existing `policy` blocks | #8 |

### 8 Codex implementation cells (parallel worktrees)

Wire-thin runtime adapters + doctor lints. Single-file output per cell; no
shared-file edits. Wire-up happens in orchestrator post-merge.

| Cell | File | External lib | Spec |
|---|---|---|---|
| `cdx-storage-contract` | `runtime/go/lazuli/storage/contract.go` | stdlib only | Interface `ObjectStore { Put, GetSignedURL, Delete, Exists, List }`, types `PutOpts`, `SignOpts` |
| `cdx-storage-s3` | `runtime/go/lazuli/storage/s3.go` | `github.com/aws/aws-sdk-go-v2/service/s3` | Constructor + impl of `ObjectStore` from contract.go |
| `cdx-storage-test` | `runtime/go/lazuli/storage/storage_test.go` | stdlib + in-memory mock | Table tests for Put/Sign/Exists/List against fake |
| `cdx-encryption-aes` | `runtime/go/lazuli/encryption/aes_gcm.go` | stdlib `crypto/aes` + `crypto/cipher` | `Encrypt([]byte, key []byte) → []byte`, `Decrypt`, nonce mgmt |
| `cdx-encryption-test` | `runtime/go/lazuli/encryption/aes_gcm_test.go` | stdlib | Round-trip + tamper-detect tests |
| `cdx-report-csv` | `runtime/go/lazuli/report/csv.go` | stdlib `encoding/csv` | Writer wrapping io.Writer; column headers; row encoder helper |
| `cdx-report-xlsx` | `runtime/go/lazuli/report/xlsx.go` | `github.com/xuri/excelize/v2` | Same shape as CSV writer; sheet name; column widths |
| `cdx-lint-audit002` | `crates/lazuli_analyzer/src/correctness/audit_002.rs` | none | Doctor lint AUDIT-002 (event emitted without `audit_log` capability declared) per [vocab audit findings](../../C:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_vocab_audit_findings_2026-05-14.md) |

## Wave 2 — pending Wave 1 outcomes

- Implementation cells for Wave 1 proposals after they PASS ≥ 8.5.
- Idempotent-enqueue lint (#10) — small once `command` surface is touched.
- Aggregate/materialized view design (#11).
- Worker-process-topology design (#7) — depends on #5 (poller) shape.
- `lazuli dev` multi-frontend lifecycle (#17).

## Wave 3 — plugin ecosystem

- `@plugin/supabase-auth` (#19) — first plugin as **canon shape** reference.
- `@plugin/scalars-pt-BR` (#12) — locale scalars.
- `@plugin/sentry` (#18) — observability provider.
- `lazuli import` Drizzle → `.lzi` (#16) — port tooling.
- Plugin authoring docs revisit ([`docs/plugin-authoring.md`](../plugin-authoring.md)) based on first plugin.
- Plugin SDK package (`runtime/go/lazuli/plugin/`) — emerges from first 1-2 plugins, not designed upfront.

## Decision log

- **2026-05-14**: bank adapters (V8, Prata, Bankerize, etc.) are **handlers in
  consumer repo**, not Lazuli plugins. Plugin = extends `.lzi`/`.lzx` surface;
  handler = uses surface via `@fn`. See conversation 2026-05-14 (claude main).
- **2026-05-14**: pt-BR scalars stay deferred per
  [`project_validation_strategy_2026-05-14`](../../C:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_validation_strategy_2026-05-14.md).
- **2026-05-14**: storage runtime can ship in Wave 1 (interface + S3 impl) even
  though surface lowering is still 🟡 — runtime contract is independent of
  whether codegen calls it yet.

## How to read this doc

- Each ⬜ → drafts a proposal, grades, implements.
- Each 🟡 → proposal exists; needs cells (split by layer).
- Each 🟢 → confirm coverage via a corbanx-shaped example; otherwise mark down.
- Each ⛔ → no work; verify trigger condition periodically.

## How wave dispatch reports back

Cells write reports to `/c/tmp/<cell-id>-report.txt`. Orchestrator collects on
notification, cherry-picks atomic commits onto `main`, re-grades any proposal
revs, kicks the next wave.
