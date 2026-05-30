# Lazuli `.lzx` Grammar — Routes, Experiences, Surfaces

**Status**: Reference grammar for `.lzx` files (canonical indent).
Sibling of `docs/grammar.lzi.md`; shares lexical layer
(`docs/grammar.lzi.md §1`) and predicate language
(`docs/grammar.lzi.md §20`).

`.lzx` files own three concerns:

1. **Top-level routes** — typed URL routes with platform/audience.
2. **Experiences** — abstract view models, platform-agnostic.
3. **Surfaces** — concrete platform projections with view layouts.

Filename convention: `<feature>.lzx` for abstract experiences,
`<feature>.<platform>.lzx` for platform projections. The platform
suffix is the protected segment immediately before `.lzx`.

## 1. Lexical layer

Identical to `docs/grammar.lzi.md §1`. Re-uses `INDENT`/`DEDENT`/
`NEWLINE`, `IDENT_LOWER`, `IDENT_UPPER`, `STRING`, `INTEGER`,
`DECIMAL`, `DURATION`. Reserved words specific to `.lzx`:

```
action anchor at audience block columns detail experience extends
extensible_by filter form imports lazy lookup mobile mode
on_unauthenticated on_unauthorized opens params path platforms
platform policy redirect requires_lifecycle requires_lifecycle_in
route search slot source submit surface tests to view web
```

These are recognized as keywords only inside their parent
construct — the `.lzx` grammar is contextual, like `.lzi`.

## 2. File-level structure

```ebnf
file              = top_level_decl+ ;

top_level_decl    = route_decl
                  | experience_decl
                  | surface_decl ;
```

A `.lzx` file may contain any mix of these in any order; `lazuli
check` warns on out-of-order declarations following the
recommended sequence: routes → experiences → surfaces.

## 3. Route declaration (top-level)

```ebnf
route_decl        = "route" IDENT_LOWER NEWLINE
                    INDENT route_body DEDENT ;

route_body        = ( "path" STRING NEWLINE
                    | "route" route_slot_decl NEWLINE
                    | "params" NEWLINE INDENT param_slot+ DEDENT
                    | "to" target_view_call NEWLINE
                    | "surface" feature_ref platform_target NEWLINE
                    | "audience" IDENT_LOWER NEWLINE
                    | "lazy" boolean_literal NEWLINE
                    )+ ;

route_slot_decl   = IDENT_LOWER ":" type_ref ;
param_slot        = IDENT_LOWER ":" type_ref presence? NEWLINE ;
type_ref          = scalar_type | qualified_type_ref ;
qualified_type_ref = IDENT_UPPER ( "." IDENT_LOWER )* ;

target_view_call  = feature_ref "." "view" "." IDENT_LOWER
                    ( "(" arg_list ")" )? ;
arg_list          = arg ( "," arg )* ;
arg               = IDENT_LOWER ":" "route" "." IDENT_LOWER ;

platform_target   = "web" | "mobile" ;
boolean_literal   = "true" | "false" ;

feature_ref       = IDENT_LOWER ;
scalar_type       = "ID" | "Text" | "Boolean" | "Integer"
                  | "Decimal" | "Date" | "DateTime" | "JSON" ;
presence          = "required" | "optional" ;
```

## 4. Experience declaration

