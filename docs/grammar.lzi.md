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
access_ttl agent api at audience auth backoff budget by cache calls case channel
chunk client command commands compatibility constraints context contract
context_files cost creates default defaults delegated_to deletes deny
denies discriminator domain emits enum env environment environments
errors error escape_route event event_group evals expose extends
extensible_by extensions fanout feature field fields filter filters
flow fn forbids from grace group handler has hash has_many hook idempotency
identity in input integration integrations job key keys knowledge
let lookup max_tokens method migrations migration_lock model modifier
multi_tenant mfa non_goals notification of on on_delete oauth optional
order otherwise out_of_scope output owner packs paginate params parent
password path pii platform policies policy policy_for prompt provides
public publishes purpose query query.list query.lookup query.sql rate_limit
read record recipient refs registry relation required requires resource
refresh_ttl restrict retention retry returns role rotation route rule run schedule scope
search seed self service services session sessions soft_delete source
sql step submit subscribes surface tags target template temperature
tenancy tenant tenant_from terminate test tests then through timeouts
theft_detection_action timeout timestamps to tools top top_p totp trace trigger ttl type
uniques unique uses using validates validator value verify view views
webhook when when_denied window_function with workflow workspace write
```

(Names in this list are recognized as keywords only inside their
parent construct — for instance, `policy` is a keyword as a child of
`command`, `agent`, `flow`, etc., but a feature named `policy_holders`
is fine because feature names use `IDENT_LOWER` and contextual lookup.)

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
                    | workflow_block
                    | rule_block
                    | event_block
                    | event_group_block
                    | event_trace_block
                    | agent_block
                    | flow_block          (* Cut B *)
                    | knowledge_block     (* Cut B sketch *)
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
                  | non_goals_block ;

purpose_stmt      = "purpose" STRING NEWLINE ;

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
                    | retention_decl
                    | audit_decl
                    | field_decl
                    | has_many_decl
                    | many_decl
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
                    presence? relation_modifier? NEWLINE ;

type_expr         = scalar_type
                  | semantic_type
                  | cap_type
                  | enum_ref
                  | resource_ref ;

scalar_type       = "ID" | "Text" | "Boolean" | "Integer" | "Decimal"
                  | "Date" | "DateTime" | "JSON" ;
semantic_type     = "@semantic." IDENT_UPPER ;
cap_type          = "@cap." IDENT_UPPER ( "(" cap_args ")" )? ;
cap_args          = cap_arg ( "," cap_arg )* ;
cap_arg           = IDENT_LOWER ":" cap_arg_value ;
cap_arg_value     = STRING | INTEGER | DURATION | SIZE | namespace_ref
                  | IDENT_LOWER | "true" | "false" ;
enum_ref          = IDENT_UPPER ;                   (* parser-resolved *)
resource_ref      = IDENT_UPPER ;                   (* parser-resolved *)

field_marker      = "@pii." IDENT_LOWER
                  | "@key." IDENT_LOWER
                  | "@adapter." IDENT_LOWER
                  | "discriminator" ;

presence          = "required"
                  | "optional"
                  | "=" default_value ;
default_value     = STRING | INTEGER | DECIMAL | "true" | "false"
                  | IDENT_LOWER | namespace_ref ;

relation_modifier = "on_delete" ( "restrict" | "cascade" | "nullify" ) ;

has_many_decl     = "has_many" IDENT_LOWER ":" IDENT_UPPER
                    ( "inverse" IDENT_LOWER )? NEWLINE ;

many_decl         = IDENT_LOWER ":" "many" IDENT_UPPER NEWLINE ;

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

uniques_block     = "uniques" NEWLINE
                    INDENT unique_entry+ DEDENT ;
unique_entry      = ident_list NEWLINE ;

enum_decl         = "enum" IDENT_UPPER NEWLINE
                    INDENT enum_variant+ DEDENT ;
enum_variant      = IDENT_LOWER ( "value" STRING )? NEWLINE ;

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
                    | emits_clause
                    | returns_clause
                    | invalidates_clause
                    | error_emit
                    | policy_clause
                    | rate_limit_clause
                    | idempotency_clause
                    | audit_clause
                    | tests_block
                    )+ ;

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

rate_limit_clause = "rate_limit" STRING NEWLINE ;

idempotency_clause = "idempotency" "by" idempotency_source
                     ( "," idempotency_source )* NEWLINE ;
idempotency_source = ( "input" | "envelope" | "payload" | "schedule"
                     | "tenant" | "ctx" ) "." IDENT_LOWER
                   ( "." IDENT_LOWER )* ;

audit_clause      = "audit" ( "none" | "field" IDENT_LOWER
                            | "target" IDENT_UPPER )+ NEWLINE ;
```

