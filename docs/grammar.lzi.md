# Lazuli `.lzi` Grammar — Canonical Indent Form

**Status**: Reference grammar for `.lzi` files in canonical indentation form
(the v0 target). Includes the AI primitives Cut A from
the `ai-primitives-v0` proposal (operational archive) (`tools` child of `agent`,
discriminated `output`, `evals` block).

This file is a normative artifact. Where this grammar disagrees with
prose docs, **the prose wins** (this is a *parser-shaped* projection;
many invariants are not expressible in EBNF and live in
`docs/canonical-semantics.md`, `docs/invariants.md`, and
`docs/design-decisions.md`).

The grammar covers `.lzi` only. `.lzx` (experiences/surfaces),
`app.lzi`, `registry.lzi`, `workspace.lzi`, and `contract.lzi` are
sibling grammars.

## Conventions

- EBNF flavor: `production = expression ;`, with alternation `|`,
  optional `?`, zero-or-more `*`, one-or-more `+`, grouping `( ... )`,
  terminal strings in double quotes.
- `ALL_CAPS` are terminal token classes produced by the lexer
  (`NEWLINE`, `INDENT`, `DEDENT`, `IDENT_LOWER`, `IDENT_UPPER`,
  `STRING`, `INTEGER`, `DECIMAL`, `DURATION`).
- `snake_case` are non-terminals.
- `# comments` are non-normative.

## Validations not in this grammar

Doctor and LSP enforce the following beyond what EBNF can express:

- The closed reference-namespace catalog (`@role.*`, `@scope.*`,
  `@actor.*`, `@policy.*`, `@semantic.*`, `@cap.*`, `@pii.*`, `@key.*`,
  `@client.*`, `@fn.*`, `@hook.*`, `@validator.*`, `@adapter.*`,
  `@query_modifier.*`, `@anchor.*`, `@llm.*`, `@tool.*`).
- The closed scalar/built-in type catalog (`ID`, `Text`, `Boolean`,
  `Integer`, `Decimal`, `Date`, `DateTime`, `JSON`).
- The closed predicate-language operator set (`=`, `!=`, `has`, `AND`,
  `OR`).
- Cross-feature reference resolution (must appear in `uses`).
- Canonical block order inside `feature` (lexer/parser permit any
  order; `lazuli check` warns on out-of-order blocks).
- Effect-derivation for `tools` entries.
- Required children per construct (e.g., `agent` requires `policy`,
  `output`, `model`, `prompt`).

## 1. Lexical layer

### 1.1 Tokens

```ebnf
NEWLINE         = "\n" | "\r\n" ;
INDENT          = ? virtual token emitted when indent depth increases ? ;
DEDENT          = ? virtual token emitted when indent depth decreases ? ;
COMMENT         = "#" { ? any char except "\n" ? } ;

IDENT_LOWER     = ( "a"…"z" | "_" ) { "a"…"z" | "0"…"9" | "_" } ;
IDENT_UPPER     = ( "A"…"Z" ) { "a"…"z" | "A"…"Z" | "0"…"9" | "_" } ;

STRING          = '"' { ? any char except '"' ? | '\"' } '"' ;
INTEGER         = "0" | ( "1"…"9" { "0"…"9" } ) ;
DECIMAL         = INTEGER "." ( "0"…"9" )+ ;
DURATION        = INTEGER ( "ms" | "s" | "m" | "h" | "d" | "w" | "mo" | "y" ) ;
SIZE            = INTEGER ( "b" | "kb" | "mb" | "gb" ) ;
```

### 1.2 Indentation contract

- The lexer tracks indent depth as a stack of column counts. Each
  source line must indent by the same fixed unit (the file's first
  indent establishes the unit; mixing tabs and spaces is rejected).
- When a line's indent depth exceeds the top of the stack, the lexer
  emits `INDENT` (one token, regardless of how many units jumped — but
  multi-unit jumps are rejected as ill-formed).
- When depth decreases, the lexer emits one `DEDENT` per stack pop
  before the line's tokens.
- Blank lines and pure-comment lines do not affect indent.
- A `NEWLINE` follows every logical line that is not the final
  `DEDENT` of the file.

### 1.3 Reserved words

Reserved words are recognized by `IDENT_LOWER` matching the closed
list. Any `IDENT_LOWER` not on this list is a user identifier.

```
access_ttl agent api append_only approval at attach_ctx audience auth backoff
budget by cache calls case chain channel
chunk client command commands compatibility computed_date constraints context contract
context_files cost creates default defaults delegated_to deletes deny
denies discriminator domain emits enum env environment environments
errors error escape_route event event_group evals expose extends
extensible_by extensions fanout feature field fields filter filters
flow fn forbids from grace group handler has hash has_many hook idempotency
identity in input integration integrations job key keys knowledge
let lifecycle lookup many_through materialize max_tokens method migrations migration_lock model modifier
multi_tenant mfa non_goals notification of offset on on_delete oauth optional
order otherwise out_of_scope output owner packs paginate params parent
password path pii platform polymorphic_ref policies policy policy_for prompt provides
public publishes purpose query query.list query.lookup query.sql rate_limit
read record recipient refs registry relation reorder report required requires resource
refresh_ttl restrict retention retry returns role rotation route rule run schedule schedule_rule scope
search seed self sequential service services session sessions soft_delete source
sql state step submit subscribes surface tags target targets template temperature
tenancy tenant tenant_from terminate test tests then through timeouts
theft_detection_action timeout timestamps to tools top top_p totp trace transition trigger triggers ttl type
uniques unique uses using validates validator value verify view views
webhook when when_denied window_function with workspace write
```

(Names in this list are recognized as keywords only inside their
parent construct — for instance, `policy` is a keyword as a child of
`command`, `agent`, `flow`, etc., but a feature named `policy_holders`
is fine because feature names use `IDENT_LOWER` and contextual lookup.)

### 1.4 Compound keyword joins

A keyword spelled as more than one lexeme follows one predictable join rule, so
an author (human or LLM) can infer the spelling of a compound they have not seen.
The join encodes the *kind* of compound:

- **Dotted (`a.b`)** — `b` selects a **variant within a family** that shares
  dispatch: `query.list` / `query.lookup` / `query.sql`, `event.trace`. The head
  is a reusable noun; the tail picks the kind. The dot is reserved for this.
- **Underscore (`a_b`)** — a single **atomic concept** whose name happens to be
  two words: `event_group`, `has_many`, `many_through`, `schedule_rule`,
  `computed_date`, `tenant_from`, `on_delete`, `soft_delete`, `append_only`,
  `rate_limit`, `escape_route`, `polymorphic_ref`.
- **Space (`a b`)** — a **reserved modifier applied to a reserved head**, where
  both words are independently meaningful keywords and the construct reads as
  "apply `a` to `b`": `public contract`, `scope override`, `previously migrated`
  / `previously alias`.

So `event_group` is never `event.group` (it is not a variant of `event`), and
`public contract` is never `public_contract` (it is modifier + head, not one
atom). Pick the join by classifying the compound, not by taste.

## 2. File-level structure

