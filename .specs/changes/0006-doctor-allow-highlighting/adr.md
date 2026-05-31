---
id: 0006
title: doctor:allow highlighting — color the suppression directive in the grammar
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Highlight `doctor:allow` by replacing the flat `comments` rule with an ordered, most-specific-first pattern list

## Context
- The `comments` repository rule in `editors/vscode/syntaxes/lazuli.tmLanguage.json` (~lines 337–344) is one flat match: `{"name":"comment.line.number-sign.lazuli","match":"#.*$"}`. It is referenced from the top-level `patterns` and `include`d inside many block rules (`scope-block`, `tests-block`, …), so whatever shape it takes propagates everywhere comments are allowed.
- `doctor:allow` is the canonical lint-suppression directive (85 uses across 31 pilot files). It is semantically load-bearing — it suppresses a doctor finding and records *why* — yet renders identically to throwaway chatter.
- Precedent already exists: `scope-block` (~lines 808–828) scopes the `reason` keyword (`entity.name.function.statement.scope.lazuli`) and `include`s `#strings` for the quoted reason text. The grammar already knows how to dignify `reason`; suppressions just never got the same treatment.
- The `comments` rule sits AFTER the `_kw_generated_end` fence (~line 336), so it is hand-editable. The `#kw-*` rules above the fence are emitted by `cargo xtask gen-tmlanguage` and must not be touched; no regeneration is needed here.

## Decision
- **Replace the single flat `comments` pattern with an ordered `patterns` list, most-specific first.** TextMate tries patterns top-to-bottom and takes the first match, so the `doctor:allow` pattern must precede the catch-all `#.*$`:
  1. A `doctor:allow` match (a `begin`/`end` or single `match` with captures) that scopes: `#` → `punctuation.definition.comment.lazuli`; `doctor:allow` → `keyword.control.directive.lazuli`; `<CODE>` → `entity.name.tag.lazuli`; `reason` → `keyword.control.directive.reason.lazuli` (mirroring the `scope-block` treatment); and `include`s `#strings` for the quoted reason so the existing string scope applies.
  2. The existing catch-all `{"name":"comment.line.number-sign.lazuli","match":"#.*$"}` as the fallback for every plain comment.
- **Keep the change confined to the `comments` repository rule.** Because every block `include`s `#comments`, editing this one rule lights up `doctor:allow` everywhere (top-level and inside any block) with zero other edits.
- **Snapshot the behavior.** Add a `doctor:allow` line to a grammar test fixture under `editors/vscode/tests/grammar/` and regenerate the `.snap` via `npm run test:grammar:lazuli` (which runs `vscode-tmgrammar-snap --updateSnapshot`). The `:check` variant then guards against silent regressions.

## Alternatives considered
- **A TextMate injection grammar (separate scopeName injected into `source.lazuli`)** — rejected: heavier machinery for a single comment shape; the repository-rule edit is local, reviewable, and already the established pattern (`scope-block`).
- **Semantic tokens via the LSP instead of TextMate** — rejected for this spec: TextMate static highlighting works without the server running and is where every other token already lives. Semantic tokens would be a parallel, redundant surface. (Agent-first parity is unaffected: the CLI/doctor already names the directive in plaintext; this is purely the IDE static view.)
- **Match the whole directive as one new comment subscope without breaking out captures** — rejected: the value is in distinguishing `<CODE>` and the `reason` text; a single opaque scope wouldn't let a theme color the code differently from the chatter.
- **Regenerate `#kw-*` to add `doctor:allow`** — rejected and unnecessary: `doctor:allow` is a comment-borne directive, not a statement keyword; it lives below the generated fence by design.

## Consequences
**We accept:** one more pattern in the `comments` rule and a small ordering invariant (most-specific first) that a future editor must preserve; a new `.snap` baseline to maintain.
**We gain:** suppressions read as first-class directives everywhere comments are allowed, consistent with how `scope override` already treats `reason`; reviewers and agents can spot a newly-added or un-reasoned allow at a glance.
**We watch:** if the `doctor:allow` regex drifts from the `allow_comment.rs` matcher (spec 0007), the grammar could highlight a form the doctor doesn't honor (or vice-versa). The `comment-hygiene.md` idiom doc names both surfaces so they stay aligned; 0007 owns the parser side.
