# Lazuli

Lazuli is an **AI-first declarative metalanguage** for product semantics. Author intent once in `.lzi` / `.lzx`, compile to working Go (backend), React (web), and React Native Expo (mobile). The language is designed so an LLM can read source cold and infer intent without external docs, and author source cold given a spec.

```txt
.lzi/.lzx (intent)  →  IR (typed semantic graph)  →  Go + React + Expo
        ↑                       ↑
   you author              compiler owns
```

**Lazuli** is the framework: language + IR + Rust compiler + Go/TS runtime libraries + CLI + LSP + MCP server. **Lazurite** is the opinionated distribution on top (folder conventions + `Lazurite.toml` manifest + `lazuli new` template). One distro shipped today; the design space supports others.

## What's in this repo

| Path | What |
|---|---|
| `crates/` | Rust workspace — parser, IR, analyzer, doctor, LSP, MCP, code generators, CLI |
| `runtime/go/lazuli/` | Hand-maintained Go runtime library (wire-thin adapters over Go stdlib + battle-tested libraries) |
| `runtime/ts/lazuli/` | Hand-maintained TypeScript runtime library (web + native conditional exports) |
| `editors/vscode/` | VS Code extension (syntax + LSP client) |
| `examples/` | Canonical fixtures — kitchen-sink + per-feature pressure tests |
| `lazurite/templates/` | Lazurite distro starter template |
| `migrations/recipes/` | Version-to-version migration recipes |
| `skills/audit/` | Portable LLM skill bundle for grading any `.lzi` against the canonical rubric |
| `docs/` | Framework specification (see [docs/README.md](docs/README.md)) |

## Example

```lzi
feature customer
  purpose "CRM customers within an org. Tracks lifecycle status, ownership, and tier."

  uses org, user

  domain
    enum CustomerStatus
      lead
      active
      archived

    resource Customer
      tenancy org
      name: Text required
      email: @semantic.Email required
      status: CustomerStatus = lead

    query.list list
      order created_at desc
      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id

      event created
        email: @semantic.Email

  policies
    create: @role.admin
    read: @scope.same_org

  command create
    input name, email
    policy @policy.create
    creates Customer from input
    emits customer_created

  surface web admin
    view list Table
      source query.list
      columns name, email, status
```

## CLI

```bash
cargo run -p lazuli_cli -- parse examples/crm.lzi
cargo run -p lazuli_cli -- check examples/full-capsule/full-capsule.lzi --security-profile strict
cargo run -p lazuli_cli -- compile examples/crm.lzi --out generated/crm
cargo run -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi --expand=all --format=json
cargo run -p lazuli_cli -- init examples/new-app.lzi
cargo run -p lazuli_cli -- lsp
```

Canonical fixture helpers:

```bash
powershell -ExecutionPolicy Bypass -File tools/generate-fixtures.ps1
powershell -ExecutionPolicy Bypass -File tools/generate-fixtures.ps1 -Check
```

## Reference fixtures

- **`examples/full-capsule/`** — kitchen-sink audit fixture for LLM cold-read. The `.lzi` contract plus sibling `.lzx` experience/projection files for web + mobile + admin + sales.
- **`examples/marketplace-mini/`** — smaller marketplace shape (`buyer` + `vendor` audiences).
- **`examples/marketplace-mini-mobile/`** — mobile-target reference (Expo Router scaffold).
- **`examples/multi-tenant-auth/`** — multi-tenant auth flow (email+password, tenant-scoped sessions, TOTP MFA).
- **`examples/customer.ctx.md`** — co-located prose context convention.
- **Pressure fixtures**: `customer-capsule.lzi`, `linear-issue.lzi`, `user-auth.lzi`, `notification.lzi`, `billing.lzi`, `comment.lzi`, `org-team.lzi`, `import-csv.lzi`, `audit-log.lzi`, `field-permissions.lzi`.

## VS Code extension

Lives in `editors/vscode/`. Grammar-only today (LSP client wiring in progress).

```bash
code editors/vscode
```

Press `F5` for an Extension Development Host. Opens `examples/customer-capsule.lzi`. The highlighter follows the canonical indentation-based syntax: blocks by indentation, typed fields with `:`, defaults with `=`, transitions with `->`, app/runtime contracts in `app.lzi`, typed routes + projections in `.lzx`, and explicit semantic groups (`domain`, `policies`, `command`, `workflow`, `surface`, `extensions`).

## Working rules for AI agents

[`CLAUDE.md`](CLAUDE.md) (mirrored at [`AGENTS.md`](AGENTS.md)) is the operating manual for any AI agent working in this repo. Read it before substantive work. Highlights:

- **Founding principle**: Lazuli is abstraction; the Lazuli Go runtime is **wire**, not reimplementation. ~10-50 LOC of `import + call` per adapter, never 200-800 LOC of homegrown logic.
- **Namespace policy**: `@runtime/<name>` for OSS commodity infrastructure; `@plugin/<name>` for vendor SaaS / paid APIs / specific named products (separate repos).
- **Grade-before-commit**: every design proposal grades against the 10-criterion rubric in [`docs/grading-rubric.md`](docs/grading-rubric.md). Gate at ≥ 8.5 with no individual dimension < 7.
- **Scope discipline**: see [`docs/scope-discipline.md`](docs/scope-discipline.md). Framework owns generics; specifics live in `@plugin/<name>` or app code. Boundary moves only with ≥ 3-app pilot evidence + an architect-graded proposal.

## Documentation

[`docs/README.md`](docs/README.md) is the navigation index. Quick paths:

- **Starting cold (LLM or human)** → [`docs/quickref.md`](docs/quickref.md), then [`docs/canonical-semantics.md`](docs/canonical-semantics.md) for the full spec.
- **Writing tools / alternative implementations** → [`docs/invariants.md`](docs/invariants.md) (enforceable contract) + [`docs/ir-abi.md`](docs/ir-abi.md).
- **Authoring a plugin** → [`docs/plugin-authoring.md`](docs/plugin-authoring.md).
- **Grading your own `.lzi`** → drop [`skills/audit/`](skills/audit/) into your Claude Code skills and run the audit.

## License

MIT. See [`LICENSE`](LICENSE).