```ebnf
file              = feature_block ;

feature_block     = "feature" IDENT_LOWER NEWLINE
                    INDENT feature_body DEDENT ;

feature_body      = ( meta_block
                    | defaults_block
                    | uses_block
                    | refs_block
                    | domain_block
                    | policies_block
                    | errors_block
                    | auth_block
                    | command_block
                    | api_block
                    | webhook_block
                    | job_block
                    | notification_block
                    | report_block
                    | rule_block
                    | event_block
                    | event_group_block
                    | event_trace_block
                    | agent_block
                    | flow_block          (* Cut B *)
                    | extensions_block
                    | escape_route_block
                    | surface_block       (* feature-side surface declarations *)
                    | extends_block       (* cross-feature view extension *)
                    )+ ;
```

Block order is enforced by `lazuli check` against the canonical order
in `docs/canonical-semantics.md §Quick Reference`, not by the grammar.

## 3. Meta blocks

```ebnf
meta_block        = purpose_stmt
                  | non_goals_block
                  | knowledge_stmt ;

purpose_stmt      = "purpose" STRING NEWLINE ;

(* Feature context prose is resolved by CONVENTION, not a keyword: a
   co-located `<feature>.ctx.md` markdown sidecar next to the `.lzi`,
   probed at a SINGLE base (the `.lzi` directory; no path argument, no
   project-root fallback, no override). The former `attach_ctx "<path>"`
   meta statement is retired — the parser hard-errors
   `E-ATTACH-CTX-RETIRED` (mirroring the retired `context "..."` form,
   `E-CONTEXT-RETIRED`). See docs/canonical-semantics.md
   §"feature-context-vocabulary". *)

(* Iron-hand context directive. Names the bareword sector slug whose
   `knowledge/<sector>/` vault the feature draws authoring
   knowledge from. Cardinality 0..1; the planned `VOCAB-KNOWLEDGE-*`
   doctor lints cross-check the sector against its on-disk vault. *)
knowledge_stmt    = "knowledge" IDENT_LOWER NEWLINE ;  (* knowledge billing *)

non_goals_block   = "non_goals" NEWLINE
                    INDENT
                      ( "delegated_to" NEWLINE INDENT delegate_entry+ DEDENT
                      | "out_of_scope" NEWLINE INDENT scope_entry+ DEDENT
                      )+
                    DEDENT ;

delegate_entry    = IDENT_LOWER "->" feature_ref NEWLINE ;
scope_entry       = IDENT_LOWER ":" STRING NEWLINE ;

feature_ref       = IDENT_LOWER ;                  (* feature id *)
```

## 4. Defaults

```ebnf
defaults_block    = "defaults" NEWLINE
                    INDENT default_entry+ DEDENT ;

default_entry     = "tenancy" tenancy_value NEWLINE
                  | "timestamps" ( ident_list )? NEWLINE
                  | "no_timestamps" NEWLINE
                  | "policy_for" construct_list ":" actor_atom NEWLINE
                  | "audit" ( "all" | "none" | ident_list ) NEWLINE
                  | "soft_delete" NEWLINE ;

tenancy_value     = "org" | "team" | "user" | "tenant" | "none"
                  | IDENT_LOWER ;
construct_list    = construct_kind ( "," construct_kind )* ;
construct_kind    = "jobs" | "webhooks" | "commands"
                  | "queries" | "agents" ;
actor_atom        = "@actor." IDENT_LOWER ;
ident_list        = IDENT_LOWER ( "," IDENT_LOWER )* ;
```

## 5. Uses and refs

```ebnf
uses_block        = "uses" feature_ref ( "," feature_ref )* NEWLINE ;

refs_block        = "refs" NEWLINE
                    INDENT ref_group+ DEDENT ;

ref_group         = IDENT_LOWER ":" namespace_ref ( "," namespace_ref )* NEWLINE ;
namespace_ref     = "@" IDENT_LOWER ;
```

## 6. Domain (resources, enums, records)

