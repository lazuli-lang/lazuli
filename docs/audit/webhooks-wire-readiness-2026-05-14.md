# Webhooks Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/webhooks/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `contract.go` | 72 | `lazuli.dev/runtime/lazuli/jobs` | **wire** | - | - |
| `receive.go` | 41 | _none_ | **wire** | - | - |
| `verify.go` | 23 | _none_ | **wire** | - | - |

---

## Summary

**3/3 files (100.0%) are wire-clean.** No file crosses the CLAUDE.md rewrite threshold, and the bucket currently contains only lowered contract shapes, a minimal router mount stub, and a short stdlib HMAC verifier.

### Top 3 risks for the Hostpoint port

1. **`receive.go` is still a mount stub** - it registers routes but returns `501 Not Implemented`, so any downstream product port that relies on inbound webhooks will need the receiver lifecycle filled in before live traffic.

2. **`verify.go` only supports raw SHA-256 HMAC hex** - this is acceptable commodity runtime wire, but provider-specific signature formats such as Stripe-style timestamped headers must stay out of core and move to `@plugin/<name>` on consumption.

3. **Replay, DLQ, retry, idempotency, and tenant scoping are contract-only today** - `contract.go` carries the DSL shape, but the runtime does not yet enforce those behaviors. Product ports should treat the fields as codegen-ready vocabulary, not completed delivery semantics.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 3 |
| Wire-clean | 3 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Hostpoint-blocker risk | Medium (`receive.go` must be implemented before inbound webhook traffic) |
