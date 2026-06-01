# One feature, one capability

## Reach for this

Author each `feature` as a single capability whose resources actually relate to one another — if a feature's resources form two or more clusters with no relationship between them, it is bundling independent capabilities and should be split.

## Before (hand-rolled) / After (idiomatic)

**Before** — hostpoint `platform.lzi` bundles three unrelated resources under one feature name:

```lzi
feature platform
  resource LegalDoc        # terms / privacy documents
    title: Text required
    body: Text required
  resource PlatformConfig  # key/value runtime settings
    key: Text required
    value: Text required
  resource DataRequest     # GDPR export / erasure requests
    email: Text required
    status: Text required
```

`LegalDoc`, `PlatformConfig`, and `DataRequest` share **no** FK, `has_many`, or `on_delete` edge — the resource graph is three isolated nodes (0 edges). At 170 LOC it sails under any line-count threshold, yet it is the worst cohesion violation in the pilot: three capabilities wearing one feature's name.

**After** — split into three single-capability features (spec 0009):

```
platform.lzi  →  legal.lzi          (LegalDoc + its commands/queries)
              →  data_requests.lzi  (DataRequest + its lifecycle)
              →  feature_flags.lzi  (PlatformConfig)
```

Each resulting feature is one connected component: a single capability you can read cold without untangling unrelated concerns.

## Enforced by

`LZI-FEATURE-COHESION-002` — fires (Warning) when a feature's intra-feature resource graph (nodes = resources; edges = FK fields + `has_many` + `on_delete` between two resources of the same feature) has ≥2 disconnected components. The finding is non-waivable-in-spirit: `# doctor:allow LZI-FEATURE-COHESION-002` is honored mechanically, but you can't honestly waive "these resources have no relationship" — the only real fix is to split the feature.

(The sibling `LZI-FEATURE-COHESION-001` catches the related-but-distinct anti-pattern of multiple `feature` blocks packed in one `.lzi` without a shared name prefix. `LZI-FILE-SIZE-001` is the demoted cold-read nudge, re-keyed off distinct `(resource × effect)` pairs rather than raw LOC — it no longer fires on legit-large-but-cohesive features.)
