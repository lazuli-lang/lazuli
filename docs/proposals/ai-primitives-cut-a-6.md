# Proposal: Cut A.6 — Tool result schema in `registry.lzi`

**Status**: Draft proposal. Depends on Cut A
(`docs/proposals/ai-primitives-v0.md`) registry-side IR; lands as a
small follow-on, ideally bundled with Cut A's registry-side cut.

**Owner**: TBD. **Target version**: same `LZI_LANG` minor as Cut A's
registry-side IR landing, or the next minor.

## Motivation

Cut A's `tools` block on `agent` lets an LLM dispatch first-party
capabilities (`<feature>.<kind>.<name>`) and adapter-provided tools
(`@tool.<name>`).

For first-party tools, the result schema is the underlying
capability's result type — `customer.query.by_id` returns `Customer`,
known to Lazuli. Doctor can check that a prompt or a downstream
`agent.flow step on classify.<branch>` references fields that exist
on that result.

For `@tool.*` adapter tools, the result schema is **opaque**.
`@tool.web_search` returns *something* — a list of titles? URLs?
snippets? Lazuli does not know. The agent's prompt template may
reference `tools.web_search.title`, but doctor cannot validate that
field exists. The author writes it correctly, hopes for the best,
and discovers errors at runtime.

Cut A's `RegistryToolEntry` carries `effect` and `pii_classes`. The
result schema is the third field of the same surface. Adding it
costs almost nothing and pays back immediately: every LLM-authored
prompt that references `@tool.*` becomes statically checkable.

## Scope

- `result <Record>` child of `tool <name>` in `registry.lzi`.
- `RegistryToolEntry.result_record: Option<RecordRef>` IR field.
- Doctor diagnostic
  `agent_tool_result_field_unknown_diagnostics` for prompt /
  evals references to non-existent tool result fields.
- PII propagation from tool result records: any `@pii.*` marker on
  a tool result field flows into `Agent.tools[].resolved_pii_classes`.

## Promotion gate

Cut A.6 lands when **at least one `@tool.*` adapter is in use AND
at least one agent's prompt or evals references a result field of
that tool**. Wiring the tool is not enough; the result schema only
earns its weight once an author writes `tools.<name>.<field>` and
expects doctor to validate it.

The gate is evidence-shaped, not pressure-shaped: until a real
prompt or eval references the result, the schema is unused metadata.
Once one product writes such a reference, the schema becomes load-
bearing for every agent that calls the tool.

## Syntax

In `registry.lzi`:

```lazuli
tools
  tool web_search
    effect read
    pii_class behavioral
    result Record
      title: Text required
      url: Text required
      snippet: Text required
      published_at: DateTime optional

  tool calendar.create_event
    effect write
    result Record
      event_id: ID required
      confirmation_link: Text required

  tool http.fetch_get
    effect read
    pii_class external
    result Record
      status: Integer required
      body: Text required
      content_type: Text required
```

Inline `result Record` form mirrors the existing `record` declaration
in `contract.lzi` (`docs/grammar.contract.md §4`), reusing the same
field shape: `<name>: <type> [@<marker>...] required|optional`.

## Rules (normative)

- **Header**: `result Record` opens an indented block of fields.
  Each field follows the canonical field shape: `<name>:
  <type> [@<marker>...] required|optional`.
- **Type catalog**: same scalars as elsewhere (`ID`, `Text`, etc.).
  `@semantic.*` and `@cap.*` types allowed. Cross-feature record
  references (`<feature>.<RecordName>`) are not allowed in this cut
  — the registry must not depend on feature graph order. Keep tool
  results self-contained.
- **PII markers**: each field may carry `@pii.<class>` markers.
  These are merged with the tool's top-level `pii_class` list to
  form the full class set; the field-level marker is more specific
  (e.g., the tool may declare `pii_class external` and a field may
  declare `email: Text @pii.contact required`, which is allowed and
  results in `{external, contact}`).
- **Optional**: `result` is optional. A tool without a `result`
  declaration is treated as `result Record { /* opaque */ }`. Doctor
  cannot check field references on opaque tools but does not warn
  by default; `lazuli check --strict-tools` emits
  `tool_result_opaque_warning` for adapter tools without a declared
  result.