```ebnf
domain_block      = "domain" NEWLINE
                    INDENT domain_entry+ DEDENT ;

domain_entry      = resource_decl | enum_decl | record_decl ;

resource_decl     = "resource" IDENT_UPPER NEWLINE
                    INDENT resource_body DEDENT ;

resource_body     = ( previously_clause
                    | tenancy_decl
                    | timestamps_decl
                    | soft_delete_decl
                    | append_only_decl
                    | retention_decl
                    | audit_decl
                    | field_decl
                    | has_many_decl
                    | many_through_decl
                    | polymorphic_ref_decl
                    | validates_decl
                    | constraints_block
                    | uniques_block
                    )+ ;

previously_clause = "previously" ( "migrated" | "alias" ) IDENT_LOWER NEWLINE ;

(* Cross-feature contract annotation. Appears IMMEDIATELY ABOVE the
   declaration of <Symbol>. See the `cross-feature-contracts` proposal (operational archive)
   §5.1. Compound keyword `public contract` enters the closed reserved
   word set; `public` has no other use. The version monotonically
   increases per symbol. *)
public_contract_clause = "public" "contract" IDENT_PASCAL "as" "v" INTEGER NEWLINE ;

field_decl        = IDENT_LOWER previously_clause? ":" type_expr field_marker*
                    cross_feature_target? presence?
                    ( computed_date_clause | schedule_rule_clause )?
                    relation_modifier? NEWLINE ;

type_expr         = scalar_type
                  | semantic_type
                  | cap_type
                  | enum_ref
                  | resource_ref ;

scalar_type       = "ID" | "Text" | "Boolean" | "Integer" | "Decimal"
                  | "Date" | "DateTime" | "JSON" ;
(* SPEC-04 — the CLOSED CORE semantic catalog is spelled BARE (a reserved
   PascalCase type name, like `Text`). `@semantic.<X>` is a DEPRECATED alias for
   a core type (`lazuli fmt` normalizes to bare) and the LIVE form for an OPEN
   plugin-declared scalar (`@semantic.TaxID`, `@semantic.BrazilianCPF`). The
   closed core set is `docs/closed-catalogs.md` (Email, Phone, Url, Uuid,
   Currency, GeoPoint, HexColor, Percentage, Money). *)
semantic_type     = SEMANTIC_CORE                  (* canonical: `Email`, `Money` *)
                  | "@semantic." IDENT_UPPER ;     (* deprecated-core alias OR plugin scalar *)
(* e.g. brand_color: HexColor required *)
(* e.g. completion: Percentage = 0 *)

(* SPEC-04 — capability types are spelled BARE; `@cap.<X>(...)` is a deprecated
   alias `lazuli fmt` normalizes. *)
cap_type          = CAP_NAME ( "(" cap_args ")" )?           (* canonical: `Encrypted(key:@key.tenant)` *)
                  | "@cap." IDENT_UPPER ( "(" cap_args ")" )? ;
cap_args          = cap_arg ( "," cap_arg )* ;
cap_arg           = IDENT_LOWER ":" cap_arg_value ;
cap_arg_value     = STRING | INTEGER | DURATION | SIZE | namespace_ref
                  | IDENT_LOWER | "true" | "false" ;
enum_ref          = IDENT_UPPER ;                   (* parser-resolved *)
resource_ref      = IDENT_UPPER ;                   (* parser-resolved *)

field_marker      = "@pii." IDENT_LOWER
                  | "@key." IDENT_LOWER
                  | "@adapter." IDENT_LOWER
                  | "@slug"                              (* auto-unique URL slug column *)
                  | "@full_text"                         (* tsvector source for `fts on (...)` *)
                  | owner_axis_marker                    (* ownership-chain projection FK *)
                  | "discriminator" ;

(* `ir-resource-conventions-owner-scope` §7.1 — names the FK column the
   ownership scope projects through. `<ident>` is a bare snake_case
   column; string literals are rejected. *)
owner_axis_marker = "@owner_axis" "(" "through" ":" IDENT_LOWER ")" ;
(* e.g. host: Host required @owner_axis(through: organization_id) *)

presence          = "required"
                  | "optional"
                  | "=" default_value ;
default_value     = STRING | INTEGER | DECIMAL | "true" | "false"
                  | IDENT_LOWER | namespace_ref ;

relation_modifier = "on_delete" ( "restrict" | "cascade" | "nullify" ) ;

(* GAP-12 — cross-feature FK annotation. Meaningful only on `ID` fields;
   the named feature must appear in the declaring feature's `uses`. *)
cross_feature_target = "target" "@feature." feature_ref "." IDENT_UPPER ;
(* e.g. default_department_id: ID target @feature.department.Department *)

(* W3 GAP-03 — computed `Date` field: `<base_field> + <offset>` days. The
   base is a sibling `Date` field; the offset is an `Integer` field name
   or an integer literal (days). Mutually exclusive with `derived from`. *)
computed_date_clause = "computed_date" "from" IDENT_LOWER
                       "offset" ( IDENT_LOWER | INTEGER ) ;
(* e.g. due_date: Date computed_date from campaign_start offset offset_days *)

(* W4 GAP-08 — rule-driven computed `Date`. The base `Date` is selected by
   a binding fn `@fn.<rule>(<arg>)`; `<offset>` is days (field or literal). *)
schedule_rule_clause = "schedule_rule" "from"
                       "@fn." IDENT_LOWER "(" expr ")"
                       "offset" ( IDENT_LOWER | INTEGER ) ;
(* e.g. due_date: Date schedule_rule from @fn.date_rule(input.rule) offset 7 *)

has_many_decl     = "has_many" IDENT_LOWER ":" IDENT_UPPER
                    ( "inverse" IDENT_LOWER )? NEWLINE ;

(* GAP-AUDIT-02 — `append_only` resource modifier (bare line, like
   `soft_delete`). Insert-only resource; doctor `RESOURCE-APPEND-ONLY-001`
   rejects update/delete commands targeting it. *)
append_only_decl  = "append_only" NEWLINE ;

(* GAP-07 — M:N junction carrying its own payload metadata. The junction
   name + `to <Partner>` endpoint sit on the header; at least one payload
   field is required (a metadata-free junction is a plain `has_many`). *)
many_through_decl = "many_through" IDENT_UPPER "to" IDENT_UPPER NEWLINE
                    INDENT field_decl+ DEDENT ;
(* e.g. many_through JobMember to User
          role_in_job: Text required *)

(* GAP-13 — polymorphic FK: a type-discriminator field + an id field that
   may point at any resource in the bracketed target list. *)
polymorphic_ref_decl = "polymorphic_ref" IDENT_LOWER IDENT_LOWER
                       "targets" "[" IDENT_UPPER ( "," IDENT_UPPER )* "]" NEWLINE ;
(* e.g. polymorphic_ref entity_type entity_id targets [Job, Activity] *)

tenancy_decl      = "tenancy" tenancy_value NEWLINE ;
timestamps_decl   = "timestamps" ( ident_list )? NEWLINE ;
soft_delete_decl  = "soft_delete" NEWLINE ;
retention_decl    = "retention" DURATION NEWLINE ;
audit_decl        = "audit" ( "all" | "none" | ident_list ) NEWLINE ;

validates_decl    = "validates" ( "field" IDENT_LOWER )?
                    ( "@validator." IDENT_LOWER | STRING ) NEWLINE
                  | "validates" "resource"
                    ( "@validator." IDENT_LOWER | STRING ) NEWLINE ;

constraints_block = "constraints" NEWLINE
                    INDENT constraint_entry+ DEDENT ;
constraint_entry  = "unique" ident_list ( "per" "record" )? NEWLINE ;

(* GAP-NEW-001 — DDL `unique` resource constraint (authored at resource
   child indent, alongside `index on`/`fts on`). The parenthesized field
   list is a table UNIQUE constraint; an optional trailing `when
   <predicate>` lowers to a PostgreSQL partial unique index. A bare
   single-field head is legal ONLY when `when` follows. *)
unique_constraint = "unique" ( "(" ident_list ")" | IDENT_LOWER )
                    ( "when" predicate )? NEWLINE ;
(* e.g. unique (workspace, email) *)
(* e.g. unique is_default when is_default = true *)

uniques_block     = "uniques" NEWLINE
                    INDENT unique_entry+ DEDENT ;
unique_entry      = ident_list NEWLINE ;

enum_decl         = "enum" IDENT_UPPER NEWLINE
                    INDENT enum_variant+ DEDENT ;
enum_variant      = IDENT_LOWER enum_storage? enum_variant_metadata? NEWLINE ;

(* Storage value — closed two-arm catalog (integer or quoted string). The
   retired `value` keyword does NOT parse; `=` separates name from value.
   crates/lazuli_syntax/src/parser/lzi/enums.rs:60-76. *)
enum_storage      = "=" ( INTEGER | STRING ) ;

(* Optional UI metadata, colon-introduced. `label` is mandatory when metadata
   is present; `hint` / `icon` follow. Storage and metadata are independent
   optional slots. crates/lazuli_syntax/src/parser/lzi/enums.rs:106-179. *)
enum_variant_metadata = ":" enum_metadata_item ( "," enum_metadata_item )* ;
enum_metadata_item = "label" metadata_key
                  | "hint" metadata_key
                  | "icon" STRING ;
metadata_key      = "@translation." IDENT_LOWER | IDENT_LOWER ;

record_decl       = "record" IDENT_UPPER NEWLINE
                    INDENT field_decl+ DEDENT ;
```

## 7. Policies and errors

```ebnf
policies_block    = "policies" NEWLINE
                    INDENT policy_entry+ DEDENT ;

policy_entry      = IDENT_LOWER ":" policy_atom_list NEWLINE
                    ( INDENT when_denied_clause DEDENT )?
                  | "fields" NEWLINE INDENT field_policy+ DEDENT ;
(* SPEC-07 C: the category IDENT_LOWER must NOT be a command effect verb —
   not `create`/`read`/`update`/`delete` nor the plural `creates`/`updates`/
   `deletes`/`reads`. At a `policy @policy.<x>` site a CRUD name reads as a
   write effect, not an authorization category. Use semantic names
   (`author`/`view`/`edit`/`remove`/`manage`); enforced by
   POLICY-CATEGORY-SHADOWS-EFFECT-001. NOTE: the field_policy_axis `read` /
   `write` below are access DIRECTIONS, a different closed catalog — allowed. *)

when_denied_clause = "when_denied" translation_key_ref NEWLINE ;
translation_key_ref = "@translation." IDENT_LOWER
                    | "@translation." IDENT_LOWER "." IDENT_LOWER ;

policy_atom_list  = policy_atom ( "," policy_atom )* ;
policy_atom       = "@role." IDENT_LOWER
                  | "@scope." IDENT_LOWER
                  | "@actor." IDENT_LOWER
                  | "@policy." IDENT_LOWER ;

field_policy      = IDENT_LOWER ":" policy_atom_list NEWLINE
                  | IDENT_LOWER NEWLINE INDENT field_policy_axis+ DEDENT ;
field_policy_axis = ( "read" | "write" ) ":" policy_atom_list NEWLINE ;

errors_block      = "errors" NEWLINE
                    INDENT errors_body DEDENT ;
errors_body       = ( errors_default_line
                    | errors_expose_line
                    | errors_code_message_line
                    )+ ;
errors_default_line       = "default" ( "hide" | "expose" ) NEWLINE ;
errors_expose_line        = "expose" "client" ( "4xx" | "5xx" )
                            ident_list NEWLINE ;
errors_code_message_line  = error_code "message" translation_key_ref NEWLINE ;
error_code        = "policy_denied" | "validation_failed" | "tenant_mismatch"
                  | "not_found" | "rate_limited" | "bad_request"
                  | "method_not_allowed" | "integration_error" ;
```

