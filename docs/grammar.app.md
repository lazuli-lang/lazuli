# Lazuli `app.lzi` Grammar — Application Manifest

**Status**: Reference grammar for `app.lzi` (canonical indent).
Sibling of `docs/grammar.lzi.md`; shares lexical layer
(`docs/grammar.lzi.md §1`).

`app.lzi` declares the provider-neutral operational contract for a
Lazuli application: which features participate, which targets get
generated, how environments are configured, what runtime units
serve which workloads, and the deploy topology.

A repo has at most one `app.lzi` per Lazuli application root. A
multi-app repo uses sibling app roots, optionally with a
`workspace.lzi` (see `docs/grammar.workspace.md`).

## 1. Lexical layer

Identical to `docs/grammar.lzi.md §1`. Reserved words specific to
`app.lzi`:

```
api app architecture async backend bindings capabilities client
communication compatibility consumes contract default
default_locale default_policy default_timezone deploy destructive_migrations
enforce_service_boundaries env environments expose exposes
external gateway group healthcheck integration integrations
internal locale migration_lock migrations mobile mode optional
on_unauthenticated on_unauthorized owns packs path policy production
propagate provider provides publishes queue queues rate_limit readiness
redirect required require_approval restore retry route_guard rpc runs runtime
serves service service_ready services stream subscribes sync targets timeout
title topology unit units uses validation version web webhooks
workflows
```

## 2. File-level structure

```ebnf
file              = "app" IDENT_UPPER NEWLINE
                    INDENT app_body DEDENT ;

app_body          = ( meta_stmt
                    | uses_block
                    | bindings_block
                    | targets_block
                    | environments_block
                    | urls_block
                    | env_block
                    | architecture_block
                    | services_block
                    | communication_block
                    | runtime_block
                    | capabilities_block
                    | integrations_block
                    | packs_block
                    | deploy_block
                    | migrations_block
                    | not_found_redirect
                    | auth_failed_redirect
                    | actor_query_decl
                    | route_guard_block
                    )+ ;

meta_stmt         = "title" STRING NEWLINE
                  | "version" STRING NEWLINE
                  | "default_locale" STRING NEWLINE
                  | "default_timezone" STRING NEWLINE ;

not_found_redirect = "not_found" IDENT_LOWER NEWLINE ;
auth_failed_redirect = "auth_failed_redirect" IDENT_LOWER NEWLINE ;
actor_query_decl  = "actor_query" qualified_query_ref NEWLINE ;

route_guard_block = "route_guard" NEWLINE
                    INDENT route_guard_entry* DEDENT ;

route_guard_entry = "default_policy" policy_ref NEWLINE
                  | "on_unauthenticated" "redirect" STRING NEWLINE
                  | "on_unauthorized" "redirect" STRING NEWLINE
                  | "skeleton" "@client." IDENT_LOWER NEWLINE ;

qualified_query_ref = IDENT_LOWER "." "query" "." IDENT_LOWER ;
policy_ref        = "@policy." IDENT_LOWER
                  | "@scope." IDENT_LOWER
                  | "@role." IDENT_LOWER
                  | "@actor." IDENT_LOWER ;
```

## 3. Uses, bindings, targets, environments, urls

