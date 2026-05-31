---
id: 0004
title: Defaults Hoist — defaults rate_limit + defaults audit
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Hoist rate_limit + audit into the existing `defaults` block, reusing the policy_for inheritance rule

## Context
- The `defaults` block keywords live in `crates/lazuli_keywords/src/registry/sections/s11.rs` (~line 218, "defaults-block: project-default modifiers": `tenancy`/`timestamps`/`soft_delete`/`retention`, `Context::Defaults`); the `policy_for` defaults key lives in `s05.rs`'s sibling `s08.rs:31` (also `Context::Defaults`). An `audit` block context already exists (`Context::Audit`, s11.rs ~line 243).
- The hoist precedent is live: `defaults policy_for commands:` is used at hostpoint `app/features/traveler/traveler.lzi` and `app/features/host/host.lzi`. Inheritance with per-command override is an established, taught pattern.
- The pilot audit found ~445 duplicate lines of `rate_limit`/`audit` across pauta + hostpoint with near-zero variation — the largest mechanical repetition in the corpus.

## Decision
- Add `rate_limit` and `audit` as feature-level `defaults` keys, using the **same inheritance rule already established for `policy_for`/`tenancy`/`timestamps`**:
  - `defaults rate_limit "<spec>"` and `defaults audit default` declare a feature default.
  - Each command inherits the default unless it sets its own value (per-command value wins).
  - `audit off` on a command opts that command out of the inherited default.
- `rate_limit` stays a **string** spec; `audit` keeps its existing keyword shape. The string→struct axis is explicitly out of scope (PRD non-goals; index defers it).
- Codegen resolves the effective per-command value as `command_value.or(default_value)`, with `audit off` clearing the inherited default, so emitted Go is byte-identical to the fully-explicit form.
- `lazuli doctor` gains one hint rule: a feature spelling an identical `rate_limit` or `audit` on ≥3 commands → suggest the hoist (deep-links `docs/lazuli_way/feature-defaults.md`).

## Alternatives considered
- **Bundle the string→struct redesign in the same change** — rejected: couples a mechanical, zero-risk hoist to a contested data-model change; ships slower and risks the migration. Deferred (index).
- **A separate top-level `rate_limits:` / `audit:` block** instead of reusing `defaults` — rejected: fragments the hoist surface; `policy_for` already set the precedent that hoisted modifiers live in `defaults`.
- **Doctor auto-rewrite (codemod) instead of a hint** — rejected for this change: hint first, prove the idiom; a codemod can follow.
- **Lower the ≥3 threshold to ≥2** — rejected: two identical commands are common and benign; ≥3 is where a hoist clearly pays for itself without noise.

## Consequences
**We accept:** one more pair of keys in the `defaults` surface; codegen must thread effective-value resolution for two more modifiers; the migrate cell is `parallel_safe: false` (pilot `.lzi` contention).
**We gain:** one inheritance rule for every hoisted modifier (`tenancy`/`timestamps`/`soft_delete`/`retention`/`policy_for`/`rate_limit`/`audit`) — no new mental model; ~445 lines deleted across the pilots; silent drift between copies eliminated; the 0001 seed `note` upgraded to demonstrate the idiom.
**We watch:** if authors start wanting per-command `rate_limit` variation often, that is the signal to revisit the deferred string→struct axis.
