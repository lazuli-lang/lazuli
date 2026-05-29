# full-capsule — Knowledge Vault (seeded reference)

This is a **seeded reference vault** that exercises the `knowledge <sector>`
field and the five `VOCAB-KNOWLEDGE-*` doctor rules end-to-end, and stays
`lazuli doctor`-clean. It is the in-place, runnable instantiation of
`docs/proposals/knowledge-vault-structure.md`; the pristine empty skeleton
that `lazuli new` lays down lives at
`lazurite/templates/default/knowledge/`.

`knowledge/` is **first-class committed source** — the curated memory of the
project, the source of truth a feature points at with `knowledge <sector>`.
It is NOT `.lazuli/` (the gitignored cache, home only to the *derived* index).

The `customer` feature in `full-capsule.lzi` declares `knowledge lazuli-way`,
so this vault must contain a `lazuli-way/` folder (or
`VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` fires).

---

## What is seeded here

```
knowledge/
├── README.md                                  # this file (the conventions)
├── _inbox/                                     # gated-write draft staging
├── decisions/
│   └── 0001-knowledge-vs-attach-ctx.md         # ADR, tier: approved
└── lazuli-way/
    ├── 0001-wire-not-reimplement.md            # doctrine, tier: approved
    └── 0002-five-escape-hatches.md             # doctrine, tier: approved
```

Every exemplar is seeded at **`tier: approved`**, deliberately. The gate rule
`VOCAB-KNOWLEDGE-UNGATED-WRITE-001` only inspects `gold` docs and requires a
prior `draft` revision in git history; authoring straight to `gold` without
that history is the anti-`lixão` (anti-dump) signal it catches. Seeding at
`approved` keeps `gold`-only gates (UNGATED-WRITE, STALE, DUP-TOPIC) silent
while still demonstrating a real, trusted, cited corpus. (The scanner also
*skips* a doc with no git history — so an uncommitted vault stays clean
either way; `approved` is the belt-and-suspenders choice.)

Each `cites:` entry resolves to a real symbol in the IR (`customer`,
`customer.Customer`, `customer.CustomerNote`, `customer.list`) so
`VOCAB-KNOWLEDGE-DANGLING-CITE-001` stays silent — citations are how the
compiler catches a doc that drifts off a renamed symbol.

---

## Layout

```
knowledge/<sector>/NNNN-<slug>.md
```

`NNNN` is a zero-padded 4-digit **ordinal** — the migration-style join key
threading filename, `supersedes:`, and `cites: { kind: item }`. Ordering
within a sector is by ordinal, not date.

## Sectors (closed catalog, dev-extensible)

A sector is the slug in `knowledge <sector>` **and** the folder
`knowledge/<sector>/`. The catalog is closed + opinionated; open-vocabulary
knowledge devolves into a per-project dialect, the same failure the `.lzi`
grammar forbids in the domain.

### Core sectors

| Sector       | Captures |
|--------------|----------|
| `decisions`  | Why we decided: context, options table, choice, consequences (ADR). |
| `changes`    | A change is the lifecycle-bearing unit of work; its contents are the specs — requirement (PRD) / technical sketch (TECH-SPEC). Work-bearing — carries an optional `status:`. |
| `gaps`       | A framework gap: workaround, proposed primitive, disposition, doctor-code, wave. |
| `lazuli-way` | Doctrine / method: governing rules, protocols, escape-hatch discipline. Typically `gold`, pulled in slices. |

### Dev-extensible (domain sectors)

You **may** add a sector for your own project's domain (`billing`, `iam`,
`media`): create `knowledge/<sector>/` **and** reference it from `knowledge
<sector>` on a feature (the two are tied by SECTOR-UNKNOWN). A new sector
inherits the same closed frontmatter schema and tier lifecycle — you extend
the *axis* (which sectors exist), never the internal vocabulary. Slugs are
`kebab-case`, no vendor/product namespace.

---

## Frontmatter schema (the contract)

