# Lazuli VS Code Extension

Adds Lazuli `.lzi` language support:

- Syntax highlighting for feature capsules
- Bracket and comment configuration

## Development

This is intentionally syntax-only for now. Open this folder in VS Code and press `F5` to run it as an Extension Development Host, or package it with `vsce` later.

No Lazuli language server is started by this extension yet.

The sample syntax follows the indentation-based capsule sketch:

```txt
feature name
  purpose "short product reason"

  non_goals
    "thing this feature does not own"

  domain
    enum Kind
      one

    resource Thing
      name: Text required

    constraints
      unique name per org_id

    query list
      where
        org_id = ctx.user.org_id

    rule "human text"
      forbid Invoice.create where thing.deleted_at != nil

    event created
      id: ID

  policies
    read same_org

  command create
    input name
    policy read
    emits created

  workflow status
    archive: active -> archived
      policy read

  surface web admin
    view list Table
      source query.list
      columns name

  extensions
    client status_cell: CellRenderer[Thing]
    server before_create: Hook[CreateThing]
```

Current consistency rules:

- Blocks are opened by indentation, not `do`/`end` or braces.
- Fields and typed extension contracts use `name: Type`.
- Defaults use `=`, e.g. `status: Status = active`.
- Transitions use `->`, e.g. `archive: active -> archived`.
- `<feature>.ctx.md` sits next to the capsule by convention. `context` is only an override for non-standard locations.
- Context files are source, not generated frontend/backend output.

Highlighting groups:

- Structural constructors: `feature`, `resource`, `enum`, `query`, `command`, `workflow`, `view`, `rule`, `event`, `escape_route`.
- Layer sections: `domain`, `surface`, `extensions`.
- Section containers: `constraints`, `policies`, `params`, `where`, `cells`, `non_goals`.
- Internal statements: `input`, `policy`, `emits`, `forbid`, `message`, `source`, `columns`, and similar verbs.
