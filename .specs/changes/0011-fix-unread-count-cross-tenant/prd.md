---
id: 0011
title: Fix unread_count cross-tenant — self-contradicting tenancy contract in Pauta notifications
type: prd
status: ready
created: 2026-05-31
depends_on: []
parallel_safe: true
track: pilot
severity: HIGH
---

# PRD — Fix unread_count cross-tenant

## Problem
Pauta-web's notification badge query `features/notifications/queries/unread_count.sql` uses NAMED params (`:user_id` / `:org_id`) and asserts in its header comment that "the framework supplies them from ctx" / auto-injects tenant scope. That contract is FALSE for this build. Every other Pauta `query.sql` uses POSITIONAL `$N` and explicitly documents the opposite contract:
- `dashboard.lzi:44-47` — "Tenancy is NOT auto-injected for query.sql in this build (the positional-$N contract): every query declares `params org_id: ID required` bound to $1, and the SQL filters `<table>.org_id = $1`".
- `admin_panel.lzi:55-57` — "query.sql receives NO auto-injected tenant scope in this build (the positional-$N contract)".
- `reports_exports.lzi:280-282` — same positional-$N contract; each declares `org_id: ID required` bound to `$1`.

Worse, the `.lzi` block at `notifications.lzi:229-232` declares **NO `params`** for `unread_count`. The codegen (`crates/lazuli_codegen_go/src/emitter/query/sql.rs` + `query/args.rs`) builds the typed args struct strictly from the declared `params` block, binds them positionally, and loads the SQL body verbatim; it does NOT rewrite `:user_id`/`:org_id` and does NOT inject a tenant predicate. (There is no `bind ... from ctx` construct in this build — args come only from `params`.) So the badge count either fails to bind the named placeholders (the pgx runtime is positional `$N`) or, if it binds at all, the `WHERE org_id = :org_id` predicate is never satisfied from ctx — the count is wrong and potentially cross-tenant. This is a HIGH-severity tenancy leak on a user-facing surface.

## Goal
Rewrite `unread_count.sql` + its `notifications.lzi` `query.sql` block to match the ONE real contract: positional `$N` placeholders, a declared `params` block (`user_id` + `org_id`), and a VISIBLE tenant predicate in the SQL. The badge count must be provably scoped to the actor's own rows within the active org, identical in shape to the working Pauta query.sql files (`dashboard/queries/jobs_by_status.sql` is the reference).

## Users & jobs
- **Pauta end user**: sees an unread badge. Job: "the count is MY unread notifications in MY org, never another tenant's."
- **Authoring agent**: copies the notification feature as a pattern. Job: "the escape-hatch query.sql here must match the canonical positional-$N tenancy contract, not invent a fictional auto-inject."
- **Security reviewer**: audits tenancy. Job: "every query.sql restates its tenant predicate visibly; no query claims a contract the build doesn't honor."

## Scope
### In
- Rewrite `app/features/notifications/queries/unread_count.sql`: `:user_id`/`:org_id` → `$1`/`$2`; add the positional-params header comment; keep the visible `user_id = $1 AND org_id = $2` predicate.
- Rewrite the `notifications.lzi:227-232` `query.sql unread_count` block: add the `params` block (`user_id: ID required`, `org_id: ID required`) bound positionally; fix the misleading header comment.
- A handler/integration test proving the count is org-scoped (a notification in org B does NOT increment org A's count).
### Out
- Any framework/IR/codegen change — this is a PILOT fix against the existing contract, not a language feature. The DoD "Teach" gate is N/A (no idiom ships).
- Changing the auto-inject contract itself (that is spec 0010's `ESC-SQL-TENANCY-CONTRACT-001` enforcement work; this spec is downstream-protected by it).

## Behaviour
- `unread_count` binds two positional args (`$1` = actor id, `$2` = active org id) supplied by the typed args struct codegen emits from the declared `params`.
- The SQL filters `WHERE user_id = $1 AND org_id = $2 AND is_read = false` — the tenant predicate is visible in the file, not assumed from ctx.
- A notification row in a foreign org is never counted.

## Success metric
`lazuli check . && lazuli doctor . && go build ./...` clean in Pauta; the new handler test asserts a cross-org notification is excluded from the count; the file no longer contains `:user_id` / `:org_id` or the "framework supplies them from ctx" claim.

## Risks
- The fix could mask a genuine framework gap (maybe auto-inject SHOULD exist). Mitigation: three independent Pauta files + the codegen source agree the positional-$N + declared-`params` contract is the real one; conforming to it is correct today. If auto-inject is later built, it is a separate change that migrates ALL query.sql uniformly.
- Param ordering mismatch (actor vs org) silently re-leaks. Mitigation: the header comment pins `$1 = user_id (actor)`, `$2 = org_id (tenant)`; the handler test asserts org-scoping directly.

## Open questions
- None blocking. The contract is determined empirically in the plan's first step (verified: declared `params` block → positional `$N`; no `bind`/auto-inject exists).