```ebnf
uses_block        = "uses" NEWLINE
                    INDENT feature_ref+ DEDENT ;
feature_ref       = IDENT_LOWER NEWLINE ;

bindings_block    = "bindings" NEWLINE
                    INDENT binding_entry+ DEDENT ;
binding_entry     = qualified_slot "=" qualified_integration NEWLINE ;
qualified_slot    = IDENT_LOWER "." IDENT_LOWER ;
qualified_integration = "integrations" "." IDENT_LOWER ;

targets_block     = "targets" NEWLINE
                    INDENT target_entry+ DEDENT ;
target_entry      = ( "backend" "go"
                    | "web" "react"
                    | "mobile" "expo"
                    | IDENT_LOWER IDENT_LOWER
                    ) NEWLINE ;

environments_block = "environments" NEWLINE
                     INDENT env_name+ DEDENT ;
env_name          = IDENT_LOWER NEWLINE ;

urls_block        = "urls" NEWLINE
                    INDENT url_entry+ DEDENT ;
url_entry         = ( "web" | "api" ) IDENT_LOWER STRING NEWLINE ;

env_block         = "env" NEWLINE
                    INDENT env_decl+ DEDENT ;
env_decl          = ( "group" IDENT_LOWER NEWLINE
                      INDENT env_var_decl+ DEDENT
                    | env_var_decl
                    ) ;
env_var_decl      = ( "server" | "client" | "mobile" ) IDENT_UPPER ":"
                    type_ref ( "required" | "optional" )
                    ( "in" IDENT_LOWER )? NEWLINE ;
type_ref          = "Text" | "URL" | "Secret" | "Integer"
                  | "Boolean" | "JSON" | IDENT_UPPER ;
```

## 4. Architecture and services

```ebnf
architecture_block = "architecture" NEWLINE
                     INDENT architecture_entry+ DEDENT ;
architecture_entry = "mode" architecture_mode NEWLINE
                   | "service_ready" boolean NEWLINE
                   | "enforce_service_boundaries" boolean NEWLINE ;
architecture_mode  = "monolith" | "modular_monolith"
                   | "microservices" ;

services_block    = "services" NEWLINE
                    INDENT service_decl+ DEDENT ;

service_decl      = "service" IDENT_LOWER NEWLINE
                    INDENT service_body DEDENT ;

service_body      = ( "owns" feature_ref_list NEWLINE
                    | "exposes" NEWLINE INDENT exposure+ DEDENT
                    | "publishes" event_pattern_list NEWLINE
                    | "consumes" event_pattern_list NEWLINE
                    | "subscribes" event_pattern_list NEWLINE
                    )+ ;

feature_ref_list  = IDENT_LOWER ( "," IDENT_LOWER )* ;
exposure          = ( "command" | "query" | "api" | "webhook" )
                    qualified_op NEWLINE ;
qualified_op      = IDENT_LOWER "." ( "command" | "query" | "api"
                                    | "webhook" ) "." IDENT_LOWER ;
event_pattern_list = event_pattern ( "," event_pattern )* ;
event_pattern     = IDENT_LOWER ( "." IDENT_LOWER )* ( "*" )? ;

boolean           = "true" | "false" ;
```

## 5. Communication

```ebnf
communication_block = "communication" NEWLINE
                      INDENT communication_entry+ DEDENT ;

communication_entry = "internal" comm_protocol NEWLINE
                    | "external" comm_protocol NEWLINE
                    | "async" comm_protocol NEWLINE
                    | "propagate" propagate_list NEWLINE
                    | "timeout" "default" STRING NEWLINE
                    | "retry" "default" INTEGER
                      ( "backoff" backoff_strategy )? NEWLINE ;

comm_protocol     = "sync" "rpc" | "http" | "event_bus" | IDENT_LOWER ;
propagate_list    = propagate_atom ( "," propagate_atom )* ;
propagate_atom    = "actor" | "tenant" | "trace_id" | "request_id"
                  | IDENT_LOWER ;
backoff_strategy  = "exponential" | "linear" | "constant" ;
```

## 6. Runtime units

```ebnf
runtime_block     = "runtime" NEWLINE
                    INDENT runtime_unit+ DEDENT ;

runtime_unit      = "unit" IDENT_LOWER NEWLINE
                    INDENT runtime_unit_body DEDENT ;

runtime_unit_body = ( "serves" runtime_serves NEWLINE
                    | "runs" runtime_runs NEWLINE
                    | "healthcheck" STRING NEWLINE
                    | "readiness" STRING NEWLINE
                    | "rate_limit" STRING NEWLINE
                    )+ ;

runtime_serves    = serves_kind ( "," serves_kind )* ;
serves_kind       = "queries" | "commands" | "webhooks" | "apis"
                  | "surfaces" platform_target ;
runtime_runs      = ( "jobs" | "workflows" ) ( "*" | ident_list ) ;
platform_target   = "web" | "mobile" ;
ident_list        = IDENT_LOWER ( "," IDENT_LOWER )* ;
```

