# customer — Context

## Why This Feature Exists

Customer is the CRM-facing identity for an organization account. It tracks lifecycle, owner assignment, and tier so sales, billing, and notifications can react to customer changes without owning the customer model.

## Guidance

- Keep lifecycle changes inside the existing `workflow lifecycle` block unless a new product state is being introduced.
- `risk_score` is a domain extension, not a stored field. Avoid persisting it unless a product invariant requires history.
- Do not fold invoice behavior into this feature. Customer can emit events that invoice features consume.
- Prefer adding a typed extension over widening the DSL when behavior is specific to one customer workflow.

## Performance Notes

- The normal list query is tenant-scoped and paginated.
- Lifetime value is intentionally raw SQL because it aggregates billing data outside the customer resource.

## Decision Log

- Customer events are intentionally small. Consumers should load the current customer snapshot if they need more than the event payload.
- `at:` is optional for extensions. Omit it when the implementation follows the feature-local convention.
