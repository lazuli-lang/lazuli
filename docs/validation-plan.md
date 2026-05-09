# Lazuli Validation Plan

The syntax is mature enough to validate against real product features.

Goal: find whether Lazuli covers the repetitive 70% while leaving rich domain behavior to typed extension points.

## Signal

- If 7 of 10 features need significant extension code, Lazuli is too narrow.
- If only 1 of 10 needs significant extension code, Lazuli is probably too broad.
- The target range is 2-3 features needing non-trivial extensions.

## Feature Set

1. Basic CRM customer feature
2. Invoice with status workflow
3. Webhook integration with retry
4. Dashboard with three aggregate charts
5. CSV import with per-row validation
6. Automatic audit log
7. Multi-step wizard form
8. Field-level permissions
9. Field rename with data migration
10. Many-to-many relationship with payload

## Questions Per Feature

- Which parts fit declaratively?
- Which semantic group changes: `domain`, `policies`, `command`, `workflow`, `surface`, or `extensions`?
- Which parts need extension points?
- Are extension points small and typed, or are they replacing generated structure?
- Which custom files would an agent need to read for this feature?
- Which invariants are product rules rather than generated tests?
- Which semantic graph edges should be derived?
- Which change plans would protect data or user-facing behavior?