The `when_denied` child of a `policy_entry` is the per-policy default for the
`policy_denied` framework error code: every command using the named policy
inherits the phrasing unless it carries its own command-level
`when_denied` line. The `@translation.<key>` reference resolves against the
surrounding feature's `translation.keys` (same-feature shorthand) or against
`<feature>.@translation.<key>` for cross-feature look-ups; doctor cross-checks
under `translation_key_unknown` and the new `ERR-VOCAB-002`.

```lazuli
policies
  update: @role.admin, @role.sales
    when_denied @translation.customer_update_admin_only
```

The `errors` block under a `feature` declares the wire-envelope exposure rules
(`default hide|expose`, `expose client 4xx|5xx <fields>`) and the typed
per-code message overrides for the framework's closed catalog of eight error
codes. Codes outside the closed catalog raise `ERR-VOCAB-CODE-UNKNOWN`;
exposure fields outside `message`, `code`, `data`, `message_key` (4xx) or
`code`, `data` (5xx) raise `ERR-VOCAB-EXPOSE-UNKNOWN`. `expose client 5xx
message` is rejected (`ERR-VOCAB-EXPOSE-5XX-MESSAGE`) so framework-internal
5xx text never reaches the wire. The `message_key` exposure token publishes
the resolved `@translation.<key>` on the wire so client UIs that ship offline
catalogs (native mobile apps) can render their own strings.

```lazuli
errors
  default hide
  expose client 4xx message, code, message_key
  policy_denied message @translation.customer_signin_required
```

Author-declared **named typed errors** (e.g. `error CustomerAlreadyDeleted
status 409 expose message, code`) are declared inside `rule` and `command`
bodies via the `error_emit` clause (§8 `error_emit`) and define new error
codes rather than overriding the eight framework codes. The two surfaces
compose orthogonally.

## 8. Commands

```ebnf
command_block     = "command" IDENT_LOWER NEWLINE
                    INDENT command_body DEDENT ;

command_body      = ( previously_clause
                    | route_slots
                    | input_block
                    | params_block
                    | target_clause
                    | let_clause
                    | requires_clause
                    | validate_clause
                    | creates_clause
                    | updates_clause
                    | deletes_clause
                    | reorder_clause
                    | emits_clause
                    | returns_clause
                    | invalidates_clause
                    | error_emit
                    | policy_clause
                    | rate_limit_clause
                    | idempotency_clause
                    | approval_block
                    | audit_clause
                    | tests_block
                    )+ ;

(* GAP-REORDER-01 — batch position update. Rewrites the integer
   `<position_field>` column across the named resource's rows in one
   statement. `<Resource>` may be feature-qualified. Cardinality 0..1. *)
reorder_clause    = "reorder" IDENT_UPPER "by" IDENT_LOWER NEWLINE ;
(* e.g. reorder JobStep by position *)

route_slots       = "route" NEWLINE INDENT route_slot+ DEDENT ;
route_slot        = IDENT_LOWER ":" type_ref NEWLINE ;
type_ref          = type_expr | enum_ref | record_ref ;
record_ref        = IDENT_UPPER ;

input_block       = "input" NEWLINE INDENT input_slot+ DEDENT ;
input_slot        = IDENT_LOWER ":" type_ref presence? NEWLINE ;

params_block      = "params" NEWLINE INDENT input_slot+ DEDENT ;

target_clause     = "target" target_expr NEWLINE ;
target_expr       = qualified_query_ref "(" arg_list? ")" ;
qualified_query_ref = ( feature_ref "." )? "query" ( "." query_kind )? "." IDENT_LOWER
                    | ( feature_ref "." )? "query" "." query_kind ;
query_kind        = "list" | "lookup" | "sql" ;

arg_list          = arg ( "," arg )* ;
arg               = IDENT_LOWER ":" expr ;

let_clause        = "let" IDENT_LOWER "=" expr NEWLINE ;

requires_clause   = "requires" expr NEWLINE
                  | "requires" "@validator." IDENT_LOWER ( "(" arg_list? ")" )? NEWLINE
                  | "requires" "@policy." IDENT_LOWER NEWLINE ;

validate_clause   = "validate" "@validator." IDENT_LOWER
                    ( "(" arg_list? ")" )? NEWLINE ;

creates_clause    = "creates" IDENT_UPPER assignment_block? NEWLINE ;
updates_clause    = "updates" IDENT_UPPER assignment_block? NEWLINE ;
deletes_clause    = "deletes" IDENT_UPPER NEWLINE ;
assignment_block  = NEWLINE INDENT assignment+ DEDENT ;
assignment        = IDENT_LOWER "=" expr NEWLINE ;

emits_clause      = "emits" IDENT_LOWER ( "from" effect_ref )? assignment_block? NEWLINE ;
effect_ref        = "creates" | "updates" | "deletes" ;

returns_clause    = "returns" type_ref NEWLINE ;

invalidates_clause = "invalidates" cache_ref ( "," cache_ref )* NEWLINE ;
cache_ref         = qualified_query_ref | qualified_query_ref "*" | "query.*" ;

error_emit        = "error" IDENT_UPPER ( "if" expr )? NEWLINE ;

policy_clause     = "policy" policy_atom_list NEWLINE
                    ( INDENT when_denied_clause DEDENT )? ;

rate_limit_clause = "rate_limit" STRING NEWLINE
                  | "rate_limit" "none" NEWLINE
                    ( INDENT rate_limit_reason DEDENT )? ;
rate_limit_reason = "reason" STRING NEWLINE ;
(* `rate_limit none` is the explicit security opt-out: a mutating command  *)
(* declines a rate limit but must justify it via the `reason` child. It    *)
(* lowers to the same no-throttle spec as `rate_limit "unlimited"`; the     *)
(* reason is enforced at authoring time (LSP), not carried in the IR.       *)

idempotency_clause = "idempotency" "by" idempotency_source
                     ( "," idempotency_source )* NEWLINE ;
idempotency_source = ( "input" | "envelope" | "payload" | "schedule"
                     | "tenant" | "ctx" ) "." IDENT_LOWER
                   ( "." IDENT_LOWER )* ;

(* W4 GAP-06 — conditional human sign-off. Supply approvers via either
   `by @role.<name>` (single approver) OR `chain [...]` (ordered roles);
   not both. A trailing `sequential` on the chain enforces strict order.
   `then` (the outcome on timeout / denial) is required. *)
approval_block    = "approval" NEWLINE
                    INDENT approval_child+ DEDENT ;
approval_child    = "required_when" expr NEWLINE
                  | "by" policy_atom NEWLINE
                  | approval_chain
                  | "sequential" NEWLINE
                  | "timeout" duration_literal NEWLINE
                  | "then" ( "deny" | "allow" | "escalate" ) NEWLINE ;
approval_chain    = "chain" "[" policy_atom ( "," policy_atom )* "]"
                    "sequential"? NEWLINE ;
(* e.g. approval
          chain [@role.manager, @role.admin] sequential
          then deny *)

(* GAP-AUDIT-01 — the audit envelope also accepts a block form whose
   children include `materialize @feature.<f>.<OperationLog>`, sinking the
   audit record into an `append_only` resource in another feature. The
   target feature must be reachable via `uses`; doctor
   `AUDIT-MATERIALIZE-TARGET-001` enforces the append-only invariant. *)
audit_clause      = "audit" ( "none" | "field" IDENT_LOWER
                            | "target" IDENT_UPPER )+ NEWLINE
                  | "audit" audit_subject_list NEWLINE
                    ( INDENT audit_child+ DEDENT )? ;
audit_subject_list = audit_subject ( "," audit_subject )* ;
audit_subject     = ref | "before" | "after" | "retain" DURATION ;
audit_child       = "emit_to" IDENT_LOWER NEWLINE
                  | "data_subject" IDENT_LOWER NEWLINE
                  | "materialize" "@feature." feature_ref "." IDENT_UPPER NEWLINE
                  | "before" NEWLINE
                  | "after" NEWLINE
                  | "retain" DURATION NEWLINE ;
(* e.g. audit actor, target.id
          materialize @feature.audit.OperationLog *)
```

