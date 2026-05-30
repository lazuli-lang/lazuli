# Lazuli `workspace.lzi` Grammar — Distributed System Contract

**Status**: Reference grammar for `workspace.lzi` (canonical indent).
Sibling of `docs/grammar.lzi.md`; shares lexical layer
(`docs/grammar.lzi.md §1`).

`workspace.lzi` is the **optional** distributed-system contract for
multi-app, polyrepo, external-service, and gateway graphs. A
single-app project does not need it. This is enforced as a hard
project-design rule; reject any proposal that makes
`workspace.lzi` mandatory.

The file declares:

- Local apps (Lazuli applications living in this workspace).
- External apps (services in other languages exposing typed
  contracts).
- Shared registry (env, capabilities, integrations available to
  all apps).
- Boundaries (event topology between apps).
- Communication (default propagation, timeouts, retries across the
  workspace).
- Gateway (provider-neutral ingress).

## 1. Lexical layer

Identical to `docs/grammar.lzi.md §1`. Reserved words specific to
`workspace.lzi`:

```
apps async at boundaries communication contract default external
gateway internal local locale path propagate publishes registry
required retry route shared subscribes sync timeout to topology
url version workspace
```

## 2. File-level structure

```ebnf
file              = "workspace" IDENT_LOWER NEWLINE
                    INDENT workspace_body DEDENT ;

workspace_body    = ( meta_stmt
                    | apps_block
                    | shared_registry_block
                    | boundaries_block
                    | communication_block
                    | gateway_block
                    )+ ;

meta_stmt         = "version" STRING NEWLINE
                  | "default_locale" STRING NEWLINE
                  | "default_timezone" STRING NEWLINE ;
```

## 3. Apps block

```ebnf
apps_block        = "apps" NEWLINE
                    INDENT app_entry+ DEDENT ;

app_entry         = local_app_decl
                  | external_app_decl ;

local_app_decl    = "local" IDENT_LOWER NEWLINE
                    INDENT local_app_body DEDENT ;

local_app_body    = ( "path" STRING NEWLINE
                    | "uses" feature_ref_list NEWLINE
                    | "publishes" event_pattern_list NEWLINE
                    | "subscribes" event_pattern_list NEWLINE
                    )+ ;

external_app_decl = "external" IDENT_LOWER NEWLINE
                    INDENT external_app_body DEDENT ;

external_app_body = ( "contract" STRING NEWLINE
                    | "url" IDENT_LOWER STRING NEWLINE
                    | "publishes" event_pattern_list NEWLINE
                    | "subscribes" event_pattern_list NEWLINE
                    )+ ;

feature_ref_list  = IDENT_LOWER ( "," IDENT_LOWER )* ;
event_pattern_list = event_pattern ( "," event_pattern )* ;
event_pattern     = IDENT_LOWER ( "." IDENT_LOWER )* ( "*" )? ;
```

## 4. Shared registry

`workspace.lzi` may inline a shared registry that all local apps
inherit from in addition to their own `registry.lzi`:

```ebnf
shared_registry_block = "shared" "registry" NEWLINE
                        INDENT shared_registry_body DEDENT ;

shared_registry_body = ( "env" NEWLINE INDENT env_decl+ DEDENT
                       | "capabilities" NEWLINE INDENT capability_decl+ DEDENT
                       | "integrations" NEWLINE INDENT integration_decl+ DEDENT
                       )+ ;

(* env_decl, capability_decl, integration_decl: see grammar.registry.md *)
```

## 5. Boundaries

`boundaries` declares which app produces and consumes each event
class, making the cross-service event graph statically visible:

```ebnf
boundaries_block  = "boundaries" NEWLINE
                    INDENT boundary_entry+ DEDENT ;

boundary_entry    = event_class NEWLINE
                    INDENT boundary_body DEDENT ;

event_class       = IDENT_LOWER ( "." IDENT_LOWER )* ;

boundary_body     = ( "publishes" app_ref_list NEWLINE
                    | "subscribes" app_ref_list NEWLINE
                    )+ ;

app_ref_list      = IDENT_LOWER ( "," IDENT_LOWER )* ;
```

## 6. Communication

Defaults that apply across apps unless overridden by an app's own
`app.lzi communication` block:

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

## 7. Gateway

`gateway` is the provider-neutral ingress contract. Lazuli owns the
*shape* (which apps mount which routes); the runtime owns provider
specifics (Envoy, K8s ingress, Cloud Run, service mesh).

```ebnf
gateway_block     = "gateway" IDENT_LOWER NEWLINE
                    INDENT gateway_body DEDENT ;

gateway_body      = ( "url" IDENT_LOWER STRING NEWLINE
                    | "route" gateway_route_decl
                    )+ ;

gateway_route_decl = STRING ( "to" gateway_target )? NEWLINE
                     ( INDENT gateway_route_body DEDENT )? ;

gateway_target    = IDENT_LOWER "." IDENT_LOWER ;       (* app.feature *)

gateway_route_body = ( "rate_limit" STRING NEWLINE
                     | "timeout" STRING NEWLINE
                     | "audience" IDENT_LOWER NEWLINE
                     )+ ;
```

## 8. Validations not in this grammar

- `local <app>` `path` must point at an existing directory
  containing an `app.lzi`. `lazuli doctor` walks the workspace.
- `external <app>` `contract` must point at an existing
  `contract.lzi` (or import-supported file: OpenAPI, AsyncAPI,
  Proto, JSON Schema, Avro).
- `local <app>` `uses` features must exist in that app's
  `app.lzi`.
- `boundaries event_class` patterns must be matched by at least
  one local-app `publishes` declaration.
- `gateway route` targets must exist as `api` or `command`
  exposures in the named app's services list.
- `shared registry` definitions are merged with each app's
  registry; conflicts are doctor errors.

`crates/lazuli_cli/src/doctor/mod.rs` `workspace_contract_diagnostics`
already covers most of these.

## 9. Out of scope

- Concrete service-mesh / proxy / load-balancer configuration. Those
  are adapter or runtime concerns.
- Provider-specific gateway features (TLS termination details,
  WAF rules, custom middleware). Adapters expose those.
- Multi-region / replication topology. Runtime decision; the
  workspace declares the contract, the runtime decides how to
  materialize it.
- Per-app deploy configuration. Each app's own `app.lzi deploy`
  declares its topology.