```ebnf
experience_decl   = "experience" IDENT_LOWER NEWLINE
                    INDENT experience_body DEDENT ;

experience_body   = ( imports_stmt
                    | view_decl
                    )+ ;

imports_stmt      = "imports" feature_list NEWLINE ;
feature_list      = feature_ref ( "," feature_ref )* ;

view_decl         = "view" IDENT_LOWER ( view_kind )? NEWLINE
                    INDENT view_body DEDENT ;

view_kind         = "Table" | "Form" | "Detail" | "List"
                  | IDENT_UPPER ;     (* renderer extensions *)

view_body         = ( "source" target_query_call NEWLINE
                    | "anchor" "@anchor." IDENT_LOWER NEWLINE
                    | "extensible_by" feature_list NEWLINE
                    | "extends" "@anchor." IDENT_LOWER NEWLINE
                    | "route" route_slot_decl NEWLINE
                    | "params" NEWLINE INDENT param_slot+ DEDENT
                    | "columns" ident_list NEWLINE
                    | "fields" ident_list NEWLINE
                    | "filter" filter_field_list NEWLINE
                    | "search" "by" ident_list NEWLINE
                    | "submit" target_command_call NEWLINE
                    | view_guard_decl
                    | action_decl
                    | opens_decl
                    | block_decl
                    | platforms_decl
                    | audience_decl
                    | extension_slot_decl
                    | tests_block
                    )+ ;

target_query_call = feature_ref "." "query" ( "." IDENT_LOWER )?
                    "." IDENT_LOWER ( "(" arg_list_view ")" )? ;
target_command_call = feature_ref "." "command" "." IDENT_LOWER
                      ( "(" arg_list_view ")" )? ;
view_guard_decl  = "policy" policy_ref NEWLINE
                    ( INDENT route_guard_child+ DEDENT )? ;
route_guard_child = route_guard_redirect
                  | route_guard_lifecycle ;
route_guard_redirect = ( "on_unauthenticated" | "on_unauthorized" )
                       "redirect" STRING NEWLINE ;
route_guard_lifecycle =
                    "requires_lifecycle" IDENT_UPPER "=" IDENT_LOWER NEWLINE
                  | "requires_lifecycle_in" IDENT_UPPER
                       "[" IDENT_LOWER ( "," IDENT_LOWER )* "]" NEWLINE ;
policy_ref        = "@policy." IDENT_LOWER
                  | "@scope." IDENT_LOWER
                  | "@role." IDENT_LOWER
                  | "@actor." IDENT_LOWER ;
arg_list_view     = arg_view ( "," arg_view )* ;
arg_view          = IDENT_LOWER ":" view_arg_value ;
view_arg_value    = "route" "." IDENT_LOWER
                  | "params" "." IDENT_LOWER
                  | "row" "." IDENT_LOWER
                  | STRING
                  | INTEGER
                  | "true" | "false" ;

action_decl       = "action" IDENT_LOWER "->" target_command_call NEWLINE ;
opens_decl        = "opens" IDENT_LOWER ( "(" arg_list_view ")" )? NEWLINE ;
block_decl        = "block" "@client." IDENT_LOWER NEWLINE ;
platforms_decl    = "platforms" platform_list NEWLINE ;
audience_decl     = "audience" IDENT_LOWER NEWLINE ;
platform_list     = platform_target ( "," platform_target )* ;

ident_list        = IDENT_LOWER ( "," IDENT_LOWER )* ;
filter_field_list = filter_field ( "," filter_field )* ;
filter_field      = IDENT_LOWER filter_op? ;
filter_op         = "=" | "!=" | "has" ;
```

## 5. Surface declaration

Surfaces project an experience onto a platform, optionally fanning
out by audience:

```ebnf
surface_decl      = "surface" feature_ref platform_target NEWLINE
                    INDENT surface_body DEDENT ;

surface_body      = ( "uses" "experience" feature_ref NEWLINE
                    | "audience" IDENT_LOWER NEWLINE
                      INDENT audience_body DEDENT
                    | view_decl
                    )+ ;

audience_body     = ( view_guard_decl
                    | view_decl
                    )+ ;
```

When `audience` is used as a child block, its body contains
an optional audience guard and view-level declarations that apply only to that
audience. Top-level `view_decl` children of a surface apply to all audiences.
`policy` has cardinality 0..1 per `view` and 0..1 per `audience`.
`on_unauthenticated` and `on_unauthorized` are optional redirect children of a
`policy` guard; each has cardinality 0..1.

### Lifecycle route guards

A `policy` guard MAY also gate a route/view on a domain **lifecycle**
state, so the view only paints once the actor's row has advanced far
enough. Two forms (mutually exclusive on the same guard —
`ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001`):

```lzx
route onboarding_step
  path "/onboarding/profile"
  to host.view.profile
  surface host web
  audience host
  policy @policy.authenticated
    on_unauthenticated redirect "/sign-in"
    requires_lifecycle Host = onboarded            # exact-match form
    # — or —
    requires_lifecycle_in Host [basic_pending, address_pending]  # allow-list form
```

- `requires_lifecycle <Resource> = <state>` — exact-match: the view
  paints only when `<Resource>`'s resolved lifecycle state equals
  `<state>`. `<Resource>` is the PascalCase resource name; `<state>`
  is a bare lifecycle state declared in that resource's `lifecycle`.
- `requires_lifecycle_in <Resource> [s1, s2, …]` — allow-list: the
  view paints when the resolved state is **any** of the listed states.
  This is the canonical, grep-friendly form for any list shape; doctor
  steers projects toward it when the shorthand is used at scale or the
  two shapes are mixed (`ROUTE-LIFECYCLE-CANONICAL-FORM-001`). An empty
  allow-list (`[]`) makes the view unreachable
  (`ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002`); a state not declared on the
  resource's lifecycle is rejected
  (`ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003`).

