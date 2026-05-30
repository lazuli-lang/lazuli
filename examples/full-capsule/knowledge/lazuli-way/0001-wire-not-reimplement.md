---
title:   "Wire, not reimplementation"
slug:    wire-not-reimplement
sector:  lazuli-way
tier:    approved
created: 2026-05-29
updated: 2026-05-29
tags: [doctrine, wire, escape-hatch]
read_when: "writing an authored Go handler/adapter — before adding any logic"
---

# Wire, not reimplementation

The runtime layer is **wire, not reimplementation**. An authored Go handler/adapter does NOT reimplement primitives already in the Go stdlib or a mature library — it is ~10-50 LOC of `import + call`, never 200-800 LOC of homegrown logic.

## The test

Count the handler's external imports (`github.com/...`, `golang.org/x/...`). If it is >~100 LOC, has **zero** external imports, and the capability exists in any well-known library → violation. Rewrite as wire, or delete it and call the library directly.

## Why

The framework owns the generic 80% (routing, registration, policy expansion, migration ordering); the authored 20% is app-specific glue. Re-growing a CSV parser or argon2 impl re-pays a cost the ecosystem already paid and hides bugs the library already fixed. Lean on typed effects + library adapters over bespoke code.

This is the load-bearing rule other escape-hatch guidance defers to. It is intentionally short and `approved` (not `gold`) so the gated-write discipline (a draft must precede gold in git history) is never bypassed. Each app may promote it to `gold` once its own usage has battle-tested the rule.
