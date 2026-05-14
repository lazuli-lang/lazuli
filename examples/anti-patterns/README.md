# Anti-pattern fixtures

Fixtures in this directory are **not canonical**. They demonstrate retired,
ambiguous, or otherwise discouraged surface forms — kept as teaching tools
and as targets for the VOCAB-* lint catalog.

Do not copy these files when authoring new Lazuli source. Each entry below
links to its canonical replacement.

## Index

### `crm-aggregate-dialect.lzi`

Uses the legacy `aggregate <Name> { ... }` curly-brace dialect (grammar
alias `aggregate | entity` in `crates/lazuli_syntax/src/grammar.pest:6`).
The canonical form is the indentation-based `feature ... domain ...
resource` grammar documented in `docs/invariants.md` and required by the
project memory `project_lzi_syntax.md`.

Originally located at `examples/crm.lzi`. Audited 2026-05-13: **BLOCK @
3.24/10**. Seven of ten rubric dimensions scored below 6, primarily on
determinism (two grammar forms for one concept), AI-first readiness (an
LLM trained on this file would emit five distinct anti-patterns), and
semantic density (zero `@pii.*` tags on PII fields in a CRM).

Canonical replacement: `examples/crm.lzi`.

See `c:/tmp/claude-vocab-audit-crm.md` (transient) for the full audit and
`docs/proposals/doctor-vocabulary-lints.md` for the proposed
`VOCAB-GRAMMAR-FORM-001` lint that would auto-flag this form.