When the gate fails the runtime dispatches via the resource's
`lifecycle_routes` helper (or the optional `on_lifecycle_pending
@resume <name>` router). Each lifecycle guard has cardinality 0..1 per
`policy`. These keywords were added by the route-guard escape-hatch
work; they are parser- and LSP-recognized and round-trip through the
IR (`ViewGuard.requires_lifecycle` / `requires_lifecycle_in`).

## 6. Cross-feature view extension

Inside an experience, an extension feature targets a host's anchor:

```ebnf
extension_slot_decl = "slot" IDENT_LOWER NEWLINE
                      INDENT slot_body DEDENT ;

slot_body         = ( "before" IDENT_LOWER NEWLINE
                    | "after" IDENT_LOWER NEWLINE
                    | "block" "@client." IDENT_LOWER NEWLINE
                    | "platforms" platform_list NEWLINE
                    | "audience" IDENT_LOWER NEWLINE
                    | view_body                       (* nested view body *)
                    )+ ;
```

## 7. Tests inside views

```ebnf
tests_block       = "tests" NEWLINE
                    INDENT view_test_assertion+ DEDENT ;

view_test_assertion = ( "allows" "extension" feature_ref NEWLINE
                      | "denies" "extension" feature_ref NEWLINE
                      ) ;
```

Used by `extensible_by` anchors to verify which extension features
the host accepts. This is the SAME authored `allows` / `denies` dialect
as `.lzi` `tests` (`grammar.lzi.md` §19) — not a separate verb family.
The typed `extension <feature>` subject names the dimension, exactly as
`when <pred>` / `from <state>` / `as @role.x` name theirs.

## 7a. Surface-dialect UX primitives (wave W6)

The per-feature surface dialect (`<feature>.web.lzx` /
`<feature>.mobile.lzx`, parsed by `parser/lzx/surface/`) carries
richer presentation primitives than the abstract experience grammar
above. These close pauta UX gaps GAP-UX-01..07. Bodies indent two
spaces per level, like the rest of `.lzx`.

### 7a.1 Typed `filters` block — `date_range` cardinality (GAP-UX-07)

```ebnf
filters_block     = "filters" NEWLINE
                    INDENT filter_decl+ DEDENT ;

filter_decl       = IDENT_LOWER ":" filter_cardinality
                    ( "from" "query" )? NEWLINE ;

filter_cardinality = ( "list" "of" )? type_ref      (* single | multi *)
                   | "date_range" type_ref?         (* paired from/to picker *)
                   ;
```

`date_range` lowers to a paired from/to date picker that surfaces two
query params `<name>_from` / `<name>_to`. The backing type defaults to
`Date`; an explicit `date_range DateTime` overrides it. `from query`
URL-syncs both params.

```
filters
  created: date_range                 # defaults to Date
  effective: date_range DateTime from query
```

Doctor `LZX-DATE-RANGE-001`: the filtered field (`<name>`) must be a
`Date` / `DateTime` field on the resource backing the view's `source`.

### 7a.2 Wizard step indicator — `wizard_steps` (GAP-UX-01)

```ebnf
wizard_steps_decl = "wizard_steps" INTEGER "current" expr NEWLINE ;
```

Renders a step indicator inside a view. `current` is an expression —
typically an enum field — selecting the active step.

```
view detail registration at "/registration"
  source onboarding.query.detail
  wizard_steps 3 current registration_step
```

Doctor `LZX-WIZARD-STEPS-EXPR-001`: `current` must validate against a
declared enum field; the total must be a positive integer literal and
should match the enum's variant count (warn on mismatch).

### 7a.3 Runtime-derived tab group — `tab_group` (GAP-UX-02)

```ebnf
tab_group_decl    = "tab_group" "derived_from" IDENT_LOWER NEWLINE
                    INDENT tab_group_case+ DEDENT ;

tab_group_case    = "case" enum_variant_list "->" "tab" STRING NEWLINE ;
enum_variant_list = IDENT_UPPER ( "," IDENT_UPPER )* ;
```

Tabs whose set depends on a field's value at runtime. `derived_from`
names an enum field; each `case` maps one or more enum variants to a
tab label.

```
tab_group derived_from vehicle_type
  case TV, RADIO -> tab "Broadcast"
  case PRINT -> tab "Print"
```

Doctor `LZX-TAB-GROUP-CASE-001`: `derived_from` must be a declared
enum field; every `case` value must be a variant of that enum; warn on
non-exhaustive variant coverage.

### 7a.4 Static tabs + multi-step wizard container (GAP-UX-03)