A command-level `when_denied` is the highest-precedence step in the error
resolver chain — it overrides any per-policy `when_denied` and any
feature-level `errors policy_denied message ...` catch-all for this one
command. See `docs/canonical-semantics.md` (errors section) for the four-layer
resolver chain and `docs/proposals/ir-error-messages-vocab.md` §2 for the
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
verify_value      = "none" | "hmac" hmac_args | STRING ;
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

## 11. Workflows, rules, events

```ebnf
workflow_block    = "workflow" IDENT_LOWER "on" qualified_field_ref NEWLINE
                    INDENT workflow_body DEDENT ;
qualified_field_ref = IDENT_UPPER "." IDENT_LOWER ;

workflow_body     = ( "policy" policy_atom_list NEWLINE
                    | "emits" IDENT_LOWER assignment_block? NEWLINE
                    | transition_decl
                    )+ ;

transition_decl   = IDENT_LOWER ":" enum_value "->" enum_value
                    transition_inline?
                    NEWLINE
                    ( INDENT transition_child+ DEDENT )? ;

transition_inline = ( "requires" policy_atom_list )?
                    ( "emits" IDENT_LOWER )? ;

transition_child  = previously_clause
                  | "requires" policy_atom_list NEWLINE
                  | "emits" IDENT_LOWER assignment_block? NEWLINE
                  | tests_block ;

enum_value        = IDENT_LOWER ;

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
filter_op         = "=" | "!=" | "<" | "<=" | ">" | ">=" | "has" ;

scope_predicate   = IDENT_LOWER "=" expr NEWLINE ;
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
                  | rotation_block ;
rotation_block    = "rotation" NEWLINE
                    INDENT rotation_body* DEDENT
                  | "rotation" "true" NEWLINE ;
rotation_body     = "refresh_ttl" duration_literal NEWLINE
                  | "grace" duration_literal NEWLINE
                  | "theft_detection_action" theft_detection_action NEWLINE ;
theft_detection_action
                  = "revoke_session_family" | "revoke_user" ;
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
eval_assertion    = ( "requires" | "forbids" ) eval_predicate NEWLINE ;

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

## 16. Knowledge (Cut B sketch — pack candidate, not in v0)

```ebnf
knowledge_block   = "knowledge" IDENT_LOWER ( "from" "@pack." namespace_path )? NEWLINE
                    INDENT knowledge_body DEDENT ;

knowledge_body    = ( "source" target_expr NEWLINE
                    | "chunk" "by" IDENT_LOWER NEWLINE
                    | "retention" DURATION NEWLINE
                    | "pii" pii_class_list NEWLINE
                    | "tenant_from" idempotency_source NEWLINE
                    | "embedding" "@adapter." IDENT_LOWER NEWLINE
                    )+ ;

pii_class_list    = IDENT_LOWER ( "," IDENT_LOWER )* ;
namespace_path    = IDENT_LOWER ( "." IDENT_LOWER )* ;
```

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
                  | "when" expr ;
actor_ref         = "@role." IDENT_LOWER
                  | "@actor." IDENT_LOWER ;
```

## 20. Predicate language (closed)

```ebnf
expr              = and_expr ( "OR" and_expr )* ;
and_expr          = not_expr ( "AND" not_expr )* ;
not_expr          = "NOT"? primary ;
primary           = comparison
                  | "(" expr ")"
                  | call_expr ;

comparison        = ref ( "=" | "!=" | "has" ) value
                  | ref ;       (* truthiness *)

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

## 23. Out of scope for this file

- `.lzx` (experiences, surfaces, route declarations).
- `app.lzi` (deploy topology, env, runtime units).
- `registry.lzi` (env groups, capabilities, integrations, packs).
- `workspace.lzi` (multi-app contracts).
- `contract.lzi` (cross-language operation contracts).

Each gets a sibling grammar file. They share the lexical layer
(Section 1) and the predicate language (Section 20).