```yaml
---
# --- identity ---
title:   "Wire, not reimplementation"   # human title; matches the body H1
slug:    wire-not-reimplement           # kebab; matches filename minus NNNN
sector:  lazuli-way                      # MUST match the parent folder

# --- curation / confidence (CLOSED) ---
tier:    approved                        # draft | approved | gold | deprecated (deprecated is TERMINAL)
# status: implemented                    # OPTIONAL, work-bearing sectors only:
                                         #   planned | in-progress | implemented | blocked. ORTHOGONAL to tier.

# --- dates (ISO yyyy-mm-dd) ---
created: 2026-05-29
updated: 2026-05-29

# --- relations (optional) ---
# supersedes: 0007-old-slug              # the doc this replaces -> that doc becomes deprecated

# --- evidence (optional) ---
cites:                                   # see "Citations" below for the kind taxonomy
  - customer.Customer                    #   resolves against the IR symbol table
  - customer.CustomerNote

# --- facets (optional, FREE-FORM) ---
tags: [doctrine, wire]                   # free text; no global catalog

# --- decay (optional; REQUIRED when tier: gold) ---
# revalidate_by: 2027-05-29              # ISO date; a gold doc past this is STALE
# decay_profile: stable                  # stable | seasonal | volatile

# --- grading (ONLY on `evaluations` docs) ---
# score: 9.2
# threshold: 9.0
# passed: true
---
```

### Field reference

| Field           | Regime                                  | Required           |
|-----------------|-----------------------------------------|--------------------|
| `tier`          | **CLOSED** `draft\|approved\|gold\|deprecated` | yes         |
| `status`        | **CLOSED** `planned\|in-progress\|implemented\|blocked` — ORTHOGONAL to tier | work-bearing sectors only |
| `created`/`updated` | ISO `yyyy-mm-dd`                    | yes                |
| `supersedes`    | id (`NNNN-slug` or `sector/slug`)       | no                 |
| `cites`         | symbol refs (each classified by a CLOSED kind — see below) | no |
| `tags`          | **FREE-FORM** (no enum)                 | no                 |
| `revalidate_by` | ISO date                                | yes if `gold`      |
| `decay_profile` | **CLOSED** `stable\|seasonal\|volatile` | no                 |
| `score`/`threshold`/`passed` | numeric / bool             | `evaluations` only |

> **No `source` field.** Provenance (`human`/`ai`/`imported`) was dropped —
> near-constant in an AI-curated harness. `tier` carries the trust signal.

### Citations and EvidenceKind

Every citation is classified by a **CLOSED `EvidenceKind`**: `code` | `url` |
`item` | `issue` | `comment` | `doc`. A `code`/`item` cite names a symbol the
compiler knows — `VOCAB-KNOWLEDGE-DANGLING-CITE-001` resolves it against the
IR (`<feature>` or `<feature>.<symbol>`) and fires if it dangles. The seeded
exemplars cite `code`-kind symbols (`customer.Customer`, `customer.list`) as a
flat list — the form the scanner resolves today — and carry `doc`/`url`
evidence in the prose body. The fully-typed `cites: [{ kind: code, ref: ...}]`
object form is the design target the frontmatter template documents.

---

## Tier vs. status (orthogonal)

`tier` = knowledge confidence (trust). `status` = work progress, only on
work-bearing sectors like `changes`. A change can be `tier: approved` while
`status: in-progress`. `deprecated` is the terminal tier.

## Deprecation is DERIVED

No scan-and-move. A new doc declares `supersedes: <old>`, and the old doc's
tier becomes `deprecated`. There is **no `archive/` tree** — "obsolete" is a
tier, not a directory. `git` is the append-only dated history; the working
tree is the current gold projection.

## Gated write (anti-`lixão`)

Drafts stage in `_inbox/` as `tier: draft`, and are promoted into a sector
only when they pass four gates, each enforced by a rule:

| Discipline            | Enforcing rule |
|-----------------------|----------------|
| In-catalog            | `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` |
| Earned promotion (draft→gold in git) | `VOCAB-KNOWLEDGE-UNGATED-WRITE-001` |
| Durable (not past `revalidate_by`)   | `VOCAB-KNOWLEDGE-STALE-001` |
| Cited / resolvable    | `VOCAB-KNOWLEDGE-DANGLING-CITE-001` |
| Non-redundant (one gold per topic)   | `VOCAB-KNOWLEDGE-DUP-TOPIC-001` |

## Retrieval

By **sector** (the directory), by **tag** (frontmatter scan, AND-containment),
or by **context pack** (the future `context <name>` keyword — purpose + handoff
axes). The derived sqlite-vec index lives in `.lazuli/`, regenerated from this
vault when `grep` stops scaling.
