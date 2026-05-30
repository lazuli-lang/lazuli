---
title:   "Justified opt-outs: every escape carries a reason"
slug:    justified-opt-outs
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, security, opt-out, reason]
---

# Justified opt-outs: every escape carries a reason

Lazuli's security defaults are *forcing*: a mutating command must declare a rate
limit, an inbound webhook must verify signatures, a tenant-scoped resource must
derive its tenant. You are allowed to step outside each default — but never
silently. Every opt-out is a distinct keyword **plus a required `reason "..."`
child**, so the waiver is explicit, auditable, and greppable.

Reaching for the opt-out without the reason is a gaffe (the LSP flags it); so is
inventing a fake value just to satisfy the default (e.g. a bogus `rate_limit
"1000000 per second"` instead of an honest opt-out).

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

A webhook's dead-letter handling carries the same discipline: a silent drop on
the dead-letter queue must be an explicit waiver — `dlq drop` with a `reason`
child, **inside a `webhook`** (it is not a `job` child):

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

The `reason` is not decoration. It is the audit record an operator-of-record
reads when they ask "why does this trust boundary have a hole?" — and it is the
thing a production-profile `lazuli doctor` (or a deployment allowlist) checks for
before letting the opt-out ship. An opt-out without a reason is rejected at the
authoring layer precisely so the justification can never be lost.

`rate_limit none` and `verify none` lower to the same runtime behaviour as "no
throttle" / "no verifier" respectively — but the *authoring distinction* (you
were forced to type the word `none` and justify it) is what keeps the security
posture honest.

Authoritative spec: `docs/quickref.md` §security, `docs/grammar.lzi.md`
(`rate_limit_clause`, `verify_value`).
