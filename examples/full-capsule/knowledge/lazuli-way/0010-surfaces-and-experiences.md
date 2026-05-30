---
title:   "Surfaces and experiences"
slug:    surfaces-and-experiences
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, lzx, ui, surface, experience, audience]
read_when: "writing .lzx — UI, views, surfaces, web/mobile, anchors, extends"
---

# Surfaces and experiences

UI lives in `.lzx`, not `.lzi`. **What** a screen shows is declared once,
platform-agnostically (the *experience*); **how** it paints on web vs mobile is declared
separately, per device (the *surface*). The abstract-then-projections split keeps
React/Expo-shaped assumptions out of code that must serve both.

## Three files, three concerns

A UI feature splits across up to three `.lzx` files, co-located with its `.lzi`:

- **`<feature>.lzx`** — *abstract experience*. Platform-agnostic: which views exist, what
  query each reads (`source`), what commands its `action`s fire, which anchor it exposes.
  No layout, columns, or render mode.
- **`<feature>.web.lzx`** — *web projection*. View kinds (`Table`, `SidePanel`, `Form`),
  `columns`, `cells`, `view_mode`, audience fan-out.
- **`<feature>.mobile.lzx`** — *mobile projection* (React Native / Expo). Same experience,
  recut: `List`, `Screen`, `fields`, `sections`.

`surface customer web` and `surface customer mobile` **must live in different files**
(`customer.web.lzx` / `customer.mobile.lzx`): the platform suffix is the protected segment
before `.lzx`, doctor-enforced. No `device` keyword exists — the *filename* is the device
selector.

## The abstract experience

Names views and wires each to its data and actions. Platform-free on purpose — it must
survive a hypothetical second runtime on a different stack.

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

Wiring grammar (exact):

- `source <feature>.query.<name>` binds a list/detail view to a query; `submit
  <feature>.command.<name>` binds a create/form view to a command. A view reads via
  `source` **xor** writes via `submit` — never both.
- `action <name> -> <feature>.command.<name>(args)` fires a command. Args are
  `route.<slot>` or `row.<field>` — never free-typed values. `route.id` requires a
  matching `route id: <Type>.ID` slot on the view, mirroring
  [command-and-query-anatomy](0007-command-and-query-anatomy.md).
- `opens detail(id: row.id)` is list→detail navigation: clicking a row opens the named
  view, passing `row.id` into its route slot.

## Render modes are platform-side, in the surface

The experience says "there is a `list` view"; the *surface* says how it paints and which
**render mode** the view kind selects:

| Web view kind | Mobile view kind | Shape |
|---|---|---|
| `Table` | `List` | tabular / scroll list of rows |
| `SidePanel` / `Detail` | `Screen` | single-record detail |
| `Form` | `Form` | input form bound to a `submit` command |

A list view can also offer *user-toggleable* render modes via `view_mode`
(`table` / `kanban` / `calendar` / `gallery`), plus specialised containers — `tabs`,
`wizard`, `tab_group derived_from <enum>`, `view.board lanes derived_from <enum>` (resolve
against declared enum fields). Richer primitives; reach for them only when the screen needs them.

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

`columns`/`fields` pick which resource fields show; `search`/`filter` expose query params
as UI controls; `cells <field> @client.<widget>` swaps a column's default renderer for an
app-owned client widget. Mobile, same experience recut:

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

`audience <name>` fans a surface out by viewer class — `admin`, `account`, `public`,
`sales`, whatever your project defines (**no** closed catalog). A top-level view in a
surface applies to all audiences; a view nested under an `audience` block applies only to
that audience, and the `audience` block may carry its own `policy` guard. Use single
`policy @policy.<name>` for a role-specific surface; the OR-list `policy [@policy.a,
@policy.b]` only when one screen serves several roles.

## Routes bind URLs to views

Top-level `route` declarations (in the app-named root `.lzx`) map a typed URL to a view on
a surface. This is where `lazy` loading and route-level lifecycle guards live:

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

`requires_lifecycle` / `requires_lifecycle_in` makes the view paint only once the
resource's row reaches an allowed lifecycle state. `requires_lifecycle_in Resource [s1, s2]`
is the canonical, grep-able allow-list form; `requires_lifecycle Resource = state` is the
exact-match shorthand. States must be real states in that resource's `lifecycle` (see
[lifecycle-not-workflow](0008-lifecycle-not-workflow.md)); empty `[]` makes the view
unreachable; an undeclared state is rejected. `lazy true` is admissible on `route` only,
never on a view.

## Anchors and `extends` — view extensibility (escape hatch #4)

A view that wants extension declares an `anchor` and lists who may attach via
`extensible_by`. A different feature then `extends` that anchor, dropping a client block
into a named `slot` — cross-feature UI composition without the host knowing the extender
exists. The fourth of the [five escape hatches](0002-five-escape-hatches.md).

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

To validate, the host view must declare **both** `anchor @anchor.invoice_detail` **and**
`extensible_by invoice_notes` — the pairing is the contract. An anchor without
`extensible_by` accepts no one; `extends` on an anchor you weren't granted is rejected. A
slot may position itself (`slot timeline after activity_timeline`) and narrow by
`platforms` / `audience`.

The host can pin the contract with an inline `tests` block — same `allows`/`denies` dialect
as `.lzi` tests — asserting which extensions it accepts:

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

The abstract experience must stay device-free. Wanting `columns`, `Table`, or a kanban
toggle in `<feature>.lzx` means stop — that belongs in the `.web.lzx` / `.mobile.lzx`
projection. The experience answers *what and why*; the device surface answers *how it
paints here*. That separation is why one feature serves web and mobile from a single source
of truth.

Authoritative spec: `docs/grammar.lzx.md`, `docs/audience-policy.md`,
`docs/canonical-semantics.md`.
