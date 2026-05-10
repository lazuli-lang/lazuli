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
extensible_by filter form imports lazy lookup mobile mode opens
params path platforms platform route search slot source submit
surface tests to view web
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
                    | "audience" IDENT_LOWER NEWLINE INDENT view_decl+ DEDENT
                    | view_decl
                    )+ ;
```

When `audience` is used as a child block, its body contains
view-level declarations that apply only to that audience. Top-level
`view_decl` children of a surface apply to all audiences.

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

view_test_assertion = ( "accepted" "by" feature_ref NEWLINE
                      | "rejected" "by" feature_ref NEWLINE
                      ) ;
```

Used by `extensible_by` anchors to verify which extension features
the host accepts. The `accepted by` / `rejected by` shape is
distinct from `.lzi` `tests` (which uses `allows` / `denies`).

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
