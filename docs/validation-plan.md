# Lazuli Validation Plan

The syntax is mature enough to validate against real product features.

Goal: find whether Lazuli covers the repetitive 70% while leaving rich domain behavior to typed extension points.

## Signal

- If 7 of 10 features need significant extension code, Lazuli is too narrow.
- If only 1 of 10 needs significant extension code, Lazuli is probably too broad.
- The target range is 2-3 features needing non-trivial extensions.

## Canonical Fixture Set

The current stable-ish fixtures cover:

1. `customer` — tenancy, soft delete, lifecycle workflow, raw report query, extension points
2. `issue` — team tenancy, self-reference, labels, workflow macro event, raw tree query

Tier 2 pressure fixtures now exist for:

3. `user-auth` — auth, password login, OAuth, MFA, sessions, rate limiting
4. `notification` — consumes events from other features and sends email
5. `billing` — Stripe-style inbound webhooks, invoices, retries, background jobs
6. `comment` — polymorphic comments and explicit target resources
7. `org-team` — tenancy hosts, membership, role inheritance, multi-surface access
8. `import-csv` — CSV upload, per-row validation, batch job, progress surface
9. `audit-log` — cross-cutting event capture and query-heavy surfaces
10. `field-permissions` — field-level read/write policy pressure test

The full audit fixture also includes capability-split customer satellites (`customer_auth`, `customer_tags`, `customer_import`, `customer_outreach`) to pressure cross-feature references, view composition, explicit deletes, and pure event-consumer features.

These pressure fixtures intentionally include candidate constructs that are not part of the parser MVP yet: `auth`, `webhook`, `job`, `on`, `field_policies`, and `extends`.

## Questions Per Feature

- Which parts fit declaratively?
- Which semantic group changes: `domain`, `policies`, `command`, `workflow`, `surface`, or `extensions`?
- Which parts need extension points?
- Are extension points small and typed, or are they replacing generated structure?
- Which custom files would an agent need to read for this feature?
- Which invariants are product rules rather than generated tests?
- Which semantic graph edges should be derived?
- Which change plans would protect data or user-facing behavior?

## Known Coverage Gaps

- Auth/login flows are under pressure in `user-auth`, but the construct is not stable core yet.
- Inbound webhooks are under pressure in `billing`, but the construct is not stable core yet.
- Scheduled jobs are under pressure in `notification`, `billing`, and `import-csv`, but the construct is not stable core yet.
- Event consumption is under pressure in `notification` and `audit-log`, but the construct is not stable core yet.
- Pure event-consumer features with no resources are under pressure in `examples/full-capsule.lzi`.
- Cross-feature view composition is under pressure in `examples/full-capsule.lzi`.
- Many-to-many with payload should use explicit join resources; this is under pressure in `comment` and `org-team`.
- Recursive hierarchies currently lean on raw queries.
- Cascaded soft delete across relations is not modeled.
- Multi-surface differences (`web admin`, `mobile`, `public`) need examples.
- Error semantics have a draft in `error-contract.md`, but need target adapter validation.
- Schema migration planning has a draft in `migrations.md`, but needs implementation pressure.

## Foundation Docs

- `generation-contract.md`
- `error-contract.md`
- `project-structure.md`
- `migrations.md`
- `testing-strategy.md`
