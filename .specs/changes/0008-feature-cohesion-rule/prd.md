---
id: 0008
title: Feature cohesion rule — graph-connectivity over LOC for "one feature, one capability"
type: prd
stage: 8 of 17
status: ready
created: 2026-05-31
---

# PRD — Feature cohesion rule

## Problem
The doctor's only "this feature is doing too much" signal today is `LZI-FILE-SIZE-001`, which counts raw lines. LOC is the wrong proxy for cohesion, and the pilot audit proves it fails in both directions:
- **False positives on legit-large single capabilities.** Hostpoint `account.lzi` (591 LOC) and Pauta `media_price_tables.lzi` (686 LOC) are big because the capability is genuinely big — one connected resource graph, not a grab-bag. The rule fires anyway. Hostpoint already carries `# doctor:allow LZI-FILE-SIZE-001` waivers to silence it on `catalog.lzi:5`, `account.lzi`, `payments.lzi`, `operations.lzi` — four hand-written "cohesion outweighs the LOC tax" reasons. A rule that everyone waives teaches nothing.
- **False negatives on small grab-bags.** Hostpoint `platform.lzi` is only **170 LOC** — well under any LOC threshold, so `LZI-FILE-SIZE-001` stays silent — yet it is the *worst* cohesion violation in the pilot: `LegalDoc`, `PlatformConfig`, and `DataRequest` are three resources with **no FK, no `has_many`, no `on_delete` edge** between them. Three independent capabilities wearing one feature's name.

The honest signal isn't "how many lines" — it's "do the resources in this feature relate to each other at all." A feature whose resource graph splits into ≥2 disconnected components is bundling independent capabilities, regardless of size.

## Why now (or why ever)
Spec 0009 (split hostpoint god-files) is blocked on a precise, non-LOC cohesion finding to drive and verify the splits — and to prove afterward that the pieces are cohesive. The `one-feature-one-capability` idiom doc (stub created by 0001) needs an enforcing rule to fill its "Enforced by" line, or it ships as unenforced prose. A LOC threshold that the canonical pilot waives four times is actively miscalibrated; shipping more language features on top of a feature-decomposition signal nobody trusts compounds the drift.

## Outcome — done means
1. New rule `LZI-FEATURE-COHESION-002` fires when a feature's intra-feature resource-relation graph (nodes = resources; edges = FK fields + `has_many` + `on_delete`) has **≥2 disconnected components** — i.e. there exist two resource clusters with no relational path between them.
2. The finding is effectively **non-waivable**: a `# doctor:allow LZI-FEATURE-COHESION-002` is honored mechanically (so a deliberate, documented exception isn't blocked), but the diagnostic body states plainly that you cannot honestly waive "these resources have no relationship" — the only real fix is to split the feature. (Contrast `LZI-FILE-SIZE-001`, which is legitimately waivable for a cohesive large file.)
3. `LZI-FILE-SIZE-001` is **demoted to a warning** and re-keyed off a structural count — distinct `(resource × effect)` pairs — instead of raw line count, so it stops firing on legit-large-but-cohesive files and the four hostpoint waivers become removable (tracked, not done here).
4. Two info-level companion signals land: `uses` fan-out ≥4 → grab-bag candidate (info); cross-feature resource-name collision at ≥0.7 name similarity → duplicated-concern hint (info).
5. `docs/lazuli_way/one-feature-one-capability.md` (stub from 0001) is filled in the fixed idiom shape, citing `platform.lzi` as the before (disconnected) and the 0009 split as the after, naming `LZI-FEATURE-COHESION-002` as the enforcer.
6. Scaffold `CLAUDE.md.tmpl` + `AGENTS.md.tmpl` gain the one-line idiom bullet.

## Non-goals
- Performing the hostpoint splits — that is spec 0009. This spec ships the rule + idiom doc + scaffold bullet, and runs the rule against the pilots to confirm `platform.lzi` fires and the cohesive god-files (`account`/`payments`/`operations`/`media_price_tables`) do **not**.
- Auto-splitting or codemod. The rule reports; the human/agent splits.
- Removing the existing hostpoint `# doctor:allow LZI-FILE-SIZE-001` waivers — left for 0009's migrate cell once the files are split or the rule re-keyed below stops firing.
- Cross-feature *relation* analysis (FKs that cross feature boundaries via `uses`). The graph is intra-feature; the fan-out and name-collision signals are the cross-feature heuristics, kept info-level on purpose.
- Replacing or merging with the existing `LZI-FEATURE-COHESION-001` (multiple features per file without a shared name prefix). That rule stays as-is; this is its resource-graph **sibling** `LZI-FEATURE-COHESION-002`. Do not touch `feature_cohesion_001.rs`.

## User stories
- As an agent authoring a feature, when I bolt a second unrelated resource onto an existing feature, `LZI-FEATURE-COHESION-002` fires and tells me the two clusters share no relation — so I split before the grab-bag sets.
- As the 0009 executor, I run `lazuli doctor` on hostpoint and get a precise list of disconnected clusters per god-file to drive the split, then re-run to confirm each new feature is a single connected component.
- As a reviewer, I trust a passing cohesion check more than a passing LOC check, because it can't be satisfied by deleting comments.

## Constraints
- The graph builder reuses the IR relation model (FK fields, `has_many`, `on_delete` edges) — no new grammar.
- A single-resource feature is trivially one component → never fires (no false positive on small features).
- The `(resource × effect)` re-key for `LZI-FILE-SIZE-001` must be deterministic from the IR so the warning is stable across formatting changes.

## Open questions
None. Threshold (≥2 components) and severities decided in the ADR.
