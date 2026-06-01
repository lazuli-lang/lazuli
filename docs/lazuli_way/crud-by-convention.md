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

## The overlay is the idiom for PRODUCTION CRUD (spec 0018)

Bare `conventions [crud]` is the *trivial* case — a resource whose create/update
need no per-command policy, no events, no default values, no field renames. Real
production CRUD almost always needs those, and the bare synth can reproduce
**none** of them. That is why Pauta's 84 hand-rolled commands were correctly
**0×** adopted: switching them to bare `[crud]` would have silently changed
authz, events, and defaults.

The fix is the **`crud` overlay** — a `crud` block authored right after the
`conventions [crud]` line. It carries per-effect (`create`/`update`/`delete`)
clauses the synth composes onto the generated commands **before lowering**, so
the emitted IR (and Go) is byte-identical to the equivalent hand-rolled command.
Nothing new is lowered: every overlaid command still maps to exactly one existing
`CommandEffect` shape (RULE-VOCAB-03).

Surface (the five overlay clauses, each optional):

```
resource Customer
  # ... fields ...
  soft_delete by
  conventions [crud]
  crud
    create
      policy @policy.edit                 # REPLACES the synth's authenticated default
      validate @validator.percentage      # 0..n custom validators
      input excludes situation, is_active, is_defaulter   # drop system/derived fields
      assign situation = prospect          # default literal
      assign is_active = true              #   "
      assign category = input.category_id  # field-rename mapping
      emits customer_created               # 0..n events
    update
      policy @policy.edit
      emits customer_updated
    delete
      policy @policy.remove
      emits customer_deleted
      # soft-delete-aware automatically (the resource has `soft_delete by`, spec 0015)
```

Merge semantics: overlay `policy` **replaces** the synth default; `validate` /
`emits` / `assign` **add** to the synthesized effect; `input excludes` **removes**
fields from the synth-generated input (and their auto `<f> = input.<f>` binding).
`validate` is Doctor-only on IR (it carries no command-IR weight, exactly as a
hand-rolled `validate @validator.*` does), so it never affects IR-equivalence.

### Pauta customer trio — before / after

**Before** — `create_customer` / `update_customer` / `delete_customer` are ~120
lines of hand-rolled boilerplate
(`app/features/customer_management/customer_management.lzi:331`), each re-typing
the input, the effect bindings, the policy, and the events.

**After** — the resource opts into `conventions [crud]` + the overlay above; the
three `command` blocks are deleted. The synth reproduces them with IR-equivalence
for the clauses the overlay covers (policy, default-literal assigns, field-rename
assigns, emits, soft-delete-aware delete). Where a hand-rolled command curates its
input beyond dropping a field — e.g. exposing `category_id: ID` instead of the
resource's `category: CustomerCategory` FK — that input *re-shaping* is outside
the overlay's `input excludes` (which only removes); such a command stays
hand-authored, or migrates once the input is curated to the resource shape. The
overlay is the lever for everything it CAN reproduce exactly; it never silently
changes behavior for the rest.

### Do NOT grow the overlay into a macro language

If an author reaches for clauses the overlay can't express (multi-step logic,
conditional effects), that is the signal they need a real `@fn` command — **not**
a bigger overlay. The overlay composes onto existing IR command shapes only; that
boundary is the RULE-VOCAB-03 guarantee, not a limitation to fix later.

## Enforced by

- `crud_synth_*` diagnostics (shipped today) — the `inspect` / diagnostic
  surface that reports a resource is eligible for, and is using,
  `conventions [crud]`.
- `VOCAB-CRUD-SYNTH-AVAILABLE-001` *(shipped — spec 0002)* — the `crud`
  synth run backwards. Fires on a resource that does NOT carry
  `conventions [crud]` but hand-rolls (by name) at least `create_<r>` +
  `update_<r>` matching the synth's own member names, naming the exact
  members it would replace and suggesting the convention. Advisory
  (`warning`, never gates); incremental (a partial create/update-only
  hand-roll still fires, scoped to what matches). Soft-delete carve-out:
  when the resource is `soft_delete` / `retention`-bound, the matched
  `delete_<r>` is dropped from the suggestion and stays explicit (the synth
  delete is hard — spec 0015). Opt out with
  `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001 — reason "..."`.
  **Spec 0018 upgrade:** when the matched hand-rolled commands carry
  per-command policy / emits / default-literal assignments (the production-CRUD
  shape, e.g. Pauta `create_customer`), the message now points at the `crud`
  overlay as the migration target — not just bare `conventions [crud]`.

See the proposal: `docs/proposals/ir-resource-conventions-crud.md` and the
overlay spec `.specs/changes/0018-crud-synth-overlay/`.
