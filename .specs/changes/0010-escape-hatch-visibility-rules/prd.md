---
id: 0010
title: Escape-hatch visibility rules — raw SQL and tenancy contracts must survive a cold .lzi audit
type: prd
stage: 10 of 17
status: ready
created: 2026-05-31
---

# PRD — Escape-hatch visibility rules

## Problem
Lazuli's promise is that a cold read of the `.lzi` files tells you the whole truth about a feature's effects, reads, policies, and tenancy. Three escape hatches break that promise today, each proven by a pilot:

1. **Raw SQL buried in a `@fn` Go handler.** Hostpoint `trust/handlers/list_property_reviews.go:42-71` runs a 6-table-JOIN review read as a Go string literal, declared in `.lzi` only as an opaque `fn list_property_reviews: Function[...]`. The whole `list_my_*` / `list_property_*` / `list_host_agenda` family does the same. To `lazuli inspect`, the exposure checker, and the LSP, these reads do not exist — there is no `returns`, no `query.sql`, no shape. The escape-hatch decision tree (0001) says raw SQL belongs in a declared `query.sql`/`query.compose`, never a Go string — but nothing enforces it.

2. **Self-contradicting tenancy contract in a `query.sql`.** Pauta `notifications/queries/unread_count.sql` binds `:user_id` / `:org_id` (named params) and its header claims the framework auto-injects them — while *every other* Pauta `query.sql` uses positional `$N` and documents "tenancy is NOT auto-injected in this build" (`admin_panel.lzi:55-57`, `dashboard.lzi:47`). One file silently contradicts the project's tenancy contract; the mismatch is invisible until a cross-tenant leak.

3. **Unguarded scope override.** Pauta `admin_panel/list_all_agencies.sql` has no tenant WHERE predicate at all (cross-tenant by design) and is justified *only in a SQL comment*. The intent is legitimate (super-admin oversight) but the guard — `@actor.super_admin` on the query — is the thing that must be present and checkable, not a prose comment.

## Why now (or why ever)
The whole AI-first thesis is that the `.lzi` is the cold-readable source of truth. An escape hatch that hides a multi-JOIN read, contradicts the tenancy contract, or drops the tenant filter without a visible guard turns the audit surface into a lie — exactly the failure class the `error-renderer-escape` anti-pattern doc warns about (an escape that "decouples the source-of-truth `.lzi` audit from the runtime"). These three are the live instances in the pilots. They back the `escape-hatch-decision-tree.md` idiom (0001 created it); without rule codes, that doc is unenforced prose.

## Outcome — done means
1. **`ESC-RAWSQL-IN-HANDLER-001`** (the important one): fires when a `@fn` Go handler contains a multi-line SQL string literal (via `lazuli.DB().Query(` / `QueryRow(`) but the feature's `.lzi` declares the corresponding effect only as an opaque `fn ...: Function[...]` (no `query.sql` / no `returns`). Detected by AST-scanning `handlers/*.go` for the DB-query call and cross-referencing the feature `.lzi`.
2. **`ESC-SQL-TENANCY-CONTRACT-001`**: fires when a `query.sql` mixes named (`:x`) and positional (`$N`) binding, OR references a param the `.lzi` block doesn't declare.
3. **`ESC-SCOPE-OVERRIDE-UNGUARDED-001`**: fires when a `query.sql` has no tenant predicate AND no `@actor.<privileged>` policy on the query (a comment justification does not count).
4. The `docs/lazuli_way/escape-hatch-decision-tree.md` (created by 0001) gains the three rule codes in its "Enforced by" lines.
5. Scaffold `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` reference the rule codes alongside the decision tree.

## Non-goals
- **Migrating the pilots off the escape.** Converting the hostpoint `list_*` family to `query.sql`/`query.compose` is spec 0013 (actor-relative `query.compose`); fixing `unread_count.sql` is spec 0011 (HIGH, already filed). 0010 ships the *rules* and runs them to confirm they fire on the live instances.
- Banning raw SQL. `query.sql` is a sanctioned escape — the rule fires only when the SQL is *hidden in Go* (`ESC-RAWSQL`) or *mis-declared* (`ESC-SQL-TENANCY` / `ESC-SCOPE-OVERRIDE`), never on a well-declared `query.sql`.
- Tenancy auto-injection design. The rules encode the *current* contract (positional `$N`, not auto-injected); changing that contract is out of scope.

## User stories
- As an auditor cold-reading a feature, every multi-JOIN read is a declared `query.sql`/`query.compose` with a shape — or `ESC-RAWSQL-IN-HANDLER-001` is telling me one is hiding in Go.
- As an agent writing a `query.sql`, if I mix `:named` and `$1` or reference an undeclared param, `ESC-SQL-TENANCY-CONTRACT-001` catches the contract violation before it ships a cross-tenant bug.
- As a security reviewer, a query with no tenant filter must carry a visible `@actor.super_admin` guard, not a comment; `ESC-SCOPE-OVERRIDE-UNGUARDED-001` enforces that.

## Constraints
- `ESC-RAWSQL-IN-HANDLER-001` is **non-waivable-to-silence, only waivable-to-convert**: a `# doctor:allow` is honored mechanically, but the diagnostic body states the only correct resolution is to convert the read to a declared `query.sql`/`query.compose` — silencing without converting is recorded as debt, not a fix.
- AST scan of `handlers/*.go` must recognize `lazuli.DB().Query(` / `QueryRow(` and a multi-line string literal; cross-ref the feature's `.lzi` for a matching declared effect.
- The named-vs-positional check must read the project's contract from the existing `.lzi` comments / convention (positional `$N`), not hardcode a preference that would false-positive a future auto-inject build.

## Open questions
None. Detection mechanics and severities decided in the ADR.