## 7. Capabilities

```ebnf
capabilities_block = "capabilities" NEWLINE
                     INDENT capability_decl+ DEDENT ;

capability_decl   = capability_kind IDENT_LOWER NEWLINE
                    INDENT capability_body DEDENT ;

capability_kind   = "database" | "queue" | "object_storage" | "mailer"
                  | "event_bus" | "tracing" | "cache" | "search"
                  | IDENT_LOWER ;

capability_body   = ( "provider" IDENT_LOWER NEWLINE
                    | "adapter" adapter_ref NEWLINE
                    | "optional" boolean NEWLINE
                    )+ ;

adapter_ref       = "@runtime/" IDENT_LOWER
                  | "@plugin/" IDENT_LOWER "/" IDENT_LOWER
                  | "@adapter." IDENT_LOWER
                  | STRING ;          (* path-based local adapter *)
```

## 8. Integrations and packs

```ebnf
integrations_block = "integrations" NEWLINE
                     INDENT integration_decl+ DEDENT ;

integration_decl   = "integration" IDENT_LOWER NEWLINE
                     INDENT integration_body DEDENT ;

integration_body   = ( "type" IDENT_UPPER NEWLINE
                     | "adapter" adapter_ref NEWLINE
                     | "environments" env_list NEWLINE
                     | "credentials" NEWLINE INDENT credential_binding+ DEDENT
                     )+ ;

env_list           = IDENT_LOWER ( "," IDENT_LOWER )* ;
credential_binding = IDENT_LOWER "=" "env." IDENT_UPPER NEWLINE ;

packs_block        = "packs" NEWLINE
                     INDENT pack_entry+ DEDENT ;

pack_entry         = pack_ref ( "version" STRING )? NEWLINE ;
pack_ref           = "@runtime/" IDENT_LOWER
                   | "@plugin/" IDENT_LOWER "/" IDENT_LOWER ;
```

## 9. Deploy and migrations

```ebnf
deploy_block      = "deploy" NEWLINE
                    INDENT deploy_entry+ DEDENT ;

deploy_entry      = "topology" deploy_topology NEWLINE
                  | "provider" IDENT_LOWER NEWLINE
                  | "environments" env_list NEWLINE ;

deploy_topology   = "single_region" | "multi_region"
                  | "edge" | IDENT_LOWER ;

migrations_block  = "migrations" NEWLINE
                    INDENT migration_entry+ DEDENT ;

migration_entry   = "migration_lock" boolean NEWLINE
                  | "destructive_migrations" "require_approval" NEWLINE
                  | "compatibility" compatibility_kind NEWLINE ;

compatibility_kind = "backward" | "forward" | "none" ;
```

## 10. Validations not in this grammar

- `bindings` keys must reference a feature `requires integration`
  slot.
- `services owns` must list features actually present in `uses`.
- `services exposes` operations must exist in the owned features.
- `services publishes` patterns must match events from owned
  features.
- `services consumes` patterns must reference events declared by
  another feature in `uses`.
- `urls` env names must match `environments`.
- `env required in <env>` must reference a declared environment.
- `runtime` `serves surfaces <platform>` requires the platform to
  appear in `targets`.
- `auth_failed_redirect` and `not_found` route names must exist in
  `.lzx` route declarations.
- `route_guard` appears at most once. Its `default_policy`,
  `on_unauthenticated`, `on_unauthorized`, and `skeleton` children each appear
  at most once.
- `actor_query` references an existing query whose runtime shape is
  `LazuliActor | null`.
- `on_unauthenticated` and `on_unauthorized` redirect paths must exist in
  `.lzx` route declarations.
- `architecture mode microservices` requires
  `enforce_service_boundaries true`.

Doctor (`crates/lazuli_cli/src/doctor.rs`) enforces these.
