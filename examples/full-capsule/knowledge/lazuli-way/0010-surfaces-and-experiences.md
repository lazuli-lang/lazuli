---
title:   "Surfaces and experiences"
slug:    surfaces-and-experiences
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, lzx, ui, surface, experience, audience]
---

# Surfaces and experiences

The UI layer lives in `.lzx`, not `.lzi`. The split is deliberate and it is the
single most-misunderstood thing about Lazuli's front-end model: **what** a screen
shows is declared once, platform-agnostically; **how** it paints on web vs mobile
is declared separately, per device. Get the file convention wrong and the doctor
will tell you, but the underlying mental model — abstract experience, then device
projections — is what keeps you from writing React-shaped or Expo-shaped
assumptions into a place that has to serve both.

## Three files, three concerns

A UI feature splits across up to three `.lzx` files, all co-located with the
feature's `.lzi`:

- **`<feature>.lzx`** — the *abstract experience*. Platform-agnostic view model:
  which views exist, what query each reads (`source`), what commands its
  `action`s fire, which anchor it exposes. No layout, no columns, no render mode.
- **`<feature>.web.lzx`** — the *web projection* (the "device" surface for web).
  Concrete layout: view kinds (`Table`, `SidePanel`, `Form`), `columns`,
  `cells`, `view_mode`, audience fan-out.
- **`<feature>.mobile.lzx`** — the *mobile projection* (the React Native / Expo
  device surface). Same experience, different render: `List`, `Screen`,
  `fields`, `sections`.

The same `surface customer web` and `surface customer mobile` for one feature
**must live in different files** (`customer.web.lzx` / `customer.mobile.lzx`) —
the platform suffix is the protected segment before `.lzx`, and the doctor
enforces the filename convention. This is why you never see a `device` keyword:
the *filename* is the device selector.

## The abstract experience

The experience names views and wires each to its data and actions. It is
platform-free on purpose — it must survive a hypothetical second runtime
targeting a different stack.

```lazuli
experience invoice
  imports invoice

  view list
    source invoice.query.list
    action create -> invoice.command.create
    opens detail(id: row.id)

  view detail
    route id: Invoice.ID
    anchor @anchor.invoice_detail
    policy @policy.edit
    requires_lifecycle Invoice = open
    source invoice.query.by_id(id: route.id)
    action settle -> invoice.command.settle(id: route.id)

  view create
    submit invoice.command.create
```

Read the wiring grammar carefully — it is exact:

- `source <feature>.query.<name>` binds a *list/detail* view to a query.
  `submit <feature>.command.<name>` binds a *create/form* view to a command.
  A view reads through `source` xor writes through `submit`; don't mix them.
- `action <name> -> <feature>.command.<name>(args)` fires a command. Arguments
  are `route.<slot>` (route context) or `row.<field>` (the selected row) — never
  free-typed values. `route.id` requires a matching `route id: <Type>.ID` slot on
  the view, exactly mirroring [command-and-query-anatomy](0007-command-and-query-anatomy.md).
- `opens detail(id: row.id)` is the list→detail navigation: clicking a row opens
  the named view, passing `row.id` into its route slot.

## Render modes are platform-side, in the surface

The experience says "there is a `list` view." The *surface* says how it paints,
and which **render mode** the view kind selects:

| Web view kind | Mobile view kind | Shape |
|---|---|---|
| `Table` | `List` | tabular / scroll list of rows |
| `SidePanel` / `Detail` | `Screen` | single-record detail |
| `Form` | `Form` | input form bound to a `submit` command |

Beyond the static view kind, a list view can offer a *user-toggleable* set of
render modes via `view_mode` (`table` / `kanban` / `calendar` / `gallery`), and
specialised containers — `tabs`, `wizard`, `tab_group derived_from <enum>`,
`view.board lanes derived_from <enum>` — that resolve against declared enum
fields. Those are the surface dialect's richer presentation primitives; reach for
them only when the screen genuinely needs them.

