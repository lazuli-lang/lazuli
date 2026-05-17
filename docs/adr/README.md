# Architecture Decision Records

This directory holds the canonical record of significant architectural
decisions in Lazuli. Each ADR captures the context, the decision, and
the consequences — both at the moment of decision and, in retrospect,
once the decision has aged.

ADRs are not changelogs. They exist to answer the question "why does
Lazuli work this way?" when the code alone doesn't make it obvious.
Commit messages tell you what changed; ADRs tell you why.

## When to write an ADR

Write an ADR when a decision:

- Cuts across multiple subsystems (codegen + CLI + doctor + runtime).
- Establishes or revises a project-wide convention (folder layout,
  naming, import paths).
- Reverses or supersedes a previous direction taken in earnest.
- Resolves a recurring debate by picking a side with reasons.

Don't write an ADR for routine changes — a bug fix, a new feature
within an existing surface, a refactor that doesn't change shape.
A useful test: if a contributor six months from now will hit this
decision and wonder "why?", write the ADR.

## Format

Follow [Michael Nygard's structure](https://github.com/joelparkerhenderson/architecture-decision-record/blob/main/locales/en/templates/decision-record-template-by-michael-nygard/index.md)
adapted for project tone:

- **Status**: `proposed` / `accepted` / `superseded by ADR-NNNN` / `deprecated`.
- **Context**: what's the situation that requires a decision? What
  constraints, prior choices, and forces are in play?
- **Decision**: what was decided. Direct, declarative.
- **Mechanics** (when non-trivial): how the decision is implemented —
  enough that a contributor can understand the pattern without diving
  into code.
- **Consequences**: what becomes true after the decision. Split into
  positive / negative / neutral when the trade-offs are real.
- **Alternatives considered**: what other paths were on the table, and
  why each was rejected. This is the section that ages best — future
  readers benefit most from understanding what was *not* chosen.
- **References**: commit SHAs, related ADRs, docs that elaborate.

Number ADRs sequentially starting from `0001`. Once accepted, don't
edit content — supersede with a new ADR that links back.

## Current ADRs

- [`0001-handler-home-and-portability-tiers.md`](0001-handler-home-and-portability-tiers.md)
  — establishes the three durability tiers (Portable / Client-specific
  / Disposable) and pivots Go handler ownership from `dist/go/` back to
  `app/features/` to restore the "dist is disposable" invariant.
