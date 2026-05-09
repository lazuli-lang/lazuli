# Lazuli Generation Contract

Lazuli generates runnable software only where the `.lzi` semantics are declarative.

```txt
Declarative -> generated implementation
Custom      -> generated contract + required implementation
Raw         -> generated wiring + required external file
Escape      -> registered, not governed
```

## Declarative

These constructs should generate working Go and React with the default adapters:

- `resource`
- `constraints`
- declarative `query`
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

## Custom

These constructs generate typed contracts and require source implementation:

- `extensions client`
- `extensions server`
- candidate `auth` adapters
- candidate `job runs ext.*`
- candidate `webhook verify ext.*`
- candidate event consumers that call `ext.*`

`lazuli generate --stubs` may create editable stubs in `features/<feature>/...`.

`lazuli check --strict` must fail if required custom implementations are missing.

## Raw

`query <name> raw` must have:

- `returns`
- visible `scope`
- `sql`

Lazuli wires the SQL file into generated query handlers, but it should not silently rewrite arbitrary SQL. Tenant and soft-delete boundaries must remain visible in the capsule.

## Escape

`escape_route` registers something Lazuli should know exists but should not own.

Lazuli should not generate policies, queries, views, migrations, or tests for escape routes unless a future adapter provides explicit support.

## Modes

Recommended CLI behavior:

```bash
lazuli generate
lazuli generate --stubs
lazuli check --strict
```

- `generate`: emits generated code for declarative parts and reports missing custom implementations.
- `generate --stubs`: also creates editable custom stubs.
- `check --strict`: fails on missing custom code, missing raw files, missing escape route files, unresolved policies, and adapter contract mismatches.
