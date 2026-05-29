---
title:   "Wire, not reimplementation"
slug:    wire-not-reimplement
sector:  lazuli-way
tier:    approved
created: 2026-05-29
updated: 2026-05-29
cites:
  - customer.Customer
  - customer.CustomerNote
tags: [doctrine, wire, escape-hatch]
---

# Wire, not reimplementation

Lazuli's runtime layer is **wire, not reimplementation**. When you write an
authored Go handler or adapter, you do NOT reimplement primitives that already
exist in the Go stdlib or a mature library. Each adapter is ~10-50 LOC of
`import + call`, never 200-800 LOC of homegrown logic.

## The concrete test

Open the handler you just wrote and count its external imports
(`github.com/...`, `golang.org/x/...`). If it is over ~100 LOC, has **zero**
external imports, and the capability exists in any well-known library, you are
violating the principle — rewrite it as wire, or delete it and call the
library directly.

## Why it is doctrine

The framework owns the generic 80% (routing, registration, policy expansion,
migration ordering). The authored 20% is app-specific glue. A handler that
re-grows a CSV parser or an argon2 implementation is re-paying a cost the
ecosystem already paid, and it hides bugs the library already fixed. In this
example, `customer.Customer` and the import pipeline behind `customer.CustomerNote`
lean on typed effects and library adapters rather than bespoke code.

This doc is the load-bearing rule the other escape-hatch guidance defers to;
it is intentionally short and `approved` rather than `gold` so the gated-write
discipline (a draft must precede gold in git history) is never bypassed.
