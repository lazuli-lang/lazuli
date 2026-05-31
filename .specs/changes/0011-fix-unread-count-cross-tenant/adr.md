---
id: 0011
title: Fix unread_count cross-tenant — self-contradicting tenancy contract in Pauta notifications
type: adr
status: ready
created: 2026-05-31
depends_on: []
parallel_safe: true
track: pilot
severity: HIGH
---

# ADR — Fix unread_count cross-tenant

## Context
One Pauta `query.sql` (`unread_count`) declares a tenancy contract — named `:user_id`/`:org_id` auto-injected from ctx — that the build does not implement and that every other Pauta `query.sql` explicitly contradicts (positional `$N`, declared `params` block, no auto-inject, visible tenant predicate). The codegen confirms the real contract: `query.sql` args come ONLY from the declared `params` block (emitted as a typed args struct, bound positionally) and the SQL body is loaded verbatim. There is no `bind ... from ctx` construct and no placeholder rewrite. The result is a HIGH-severity tenancy bug: the badge count is unbound or cross-tenant.

## Decision
Conform `unread_count` to the existing positional-$N + declared-`params` contract: declare `params` (`user_id`, `org_id`), rewrite the SQL to `$1`/`$2` with a visible `WHERE user_id = $1 AND org_id = $2` predicate, and fix the lying header comment. Do NOT introduce auto-injection. Prove the fix with an org-scoped handler test.

## Options considered
1. **Conform to positional-$N + declared `params` (chosen).** Pro: matches the build, matches 3 sibling files + codegen, zero framework risk, immediately ships. Con: the `unread_count` author's mental model (auto-inject) stays unrealized — acceptable, it was never real.
2. **Build auto-injection so `:user_id`/`:org_id` work as claimed.** Pro: the comment becomes true; less SQL boilerplate. Con: a framework feature, not a pilot fix; would have to migrate ALL query.sql uniformly; out of scope for a HIGH security hotfix; high blast radius.
3. **Drop the org predicate, rely on actor-id alone.** Rejected: a user can belong to multiple orgs; actor-only scope still leaks across the user's other tenants and contradicts the canonical "restate the tenant predicate" rule.

## Consequences
- `unread_count` is shaped like `dashboard/queries/jobs_by_status.sql`: positional, `params`-declared, tenant-predicate-visible.
- The fictional auto-inject claim is removed from the codebase — no future reader copies it as a pattern.
- Regression is prevented by spec 0010's `ESC-SQL-TENANCY-CONTRACT-001`, which fires on a `query.sql` whose SQL references a tenant column not backed by a declared param / visible predicate.
- **Trade:** two lines of explicit param boilerplate per query.sql instead of magic — the framework's deliberate, auditable choice for this build. No data migration; the query is read-only.

## Decision drivers
- Security correctness now > elegance later.
- One real contract: every query.sql must look the same; the outlier is the bug.
- Pilot fixes never silently change framework contracts.
