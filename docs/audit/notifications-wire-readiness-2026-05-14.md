# Notifications Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/notifications/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `contract.go` | 81 | _none_ | **wire** | — | — |
| `digest_store.go` | 94 | _none_ | **wire** | — | — |
| `dispatch.go` | 33 | _none_ | **wire** | — | — |
| `throttle_store.go` | 125 | _none_ | **rewrite-as-wire** | `golang.org/x/time/rate` | M (≤ 200 LOC) |

### throttle_store.go note

`throttle_store.go` crosses the CLAUDE.md test: 125 effective LOC, zero external imports, and its in-process throttle path hand-rolls token bucket state, refill arithmetic, overflow protection, and retry timing. The DSL duration bridge can stay framework-specific, but the bucket implementation should be a thin wrapper around `golang.org/x/time/rate` with Lazuli-owned keying and error mapping. Distributed production throttles still belong behind `ThrottleStore` implementations in commodity runtime adapters such as Redis or Postgres, not in this core file.

---

## Summary

**3/4 files (75.0%) are wire-clean.** The bucket is mostly small framework contract glue, with one clear homegrown rate-limiter implementation that should be rewritten as wire of `golang.org/x/time/rate`.

### Top 3 risks for the Pleiades/Atelier/Erudito and Hostpoint ports

1. **`throttle_store.go` custom token bucket behavior** — Ports that rely on notification throttles could observe edge-case drift around refill timing, retry windows, or burst handling compared with a mature limiter. Rewriting this around `golang.org/x/time/rate` keeps the runtime focused on Lazuli keying and DSL validation boundaries.

2. **`dispatch.go` is still a stub** — `Send` does not yet resolve recipients, render templates, apply idempotency, fan out channels, or execute retry policy. That is not a wire-thin violation, but downstream ports cannot treat notifications as production-ready until the dispatcher wires those concerns through existing runtime primitives.

3. **Provider boundary language is loose in comments** — The bucket comments mention named products such as Sendgrid, FCM, Twilio, Slack, and Discord. Those should be moved to `@plugin/<name>` on consumption; the core notifications bucket should keep only the generic channel contract and commodity store interfaces.

---

## Punch List (Codex cells)

### Cell NOTIFICATIONS-1: Rewrite throttle_store.go as wire of golang.org/x/time/rate

**Trigger:** `throttle_store.go` is 125 effective LOC with zero external imports and reimplements token-bucket mechanics.

**Spec for Codex:**

```
File to replace: runtime/go/lazuli/notifications/throttle_store.go
Target library:  golang.org/x/time/rate

Keep the public API identical:
  type ThrottleStore interface { Allow(context.Context, ThrottleKey, NotificationThrottle) (bool, time.Time, error) }
  type ThrottleKey struct { Notification string; Recipient string; Channel Channel }
  type MemoryThrottleStore struct { ... }
  func NewMemoryThrottleStore() *MemoryThrottleStore

Implementation notes:
- Keep parseDuration unless/until a shared Lazuli duration parser exists.
- Store one rate.Limiter per computed ThrottleKey.
- Use rate.Every(window / burst) with burst >= 1.
- Use Reserve / CancelAt to compute retryAt and map denied reservations to ErrThrottleExceeded.
- Do NOT introduce vendor SaaS adapters into runtime/go/lazuli/notifications.
- Do NOT touch any file outside runtime/go/lazuli/notifications/throttle_store.go and the Go module dependency files needed for golang.org/x/time/rate.

Commit message: "runtime/notifications: rewrite throttle store as wire of x/time/rate (NOTIFICATIONS-1)"
```

**Estimated size:** M (≤ 200 LOC delta, including dependency metadata).

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 4 |
| Wire-clean | 3 (75.0%) |
| Rewrite-as-wire | 1 (`throttle_store.go`) |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 1 (NOTIFICATIONS-1) |
| Downstream-blocker risk | Medium (`dispatch.go` remains a stub; `throttle_store.go` should be rewritten before production notification throttles) |
