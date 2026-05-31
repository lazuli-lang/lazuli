---
id: 0011
title: Fix unread_count cross-tenant — self-contradicting tenancy contract in Pauta notifications
type: techspec
status: ready
created: 2026-05-31
depends_on: []
parallel_safe: true
track: pilot
severity: HIGH
test_gate: "lazuli check . && lazuli doctor . && go build ./... (in pauta-web) + handler test proving unread_count is org-scoped"
agent: unassigned
---

# TechSpec — Fix unread_count cross-tenant

## Approach
Pure pilot fix, no framework change. Conform one outlier `query.sql` to the build's real contract (empirically determined in step 1 from codegen + a working sibling query): args come ONLY from a declared `params` block, bound positionally to `$N`; the SQL body is loaded verbatim with NO placeholder rewrite and NO auto-injected tenant scope (and no `bind ... from ctx` construct exists). Two edits in Pauta (`unread_count.sql` + the `notifications.lzi` block) + one org-scoping test. The "Teach" DoD gate is **N/A**: no idiom or language feature ships here. Regression protection is delegated to spec 0010's `ESC-SQL-TENANCY-CONTRACT-001`.

## Surface
**Modify (pilot `C:\Users\lucas\dev\pauta-web-monorepo\app`):**
- `features/notifications/queries/unread_count.sql` — replace `:user_id`/`:org_id` with `$1`/`$2`; rewrite the header to the positional-params form (matching `dashboard/queries/jobs_by_status.sql`); keep `WHERE user_id = $1 AND org_id = $2 AND is_read = false`.
- `features/notifications/notifications.lzi` (block at lines 227-232) — add a `params` block (`user_id: ID required`, `org_id: ID required`); rewrite the comment that currently implies a scalar escape-hatch needs no params; remove any "framework supplies / auto-inject" language.