A command-level `when_denied` is the highest-precedence step in the error
resolver chain — it overrides any per-policy `when_denied` and any
feature-level `errors policy_denied message ...` catch-all for this one
command. See `docs/canonical-semantics.md` (errors section) for the four-layer
resolver chain and `lazuli-ops/docs/proposals/ir-error-messages-vocab.md` §2 for the
design rationale.

```lazuli
command capture_lead
  policy @policy.capture_lead
    when_denied @translation.capture_lead_signin_required
```

## 9. Custom HTTP API endpoints

```ebnf
api_block         = "api" IDENT_LOWER NEWLINE
                    INDENT api_body DEDENT ;

api_body          = ( "method" http_method NEWLINE
                    | "path" STRING NEWLINE
                    | route_slots
                    | input_block
                    | "output" output_kind NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | "rate_limit" STRING NEWLINE
                    | "handler" STRING NEWLINE
                    | "audit" audit_value NEWLINE
                    )+ ;

http_method       = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" ;
output_kind       = ( "stream" )? type_ref ;
audit_value       = "none" | ident_list ;
```

## 10. Webhooks, jobs, notifications

```ebnf
webhook_block     = "webhook" IDENT_LOWER NEWLINE
                    INDENT webhook_body DEDENT ;
webhook_body      = ( "path" STRING NEWLINE
                    | "verify" verify_value NEWLINE
                    | "tenant_from" idempotency_source NEWLINE
                    | "idempotency" "by" idempotency_source NEWLINE
                    | "handler" STRING ( "returns" type_ref )? NEWLINE
                    | "emits" IDENT_LOWER NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | "rate_limit" STRING NEWLINE
                    )+ ;
verify_value      = "hmac" hmac_args
                  | "none" ( INDENT verify_reason DEDENT )? ;
verify_reason     = "reason" STRING NEWLINE ;
(* `verify none` is the security opt-out: an inbound webhook intentionally   *)
(* skips signature verification (verified at a gateway, or genuinely         *)
(* internal). The `reason` child is required by the LSP security rule. The   *)
(* legacy path form `verify "./verifier.go"` is retired — use `verify hmac`. *)
hmac_args         = NEWLINE INDENT
                      "secret" "env." IDENT_LOWER NEWLINE
                      "algorithm" IDENT_LOWER NEWLINE
                      "header" STRING NEWLINE
                      ( "tolerance" DURATION NEWLINE )?
                    DEDENT ;

job_block         = "job" IDENT_LOWER NEWLINE
                    INDENT job_body DEDENT ;
job_body          = ( "trigger" trigger_value NEWLINE
                    | "tenant_from" idempotency_source NEWLINE
                    | "fanout" "tenants" tenancy_value NEWLINE
                    | "idempotency" "by" idempotency_source ( "," idempotency_source )* NEWLINE
                    | "retry" INTEGER ( "backoff" backoff_strategy )? NEWLINE
                    | "timeout" DURATION NEWLINE
                    | "queue" IDENT_LOWER NEWLINE
                    | "target" target_expr NEWLINE
                    | let_clause
                    | requires_clause
                    | updates_clause
                    | creates_clause
                    | deletes_clause
                    | emits_clause
                    | "calls" call_ref NEWLINE
                    | "handler" STRING NEWLINE
                    )+ ;
trigger_value     = "schedule" STRING
                  | "event" qualified_event_ref ;
qualified_event_ref = ( feature_ref "." )? IDENT_LOWER ;
backoff_strategy  = "exponential" | "linear" | "constant" ;
call_ref          = IDENT_LOWER "." IDENT_LOWER ;

notification_block = "notification" IDENT_LOWER NEWLINE
                     INDENT notification_body DEDENT ;
notification_body = ( "channel" channel_list NEWLINE
                    | "recipient" expr NEWLINE
                    | "trigger" "event" qualified_event_ref NEWLINE
                    | "template" STRING NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | "tenant_from" idempotency_source NEWLINE
                    | "idempotency" "by" idempotency_source NEWLINE
                    | "retry" INTEGER ( "backoff" backoff_strategy )? NEWLINE
                    | "rate_limit" STRING NEWLINE
                    | "emits" IDENT_LOWER NEWLINE
                    )+ ;
channel_list      = channel ( "," channel )* ;
channel           = "email" | "push" | "sms" | "in_app" ;
```

## 11. Reports, rules, events

