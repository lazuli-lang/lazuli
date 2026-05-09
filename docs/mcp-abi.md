# Lazuli MCP ABI

The MCP server in `crates/lazuli_mcp` exposes Lazuli capsules to LLM agents. It is read-heavy by design. The single write surface is text DSL; agents never patch IR directly.

This document is the contract LLMs see. Together with `ir-abi.md`, it pins the surface that toolmakers and agent integrations build against.

## Tools

### Read-Only

- `get_feature(id)` — full IR projection of a feature, including authored and derived layers.
- `get_resource(qualified_name)` — single resource node.
- `explain_query(id)` — query node plus derived effective scope and operation table entry.
- `list_features()` — feature index with purpose lines.
- `search_capsule(query)` — string search across DSL source, returning matched features and spans.
- `get_dsl_source(feature_id)` — raw `.lzi` text of a feature, with byte ranges of each top-level node.
- `get_diagnostics(feature_id)` — current analyzer diagnostics for a feature.
- `get_context(feature_id)` — content of the colocated `<feature>.ctx.md`, if present.

These tools serve agent context. They never accept patches.

### Write

There is exactly one write tool:

- `write_dsl_feature(feature_id, new_text)` — replace the `.lzi` text of a feature.

The tool runs the full pipeline against `new_text`:

1. Parse → AST.
2. Lower → IR.
3. Analyze → diagnostics.

If diagnostics contain any `error`-severity entries, the file is **not** written and the tool returns the structured error list. The agent reads errors and retries. Warnings do not block the write.

There is no `patch_ir`, no `add_field`, no `set_policy`, no `rename_command`. Editing primitives belong to the DSL, not the IR.

`write_dsl_feature` does not run codegen as a side effect. Generation is `lazuli_cli compile`, invoked separately.

## Structured Errors

Every diagnostic returned to MCP follows this shape:

```json
{
  "code": "E_RULE_OPERATION_NOT_FOUND",
  "severity": "error",
  "feature": "customer",
  "span": {
    "line": 71,
    "col": 12,
    "end_line": 71,
    "end_col": 30,
    "byte_start": 1834,
    "byte_end": 1852
  },
  "message": "Rule references operation 'reassign' which does not exist on Customer.",
  "suggested_fix": "Did you mean 'archive'?",
  "related": [
    {
      "span": {
        "line": 110,
        "col": 4,
        "end_line": 110,
        "end_col": 19,
        "byte_start": 2890,
        "byte_end": 2905
      },
      "message": "Available operations on Customer: create, archive."
    }
  ]
}
```

### Fields

- `code`: enumerated `E_*` (error) or `W_*` (warning) identifier. Stable across patches; renames are major bumps.
- `severity`: `error` | `warning` | `info`.
- `feature`: feature name the diagnostic is anchored to.
- `span`: byte-precise location in the DSL source. Always present for parse and analyzer diagnostics.
- `message`: human-readable description, present-tense, no trailing period unless multi-sentence.
- `suggested_fix`: optional. Plain text the agent may quote in a follow-up edit.
- `related`: optional list of secondary spans with context.

### Code Convention

- `E_PARSE_*` — parser errors (unbalanced indentation, unexpected token).
- `E_TYPE_*` — type resolution errors (unknown type, wrong arity).
- `E_FIELD_*` — field-level errors (unknown field, duplicate, missing required).
- `E_QUERY_*` — query errors (filter on unknown field, scope override without policy).
- `E_RULE_*` — rule errors (operation not found, predicate ceiling exceeded).
- `E_POLICY_*` — policy errors (unresolved atom).
- `E_EXTENSION_*` — extension contract errors (unresolved path, unknown contract type).
- `E_WORKFLOW_*` — workflow errors (transition with unreachable enum value, missing transition for reachable value).
- `W_*` — warnings (redundant scope, deprecated syntax, unused param).

The `code` enumeration is part of the MCP ABI and versioned with `LZIR_SCHEMA`.

## Versioning

MCP tool schemas and the `code` enumeration follow `LZIR_SCHEMA` (see `ir-abi.md`):

- New tool, new optional argument, new error code → minor bump.
- Removed tool, removed argument, removed code, semantic change → major bump.

Older clients reading newer error codes treat unknown codes as opaque strings: display the message, ignore code-specific UI.

## What MCP Never Does

- Accept structured patches over IR nodes.
- Auto-format DSL on write. `new_text` is written as provided.
- Resolve extension implementations remotely. Filesystem only.
- Run codegen as a side effect of `write_dsl_feature`.
- Serve as a shell for arbitrary file edits. Only `.lzi` and the colocated `<feature>.ctx.md` of declared features are writable, and the latter through a separate `write_context` tool documented when it ships.

## Why DSL And Not IR

LLMs read and write text better than nested JSON. DSL carries comments, intent, and ordering that LLMs use as cues. DSL output is committable and reviewable as ordinary code. Forcing agents to manipulate IR loses all of that and recreates the dual-edit-path failure mode that visual low-code editors suffer from.

The AI-first thesis does not require IR editing. It requires DSL that is dense, declarative, and parseable. That is what `.lzi` delivers, and what every MCP write surface routes through.
