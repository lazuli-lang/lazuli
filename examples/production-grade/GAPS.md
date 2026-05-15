# Production-Grade Fixture — Surface Gaps

This document enumerates every spot where the current Lazuli surface forced
us into a workaround (commented placeholder, handler escape, naming hack) to
make this fixture compile. Each entry links to the relevant proposal /
roadmap row in `docs/proposals/production-readiness.md`.

The fixture **passes** `lazuli doctor` today, but only because the
placeholders below are encoded as comments + Text fields. Once the
proposals land, each gap below should rewrite to a clean declarative
form and the corresponding handler escape disappears.

---

## G1 — RBAC catalog missing (`role` / `permission` declarations)

**Roadmap row:** gap #8 in `docs/proposals/production-readiness.md`.
**Pending proposal:** `docs/proposals/rbac-catalog-vocab.md`.

**Fixture references:**
- `features/auth/auth.lzi:60-87` — commented-out `roles` / `permissions`
  block showing the desired surface.
- `features/companies/companies.lzi:30-32` — `Membership.role` is a plain
  `Text` field instead of typed `Role`.
- `features/companies/companies.lzi:79-87` — `policies` block references
  `@role.platform_admin` / `@role.company_admin` with no catalog binding;
  doctor accepts the reference but nothing enforces the role exists.
- `features/credentials/credentials.lzi:60-65` — same pattern for
  `@role.company_admin`.
- `features/queries/queries.lzi:135-141` — same; cross-feature policy
  reference works only because doctor doesn't yet check `@role.*` against a
  catalog.

**Workaround:** every role mention is a string token; no completion, no
typo detection, no per-role permission catalog. Once `rbac-catalog-vocab`
lands the `roles` + `permissions` blocks in `features/auth/auth.lzi`
become first-class.

---

## G2 — Field-level encryption: missing key-management vocabulary

**Roadmap row:** gap #2 in `production-readiness.md`.
**Pending proposal:** `docs/proposals/encryption-vocab.md`.

**Fixture references:**
- `features/credentials/credentials.lzi:33-38` — `username`, `secret`,
  `token` use `@cap.Encrypted(key:@key.tenant)` from the cap catalog, but
  `@key.tenant` itself is a magic identifier with no declaration in the
  surface. There is no place to say "the tenant key comes from KMS X,
  rotates every Y days, fallback for decrypt is Z".

**Workaround:** the runtime contract is undefined — doctor accepts
`@key.tenant` because the cap catalog allows it, but generated code has
nowhere to look up the actual key source. The `runtime/go/lazuli/encryption/`
codex cells (`cdx-encryption-aes`, `cdx-encryption-test`) ship the
underlying AES-GCM primitive; surface still missing.

---

## G3 — CSV / XLSX export: no `query.export` or `kind report` surface

**Roadmap row:** gaps #3 + #4 in `production-readiness.md`.
**Pending proposal:** `docs/proposals/report-vocab.md`.

**Fixture references:**
- `features/queries/queries.lzi:191-209` — `command download_results` is
  declared as a plain command that `returns Text`. The block-comment shows
  the desired declarative surface (`export format csv, xlsx`).
- `features/queries/queries.lzi:237-238` — handler escape: two `fn`
  extension stubs (`export_results_csv`, `export_results_xlsx`) which in
  production would be 30-60 LOC each using `encoding/csv` /
  `github.com/xuri/excelize/v2`.

**Workaround:** must hand-author Go handlers for both formats and wire
them as `@fn` extensions, with no doctor/codegen support for the
report shape. Codex cells `cdx-report-csv` / `cdx-report-xlsx` provide
the runtime primitives; surface layer is the gap.

---

## G4 — Async polling cursor: no `kind poller` vocabulary

**Roadmap row:** gap #5 in `production-readiness.md`.
**Pending proposal:** `docs/proposals/poller-vocab.md`.

**Fixture references:**
- `features/queries/queries.lzi:69-92` — `resource PendingResolution` is
  hand-authored: `attempts`, `next_check_at`, `resolved_at`,
  `final_status`, `gender_retry_count`. Verbatim re-implementation of the
  v8_pending pattern in the source app.
- `features/queries/queries.lzi:120-130` — `query.list pending_resolutions`
  hand-encodes "find me rows where `resolved_at = nil AND next_check_at <
  ctx.now`" — the cursor query the poller loop reads.
- `features/queries/queries.lzi:217-225` — `job resolve_pending_provider_b`
  uses `trigger schedule "*/30 * * * * *"` (sub-minute cron) + a handler
  to drive the poll loop.

**Workaround:** entire poller pattern is one resource + one query +
one job + one handler, repeated per provider that needs async resolution.
The desired surface (per the proposal title) would name the pattern
once, e.g.:

```
kind poller resolve_provider_b on QueryResult
  cursor PendingResolution.next_check_at
  attempts max 12 backoff exponential
  final_status PendingResolution.final_status
