# Lazuli VS Code Extension

Adds Lazuli `.lzi` language support:

- Syntax highlighting for canonical feature capsules
- Bracket configuration

## Development

This is intentionally syntax-only for now. Open this folder in VS Code and press `F5` to run it as an Extension Development Host, or package it with `vsce` later.

No Lazuli language server is started by this extension yet.

The sample syntax follows the canonical indentation-based capsule sketch:

```txt
feature name
  purpose "short product reason"

  non_goals
    anti_pattern.unowned: "thing this feature does not own"

  uses org

  domain
    enum ThingStatus
      active
      archived

    resource Thing
      tenancy org
      name: Text required
      status: ThingStatus = active

    constraints
      unique name per org

    query list
      order created_at desc

    event thing_created
      thing_id: ID

  policies
    create: role_admin
    update: role_admin
    read: same_org

  command create
    creates Thing
    input name
    emits thing_created

  command rename
    params
      id: ID
    target thing = query.by_id(id: params.id)
    input name
    policy update

  workflow status on Thing.status
    policy update

    archive: active -> archived

  surface web admin
    view list Table
      source query.list
      columns name, status

  extensions
    client status_cell: CellRenderer[Thing]
    server before_create: Hook[CreateThing]
```

Current consistency rules:

- Blocks are opened by indentation, not `do`/`end` or braces.
- Fields, typed extension contracts, and command derivations use `name: Type` or `name = expression`.
- Required/optional is explicit in canonical resource fields.
- Defaults use `=`, e.g. `status: Status = active`.
- Transitions use `->`, e.g. `archive: active -> archived`.
- Required command fields that are not caller input should be explicit with `derive`.
- Commands that mutate an existing record should bind it with `target`.
- `<feature>.ctx.md` sits next to the capsule by convention. `context` is only an override for non-standard locations.
- Context files are source, not generated frontend/backend output.

Highlighting groups:

- Structural constructors: `feature`, `resource`, `enum`, `query`, `command`, `workflow`, `view`, `rule`, `event`, `webhook`, `job`, `auth`, `field_policies`, `extends`, `escape_route`.
- Layer sections: `domain`, `surface`, `extensions`.
- Section containers: `defaults`, `constraints`, `policies`, `params`, `key`, `scope`, `filters`, `cells`, `non_goals`.
- Internal statements: `event_payload`, `observability_only`, `creates`, `updates`, `deletes`, `input`, `derive`, `target`, `policy`, `emits`, `trigger`, `idempotency`, `retry`, `handler`, `validates`, `deny`, `message`, `source`, `submit`, `columns`, `fields`, `previously`, and similar verbs.
