# Lazuli

Lazuli is an experimental application metalinguage for describing product semantics once and compiling them into concrete targets.

The first architecture is intentionally split:

```txt
Rust  -> compiler, parser, IR, planner, LSP, CLI, MCP
Go    -> first backend target
React -> first frontend target
```

This repository currently contains a small MVP:

- `.lzi` parser
- semantic IR
- analyzer with basic validation
- Go backend code generator
- TypeScript/React frontend code generator
- CLI commands for `parse`, `compile`, `init`, and `lsp`
- VS Code syntax highlighting for `.lzi`

## Example

```lzi
feature customer
  purpose "CRM customers within an org. Tracks lifecycle status, ownership, and tier."

  domain
    resource Customer
      name: Text required
      email: Email required
      status: CustomerStatus = lead

    query list
      where
        org_id = ctx.user.org_id

  policies
    create role_admin
    read same_org

  command create
    input name, email
    policy create
    emits customer_created

  surface web admin
    view list Table
      source query.list
      columns name, email, status
```

## Commands

```bash
cargo run -p lazuli_cli -- parse examples/crm.lzi
cargo run -p lazuli_cli -- compile examples/crm.lzi --out generated/crm
cargo run -p lazuli_cli -- init examples/new-app.lzi
cargo run -p lazuli_cli -- lsp
```

Generated output is written under:

```txt
generated/crm/
  backend/
  frontend/
```

## VS Code Syntax Highlighting

The extension lives in `editors/vscode` and is intentionally grammar-only for now.

```bash
code editors/vscode
```

Press `F5` to run it as an Extension Development Host. It opens `examples/customer-capsule.lzi`.

If you have the repository root open instead, `F5` also works; the root launch config points at `editors/vscode`.

The highlighter currently follows the indentation-based Lazuli sketch: blocks by indentation, typed fields with `:`, defaults with `=`, transitions with `->`, short feature context (`purpose`, `non_goals`, and optional `context` override), semantic groups (`domain`, `policies`, `surface`, `extensions`), query/command/workflow/view declarations, typed extensions, rules, and events. The semantic graph is intentionally not authored in `.lzi`; it should be derived later by the compiler. This extension does not start the Lazuli LSP yet.

The fuller syntax fixtures are:

- `examples/full-capsule.lzi` for the full Customer capsule.
- `examples/customer.ctx.md` for the co-located prose context convention.
- `examples/linear-issue.lzi` as a pressure test for state transitions, self references, labels, filters, and custom UI blocks.

## Design Notes

- [Extension points](docs/extension-points.md)
- [Validation plan](docs/validation-plan.md)
