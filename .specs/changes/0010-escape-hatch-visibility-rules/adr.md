---
id: 0010
title: Escape-hatch visibility rules — raw SQL and tenancy contracts must survive a cold .lzi audit
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Escape hatches must be visible to a cold .lzi audit; raw-SQL-in-Go is waivable-to-convert, not waivable-to-silence

## Context
- The house principle: a cold read of the `.lzi` is the source of truth for effects, reads, policies, and tenancy. The `error-renderer-escape` anti-pattern doc establishes the house style for a sanctioned-but-invisible escape — it is documented as an escape, and the doc enumerates exactly which doctor diagnostics go silent because of it. The three escapes here are *not* sanctioned-and-documented; they are accidental holes that need to become visible.
- Three live pilot instances pin the failure modes: SQL hidden in a `@fn` Go handler (hostpoint trust `list_*` family); a `query.sql` whose binding style + header contradict the project's tenancy contract (pauta `unread_count.sql`); a `query.sql` with no tenant predicate guarded only by a comment (pauta `list_all_agencies.sql`).
- The detection surfaces already exist: doctor has a Go handler walker (`handler_walker.rs`) and reads feature `.lzi` IR; `query.sql` files + their `.lzi` `query.sql` blocks are both inspectable. So all three are statically detectable today.

## Decision
- **Ship three rules under an `escape_hatch` doctor category:**
  - `ESC-RAWSQL-IN-HANDLER-001` — AST-scan each `handlers/*.go` for `lazuli.DB().Query(` / `QueryRow(` with a multi-line string literal; cross-reference the feature `.lzi`. Fire when the corresponding effect is declared only as an opaque `fn ...: Function[...]` (no `query.sql`, no `returns`). The 6-JOIN read in `list_property_reviews.go:42-71` is the canonical fire.
  - `ESC-SQL-TENANCY-CONTRACT-001` — parse each `query.sql` for binding markers; fire if it mixes `:named` and `$N`, or references a param not declared in the `.lzi` `query.sql` block. `unread_count.sql` (`:user_id`/`:org_id` against a positional-`$N` project) is the canonical fire.
  - `ESC-SCOPE-OVERRIDE-UNGUARDED-001` — fire when a `query.sql` has no tenant predicate in its WHERE AND no `@actor.<privileged>` policy on the query. A SQL comment is not a guard. `list_all_agencies.sql` *passes* (it has `@actor.super_admin`); a sibling that dropped the guard would fire.
- **`ESC-RAWSQL-IN-HANDLER-001` is waivable-to-convert, not waivable-to-silence.** `# doctor:allow` is honored mechanically (uniform allow-comment contract, spec 0007), but the diagnostic body states the only correct resolution is to convert the read into a declared `query.sql`/`query.compose`; a bare silence is debt, not a fix. We enforce this by message + grader convention rather than making the rule un-silenceable, for the same footgun reason as `FEATURE-COHESION-001` (spec 0008): a hard-un-waivable rule is dangerous when the detector has an edge-case false positive.
- **The other two are ordinary warnings** (`ESC-SQL-TENANCY` / `ESC-SCOPE-OVERRIDE`) — a mis-declared or unguarded query is a contract bug to fix, but the detector is precise enough that a normal waivable warning is right.
- **Rules encode the *current* tenancy contract, read from convention, not hardcoded.** Positional `$N`, not auto-injected — derived from the project's prevailing `query.sql` style so a future auto-inject build doesn't make the rule lie.

## Alternatives considered
- **Ban raw SQL in handlers entirely** — rejected: `query.sql`/`query.compose` are sanctioned escapes; the problem is *hidden* or *mis-declared* SQL, not SQL. The rule targets invisibility, not the escape itself.
- **Make `ESC-RAWSQL` a hard error ignoring `doctor:allow`** — rejected: same footgun as a hard-un-waivable cohesion rule; "waivable-to-convert by message" gets the teaching pressure without the false-positive trap.
- **Lint the SQL comment for a justification instead of requiring `@actor` guard** — rejected for `ESC-SCOPE-OVERRIDE`: a comment is not machine-checkable as a guard and can drift from the actual policy. The `@actor.<privileged>` policy is the real, inspectable guard; require it.
- **Regex-grep handlers for `SELECT`** — rejected: too noisy (catches strings in tests, comments, migrations). AST-scan for the actual `DB().Query(` call site + cross-ref the `.lzi` is precise.

## Consequences
**We accept:** a new `escape_hatch` doctor category with three rules + a Go-AST cross-reference path (reusing `handler_walker.rs`). The rules fire on the pilots today (that's the point) — they will show as findings until 0011/0013 migrate the pilots; that backlog is intentional and tracked, not a regression.
**We gain:** the `.lzi` cold-audit promise is enforced — hidden multi-JOIN reads, contradictory tenancy contracts, and unguarded scope overrides become visible findings; the `escape-hatch-decision-tree.md` idiom gets real "Enforced by" rule codes instead of prose.
**We watch:** if `ESC-RAWSQL` false-positives on a legitimate non-read DB call (e.g. a vendor-required imperative statement), tighten the AST matcher (require a `Query`/`QueryRow` returning rows), don't relax the rule to silence-by-default.
