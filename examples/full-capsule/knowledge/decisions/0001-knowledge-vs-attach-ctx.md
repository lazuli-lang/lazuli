---
title:   "knowledge <sector> is a vault pointer, not an attach_ctx sidecar"
slug:    knowledge-vs-attach-ctx
sector:  decisions
tier:    approved
created: 2026-05-29
updated: 2026-05-29
cites:
  - customer
  - customer.Customer
tags: [grammar, context, knowledge-vault]
---

# knowledge <sector> is a vault pointer, not an attach_ctx sidecar

## Context

A feature already has `attach_ctx "<path>"` — a single markdown sidecar that
anchors cold readers. When the knowledge-vault design landed, the question was
whether durable project knowledge should ride on `attach_ctx` (one file per
feature) or get a new field.

## Decision

Add a distinct scalar field, `knowledge <sector>`, naming a **sector** in the
committed `knowledge/<sector>/` vault. `attach_ctx` stays a 1:1 feature
sidecar; `knowledge` is a 1:many pointer into a curated, tiered, cross-feature
corpus. The `customer` feature here declares `knowledge lazuli-way`.

## Options considered

| Option | Pros | Cons | Chosen |
|---|---|---|---|
| Overload `attach_ctx` with a folder | No new keyword | Conflates a single context file with a tiered multi-doc vault; no place for tier/cites/decay | no |
| Expandable `knowledge { tier, relates, feeds }` block | Rich in-DSL metadata | Config-in-DSL; breaks "one way to say each thing"; tier/relations belong to the *document*, not the feature | no |
| Scalar `knowledge <sector>` (frontmatter carries the rest) | Minimal grammar (one bareword); document owns tier/cites/decay; git owns history | Needs the file-layer + doctor rules to add teeth | **yes** |

## Consequences

- Grammar adds exactly one scalar field; everything else (tier, supersedes,
  cites, decay) is frontmatter + git, validated by the `VOCAB-KNOWLEDGE-*`
  doctor family rather than by the parser.
- `customer.Customer` and the feature `customer` are anchorable from vault docs
  via `cites:`, so the compiler can catch a doc that drifts off a renamed
  symbol (the "memory is the compiler" property).
- The sector slug must back a real `knowledge/<sector>/` folder, or
  `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` fires.
