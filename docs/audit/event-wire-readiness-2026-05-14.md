# Event Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `runtime/go/lazuli/event.go`, `runtime/go/lazuli/eventbus.go` top-level event contract and in-process v0 event bus glue.

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `event.go` | 26 | _none_ | **wire** | — | — |
| `eventbus.go` | 53 | _none_ | **wire** | — | — |

---

## Summary

**2/2 files (100.0%) are wire-clean.** The event topic is compact framework vocabulary plus a deliberately minimal in-process v0 bus; neither file crosses the >100 LOC / zero-external-import threshold, and neither reimplements a known commodity event broker. There are no focused `event*_test.go` files in `runtime/go/lazuli/`; current coverage appears indirect through command emission, handle publication, and Go codegen paths.

### Top 3 risks for downstream product ports

1. **Best-effort synchronous delivery semantics** — `eventbus.go` invokes subscribers in registration order and logs errors without retry or propagation. Pleiades / Atelier / Erudito should treat this as v0 in-process reaction glue until durable River/NATS/Redis-backed delivery lands.

2. **Global subscriber registry** — the package-level bus is simple and wire-clean, but it means tests and multi-app processes can share subscriber state unless callers isolate process lifetime or add explicit reset hooks in test-only code.

3. **Event descriptor metadata is intentionally thin** — `event.go` exposes pattern/resource/name/payload type, while richer event-group audit metadata remains outside the Go runtime shape. Product ports that need event catalogs, docs, or observability labels may need codegen-side metadata rather than expanding this runtime file into mechanism.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 2 |
| Wire-clean | 2 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (semantic/durability limits only; no wire-thin rewrite required) |
