# CRUD by convention

## Reach for this

When a resource needs the ordinary create/update/delete commands, add
`conventions [crud]` to the resource instead of hand-writing one `command` block
per operation. The compiler synthesizes the create + update commands (and their
typed inputs/results) from the resource's fields, so they can't silently drift
out of sync with the schema.

## Before (hand-rolled) / After (idiomatic)

**Before** — a `command` per operation, each re-typing the resource's fields by
hand (drift risk: the command `input` and the resource diverge over time):

```
# hostpoint app/features/catalog/catalog.lzi:484
command create_property
  input
    name: Text required validate utf8_safe
    category: PropertyCategory required
  policy @policy.authenticated
  rate_limit "10 per 10 minutes per ip"
  returns Property
  audit default
  handler @fn.create_property

# hostpoint app/features/catalog/catalog.lzi:495 — 20+ input fields re-typed by hand
command update_property
  input
    property_id: ID required
    name: Text optional validate utf8_safe validate max_size: 160
    # ... 18 more fields mirroring Property ...
  policy @policy.authenticated
  rate_limit "30 per 10 minutes per ip"
  audit default
  handler @fn.update_property
```

Pauta-web is the cautionary tail: **84 hand-rolled** create/update/delete
commands across 13 features, `conventions [crud]` used **0×** — because nothing
taught the idiom. Closing that gap is the reason this whole loop exists.

**After** — declare the convention on the resource; create/update are generated
from the field set:

```
# hostpoint app/features/catalog/catalog.lzi:183
resource Property
  org: Org required
  host: Host required @owner_axis(through: user)
  name: Text required validate utf8_safe
  # ... fields ...
  conventions [crud]
```

The seed `note` feature shipped by `lazuli new`
(`lazurite/templates/default/app/features/note/note.lzi`) is the minimal worked
example.

### Caveat — delete stays explicit for now

`conventions [crud]` synthesizes create + update today. **Delete** still wants an
actor column (`deleted_by`) on `soft_delete` before it can be auto-generated
safely; that lands in **spec 0015** ([soft-delete.md](soft-delete.md)). Until
then, keep an explicit delete / soft-delete command where you need one.

## Enforced by

- `crud_synth_*` diagnostics (shipped today) — the `inspect` / diagnostic
  surface that reports a resource is eligible for, and is using,
  `conventions [crud]`.
- `VOCAB-CRUD-SYNTH-AVAILABLE-001` *(incoming — spec 0002)* — fires on a
  hand-rolled create/update command set that could be replaced by
  `conventions [crud]`, suggesting the convention.

See the proposal: `docs/proposals/ir-resource-conventions-crud.md`.
