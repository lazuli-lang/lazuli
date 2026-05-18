# lazuli_ir changelog

Per-crate companion to the top-level workspace [`CHANGELOG.md`](../../CHANGELOG.md).
Tracks shape additions to the IR types exposed by this crate. The
crate-version axis (`Cargo.toml`) and the `LZIR_SCHEMA` JSON-ABI axis
(`src/lib.rs`) are owned by the release manager and bumped separately;
this file records the additive surface that will land under the next
release bump.

## [Unreleased]

### Added

- **IR Error-Vocab Cell IR-1 (2026-05-18) — typed translation key
  references + feature-level `errors` block lowering**, additive only.
  Implements
  [`docs/proposals/ir-error-messages-vocab.md`](../../docs/proposals/ir-error-messages-vocab.md)
  §3 + §11 Cell IR-1.

  New shapes:
  - `TranslationKeyRef { key: String, span_ref: Option<SpanRef> }` —
    typed `@translation.<key>` reference complementing the legacy
    `Rule.message_ref: Option<String>`. The legacy slot stays
    untouched; v2 migrates rules onto this struct.
  - `ErrorExposureDefault { Hide, Expose }` — closed-catalog
    `default hide` / `default expose` resolution for the lowered
    `errors` block. Serialized in snake_case.
  - `FeatureErrors { default, exposure_4xx, exposure_5xx, messages,
    field_messages, span_ref }` — lowering of the `feature.errors`
    block. Subsumes the pre-existing LSP-only `errors default hide /
    expose client 4xx ...` validation by giving it a real IR slot.
    `field_messages` is a reserved-for-v2 slot (parser unwired in v1).
  - `FeatureErrorMessage { code: String, message: TranslationKeyRef,
    span_ref }` — one `<code> message @translation.<key>` row.
  - `FeatureFieldError { resource, field, code, message, span_ref }` —
    reserved slot for v2 per-field validator-error references.

  New optional fields (all `Option<…>`, `#[serde(default,
  skip_serializing_if = "Option::is_none")]` so pre-vocab fixtures
  deserialize unchanged):
  - `Feature.errors: Option<FeatureErrors>`
  - `PolicyCategory.when_denied: Option<TranslationKeyRef>`
  - `Command.policy_when_denied: Option<TranslationKeyRef>`
  - `ListQuery.policy_when_denied`, `LookupQuery.policy_when_denied`,
    `SqlQuery.policy_when_denied` (the three `Query` variants)
  - `Api.policy_when_denied`
  - `Webhook.policy_when_denied`
  - `Job.policy_when_denied` (reserved — v1 codegen ignores)
  - `Channel.policy_when_denied` (reserved — v1 codegen ignores)
  - `Workflow.policy_when_denied` (reserved — v1 codegen ignores)
  - `Agent.policy_when_denied` (reserved — v1 codegen ignores)

  No fields removed or renamed. `LZIR_SCHEMA` not bumped here; the
  release manager pairs the version bump with a migration recipe per
  `docs/release-policy.md`.

  Round-trip coverage: `tests/error_vocab_round_trip.rs` constructs a
  `Feature` with every new field populated, serializes via serde to
  JSON, deserializes back, and asserts equality — plus targeted tests
  confirming that pre-vocab JSON fixtures deserialize with the new
  fields as `None`/empty.
