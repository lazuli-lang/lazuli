# Lazuli VS Code Extension

Adds Lazuli `.lzi` and `.lzx` language support:

- Syntax highlighting for canonical feature capsules
- Syntax highlighting for app manifests, routes, experiences, and projections
- Bracket configuration

## Development

This is intentionally syntax-only for now. Open this folder in VS Code and press `F5` to run it as an Extension Development Host, or package it with `vsce` later.

No Lazuli language server is started by this extension yet.

The sample syntax follows the canonical indentation-based capsule sketch:

```txt
app ExampleApp
  uses
    thing
  targets
    backend go
    web react
  environments
    local
  runtime
    unit api
      serves queries, commands
  deploy
    migrations before_deploy

feature name
  purpose "short product reason"

  non_goals
    out_of_scope
      unowned: "thing this feature does not own"

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
    input name
    policy create
    creates Thing
      name = input.name
    emits thing_created

  command rename
    route id: ID
    input
      name: Text
    target query.by_id(id: route.id)
    policy update
    updates Thing
      name = input.name

  workflow status on Thing.status
    policy update

    archive: active -> archived

experience thing
  imports thing

  view list
    source thing.query.list

surface thing web
  uses experience thing

  audience admin
    view list Table
      columns name, status

  extensions
    client status_cell: CellRenderer[Thing]
    server before_create: Hook[CreateThing]
```

Current consistency rules:

- Blocks are opened by indentation, not `do`/`end` or braces.
- Fields, typed extension contracts, and assignment blocks use `name: Type` or `name = expression`.
- Required/optional is explicit in canonical resource fields.
- Defaults use `=`, e.g. `status: Status = active`.
- Transitions use `->`, e.g. `archive: active -> archived`.
- Resource writes should use assignment blocks under `creates` or `updates`.
- Route/context values used by commands should be explicit with `route`.
- Commands that mutate an existing record should bind it with `target`.
- `<feature>.ctx.md` sits next to the capsule by convention. `context` is only an override for non-standard locations.
- Context files are source, not generated frontend/backend output.

Highlighting groups use standard TextMate scopes so IDE themes can style them
without Lazuli-specific colors:

- Structural constructors: `app`, `registry`, `feature`, `experience`, `route`, `resource`, `record`, `enum`, `query`, `command`, `api`, `workflow`, `view`, `rule`, `event`, `event_group`, `webhook`, `job`, `auth`, `extends`, `escape_route`.
- Layer sections: `domain`, `surface`, `extensions`.
- Section containers: `defaults`, `constraints`, `policies`, `errors`, `params`, `route`, `key`, `scope`, `filters`, `cells`, `payload`, `non_goals`, `delegated_to`, `out_of_scope`, `requires`, `targets`, `environments`, `urls`, `env`, `group`, `integrations`, `bindings`, `capabilities`, `architecture`, `services`, `communication`, `runtime`, `deploy`.
- Internal statements: `creates`, `updates`, `deletes`, `input`, `route`, `let`, `target`, `policy`, `emits`, `invalidates`, `trigger`, `idempotency`, `retry`, `handler`, `validate`, `validates`, `deny`, `permits`, `forbids`, `message`, `source`, `submit`, `columns`, `fields`, `sections`, `slot`, `platforms`, `previously`, `migrated`, `alias`, and app/runtime verbs such as `service`, `owns`, `exposes`, `publishes`, `consumes`, `credentials`, `propagate`, `serves`, `runs`, `healthcheck`, `migrations`, and `rollback`.