## Doctor diagnostics

| Id | Severity | Source |
|---|---|---|
| `agent_tool_result_field_unknown_diagnostics` | error | A6 |
| `tool_result_opaque_warning` | warning (only under `--strict-tools`) | A6 |
| `tool_result_pii_class_unknown_diagnostics` | error | A6 |

`agent_tool_result_field_unknown_diagnostics` fires when:

- An agent's `prompt` references `tools.<tool>.<field>` and `<field>`
  is not declared on the tool's `result Record`.
- An agent's `evals` reference `tools.<tool>.calls.<field>` and
  `<field>` is not declared.
- A discriminated `output` references a tool result discriminator
  that does not exist.

The diagnostic body lists the fields that *are* declared on the
result record so the author can correct the typo without a separate
lookup.

## IR delta

Extend `RegistryToolEntry` (already added in Cut A Phase 2,
`crates/lazuli_ir/src/lib.rs` near the existing registry shapes):

```rust
pub struct RegistryToolEntry {
    pub name: String,
    pub effect: ToolEffect,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pii_classes: Vec<QualifiedName>,
    pub adapter: Option<QualifiedName>,
    // NEW (Cut A.6):
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_record: Option<ResultRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

pub struct ResultRecord {
    pub fields: Vec<ContractField>,  // reuse the existing IR shape
}
```

The `ContractField` shape already exists at
`crates/lazuli_ir/src/lib.rs:78`:

```rust
pub struct ContractField {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requiredness: Option<String>,
}
```

Reuse is correct: tool result fields share the same shape as
contract record fields (name, type, markers list including
`@pii.<class>`, optional/required). The marker list is a flat
`Vec<String>`; doctor extracts `@pii.*` markers when computing
PII propagation rather than splitting them at IR-shape time. This
matches the existing IR convention and avoids premature axis-
splitting.

`LZIR_SCHEMA`: minor bump (additive). `LZI_LANG`: minor bump.

## PII propagation pass

The expand pass that populates `Agent.tools[].resolved_pii_classes`
(Cut A Phase 2 §4.3) gains one input: when resolving `@tool.<name>`,
walk the tool's `result_record.fields[].markers` and merge any
`@pii.*` markers into the resolved class set.

```rust
fn resolve_tool_pii_classes(
    tool: &RegistryToolEntry,
) -> Vec<QualifiedName> {
    let mut classes: BTreeSet<QualifiedName> =
        tool.pii_classes.iter().cloned().collect();
    if let Some(record) = &tool.result_record {
        for field in &record.fields {
            for marker in &field.markers {
                if let Some(pii) = parse_pii_marker(marker) {
                    classes.insert(pii);
                }
            }
        }
    }
    classes.into_iter().collect()
}
```

Cut A's `agent_pii_unsafetied_warning` benefits automatically: an
agent calling `@tool.contact_lookup` with a result schema declaring
`email: Text @pii.contact` and no `safety` validator triggers the
existing warning without any new diagnostic.

**Interaction with Cut A.5**: Cut A.5 introduces the lattice check
(`agent_safety_pii_coverage_gap_diagnostics`) that ensures the union
of declared `safety` validators *covers* the union of resolved PII
classes. Cut A.6 only widens that union by walking tool result
schemas. **A.5 is the coverage gate; A.6 is one of A.5's inputs.**
Both must land for the full check to be useful: A.6 alone widens
without enforcing; A.5 alone checks coverage but misses tool-result
PII unless A.6 has populated the resolved set.

## Inspect delta

`--expand=tools` (introduced in Cut A) includes `result_record`
when present:

```json
{
  "agent": "summarize_customer",
  "tools": [
    {
      "ref": "@tool.web_search",
      "effect": "read",
      "pii_classes": ["@pii.behavioral"],
      "result_record": {
        "fields": [
          {"name": "title",     "type": "Text", "presence": "required"},
          {"name": "url",       "type": "Text", "presence": "required"},
          {"name": "snippet",   "type": "Text", "presence": "required"}
        ]
      }
    }
  ]
}
```

`--expand=summary` continues to list tool refs without the result
shape; the schema appears only under `--expand=tools`.

## Why language, not pack

Three reasons:

1. The check cross-checks two language artifacts (an agent's
   prompt/evals reference and a registry's tool declaration).
   Pack-level checks would require the pack to re-implement
   prompt-reference resolution, which is doctor's job.
2. The IR field lives on a registry IR struct already in scope
   from Cut A. Adding a sibling field is the cheapest possible
   delta.
3. PII propagation reuses the closed `@pii.*` namespace. Putting
   PII coverage in a pack would fork the namespace check.

## Why not in Cut A

The architect's first re-grade explicitly rejected bundling beyond
the three core primitives. Cut A.6 stays a follow-on for the same
reason as Cut A.5: each cut keeps a single dominant concern.

## Acceptance criteria

- Cut A's `RegistryToolEntry` IR shape has shipped with `effect`
  and `pii_classes`.
- Pilot product confirmed using at least one `@tool.*` adapter
  tool consumed by an agent's prompt or evals.
- `result Record` block parses, lowers, and is honored.
- `agent_tool_result_field_unknown_diagnostics` implemented and
  tested with three cases:
  - reference exists (passes)
  - reference is a typo (errors with available-fields list)
  - tool has no `result` declared (no-op without `--strict-tools`)
- PII propagation pass extended; existing `agent_pii_unsafetied_warning`
  fires for tool-result PII fields without a separate diagnostic.
- `--expand=tools` reports `result_record` when present.
- `docs/grammar.registry.md §8` already sketches this; promote
  from sketch to "shipped" once implementation lands.
- `docs/invariants.md` registry section adds one bullet on
  `result Record` shape.
- `docs/design-decisions.md` records: *tool result schema lives on
  the registry entry, not on the agent's tool binding, because the
  shape is intrinsic to the tool — not the consumer.*

## Non-goals

- Cross-feature `result <feature>.<RecordName>` references. The
  registry must not depend on feature graph order. Authors who
  need cross-feature schema duplicate the relevant fields inline.
- Schema versioning per tool. If the adapter changes its result
  shape, the registry entry updates; tracking historical shapes
  is out of scope.
- Streaming result records. Tools today return a single record;
  streaming tools (e.g., a search that streams results) belong to
  Cut B's `flow` and adapter design.
- Optional / variant tool results (sometimes returns A, sometimes
  B). Use a `discriminator` field on the result record, the same
  shape Cut A introduces for `Agent.output`.

## Migration impact

Existing `@tool.*` declarations without `result`: zero impact,
treated as opaque, no warnings unless `--strict-tools` is enabled.

Agents whose prompts already reference tool result fields: zero
impact today (no validation), but as soon as the registry declares
the result schema, prior incorrect references become errors.
Recommended migration: ship Cut A.6 with `tool_result_opaque_warning`
under `--strict-tools` first, then promote to default after one
cut so teams can backfill declarations on adapter tools they own.

## Reserved

- Cross-feature `result <feature>.<Record>` references.
- Variant return types via discriminated union (would build on
  Cut A's discriminator IR).
- Tool result schemas declared inline on the agent's binding
  (overrides registry shape). Reserved for if real adapter
  collisions force per-binding overrides; today assume registry is
  authoritative.

## Release timing

Ship **one cycle after** Cut A's registry-side IR lands, not
bundled. Three reasons:

1. Cut A's registry-side delta (`RegistryToolEntry { effect,
   pii_classes, adapter, span_ref }`) is already load-bearing for
   three diagnostics on its own. Adding `result_record`
   simultaneously expands the registry surface ~50% LOC and mixes
   two distinct concerns (tool authorization vs. tool schema) in
   one review.
2. The promotion gate is evidence-gated. Until one pilot product
   wires `@tool.*` and references a result field, the schema is
   unused metadata. Bundling tempts skipping the gate.
3. Sequenced after, A.6 is purely additive: one optional IR field,
   one new diagnostic, one PII propagation extension. Zero risk
   to Cut A's chain (Phase 1 → Phase 2 → Phase 3 from the impl plan).

Cut A.5 (`safety` accepting a list) has stronger pre-existing
pressure than A.6, and they interact (see §"PII propagation pass"
interaction note). Recommended sequence: Cut A → Cut A.5 → Cut A.6.
Each lands only when its own gate fires.
