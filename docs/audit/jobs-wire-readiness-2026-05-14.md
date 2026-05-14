# Jobs Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/jobs/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `builders.go` | 22 | _none_ | **wire** | — | — |
| `contract.go` | 64 | _none_ | **wire** | — | — |
| `dispatch.go` | 200 | `river`, `river/rivertype` | **wire** | — | — |
| `retry.go` | 33 | _none_ | **wire** | — | — |

---

## Summary

**4/4 files (100.0%) are wire-clean.** The bucket already follows the intended split: Lazuli owns the closed contract vocabulary, retry policy catalog, and generated-code dispatch surface, while River owns the durable queue, worker registration, insert semantics, and adapter-side retry behavior. No file meets the `rewrite-as-wire` rule, and no delete candidates were found.

### Top 3 risks for downstream product ports

1. **River-specific surface in `dispatch.go`** — The dispatcher is intentionally wired to `github.com/riverqueue/river`, which matches the current architecture decision. If Pleiades/Atelier/Erudito need an alternate queue backend later, the new adapter should live behind the existing `Dispatcher` interface instead of expanding this bucket into a multi-queue abstraction layer.

2. **Scheduled jobs are only a boot-time hook** — `EnqueueSchedule` is a v0 no-op because River periodic jobs must be registered before client start. Hostpoint or other scheduled-heavy ports will need codegen/boot orchestration to thread scheduled contracts into River config; this is integration wiring, not a runtime rewrite.

3. **Inline retry helper is framework policy** — `retry.go` hand-computes fixed/exponential delays, but it is only 33 effective LOC and represents Lazuli's closed retry vocabulary for inline dispatch/tests. Adapter-specific jitter, caps, persistence, and dead-letter behavior must stay in River or another queue adapter rather than growing here.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 4 |
| Wire-clean | 4 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low |