```lazuli
surface invoice web
  uses experience invoice

  audience admin
    policy @policy.edit

    view list Table
      columns number, amount, status
      search number
      filter status
      cells status @client.status_cell
      view_mode
        table
        kanban

    view detail SidePanel
      sections header, timeline

    view create Form
      fields number, amount
      submit invoice.command.create
```

`columns`/`fields` pick which resource fields show; `search`/`filter` expose
query params as UI controls; `cells <field> @client.<widget>` swaps a column's
default renderer for an app-owned client widget. The mobile projection is the
same experience, recut for the device:

```lazuli
surface invoice mobile
  uses experience invoice

  audience admin
    view list List
      fields number, status

    view detail Screen
      sections header, timeline
```

## Audiences gate who sees the surface

`audience <name>` fans a surface out by viewer class — `admin`, `account`,
`public`, `sales`, whatever your project defines (there is **no** closed
catalog). A top-level view inside a surface applies to all audiences; a view
nested under an `audience` block applies only to that audience, and the
`audience` block may carry its own `policy` guard. Use the single
`policy @policy.<name>` form for a role-specific surface; the OR-list form
`policy [@policy.a, @policy.b]` only when the same screen serves several roles.

## Routes bind URLs to views

Top-level `route` declarations (in the app-named root `.lzx`) map a typed URL to
a view on a surface. This is where `lazy` loading and route-level lifecycle
guards live:

```lazuli
route admin_invoice_detail
  path "/admin/invoices/:id"
  route id: Invoice.ID
  to invoice.view.detail(id: route.id)
  surface invoice web
  audience admin
  lazy true
  policy @policy.edit
    requires_lifecycle_in Invoice [draft, open]
```

The `requires_lifecycle` / `requires_lifecycle_in` guard makes the view paint
only once the resource's row has reached an allowed lifecycle state — the
allow-list `requires_lifecycle_in Resource [s1, s2]` is the canonical, grep-able
form; the exact-match `requires_lifecycle Resource = state` is the shorthand. The
states must be real states declared in that resource's `lifecycle` (see
[lifecycle-not-workflow](0008-lifecycle-not-workflow.md)); an empty `[]` makes the
view unreachable, and an undeclared state is rejected. `lazy true` is admissible
on `route` only, never on a view.

## Anchors and `extends` — view extensibility (escape hatch #4)

A view that wants to be extended by sibling features declares an `anchor` and
lists who may attach via `extensible_by`. A different feature then `extends` that
anchor, dropping a client block into a named `slot`. This is the fourth of the
[five escape hatches](0002-five-escape-hatches.md): cross-feature UI composition
without the host feature knowing the extender exists.

```lazuli
experience invoice_notes
  imports invoice_notes, invoice

  view notes
    source invoice_notes.query.list

  extends @anchor.invoice_detail
    slot aside
      block @client.note_editor
      platforms web, mobile
      audience admin
```

For this to validate, the host view must declare both `anchor @anchor.invoice_detail`
**and** `extensible_by invoice_notes`. The pairing is the contract: an anchor
without `extensible_by` accepts no one, and `extends` an anchor you weren't
granted is rejected. A slot may also position itself (`slot timeline after
activity_timeline`) and narrow itself by `platforms` / `audience`.

The host can pin the contract with an inline `tests` block — the same
`allows`/`denies` dialect as `.lzi` tests — asserting which extensions it accepts:

```lazuli
experience invoice
  imports invoice

  view detail
    route id: Invoice.ID
    anchor @anchor.invoice_detail
    source invoice.query.by_id(id: route.id)
    extensible_by invoice_notes

    tests
      allows extension invoice_notes
      denies extension billing
```

## The one rule to never violate

The abstract experience must stay device-free. If you find yourself wanting to
put `columns` or `Table` or a kanban toggle into `<feature>.lzx`, stop — that
belongs in the `.web.lzx` / `.mobile.lzx` projection. The experience answers
*what and why*; the device surface answers *how it paints here*. That separation
is the whole reason a single feature can serve web and mobile from one source of
truth.

Authoritative spec: `docs/grammar.lzx.md`, `docs/audience-policy.md`,
`docs/canonical-semantics.md`.