**Create (pilot):**
- A handler/integration test under `features/notifications/handlers/` (the pilot's Go test location for query.sql) asserting `unread_count` over a fixture with notifications in two orgs returns only the active org's unread count.

**Reference only (do NOT modify):**
- `crates/lazuli_codegen_go/src/emitter/query/sql.rs` + `query/args.rs` — proof that args derive solely from the declared `params` block (typed args struct), SQL is loaded verbatim, runtime is pgx positional (`PATTERN_QUERY_PGX_SQL`). No `bind`/auto-inject path exists.
- `features/dashboard/queries/jobs_by_status.sql` (`WHERE j.org_id = $1`) + `dashboard.lzi:53-58` (`params\n  org_id: ID required`) — the canonical positional-$N + declared-`params` shape to copy.

## Contracts
**The real `query.sql` tenancy contract (this build):**
```
- query.sql args come ONLY from the declared `params` block (emitted
  as a typed args struct), bound positionally to $1, $2, ... in
  declaration order. There is NO `bind ... from ctx` construct.
- The SQL body is loaded from disk verbatim; the framework does NOT
  rewrite placeholders and does NOT inject any tenant predicate.
- Therefore: every tenant-scoped query.sql MUST declare its tenant
  param AND restate the tenant predicate (`<table>.org_id = $N`) in
  the SQL. Tenancy is NOT auto-injected for query.sql in this build.
```

**Target `unread_count.sql` (shape):**
```sql
-- query.sql notifications.unread_count
-- Count of unread notifications for the current user within the active
-- tenant. Single-integer aggregate (the scalar escape hatch a typed
-- query.list cannot return). Returns Integer.
--
-- Params (positional):
--   $1 :: id  -- user_id  (actor id)
--   $2 :: id  -- org_id   (tenant scope)
SELECT COUNT(*)::INTEGER AS unread_count
FROM notification
WHERE user_id = $1
  AND org_id  = $2
  AND is_read = false;
```

**Target `notifications.lzi` block:**
```
  # Single-integer unread badge count. Join-free COUNT over the actor's
  # own unread rows is the raw-SQL escape hatch (query.list cannot return
  # a scalar). Positional-$N contract: tenancy is NOT auto-injected, so
  # user_id ($1) + org_id ($2) are declared params and the SQL restates
  # the tenant predicate.
  query.sql unread_count
    returns Integer
    sql "./queries/unread_count.sql"
    policy @policy.view
    params
      user_id: ID required
      org_id: ID required
```

## Plan — for the executing agent
1. **Determine the real contract (FIRST).** Read `crates/lazuli_codegen_go/src/emitter/query/sql.rs` + `query/args.rs` and one working Pauta query.sql pair (`dashboard.lzi` block at 53-58 + `dashboard/queries/jobs_by_status.sql`). Confirm: (a) args derive from the declared `params` block, (b) binding is positional `$N`, (c) no auto-inject, no placeholder rewrite, no `bind ... from ctx`. Record the finding in the commit body. (Pre-verified in this spec's research: the real contract is positional-$N + a declared `params` block — `dashboard.lzi` carries `params org_id: ID required` bound to `$1` in 6 query.sql blocks; there is NO `bind ... from ctx` construct anywhere in the pilot (0 matches), so do not author one. Note: a `recent_jobs.sql` file does exist in the pilot but its `.lzi` block uses the `params` contract, not `bind` — use `dashboard/queries/jobs_by_status.sql` + its `params`-declaring block as the canonical reference.)
2. Rewrite `unread_count.sql` to the target shape: `:user_id`→`$1`, `:org_id`→`$2`, positional-params header, visible tenant predicate. Delete the "framework supplies them from ctx" sentence.
3. Edit the `notifications.lzi` `query.sql unread_count` block: add the `params` block (`user_id: ID required`, `org_id: ID required`) in the order that matches `$1`/`$2`; fix the comment.
4. Confirm the caller (the handler/wrapper that invokes `unread_count`, e.g. a badge handler) passes `{user_id: ctx.actor.id, org_id: <active org>}` into the args struct. Source the org id from the same active-tenant axis the other Pauta query.sql call sites use.
5. Write the org-scoping test: seed two orgs, two users, unread notifications in each; assert `unread_count` for user A in org A returns only A's unread count and excludes org B's rows.
6. Run the test_gate: `lazuli check .`, `lazuli doctor .`, `go build ./...` in Pauta, plus the new test. All green.

## Tests first (TDD)
- [ ] `unread_count_is_org_scoped` — a notification in org B is NOT counted for a user querying in org A.
- [ ] `unread_count_is_actor_scoped` — another user's unread rows in the SAME org are NOT counted.
- [ ] `unread_count_counts_only_unread` — `is_read = true` rows are excluded.
- [ ] `sql_uses_positional_params` — the `.sql` file contains `$1`/`$2` and NOT `:user_id`/`:org_id` (string assertion guarding regression).
- [ ] `lzi_declares_params` — the `notifications.lzi` block declares `user_id` + `org_id` params (so codegen emits the args struct).

## Gate
test_gate green **and** the four-gate DoD below satisfied (Teach = N/A, explicitly):

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen. **N/A — no framework code changes; pilot-only fix.**
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web. **DONE in pauta-web (the only affected pilot); hostpoint has no equivalent query.**
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added. **N/A — no idiom ships; the canonical positional-$N contract is already taught by spec 0010's escape-hatch decision tree + `ESC-SQL-TENANCY-CONTRACT-001`. This is a one-off conformance fix, not a teachable primitive.**
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. **DELEGATED — spec 0010's `ESC-SQL-TENANCY-CONTRACT-001` fires on exactly this anti-shape (a query.sql whose SQL references a tenant column with no backing declared param / visible predicate, or that claims auto-inject). This spec's `sql_uses_positional_params` test is the local regression guard until 0010's rule lands.**

A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it. — Gates 3/4 are satisfied here by explicit N/A + delegation to 0010, not by omission.

## Risks & rollback
- **Param order swap** (`$1`/`$2` actor vs org) re-leaks. Mitigation: header pins the order; `unread_count_is_org_scoped` + `unread_count_is_actor_scoped` both must pass.
- **Call site lacks the org id.** Mitigation: source it from the same active-tenant axis the other Pauta query.sql call sites use (step 4); if the badge handler currently passes nothing, this fix surfaces that gap — fixing it is in scope.
- **0010 not yet merged** so no framework-level enforcement. Mitigation: the `sql_uses_positional_params` string test guards regression in-pilot meanwhile; index already orders 0011 as front-loadable and independent.

**Rollback:** `git revert` the Pauta commit — two edited files + one test; the prior (buggy) state returns. No framework state to unwind; the query is read-only.
