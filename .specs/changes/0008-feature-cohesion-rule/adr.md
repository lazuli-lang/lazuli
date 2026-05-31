---
id: 0008
title: Feature cohesion rule — graph-connectivity over LOC for "one feature, one capability"
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Cohesion is graph connectivity, not line count; the disconnected-cluster finding is non-waivable

## Context
- "One feature, one capability" is a house idiom with no enforcement. The only adjacent rule, `LZI-FILE-SIZE-001`, measures LOC — and the pilot audit shows LOC is uncorrelated with cohesion: legit-large cohesive files (`account.lzi` 591, `media_price_tables.lzi` 686) fire it; a tiny grab-bag (`platform.lzi` 170) sails under it while being the worst actual violation.
- Hostpoint already votes against the LOC rule with its feet: four `# doctor:allow LZI-FILE-SIZE-001` waivers (`catalog.lzi:5`, `account.lzi:4`, `payments.lzi:3`, `operations.lzi:2`), each saying some version of "cohesion outweighs the LOC tax." The waivers *are* the evidence that the signal is miscalibrated.
- A rule named `LZI-FEATURE-COHESION-001` already exists (`feature_cohesion_001.rs`) but answers a different question: are multiple `feature` blocks packed in one file without a shared name prefix. It does NOT inspect resource relations. So the resource-graph cohesion check is a genuinely new, complementary rule — shipped as the sibling `LZI-FEATURE-COHESION-002`, leaving -001 untouched.
- The IR already carries the relation graph (FK fields, `has_many`, `on_delete`). A resource feature's cohesion is observable directly: build the undirected graph of intra-feature resources and count connected components. ≥2 components = the feature bundles independent capabilities. This is high-precision: `platform.lzi`'s `LegalDoc` / `PlatformConfig` / `DataRequest` are three isolated nodes; no honest waiver can claim they relate.

## Decision
- **Connectivity is the cohesion signal.** Ship `LZI-FEATURE-COHESION-002`: nodes = resources declared in the feature; edges = every FK field reference, `has_many`, and `on_delete` relation *between two resources of the same feature*. Fire (warn) iff the graph has ≥2 connected components. Report each component as a named cluster so the fix (which resources go to which new feature) is read straight off the diagnostic.
- **The disconnected-cluster finding is non-waivable-in-spirit.** `# doctor:allow LZI-FEATURE-COHESION-002` is still honored mechanically (we don't special-case suppression — that would break the uniform allow-comment contract from spec 0007), but the diagnostic body states: "you can't honestly waive this — these resources have no relationship; the fix is to split the feature." We enforce by message + grader convention, not by making the rule un-silenceable, because a hard-un-waivable rule is a footgun when the IR mis-models a relation.
- **Demote `LZI-FILE-SIZE-001` to a warning and re-key it off `(resource × effect)` count, not LOC.** A feature's real surface area is "how many distinct resources × how many distinct effects (command/query/job kinds) touch them," which is invariant to comments and formatting. LOC stops being the trigger; it survives only as informative metadata in the body. This kills the false positives that produced the four waivers.
- **Cross-feature heuristics stay info-level.** `uses` fan-out ≥4 → grab-bag *candidate* (info); cross-feature resource-name similarity ≥0.7 → duplicated-concern *hint* (info). These are softer (a hub feature can legitimately fan out; two features can legitimately have a `Config`), so they inform rather than warn.

## Alternatives considered
- **Keep LOC, just raise the threshold** — rejected: no threshold separates `platform.lzi` (170, a violation) from `account.lzi` (591, clean). LOC and cohesion are orthogonal; tuning one axis can't fix a wrong-axis measurement.
- **Make `LZI-FEATURE-COHESION-002` a hard error that ignores `doctor:allow`** — rejected: violates the uniform allow-comment contract (spec 0007) and is a footgun if the IR under-models a relation (e.g. a relation expressed only in a `query.sql` JOIN the graph can't see). Non-waivable-by-message gets the same teaching pressure without the footgun.
- **Weight edges / use a clustering threshold instead of strict components** — rejected as premature: strict connected-components is parameter-free and unambiguous; "2 weakly-connected clusters" needs a tuning knob we have no data to set. Revisit only if a real feature shows a thin-but-real bridge that should still split.
- **Drop `LZI-FILE-SIZE-001` entirely** — rejected: a genuinely cold-read-expensive file (even cohesive) is still worth a *warning* nudge toward per-file splits; we keep it, demoted and re-keyed, rather than delete the signal.

## Consequences
**We accept:** a second decomposition rule (two rules now speak to feature shape), and the `(resource × effect)` re-key means `LZI-FILE-SIZE-001` changes what it fires on — existing waivers may stop matching (that's the point; 0009 removes them). The connectivity graph is blind to relations expressed only in raw SQL (which 0010's rules separately flag), so a feature that's "connected" only through a `query.sql` JOIN could still fire — acceptable, since that itself is a smell.
**We gain:** a precise, formatting-invariant cohesion signal that catches `platform.lzi` and clears the four legit-large files; an enforcer for the `one-feature-one-capability` idiom; a driver + verifier for the 0009 splits.
**We watch:** if `LZI-FEATURE-COHESION-002` ever fires on a feature the team insists is cohesive, that's a signal the IR is under-modeling a relation — fix the graph builder, don't waive the finding.
