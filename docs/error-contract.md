# Lazuli Error Contract

Generated targets should expose consistent errors across Go, React, CLI, LSP, MCP, and tests.

## Runtime Error Kinds

| Kind | Typical HTTP | Source |
|------|--------------|--------|
| `validation_failed` | 422 | input/field validation |
| `policy_denied` | 403 | `policy` check |
| `rule_denied` | 409 | `rule deny ... when ...` |
| `not_found` | 404 | `key` lookup inside effective scope |
| `conflict` | 409 | unique constraint, workflow invalid transition |
| `sql_query_failed` | 500 | `query.sql` execution |
| `extension_failed` | 500 | custom extension error |
| `webhook_rejected` | 401/400 | signature/idempotency failure |
| `job_failed` | 500 | background job failure |

## Error Shape

Generated APIs should return structured errors:

```json
{
  "kind": "rule_denied",
  "code": "customer.reassign.archived",
  "message": "Cannot reassign an archived customer",
  "feature": "customer",
  "path": "domain.rule.archived_customers_cannot_be_reassigned",
  "source": "features/customer/customer.lzi:42"
}
```

## Source Mapping

Every generated error should point back to `.lzi` source where possible:

```txt
feature customer
path domain.rule."archived customers cannot be reassigned"
generated backend/customer/commands.go:118
```

Generated code errors should not force agents to debug generated files first. The source map points back to the capsule.

## Surface Behavior

React surfaces should receive the same error kind/code as the backend. UI adapters can map:

- `validation_failed` to field errors
- `policy_denied` to forbidden state
- `rule_denied` to business message
- `not_found` to empty/not-found panel
- `conflict` to retry or refresh prompt

The message in a `rule` is user-facing unless an adapter overrides localization.

## CLI inspect symbol-mode (`lazuli inspect <qualified-symbol>`)

Per `docs/proposals/lsp-symbol-origin.md` §5.4. When the inspect CLI is invoked in symbol-mode (per §5.3 lexical disambiguation), lookup failures are emitted as a soft error envelope on stdout with `exit 0`. Hard errors (parse failure on the module, IO error reading `.lzi` files) exit non-zero with stderr context.

Symbol-mode error codes:

| Code | Trigger | Resolution |
|---|---|---|
| `SYMBOL_NOT_FOUND` | The qualified symbol does not exist in the project's `SymbolOriginIndex`. | Confirm the spelling; if the symbol lives in another feature, qualify the lookup via `<feature>.<name>`. |
| `AMBIGUOUS_SYMBOL` | A bare-name lookup matches symbols in 2+ features. | Qualify the lookup. The `candidates` array in the envelope lists every match. |

Envelope shape:

```json
{
  "error": {
    "code": "SYMBOL_NOT_FOUND",
    "message": "no declaration named `Banana` in any feature of this project"
  }
}
```

```json
{
  "error": {
    "code": "AMBIGUOUS_SYMBOL",
    "message": "`Gender` is declared in multiple features; qualify the lookup as `<feature>.Gender`",
    "candidates": ["account.Gender", "host.Gender"]
  }
}
```

Soft-error exit policy matches `lazuli doctor` — findings are data, not failures. Scripts consuming the CLI parse the envelope and decide.