**`workflow` is retired.** The feature-level `workflow ... on <Resource>.<field>`
block no longer parses — the parser hard-errors with `E-WORKFLOW-RETIRED`.
Lifecycle is now expressed two ways: the resource-level `lifecycle <field>`
block (declares `state` / `transition` edges on the discriminator field), and
the command-level `triggers transition <name>[, <name>]` clause (binds a
command's write to a lifecycle edge). See `docs/lifecycle-transitions.md`.

```ebnf
enum_value        = IDENT_LOWER ;

(* Report vocab — generated-document feature child. Projects a `query.*`
   `source` through a closed `columns` catalog into one or more output
   `formats`. `input` (W5 GAP-REPORT-01) threads request-time params to the
   source query, reusing the command input-slot grammar. `source` and a
   non-empty `formats` list are required. *)
report_block      = "report" IDENT_LOWER NEWLINE
                    INDENT report_body DEDENT ;
report_body       = ( input_block
                    | "source" query_ref NEWLINE
                    | report_columns_block
                    | "formats" ident_list NEWLINE
                    | "storage" IDENT_LOWER NEWLINE
                    | "visibility" IDENT_LOWER NEWLINE
                    | "signed_ttl" DURATION NEWLINE
                    | "filename" STRING NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | rate_limit_clause
                    | audit_clause
                    )+ ;
query_ref         = ( feature_ref "." )? "query" ( "." query_kind )? "." IDENT_LOWER ;
report_columns_block = "columns" NEWLINE INDENT report_column+ DEDENT ;
report_column     = IDENT_LOWER "from" report_column_source
                    ( "label" STRING )? ( "format" STRING )? NEWLINE ;
report_column_source = "row" "." IDENT_LOWER
                  | "@fn." IDENT_LOWER "(" ( expr ( "," expr )* )? ")" ;
(* e.g. report monthly_audit
          input
            period_start: Date required
          source customer.query.list
          columns
            ltv from @fn.lifetime_value(row.id) label "Valor"
          formats csv, xlsx *)

rule_block        = "rule" IDENT_LOWER NEWLINE
                    INDENT rule_body DEDENT ;
rule_body         = ( "deny" expr NEWLINE
                    | "allow" expr NEWLINE
                    | "message" STRING NEWLINE
                    | error_emit
                    | tests_block
                    )+ ;

event_block       = "event" IDENT_LOWER NEWLINE
                    ( INDENT event_body DEDENT )? ;
event_body        = ( "topic" STRING NEWLINE
                    | "payload" NEWLINE INDENT payload_field+ DEDENT
                    | "tenant_from" idempotency_source NEWLINE
                    )+ ;
payload_field     = field_decl ;

event_group_block = "event_group" event_pattern "on" IDENT_UPPER NEWLINE
                    INDENT event_group_body DEDENT ;
event_pattern     = IDENT_LOWER ;          (* `customer_*` glob handled by lexer *)
event_group_body  = ( "payload" NEWLINE INDENT payload_field+ DEDENT
                    | "policies" NEWLINE INDENT policy_entry+ DEDENT
                    | event_block
                    )+ ;

event_trace_block = "event.trace" IDENT_LOWER NEWLINE
                    INDENT event_body DEDENT ;
```

## 12. Queries

```ebnf
query_block       = query_list_decl
                  | query_lookup_decl
                  | query_sql_decl ;

query_list_decl   = "query.list" IDENT_LOWER NEWLINE
                    INDENT query_list_body DEDENT ;

query_list_body   = ( params_block
                    | "filters" filter_field_list NEWLINE
                    | "search" "by" ident_list ( "mode" search_mode )? NEWLINE
                    | "scope" NEWLINE INDENT scope_predicate+ DEDENT
                    | "scope" "override" "(" tenancy_value ")" NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | reason_clause
                    | "cache" cache_args NEWLINE
                    | "paginate" INTEGER NEWLINE
                    | "order" order_list NEWLINE
                    | "modifier" "@query_modifier." IDENT_LOWER
                      ( "(" arg_list? ")" )? NEWLINE
                    )+ ;

filter_field_list = filter_field ( "," filter_field )* ;
filter_field      = IDENT_LOWER filter_op? ;
filter_op         = "==" | "!=" | "<" | "<=" | ">" | ">=" | "has" ;

scope_predicate   = IDENT_LOWER "==" expr NEWLINE ;
search_mode       = "contains" | "prefix" | "fulltext" ;
cache_args        = "key" STRING ( "ttl" DURATION )? ;
order_list        = order_entry ( "," order_entry )* ;
order_entry       = IDENT_LOWER ( "asc" | "desc" )? ;
reason_clause     = "reason" STRING NEWLINE ;

query_lookup_decl = "query.lookup" IDENT_LOWER
                    ( "by" IDENT_LOWER ":" type_ref )?
                    NEWLINE
                    INDENT query_lookup_body DEDENT ;

query_lookup_body = ( params_block
                    | "key" "=" expr NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | "cache" cache_args NEWLINE
                    | "scope" NEWLINE INDENT scope_predicate+ DEDENT
                    | "scope" "override" "(" tenancy_value ")" NEWLINE
                    | reason_clause
                    )+ ;

query_sql_decl    = "query.sql" IDENT_LOWER NEWLINE
                    INDENT query_sql_body DEDENT ;
query_sql_body    = ( params_block
                    | "sql" STRING NEWLINE
                    | "returns" type_ref NEWLINE
                    | "scope" NEWLINE INDENT scope_predicate+ DEDENT
                    | "scope" "override" "(" tenancy_value ")" NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | reason_clause
                    )+ ;
```

## 13. Auth

```ebnf
auth_block        = "auth" NEWLINE
                    INDENT auth_body DEDENT ;
auth_body         = ( "identity" IDENT_UPPER NEWLINE
                    | password_block
                    | oauth_block
                    | mfa_block
                    | sessions_block
                    )+ ;
password_block    = "password" NEWLINE
                    INDENT
                      ( "hash" hash_args NEWLINE )+
                    DEDENT ;
hash_args         = "algorithm" IDENT_LOWER ;
oauth_block       = "oauth" NEWLINE
                    INDENT oauth_provider+ DEDENT ;
oauth_provider    = "adapter" "@adapter." IDENT_LOWER NEWLINE ;
mfa_block         = "mfa" NEWLINE
                    INDENT
                      ( "totp" NEWLINE
                      | "enroll" "@validator." IDENT_LOWER NEWLINE
                      )+
                    DEDENT ;
sessions_block    = "sessions" NEWLINE
                    INDENT sessions_body+ DEDENT ;
sessions_body     = "resource" IDENT_UPPER NEWLINE
                  | "ttl" duration_literal NEWLINE
                  | "refresh" ( "true" | "false" ) NEWLINE
                  | "access_ttl" duration_literal NEWLINE
                  | rotation_block
                  | cookie_block ;
rotation_block    = "rotation" NEWLINE
                    INDENT rotation_body* DEDENT
                  | "rotation" "true" NEWLINE ;
rotation_body     = "refresh_ttl" duration_literal NEWLINE
                  | "grace" duration_literal NEWLINE
                  | "theft_detection_action" theft_detection_action NEWLINE ;
theft_detection_action
                  = "revoke_session_family" | "revoke_user" ;
cookie_block      = "cookie" NEWLINE
                    INDENT cookie_attr+ DEDENT ;
cookie_attr       = "name" STRING NEWLINE
                  | "same_site" same_site_value NEWLINE
                  | "secure" ( "true" | "false" ) NEWLINE
                  | "http_only" ( "true" | "false" ) NEWLINE
                  | "domain" STRING NEWLINE
                  | "path" STRING NEWLINE ;
same_site_value   = "lax" | "strict" | "none" ;
duration_literal  = STRING | DURATION ;
```

`access_ttl` is scoped to `auth.sessions`; it is a duration with framework
default `15m` when rotation is enabled.

`rotation` is scoped to `auth.sessions`; it is a nested block and its presence
enables refresh-token rotation. `rotation true` is the legacy shorthand and is
auto-promoted to the empty nested block.

`refresh_ttl` is scoped to `auth.sessions.rotation`; it is a duration with
framework default `14d`.

`rotation_grace` is the semantic slot represented in source by
`grace <duration>` under `auth.sessions.rotation`; its framework default is
`1m`.

`theft_detection_action` is scoped to `auth.sessions.rotation`; it is a closed
enum (`revoke_session_family` | `revoke_user`) with default
`revoke_session_family`.

`cookie` is scoped to `auth.sessions`; it is a nested block declaring the
session-cookie transport envelope. Every attribute is optional — an absent
attribute leaves the runtime's hardcoded cookie literal for that axis in place,
so the block overrides only what it names. The six reserved attributes are:

- `name` — the cookie name (string; runtime default `lazuli_session`).
- `same_site` — CSRF policy, a closed catalog `lax` | `strict` | `none`
  (default `lax`; `none` requires `secure true` per RFC 6265bis).
- `secure` — `Secure` (TLS-only) flag, `true` | `false`.
- `http_only` — `HttpOnly` (hidden from `document.cookie`) flag, `true` | `false`.
- `domain` — the cookie `Domain` attribute (string, e.g. `".example.com"`).
- `path` — the cookie `Path` attribute (string, e.g. `"/app"`).

The `same_site` / `secure` / `http_only` attribute vocabulary is shared with
the app-level `app.cookie` profiles (app manifest); only the parent scope
differs.

## 14. Agent (with Cut A AI primitives)

```ebnf
agent_block       = "agent" IDENT_LOWER NEWLINE
                    INDENT agent_body DEDENT ;

agent_body        = ( input_block
                    | "context" target_expr NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | "rate_limit" STRING NEWLINE
                    | "output" agent_output NEWLINE
                    | "model" "@llm." IDENT_LOWER NEWLINE
                    | "temperature" DECIMAL NEWLINE
                    | "max_tokens" INTEGER NEWLINE
                    | "top_p" DECIMAL NEWLINE
                    | "seed" INTEGER NEWLINE
                    | "prompt" STRING NEWLINE
                    | "safety" validator_ref_list NEWLINE
                    | tools_block
                    | evals_block
                    )+ ;

agent_output      = "stream" type_ref
                  | "discriminator" IDENT_UPPER
                  | type_ref ;

validator_ref_list = "@validator." IDENT_LOWER
                     ( "," "@validator." IDENT_LOWER )* ;

tools_block       = "tools" NEWLINE
                    INDENT tool_entry+ DEDENT ;
tool_entry        = qualified_tool_ref NEWLINE ;
qualified_tool_ref = "@tool." IDENT_LOWER ( "." IDENT_LOWER )*
                   | ( feature_ref "." )? tool_kind "." IDENT_LOWER ;
tool_kind         = "query" | "command" | "api"
                  | "query.list" | "query.lookup" | "query.sql" ;

evals_block       = "evals" NEWLINE
                    INDENT eval_case+ DEDENT ;
eval_case         = "case" IDENT_LOWER NEWLINE
                    INDENT eval_assertion+ DEDENT ;
eval_assertion    = ( "allows" | "denies" ) eval_predicate NEWLINE ;

eval_predicate    = predicate
                  | eval_contains
                  | eval_tools_calls ;

(* Cut A predicate-language extensions, scoped to evals only *)
eval_contains     = ref "contains" ( STRING | semantic_type ) ;
eval_tools_calls  = "tools.calls" ( "includes" | "excludes" ) qualified_tool_ref ;
```

## 15. Flow (Cut B sketch — not in v0)

```ebnf
flow_block        = "flow" IDENT_LOWER NEWLINE
                    INDENT flow_body DEDENT ;

flow_body         = ( input_block
                    | "policy" policy_atom_list NEWLINE
                    | "rate_limit" STRING NEWLINE
                    | "budget" budget_axis NEWLINE
                    | flow_step
                    | "output" agent_output ( "from" "step." IDENT_LOWER )? NEWLINE
                    | "emits" IDENT_LOWER NEWLINE
                    )+ ;

flow_step         = "step" "entry" IDENT_LOWER step_action NEWLINE
                  | "step" "on" IDENT_LOWER "." IDENT_LOWER step_action NEWLINE
                  | "step" "otherwise" step_action NEWLINE
                  | "step" IDENT_LOWER step_action NEWLINE ;

step_action       = NEWLINE INDENT
                      "by" "agent." qualified_agent_ref "(" arg_list? ")" NEWLINE
                      ( "then" qualified_command_ref "(" arg_list? ")" NEWLINE )*
                    DEDENT ;

qualified_agent_ref = ( feature_ref "." )? IDENT_LOWER ;
qualified_command_ref = ( feature_ref "." )? "command" "." IDENT_LOWER ;

budget_axis       = "tokens" INTEGER "per" budget_scope ;
budget_scope      = "request" ;        (* aggregate scopes are pack territory *)
```

## 16. Knowledge

`knowledge` is a scalar meta statement (`knowledge_stmt` — `knowledge <sector>`, sibling of
`purpose` / `non_goals`), cross-checked against its on-disk vault by the
`VOCAB-KNOWLEDGE-*` doctor rules. See `lazuli-ops/docs/proposals/knowledge-sector-field.md`.
(Feature context prose is resolved by the `<feature>.ctx.md` convention, not a
keyword; the former `attach_ctx` meta statement is retired — `E-ATTACH-CTX-RETIRED`.)

An earlier block-form RAG sketch (`source` / `chunk by` / `embedding @adapter`) was never
wired on any executable face and has been removed; vector/embedding retrieval is
companion-`@plugin` territory, not core grammar (see the proposal's out-of-scope section
for the disposition).

## 17. Surfaces (feature-side)

`surface` blocks live primarily in `.lzx`. Inside `.lzi`, only the
extension-attachment shape is allowed:

```ebnf
surface_block     = "surface" IDENT_LOWER platform_target NEWLINE
                    INDENT surface_body DEDENT ;
platform_target   = "web" | "mobile" ;
surface_body      = ( "uses" "experience" IDENT_LOWER NEWLINE
                    | "audience" IDENT_LOWER NEWLINE INDENT view_block+ DEDENT
                    | view_block
                    )+ ;

view_block        = "view" IDENT_LOWER IDENT_UPPER NEWLINE
                    INDENT view_body DEDENT ;
view_body         = ( "columns" ident_list NEWLINE
                    | "source" target_expr NEWLINE
                    | "submit" qualified_command_ref NEWLINE
                    | "filter" filter_field_list NEWLINE
                    | "search" "by" ident_list NEWLINE
                    | "extends" "@anchor." IDENT_LOWER NEWLINE
                    | "extensible_by" feature_ref ( "," feature_ref )* NEWLINE
                    | tests_block
                    )+ ;

extends_block     = "extends" "@anchor." IDENT_LOWER NEWLINE
                    INDENT extends_body DEDENT ;
extends_body      = ( "slot" IDENT_LOWER NEWLINE INDENT slot_body DEDENT
                    | "platforms" platform_list NEWLINE
                    | "audience" IDENT_LOWER NEWLINE
                    )+ ;
slot_body         = ( "before" IDENT_LOWER NEWLINE
                    | "after" IDENT_LOWER NEWLINE
                    | "block" IDENT_LOWER STRING NEWLINE
                    )+ ;
platform_list     = platform_target ( "," platform_target )* ;
```

## 18. Extensions and escape routes

```ebnf
extensions_block  = "extensions" NEWLINE
                    INDENT extension_decl+ DEDENT ;

extension_decl    = ( "fn" | "validator" | "hook" | "client"
                    | "adapter" | "query_modifier" )
                    IDENT_LOWER ":" extension_type NEWLINE
                    INDENT
                      "at" extension_source NEWLINE
                    DEDENT ;
extension_type    = IDENT_UPPER ( "[" type_ref "]" )? ;
extension_source  = STRING ;          (* "./path.go" or "@runtime/x" or "@plugin/x/y" *)

escape_route_block = "escape_route" IDENT_LOWER NEWLINE
                     INDENT escape_body DEDENT ;
escape_body       = ( "method" http_method NEWLINE
                    | "path" STRING NEWLINE
                    | "policy" policy_atom_list NEWLINE
                    | "tenant_from" idempotency_source NEWLINE
                    | "handler" STRING NEWLINE
                    )+ ;
```

## 19. Tests

```ebnf
tests_block       = "tests" NEWLINE
                    INDENT test_assertion+ DEDENT ;

test_assertion    = ( "allows" | "denies" )
                    test_clause+ NEWLINE ;
test_clause       = "from" enum_value
                  | "as" actor_ref
                  | "when" expr
                  | "extension" feature_ref ;
actor_ref         = "@role." IDENT_LOWER
                  | "@actor." IDENT_LOWER ;
```

Authored tests speak ONE verb pair — `allows` / `denies` — and let the
typed subject after the verb name the dimension: `when <pred>` for a
predicate, `from <state>` / `as @role.x` for a transition edge/actor,
and `extension <feature>` for view extensibility. The `extension`
subject is shared with `.lzx` view tests (`grammar.lzx.md` §7), which
reuse this same `test_assertion` rather than a separate verb family.
Generated command actor-matrix rows are the one exception: they use
`permits` / `forbids` (Section on command tests) to signal "this row is
machine-derived from `policy @policy.*`, do not hand-edit."

## 20. Predicate language (closed)

```ebnf
expr              = and_expr ( "OR" and_expr )* ;
and_expr          = not_expr ( "AND" not_expr )* ;
not_expr          = "NOT"? primary ;
primary           = comparison
                  | "(" expr ")"
                  | call_expr ;

comparison        = ref ( "==" | "!=" | "has" ) value
                  | ref ;       (* truthiness *)
(* Predicate equality is `==`. Bare `=` is NOT a comparison: it is
   assignment / field-default / enum-storage only. Lifecycle state
   bindings (`requires_lifecycle X = state`, `only_when lifecycle X = state`)
   also keep `=` — they bind a state, not compare. *)

call_expr         = "@validator." IDENT_LOWER "(" arg_list? ")"
                  | "@fn." IDENT_LOWER "(" arg_list? ")" ;

ref               = ref_root ( "." IDENT_LOWER )+ ;
ref_root          = "input" | "params" | "ctx" | "self" | "target"
                  | "envelope" | "payload" | "schedule" | "tenant"
                  | "route" | "output" | "tools" | IDENT_LOWER ;

predicate         = expr ;        (* alias *)

value             = STRING | INTEGER | DECIMAL | DURATION | SIZE
                  | "true" | "false" | "nil"
                  | namespace_ref | enum_value | ref ;
```

The eval extension `<ref> contains <token | semantic-type>` and
`tools.calls includes|excludes <tool-ref>` is admissible only inside
`evals`. The grammar enforces this through the `eval_predicate`
production (Section 14). A `lazuli check` diagnostic
`predicate_extension_outside_evals_diagnostics` rejects them
elsewhere; the parser does not emit them in other rules.

## 21. Parser-deferred validations

The grammar admits constructs that doctor or `lazuli check` reject:

- `previously` clauses with a name not present in any baseline IR are
  emitted as warnings (`previously_unknown_warning`).
- Cross-feature refs without `uses` parse, but `lazuli check` rejects
  with `cross_feature_uses_required_diagnostics`.
- Block order out of canonical sequence parses, with
  `block_order_warning`.
- Closed-namespace lookup (`@role.does_not_exist`) parses; doctor
  rejects with `namespace_atom_unknown_diagnostics`.
- Tool entries pointing to a non-existent capability parse; doctor
  rejects with `agent_tool_target_missing_diagnostics`.
- `output discriminator <T>` where `<T>` is not an enum, or
  `discriminator` marker on a field whose type is not an enum; doctor
  rejects with `agent_discriminator_field_invalid_diagnostics`.
- `evals` cases on agents without `temperature 0` and `seed`; doctor
  warns with `eval_nondeterministic_warning`.

## 22. Notes for implementers

- The lexer produces `INDENT`/`DEDENT`/`NEWLINE` tokens before the
  parser sees them. Treat the grammar as token-oriented, not
  character-oriented.
- Several productions appear in multiple parents (`policy_clause`,
  `rate_limit_clause`, `idempotency_clause`, `audit_clause`, etc.).
  Implement them once and reuse.
- `previously` may be inline-attached for backward compat; the
  canonical form is always a child statement. `lazuli fmt` rewrites
  inline forms to children.
- The grammar deliberately admits the *legacy* brace-MVP shapes
  nowhere. Files using the brace syntax must be migrated by
  `lazuli fmt --canonical` before parsing under this grammar.

## 23. App manifest route guards

`app.lzi` has its own sibling grammar in `docs/grammar.app.md`. The route-guard
surface is repeated here because `.lzi` feature views, `.lzx` surfaces, and the
app manifest participate in one guard resolution chain:

```ebnf
app_route_guard_top = actor_query_decl
                    | route_guard_block ;

actor_query_decl  = "actor_query" qualified_query_ref NEWLINE ;

route_guard_block = "route_guard" NEWLINE
                    INDENT route_guard_entry* DEDENT ;

route_guard_entry = "default_policy" policy_ref NEWLINE
                  | "on_unauthenticated" "redirect" STRING NEWLINE
                  | "on_unauthorized" "redirect" STRING NEWLINE
                  | "skeleton" "@client." IDENT_LOWER NEWLINE ;

qualified_query_ref = feature_ref "." "query" "." IDENT_LOWER ;

policy_ref        = "@policy." IDENT_LOWER
                  | "@scope." IDENT_LOWER
                  | "@role." IDENT_LOWER
                  | "@actor." IDENT_LOWER ;
```

`actor_query` references a query that returns `LazuliActor | null`; the runtime
uses it to resolve the active actor before evaluating route guards. The
`route_guard` block is app-scoped and has cardinality 0..1. Its child slots are
independent app defaults for the view -> audience -> app -> built-in guard
resolution chain.

## 24. Out of scope for this file

- `.lzx` (experiences, surfaces, route declarations).
- `app.lzi` (deploy topology, env, runtime units).
- `registry.lzi` (env groups, capabilities, integrations, packs).
- `workspace.lzi` (multi-app contracts).
- `contract.lzi` (cross-language operation contracts).

Each gets a sibling grammar file. They share the lexical layer
(Section 1) and the predicate language (Section 20).
