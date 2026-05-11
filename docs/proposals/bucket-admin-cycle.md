# Bucket Cycle: Admin (design only — Cut admin gated)

**Run**: `/lazuli-bucket-cycle bucket=admin mode=design`
**Date**: 2026-05-11
**Status**: design-only. Implementation **gated on pilot SaaS evidence** per
`docs/audit/framework-coverage-1400.md:327` ("toda a seção admin é Cut admin
gated — gera muita gramática nova, requer pilot") and roadmap §4.5
(`docs/roadmap.md:661`). No `bucket-admin-scope.md` blocker exists; the
surface authored today (`audience admin` + `@role.admin` policies +
`/admin/*` routes) already reaches IR via the surface lift (`AudienceSurface`
at `crates/lazuli_ir/src/lib.rs:1820-1829`). This document records the
**design vocabulary** so when Cut admin opens, the language work has a
typed contract to land against.

## Contexto

Admin is the §1.23 row of the roadmap (`docs/roadmap.md:232-236`) and §25 of
the framework-coverage audit (`docs/audit/framework-coverage-1400.md:322-327`).
The audit grades the coverage matrix at `25 features / L=0 / DL=4 / DF=8 /
F=13` — every DF (forms, tables, filters, search, sorting, pagination, bulk
actions, charts, theme, menu builder, breadcrumbs, custom pages,
notifications) is runtime chrome; every DL (`admin_resource`,
`admin_dashboard`, `admin_action`, admin permissions, admin audit, admin
impersonation) is **language vocabulary that does not yet exist**.

What **does** exist at L1 today, with full IR carry-through:

- `audience admin` as a surface qualifier inside `.lzx` views
  (`examples/full-capsule/full-capsule.admin.web.lzx:6,25,38`) lowered to
  `AudienceSurface { name: "admin", ... }`
  (`crates/lazuli_ir/src/lib.rs:1820-1829`).
- Top-level `route` blocks with `audience admin`
  (`examples/full-capsule/full-capsule.lzx:32,39`) lowered to
  `AppRoute { audience: Some("admin"), ... }` (`lib.rs:1714`).
- `@role.admin` as a policy actor in 16 occurrences across the canonical
  fixture (`examples/full-capsule/full-capsule.lzi:16,202-218,275,...`).
- `escape_route "/admin/customer-debug"` + `escape_route
  "/admin/customer-imports"` at `examples/full-capsule/full-capsule.lzi:425`
  and `:791` — the **already-shipped escape hatch** for admin pages that
  don't fit the declarative surface.

What does **not** exist:

- A first-class kind that says "this resource is exposed in admin chrome"
  (today the contract is "any resource referenced by a surface with
  `audience admin`" — implicit, no doctor cross-check).
- A first-class kind that names a dashboard distinct from a regular view.
- A first-class kind that names a bulk action (export, import, custom) as
  distinct from a regular command.
- A typed cross-check between `audience admin` and `@role.admin` policy:
  the surface declares "this is admin-only" and the resource declares
  "writes need @role.admin", but doctor never asserts the two agree.

The L0→L2 cycle for admin **cannot run today** because there is no pilot
authoring it. This proposal is therefore a **language design** for what
the admin bucket looks like, ready to lower once a pilot SaaS exercises
it. No codegen, no runtime, no Lazuli Go subpackage. Cuts pilot-gated explicit.

The "closed cycle" gate for admin, when Cut admin opens, reads: a resource
declared `admin_resource Customer` in the fixture surfaces as a generated
admin CRUD chrome page that respects the resource's existing
`commands.policy`, `tenant_from`, and `audit` declarations without any
hand-written admin handler — and an authored `admin_action export` lowers
to a generated bulk export endpoint backed by the same `query`/`command`
contract the non-admin surface uses. Today the gate is open at L0 (surface
audience), and **everything else waits for pilot pressure**.

## Baseline

Inventario L0/L1/L2 dos constructs já presentes no fixture. `Surface` é
"lê do fixture canônico"; `Grammar` é "parser reconhece"; `IR` é "struct
dedicado em `lazuli_ir`"; `Doctor/LSP` é "diagnostic cross-checa";
`Codegen` é "`lazuli_codegen_go` produz arquivo Go"; `Runtime` é "o
runtime Lazuli Go executa".

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `audience admin` (in `.lzx` view) | yes (`full-capsule.admin.web.lzx:6,25,38`) | line-based (canonical-indent slice) | yes (`AudienceSurface` at `lib.rs:1820`) | partial (audience reachability against command policy — `crates/lazuli_lsp/src/lib.rs`) | no | no | L1 |
| `audience admin` on top-level `route` | yes (`full-capsule.lzx:32,39`) | line-based | yes (`AppRoute.audience` at `lib.rs:1714`) | partial (same audience reachability) | no | no | L1 |
| `@role.admin` policy atom | yes (`full-capsule.lzi:16,202-218,...`) | line-based | yes (`PolicyAtom` at `lib.rs:2689`) | yes (atom resolution + role catalog) | no | no | L1 |
| `escape_route "/admin/..."` | yes (`full-capsule.lzi:425,791`) | line-based | yes (`EscapeRoute`) | yes | partial (escape — author-owned file) | partial | L1 |
| `admin_resource <Resource>` kind | **no** | n/a | n/a | n/a | n/a | n/a | **missing** |
| `admin_dashboard <name>` kind | **no** | n/a | n/a | n/a | n/a | n/a | **missing** |
| `admin_action <name>` kind | **no** | n/a | n/a | n/a | n/a | n/a | **missing** |
| Admin permissions cross-check | implicit via `@role.admin` | n/a | n/a | **missing** | n/a | n/a | **missing doctor** |
| Admin audit (cross-resource audit feed) | implicit via per-command `audit` | n/a | n/a | n/a | n/a | n/a | **missing** |
| Admin impersonation kind | **no** (audit §1.8 lists `impersonation` as roadmap; not in fixture) | n/a | n/a | n/a | n/a | n/a | **missing — speculative** |

**Summary**: surface authoring of admin **is L1 today** through `audience`,
`route`, and `@role.admin`. What's missing is the **resource-level + action-
level declarative anchor** that turns "this exists in admin" from a surface-
side qualifier into a feature-side contract doctor can cross-check. That
gap is real, but **not load-bearing for the canonical fixture today** —
the existing admin views in `full-capsule.admin.web.lzx` work as far as
inspect, doctor, and surface lowering are concerned. The pressure to add
the kinds appears only when a pilot product needs **generated admin
chrome** (Cut admin F).

## Linguagem proposta

The bucket is **already L0-expressive** for what an admin surface actually
declares (audience, routes, policy). The proposed language work splits
into three groups:

### 1. PILOT-NEEDED (Cut admin entry point) — `admin_resource <Resource>`

The minimum viable admin kind. Names a resource as exposed in admin chrome
and declares the contract the generator reads.

```
feature customer

  admin_resource Customer
    audience admin
    columns name, email, lifecycle_stage, tier, score, owner, created_at
    search name, email
    filter lifecycle_stage, tier, owner
    detail sections header, activity_timeline
    actions reassign, archive
    policy @role.admin
    audit actor, target.id
```

Closed catalog of children:

- `audience <name>` — required; closed catalog from the surface audience
  catalog (`admin`, possibly `support` or other ops audiences a pilot
  surfaces). Must resolve to an audience declared on a `surface` in a
  sibling `.lzx`.
- `columns <a>, <b>, ...` — list of `Resource.field` references. Each
  must resolve.
- `search <a>, <b>, ...` — subset of columns (or any text-shaped field
  on the resource) that powers the chrome search box. Subset closure
  checked by doctor.
- `filter <a>, <b>, ...` — subset of columns, must be either enum-shaped
  or relation-shaped. Doctor catches "filter on a Decimal".
- `detail sections <a>, <b>, ...` — list of view section names that the
  detail surface composes. Cross-checked against the `.lzx` view
  declaration.
- `actions <command-name>, ...` — list of commands declared on the
  resource that surface as bulk/single buttons in chrome. Each must
  resolve to a `command` whose policy allows `@role.admin` (cross-check).
- `policy @role.admin` — optional override; defaults to `@role.admin`.
- `audit <fields>` — mirrors the existing `audit` child on commands
  (`docs/invariants.md:93`). Same closed catalog.

**Why this is minimal**:

- It carries **no rendering hints** (no `widget`, no `theme`, no `cell`
  override) — those are surface-side (`.lzx`) and codegen-side (the admin
  generator). The kind declares *what* is exposed, not *how* it renders.
- It does **not** introduce a new policy axis; it leans on the existing
  `@role.admin` atom and `commands.policy` matrix.
- It is **redundant** with `audience admin` in `.lzx` for the surface-only
  case; the distinct value-add is the **feature-side cross-check** —
  doctor can now assert "every column referenced by `admin_resource
  Customer` exists as a `Customer` field" and "every action referenced
  surfaces as a command whose policy admits `@role.admin`", which is
  invisible to the `.lzx` side today.

Cost: medium (new kind, ~4 doctor diagnostics, IR struct). Value:
**pilot-dependent**. **Do not lower until a pilot SaaS authors at least
two `admin_resource` blocks**. The cost of the IR struct and the parser
slice extension is high relative to a single fixture user; lowering
without pilot pressure invents shape.

### 2. PILOT-NEEDED — `admin_dashboard <name>`

A dashboard distinct from a view, scoped to admin chrome:

```
feature customer

  admin_dashboard customer_health
    audience admin
    widgets
      metric total_active source customer.query.count(status: "active")
      metric churn_30d source customer.query.churn(window: "30d")
      chart revenue_trend source customer.query.revenue_trend
      list at_risk source customer.query.at_risk_customers limit 10
    policy @role.admin
```

Closed catalog of widget kinds (minimum viable):

- `metric <name>` — single scalar; requires `source <Feature>.query.<name>(args?)`.
- `chart <name>` — time-series or grouped; requires `source` returning a
  recordset. **No chart-type vocabulary in source** (`bar`/`line`/`pie`
  is rendering hint; lives in `.lzx` or admin generator config).
- `list <name>` — bounded recordset; requires `source` and `limit <N>`.

**What it deliberately does not have**:

- No grid layout (`row`, `col`, `span`) — that's chrome (F).
- No date-range picker syntax — that's a query parameter, handled
  through the existing `query` shape.
- No drill-down navigation — that's `route` + `audience admin`, which
  already work.

Cost: medium. Value: pilot-dependent. **Do not lower until two pilots
need it** — one pilot can author the same dashboard as three separate
`.lzx` views with `audience admin` and a `Grid` view-type without a kind.

### 3. PILOT-NEEDED — `admin_action <name>`

Bulk / non-CRUD admin actions:

```
feature customer

  admin_action export_customers
    audience admin
    on Customer
    selection multiple
    runs customer.query.export_csv(ids: selection.ids)
    output @cap.File(max_size:100mb, accept:text/csv)
    policy @role.admin
    audit actor, count: selection.count

  admin_action import_customers
    audience admin
    on Customer
    runs customer.command.upload(file: input.file)
    input file: @cap.File(max_size:25mb, accept:text/csv)
    policy @role.admin, @role.sales_ops
```

Closed catalog of children:

- `audience <name>` — required.
- `on <Resource>` — the resource this action attaches to. Surfaces as a
  button on the admin index page for that resource.
- `selection single | multiple | none` — closed catalog. `multiple` means
  the action receives `selection.ids: [Resource.ID]`; `single` means
  `selection.id: Resource.ID`; `none` is a feature-level action with no
  selection input.
- `runs <Feature>.{command,query}.<name>(args)` — required. The action
  delegates to an existing command/query; admin **never invents a new
  side-effect path**. This is the boundary that keeps admin from
  bypassing policy/audit/tenant_from already declared on the underlying
  command.
- `input <field>: <Type>` — optional; for import-style actions that take
  a payload beyond the selection.
- `output <Type>` — optional; for export-style actions that return a
  file.
- `policy <atoms>` — optional override on top of the underlying
  command's policy. Doctor warns if narrower than the underlying
  command's policy (impossible to satisfy) or wider (privilege
  escalation).
- `audit <fields>` — same as commands.

**Why this is minimal**: every admin action **delegates** to an existing
`command` or `query`. There is no new effect kind. `admin_action` is a
declarative **surface wrapper** that says "this command/query is exposed
in admin chrome with this selection model and these audit fields". The
contract is enforceable from typed IR cross-checks against the existing
command/query graph.

Cost: medium. Value: pilot-dependent. **Do not lower until a pilot ships
the first bulk-export or bulk-action button**. The fixture's current
`escape_route` + `audience admin` + `@cap.File` output on api covers
single-shot import/export today.

### Anti-proposals (rejected here)

- **`admin_impersonation` kind** — listed in `framework-coverage-1400.md:325`
  as DL. Rejected: impersonation is a cross-cutting auth concern, not an
  admin-chrome concern. If a pilot needs impersonation, the right home is
  `auth impersonation` inside the existing `feature customer_auth` auth
  block (`full-capsule.lzi:504-523`) — not a new kind under admin. The
  audit edges (every command becoming `acting_as.actor_id`) belong to
  auth + audit primitives that already exist.

- **`admin_form` / `admin_field_widget` / `admin_filter_widget` kinds** —
  these are the chrome (forms/tables/filters renderers, framework-coverage
  §25 DF). They are **runtime/codegen**, not language. The admin
  generator (Cut admin DF, §2.19) reads the `admin_resource` kind and
  picks widgets from the inferred field types and capability decorators
  (`@cap.File` → upload widget; `@semantic.Email` → email input; etc.).
  No vocabulary in source for that pick.

- **`admin_theme` / `admin_branding` / `menu_builder` kinds** — §25 DF.
  Pure rendering. Lives in the admin generator's runtime configuration
  (Lazuli Go or pack-supplied admin chrome). No language vocabulary.

- **`admin_chart_type "line" | "bar" | "pie"` enum** — rendering hint.
  Lives in the chart widget's `.lzx` or generator default. The language
  declares "this dashboard has a chart over this query source"; the
  generator picks a chart type from the query's row shape.

- **`admin_search_provider` kind** — search adapter selection
  (Meilisearch / Typesense / Elasticsearch). Belongs in `registry.lzi`
  as a `capability search` (`docs/audit/framework-coverage-1400.md:356-359`),
  not in admin.

- **`admin_export` kind separate from `admin_action` with `output`** —
  redundant. An export is an `admin_action` that returns a `@cap.File`
  output. No second kind needed.

- **Provider names in any admin kind** — no `forest_admin`, `retool`,
  `nocodb`, `directus`, `motor_admin`, `frappe` keywords. Adapters that
  back the admin generator flow through `@adapter.<name>` in
  `registry.lzi`. Today the admin generator is **assumed to be a Lazuli
  Go built-in** (Cut admin DF), not an external SaaS — but the boundary
  must hold if a pilot ever wants to swap.

### Cross-checks the new kinds enable (only if they land)

These are diagnostics doctor **cannot run today** because the contract
is implicit. They become possible once `admin_resource` / `admin_action`
lower:

1. `ADMIN-COLUMN-001` — column referenced in `admin_resource <X>` does
   not resolve to a field on `<X>`.
2. `ADMIN-FILTER-001` — `filter <field>` refers to a field whose type is
   not enum-shaped and not relation-shaped (filtering on `Decimal` /
   `Text` is rendering-undefined).
3. `ADMIN-ACTION-001` — `actions <cmd-name>` references a command whose
   policy does not admit `@role.admin` (or the audience's role).
4. `ADMIN-POLICY-001` — `admin_resource` and `admin_action` policy
   atoms must be a non-empty subset of the resource's `commands.policy`
   union; narrower allowed (admin restricts), wider rejected
   (privilege escalation).
5. `ADMIN-AUDIENCE-001` — `audience <name>` must resolve to a
   `surface ... audience <name>` declaration in a sibling `.lzx`. Today
   audience reachability runs the other direction; this closes the loop.
6. `ADMIN-DELEGATION-001` — `admin_action runs <Feature>.{command,query}.<name>`
   must resolve to an existing operation in the same feature graph.

Six doctor diagnostics, each pilot-gated by the kind that introduces
them.

## IR proposto

Pilot-gated — implement when Cut admin opens, not before. Recap with
shape definitions so the runtime team has a target.

### `AdminResource` struct (new — Phase L Tier 4 or later)

```rust
pub struct AdminResource {
    pub resource: String,           // The Resource being exposed
    pub audience: String,           // Audience name (e.g. "admin")
    pub columns: Vec<String>,       // Resource.field references
    pub search: Vec<String>,        // Subset of columns
    pub filter: Vec<String>,        // Subset of columns
    pub detail_sections: Vec<String>, // View section names
    pub actions: Vec<String>,       // Command names on the resource
    pub policy: Option<PolicyClause>,
    pub audit: Option<AuditSpec>,   // Reused from Cut 3
    pub span_ref: Option<SpanRef>,
}
```

### `AdminDashboard` struct (new)

```rust
pub struct AdminDashboard {
    pub name: String,
    pub audience: String,
    pub widgets: Vec<AdminWidget>,
    pub policy: Option<PolicyClause>,
    pub span_ref: Option<SpanRef>,
}

pub enum AdminWidgetKind { Metric, Chart, List }

pub struct AdminWidget {
    pub kind: AdminWidgetKind,
    pub name: String,
    pub source: SourceRef,        // <Feature>.query.<name>(args)
    pub limit: Option<u32>,       // List only
    pub span_ref: Option<SpanRef>,
}
```

### `AdminAction` struct (new)

```rust
pub struct AdminAction {
    pub name: String,
    pub audience: String,
    pub on_resource: Option<String>,
    pub selection: AdminSelection,  // Single | Multiple | None
    pub runs: OperationRef,         // <Feature>.{command,query}.<name>(args)
    pub input: Vec<AdminInputField>,
    pub output: Option<TypeRef>,    // Reuses TypeRef::Capability(File)
    pub policy: Option<PolicyClause>,
    pub audit: Option<AuditSpec>,
    pub span_ref: Option<SpanRef>,
}

pub enum AdminSelection { Single, Multiple, None }
```

### `Feature` extensions

```rust
pub struct Feature {
    // ... existing
    pub admin_resources: Vec<AdminResource>,
    pub admin_dashboards: Vec<AdminDashboard>,
    pub admin_actions: Vec<AdminAction>,
}
```

### Inspect projections

`InspectFeature.admin_resources`, `.admin_dashboards`, `.admin_actions`
with the same shape as the IR structs, plus `origin` markers. Three new
`--expand=` flags:

```bash
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=admin_resources --format=json
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=admin_dashboards --format=json
lazuli inspect examples/full-capsule/full-capsule.lzi --expand=admin_actions --format=json
```

Or a combined `--expand=admin` that projects all three.

### Grammar / parser slice

Three new `parse_admin_resource` / `parse_admin_dashboard` /
`parse_admin_action` functions in `crates/lazuli_syntax/src/parser.rs`,
landing as **Phase L Tier 5** (after Tier 4 covers
commands/resources/queries/records). Cannot land before Tier 4 because
the cross-checks against `admin_resource.actions` need the typed command
slice.

### Diagnostics added

The six `ADMIN-*` diagnostics from "Cross-checks the new kinds enable"
above. Each is IR-driven, cross-feature, and gated on the kind that
introduces it.

### JSON shape (illustrative)

```json
{
  "name": "customer",
  "admin_resources": [
    {
      "resource": "Customer",
      "audience": "admin",
      "columns": ["name", "email", "lifecycle_stage", "tier", "score", "owner", "created_at"],
      "search": ["name", "email"],
      "filter": ["lifecycle_stage", "tier", "owner"],
      "detail_sections": ["header", "activity_timeline"],
      "actions": ["reassign", "archive"],
      "policy": "@role.admin",
      "audit": {"fields": ["actor", "target.id"]},
      "origin": "admin_resource"
    }
  ],
  "admin_dashboards": [],
  "admin_actions": []
}
```

## Codegen proposto

**Pilot-gated and runtime-team territory.** When Cut admin opens, codegen
emits one new file per feature carrying admin declarations:

`dist/go/<feature>/admin.gen.go`

Contents (sketch — runtime team owns the shape):

- `RegisterAdminResources(r *admin.Registry)` — registers each
  `AdminResource` with the admin generator's typed registry.
- `RegisterAdminDashboards(r *admin.Registry)` — same for dashboards.
- `RegisterAdminActions(r *admin.Registry)` — same for actions, wired
  through the existing `command`/`query` dispatch (no new effect path).

The admin generator (runtime-side, `runtime/go/lazuli/admin/`) reads the
registry at boot and **picks chrome at generate-time, not source-time**.
No widget vocabulary, no theme, no menu-builder source. The author
declares contract; the generator picks rendering.

Boundary discipline:

- No `react-admin`, `forest-admin`, `refine`, `directus`, `nocodb`,
  `retool` reference in any generated Lazuli source. The admin chrome
  is **a single Lazuli-supplied admin pack** by default; alternative
  chrome packs (community-supplied) bind through `@adapter.admin_chrome`
  in `registry.lzi`.
- No tailwind class names, no theme tokens in source. Theme is config,
  not contract.
- The admin generator emits **React + React Native chrome** consistent
  with the existing surface-projection split (`.web.lzx` /
  `.mobile.lzx`). Mobile admin is a Cut admin sub-cut, gated separately.

## Runtime proposto

**Pilot-gated and runtime-team territory.** When Cut admin opens, the
Lazuli Go runtime delivers one new subpackage:

`runtime/go/lazuli/admin/`

Capabilities:

- `Registry` — typed registry consumed by generated `RegisterAdmin*`
  functions.
- `Generator` — the admin generator that materializes chrome from the
  registry. Default chrome is a Lazuli-supplied React admin starter;
  alternative chromes (community packs) plug through
  `@adapter.admin_chrome`.
- `Selection` — typed selection envelope passed to bulk actions
  (`SelectionSingle`/`SelectionMultiple`/`SelectionNone`).
- `Audience` — closed catalog (alphabetical: `admin`, plus pilot-defined
  additions) bound to role catalogs through the existing `@role.<name>`
  registry.

No provider names anywhere in runtime. The default admin chrome is a
**Lazuli pack** (`@runtime/admin`), not a third-party SaaS.

Lifecycle:

- Boot: `lazuli.Boot` composes `RegisterAdmin*` from every generated
  feature module into a single `admin.Registry`. Admin routes mount
  under a configurable prefix (`/admin` by default, declared in
  `app.lzi` `admin_prefix "/_admin"` if needed — a pilot-gated
  `app.lzi` child).
- Hot reload: admin chrome can re-read the registry without restarting.

## Evals/Testes propostos

Admin kinds do not have `evals` blocks today (those are agent-only). The
declarative test surface for admin is:

1. The **existing `tests` block** on commands referenced by
   `admin_action runs` already covers the bulk-action path through the
   command's own audit/policy contract.
2. A new `case`-style block on `admin_resource` (PILOT-NEEDED — defer
   until at least two pilots ask). For v0, the golden file is the
   inspect projection of `admin_resources`/`admin_dashboards`/
   `admin_actions`:

```bash
cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi \
  --format=json --expand=admin > /tmp/got.json
diff /tmp/got.json crates/lazuli_cli/tests/fixtures/full-capsule-admin.golden.json
```

The golden pins the post-cut admin inspect shape; drift fails CI.

### Doctor test (`crates/lazuli_cli/src/doctor.rs:test`)

```rust
#[test]
fn canonical_warns_for_admin_action_policy_escalation() {
    let source = "
feature customer
  resource Customer
    commands
      reassign
        policy @role.admin, @role.sales

  admin_action reassign_all
    audience admin
    on Customer
    selection multiple
    runs customer.command.reassign(ids: selection.ids)
    policy @role.admin, @role.sales, @role.support  # <-- wider than command
";
    let diags = run_doctor(source);
    assert!(diags.iter().any(|d| d.code == "ADMIN-POLICY-001"));
}
```

Six new doctor tests, one per IR-promoted diagnostic.

### LSP test

Hover on `admin_resource Customer columns name` shows resolution to
`Customer.name: Text @semantic.PersonName` with origin
`resource:Customer.field:name`. Completion inside an `admin_resource`
body after `actions ` suggests `reassign, archive, ...` from the
resource's command catalog.

## Doctor/LSP propostos

Six new diagnostics (table above). All IR-driven, cross-feature.

LSP keyword/hover catalog additions (pilot-gated):

- `admin_resource`, `admin_dashboard`, `admin_action` — added to the
  closed kind catalog.
- `columns`, `search`, `filter`, `detail`, `actions`, `widgets`,
  `selection`, `runs`, `metric`, `chart`, `list` — added as admin-scope
  child keywords with hovers.
- `selection single | multiple | none` — closed catalog completion.

No new `@<namespace>` additions. Admin lives inside the existing closed
namespace set (`@policy`, `@role`, `@actor`, `@scope`, `@adapter`,
`@cap`, `@semantic`, `@anchor`, `@client`, `@fn`, `@validator`,
`@trace`).

## Critério de "ciclo fechado"

The bucket cycle closes when **every** box is checked for at least one
admin resource + one admin dashboard + one admin action from a pilot
fixture (not the current canonical capsule, which deliberately does not
exercise admin kinds).

- [ ] Authored in a pilot fixture under `examples/<pilot>/`.
- [ ] `lazuli check` accepts the syntax.
- [ ] `lazuli inspect --expand=admin` projects the IR.
- [ ] `lazuli doctor` runs the six new `ADMIN-*` diagnostics.
- [ ] `lazuli generate` emits `dist/go/<feature>/admin.gen.go`.
- [ ] Lazuli Go executes one CRUD page + one dashboard + one bulk action
      end-to-end against a Postgres + admin-chrome rig.
- [ ] At least one Go test per kind in `runtime/go/lazuli/admin/`.
- [ ] LSP serves hover on `admin_resource columns <field>` resolving
      to the field's type with capability decorators.

**None of these run today.** Cut admin opens when a pilot SaaS authors
the kinds; the language work above is **the staging design**, not an
active implementation.

## Próximo passo

1. **Do not implement** until a pilot SaaS product authors at least
   one `admin_resource` block. The canonical fixture's
   `audience admin` + `@role.admin` + `escape_route "/admin/..."` is
   sufficient for the current language coverage gate.
2. **Wait for Phase L Tier 4** (commands/resources/queries/records in
   the canonical-indent slice — `docs/next-checklist.md:60`). Without
   Tier 4, the `admin_resource.actions` cross-check against the
   command catalog has no typed IR to read.
3. **When pilot pressure arrives**: stage as Phase L Tier 5
   (`parse_admin_resource` / `parse_admin_dashboard` /
   `parse_admin_action` in the canonical-indent slice), then six
   `ADMIN-*` doctor diagnostics, then `--expand=admin` inspect
   projection, then hand off to runtime team for the admin chrome
   pack.
4. **Resist runtime leak**. Every PR that introduces a `widget`,
   `theme`, `chart_type`, `menu`, or `provider` keyword to the admin
   surface is a boundary violation and rejected at review. The author
   declares contract; the generator picks rendering.

The pilot evidence required to open Cut admin is a real product
authoring two `admin_resource` blocks plus one `admin_action` with a
file output. Until that pressure surfaces, this document is the
canonical record of where the language work would land.

## Rows sugeridas para `docs/next-checklist.md`

Not added yet — pilot-gated. When Cut admin opens, the rows would read:

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| TBD | Admin bucket cycle (pilot-gated, Cut admin entry) | pilot-needed | `admin_resource` / `admin_dashboard` / `admin_action` kinds + six `ADMIN-*` doctor diagnostics + `--expand=admin` inspect projection + Phase L Tier 5 parser slice extension. Gated on pilot SaaS authoring at least two `admin_resource` blocks and one `admin_action` with file output. See `docs/proposals/bucket-admin-cycle.md`. |
| TBD | Admin runtime + default chrome pack (pilot-gated, Cut admin runtime) | pilot-needed | `runtime/go/lazuli/admin/` subpackage + admin generator + Lazuli-supplied React/RN admin chrome pack + `@adapter.admin_chrome` plug point. Runtime team owns. Gated on the language-side cycle landing first. |