```

---

## G5 — Plan + gate vocabulary: no command-gating by subscription plan

**Roadmap row:** gap #9 in `production-readiness.md`.
**Pending proposal:** `docs/proposals/plan-and-gate-vocab.md`.

**Fixture references:**
- `features/billing/billing.lzi:100-114` — commented `plans` catalog block
  showing the desired surface (quota per plan per command).
- `features/queries/queries.lzi:147-160` — `command start_query` has a
  block-comment placeholder for `gate billing.subscription.plan` directive.
  Today it has only `rate_limit` (per-user, not per-plan).
- `features/billing/billing.lzi:79-95` — `command upgrade_plan` updates
  the plan but has no way to declare downstream consumers that should
  re-evaluate.

**Workaround:** plan field is set as a plain enum on a Subscription
resource. Nothing in the surface ties it back to command access. Either
every command has to embed plan-check logic in its handler (escape) or
the policy `@policy.*` system has to be repurposed (boundary leak —
`@policy` is identity, not entitlement).

---

## G6 — Locale-aware scalar `@semantic.TaxID` deferred (pt-BR scalar pack)

**Roadmap row:** gap #12 in `production-readiness.md` (status: `⛔
deferred to post-pilot`).
**Decision source:** `project_validation_strategy_2026-05-14.md` —
locale scalars become an `@plugin/scalars-<locale>` kit post-pilot.

**Fixture references:**
- `features/companies/companies.lzi:14-17` — `tax_id` declared as plain
  `Text @pii.contact required unique` with comment pointing at gap #12.
- `features/companies/companies.lzi:97` — `command create_company` input
  uses plain `Text`.
- `features/queries/queries.lzi:32-33` — `Query.tax_id` plain `Text`.

**Workaround:** no validation, no formatting, no per-locale display.
Handler code parsing CPF/CNPJ etc. lives in `@fn.validate_tax_id` (not
written here; would be ~30 LOC of regex + checksum). Acceptable for
fixture; downstream products own validation handlers until the plugin
ships.

---

## G7 — `tenant_from` requires registry payload schema extension

**Status:** WORKAROUND APPLIED in this fixture (not a framework gap).
The `webhook_events.provider_callback` envelope in `registry.lzi:53-60`
was originally missing `company_id`. Without it, doctor emitted
`WEBHOOK-SCOPE-001` ("webhook does not declare `tenant_from`"). Added
`company_id: ID required` to the envelope per the production-grade-shape needs.

**Note:** in real production-grade, the callback URL embeds the company in a
signed param. The `tenant_from payload.<x>` model assumes the provider
sends the tenant id in the payload body. Acceptable for fixture.

---

## G8 — No declarative cache-with-TTL on webhook receipt

The proposal-meta-doc lists this as gap #13 in
`production-readiness.md` (status: `🟢` — runtime exists). The fixture
declares webhook handlers that should write to cache, but there is no
declarative surface tying a webhook to a cache key+TTL. Today the
handler imports the runtime cache bucket directly. This is acceptable
since the runtime exists; surface-side ergonomics could be a follow-up
but is not blocking.

---

## Remaining doctor warnings (informational)

The fixture passes `lazuli doctor` with 5 warnings, none of which point
to surface gaps:

1. `app-operational-contract` at `app.lzi:10` — false positive on `locale`
   block (the lint's whitelist is incomplete; not production-shaped).
2. 3× `env-schema-reference` for `PROVIDER_*_WEBHOOK_SECRET` — declared in
   `registry.lzi` but the LSP-tier rule is single-file. Doctor's
   cross-file check passes.
3. `workspace-boundary-contract` at `workspace.lzi:8` — minor pattern
   mismatch; safe to ignore for a single-app workspace.

---

## Summary — proposals that must land for this fixture to fully express
its intent

| ID | Gap | Status of authoring proposal |
|---|---|---|
| G1 | RBAC catalog | `rbac-catalog-vocab.md` — to draft (gap #8) |
| G2 | Encryption keys | `encryption-vocab.md` — to draft (gap #2) |
| G3 | CSV/XLSX export | `report-vocab.md` — to draft (gap #3) |
| G4 | Async poller | `poller-vocab.md` — to draft (gap #5) |
| G5 | Plan + gate | `plan-and-gate-vocab.md` — to draft (gap #9) |
| G6 | Locale scalars | deferred — `@plugin/scalars-pt-BR` post-pilot (gap #12) |

When the five Wave-1 proposals (G1–G5) land + their codegen ships, the
fixture under `examples/production-grade/` becomes the integration test that
proves each proposal's declarative form replaces a handler escape /
commented placeholder.