```ebnf
tabs_decl         = "tabs" NEWLINE
                    INDENT tab_entry+ DEDENT ;

tab_entry         = "tab" STRING "->" "view" IDENT_LOWER
                    ( "audience" IDENT_LOWER )? NEWLINE ;

wizard_decl       = "wizard" IDENT_LOWER "steps" NEWLINE
                    INDENT wizard_step+ DEDENT ;

wizard_step       = "step" INTEGER ":" view_or_form_ref NEWLINE ;
view_or_form_ref  = IDENT_LOWER ;
```

`tabs` is a static tab container; each tab points at a declared view
(optionally audience-scoped). `wizard <name> steps` is the multi-step
container (distinct from `wizard_steps`, §7a.2, which is the
indicator) — each `step` references a declared form/view.

```
tabs
  tab "Details" -> view detail
  tab "History" -> view history audience admin

wizard job_create steps
  step 1: job_basics
  step 2: job_targeting
```

Doctor `LZX-TAB-VIEW-REF-001`: each `tab -> view X` must reference a
declared view; each wizard step must reference a declared form/view.

### 7a.5 View-mode toggle + inline-editable table (GAP-UX-04)

```ebnf
view_mode_decl    = "view_mode" NEWLINE
                    INDENT render_mode+ DEDENT ;

render_mode       = ( "table" | "kanban" | "calendar" | "gallery" ) NEWLINE ;

inline_table_decl = "view" "." "inline_table" "on_change"
                    IDENT_LOWER NEWLINE ;
```

`view_mode` declares a user-toggleable set of render modes for a list
view. `view.inline_table on_change X` makes table rows
inline-editable, binding edits to an update command.

```
view list jobs at "/jobs"
  source jobs.query.list
  columns title, status
  view_mode
    table
    kanban
  view.inline_table on_change update_row
```

Doctor `LZX-VIEW-MODE-001`: each mode must be a known render mode;
`inline_table on_change` must reference a declared command whose target
resource matches the view's resource.

### 7a.6 Board view + repeatable input group (GAP-UX-05)

```ebnf
board_decl        = "view" "." "board" IDENT_LOWER? NEWLINE
                    INDENT board_lanes DEDENT ;

board_lanes       = "lanes" "derived_from" IDENT_LOWER NEWLINE ;

repeatable_decl   = "repeatable" "input" IDENT_LOWER "group"
                    group_field ( "," group_field )*
                    "validates" "sum" "(" IDENT_LOWER ")" "=" NUMBER NEWLINE ;

group_field       = IDENT_LOWER ":" TYPE_NAME ;
```

`view.board` is a kanban-style board (list-only) whose lanes are derived
from an enum field (one lane per variant) or a `has_many` relation on the
view's bound resource. `repeatable input <name> group <f>: <T>, …` is a
repeatable group of input rows with a cross-row `sum(<field>) = <n>`
constraint (e.g. installment percentages must total 100); it is valid in
`view list` and `view detail` bodies (not `view create`).

```
view list activity at "/activity"
  source activity.query.list
  columns title, status
  view.board activity_board
    lanes derived_from status

view list plans at "/plans"
  source billing.query.list
  columns title
  repeatable input installments group days: Integer, percentage: Decimal validates sum(percentage) = 100
```

Doctor `LZX-BOARD-LANES-001`: `lanes derived_from <field>` must reference
a declared enum field or a has_many relation on the bound resource.
Doctor `LZX-REPEATABLE-SUM-001`: the `sum(<field>)` field must be a
numeric field declared inside the group; the parser guarantees the `= <n>`
target is a number literal.

## 8. Validations not in this grammar

Doctor and LSP enforce beyond what EBNF can express:

- Every `route ... to <feature>.view.<name>` must reference a
  declared view in the named experience.
- `surface` declarations must reference an experience the feature
  imports.
- `audience` values must exist somewhere in the package — there is
  no closed catalog (each project defines its own audiences:
  `admin`, `account`, `public`, `sales`, etc.).
- `extends @anchor.<name>` requires the host feature to declare
  `extensible_by` with the extending feature name.
- `lazy true` is admissible only on `route` declarations (not on
  views).
- `surface customer web` and `surface customer mobile` for the
  same feature must live in different files: `customer.web.lzx`
  and `customer.mobile.lzx`. Doctor enforces the filename
  convention.
- `route public_login` and `route public_lead_capture` may both
  declare `audience public` — audiences are not unique per
  surface.

## 9. Out of scope

- `.lzi` (use `docs/grammar.lzi.md`).
- `app.lzi` (use `docs/grammar.app.md`).
- Predicate language (use `docs/grammar.lzi.md §20`).
- The relationship between `.lzx` and the IR's `Experience`,
  `PlatformSurface`, `AudienceSurface`, `PlatformView`, `AppRoute`
  shapes — that is implementer documentation, not grammar.
