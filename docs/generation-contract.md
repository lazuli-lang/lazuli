# Lazuli Generation Contract

Lazuli generates runnable software only where the `.lzi` semantics are declarative.

```txt
Declarative -> generated implementation
Custom      -> generated contract + required implementation
SQL         -> generated wiring + required external file
Escape      -> registered, not governed
```

## Declarative

These constructs should generate working Go and React with the default adapters:

- `resource`
- `constraints`
- declarative `query.list` and `query.lookup`
- `policies`
- `command`
- `workflow`
- `event`
- `surface`
- simple `many`
- `tenancy`
- `soft_delete`

Example output:

```txt
Go handlers
repositories
validators
event structs
policy calls
typed TS client
React tables/forms/panels
```

Generated source should be structured for debugging, not bundled for distribution. The Go target emits many feature/category files and lets `go build` produce the final binary. The React target emits many feature modules and lets Vite/esbuild produce the browser bundle. Single-file generated Go or React output is not canonical because it destroys useful stack traces, incremental rebuild locality, and source-to-feature correspondence.

## Custom

These constructs generate typed contracts and require source implementation:

- `extensions client`
- `extensions server`
- `auth` adapters
- `job ... trigger ... handler`
- `webhook verify "<path>"` and `handler "<path>"`
- inline view blocks and resource validators declared with implementation paths

`lazuli generate --stubs` may create editable stubs in `features/<feature>/...`.

`lazuli check --security-profile strict` must fail if required custom implementations are missing.

## SQL

`query.sql <name>` must have:

- `returns`
- visible `scope`
- `sql`

The `returns` type must resolve to a local `record`, resource, or registered external adapter contract. Lazuli wires the SQL file into generated query handlers, but it should not infer result shape from SQL text or silently rewrite arbitrary SQL. Tenant and soft-delete boundaries must remain visible in the capsule.

## Escape

`escape_route` registers something Lazuli should know exists but should not own. It must still declare `at`, `policy`, and `tenant` so the generated route manifest has a visible security envelope.

Lazuli should not generate queries, views, migrations, or internal tests for escape routes unless a future adapter provides explicit support.

## Modes

Recommended CLI behavior:

```bash
lazuli generate
lazuli generate --stubs
lazuli check --security-profile strict
```

- `generate`: emits generated code for declarative parts and reports missing custom implementations.
- `generate --stubs`: also creates editable custom stubs.
- `check --security-profile strict`: fails on missing custom code, missing SQL files, missing escape route files, unresolved policies, security omissions, and adapter contract mismatches.
