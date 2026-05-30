---
title:   "Justified opt-outs: every escape carries a reason"
slug:    justified-opt-outs
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, security, opt-out, reason]
read_when: "skipping a rate_limit, verify, or tenant scope"
---

# Justified opt-outs: every escape carries a reason

Lazuli's security defaults are *forcing*: a mutating command must declare a rate limit, an inbound webhook must verify signatures, a tenant-scoped resource must derive its tenant. You may step outside each — never silently. Every opt-out is a distinct keyword **plus a required `reason "..."` child**: explicit, auditable, greppable.

Gaffes (LSP flags them): the opt-out keyword without a `reason`; or a fake value faking compliance (e.g. bogus `rate_limit "1000000 per second"` instead of an honest opt-out).

## The four opt-outs

```lazuli
  # 1. A command that intentionally has no rate limit.
  command internal_reconcile
    policy @policy.admin
    rate_limit none
      reason "internal operator-only batch, behind a controlled network"
    updates Ledger
      reconciled_at = ctx.now
```

```lazuli
  # 2. An inbound webhook that skips signature verification
  #    (verified at the gateway, or genuinely internal).
  webhook internal_event
    path "/webhooks/internal/event"
    verify none
      reason "internal webhook, signature verified at the API gateway"
    idempotency by payload.event_id
    policy @actor.system
    handler "./integrations/record_internal_event.go"
    emits internal_event_received
```

```lazuli
  # 3. A webhook whose provider sends no tenant key in the payload.
  webhook provider_callback
    path "/webhooks/provider/callback"
    verify hmac sha256
      secret env.PROVIDER_WEBHOOK_SECRET
      header "X-Signature"
    scope global
      reason "provider sends no tenant key; handler reconciles via external_reference"
    idempotency by payload.provider_event_id
    policy @actor.system
    handler "./integrations/provider_callback.go"
    emits provider_callback_received
```

A silent drop on the dead-letter queue is the same discipline — `dlq drop` + `reason`, a `webhook` child (not a `job` child):

```lazuli
  webhook flaky_provider
    path "/webhooks/flaky/event"
    verify none
      reason "internal webhook"
    idempotency by payload.id
    policy @actor.system
    handler "./integrations/flaky.go"
    dlq drop
      reason "best-effort telemetry; losing a redelivery is acceptable"
    emits flaky_event_received
```

## Why the reason is load-bearing

The `reason` is the audit record an operator-of-record reads when asking "why does this trust boundary have a hole?", and the thing a production-profile `lazuli doctor` (or deploy allowlist) checks before shipping. A reason-less opt-out is rejected at the authoring layer so the justification can never be lost.

`rate_limit none` / `verify none` lower to the same runtime as "no throttle" / "no verifier" — but the *authoring distinction* (you were forced to type `none` and justify it) keeps the posture honest.

Authoritative spec: `docs/quickref.md` §security, `docs/grammar.lzi.md` (`rate_limit_clause`, `verify_value`).
