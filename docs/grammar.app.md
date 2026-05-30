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
                  | "lazuli_version" STRING NEWLINE
                  | "default_locale" STRING NEWLINE
                  | "default_timezone" STRING NEWLINE
                  | "subscription" "resource" qualified_resource NEWLINE ;
qualified_resource = IDENT_LOWER "." IDENT_LOWER ;

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

## 6.1. Request locale negotiation in runtime units

The `locale_negotiate` block optionally decorates a `unit` declaration (typically `unit api`) to specify how the runtime selects the effective locale for a request. All three children are optional; the runtime defaults to `source accept_language` and `strategy best_match` when the block is omitted entirely.

```ebnf
locale_negotiate_block = "locale_negotiate" NEWLINE
                         INDENT locale_negotiate_body DEDENT ;

locale_negotiate_body = ( "source" source_axis NEWLINE
                        | "strategy" match_strategy NEWLINE
                        | "fallback" STRING NEWLINE
                        )* ;

source_axis           = "accept_language" | "query_param" | "cookie"
                      | "user_profile" | "subdomain" ;

match_strategy        = "best_match" | "prefix_match" | "exact_match" ;
```

When present, `locale_negotiate` declares three orthogonal axes:

- **`source <axis>`** — Where the runtime reads the locale hint from the request: request `Accept-Language` header (default), query parameter, cookie, user profile, or subdomain. Parsed by `crates/lazuli_syntax/src/parser/lzi/locale.rs:56-57`.
- **`strategy <name>`** — How the runtime matches the hint against `app.locale.supported` tags: best-match (prefer longest prefix; default), prefix-match (any prefix), or exact-match (exact BCP-47 tag). Parsed by `crates/lazuli_syntax/src/parser/lzi/locale.rs:60-61`.
- **`fallback "<locale>"`** — A BCP-47 tag to use if negotiation fails. Must appear in `app.locale.supported`. Parsed by `crates/lazuli_syntax/src/parser/lzi/locale.rs:64-65`.

This block lowers to `lazuli_ir::nodes::locale_negotiate::LocaleNegotiate` (`crates/lazuli_ir/src/nodes/app_manifest/locale.rs:53-71`).

**Example:**

```
  runtime
    unit api
      serves queries, commands, webhooks, apis
      locale_negotiate
        source accept_language
        strategy best_match
        fallback "pt-BR"
```

(From `examples/full-capsule/app.lzi:96-99`)

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

## 10. Locale, CORS, logging, tracing, and encryption

```ebnf
locale_block      = "locale" NEWLINE
                    INDENT locale_entry+ DEDENT ;

locale_entry      = "default" STRING NEWLINE
                  | "supported" string_list NEWLINE
                  | "fallback" STRING "->" STRING NEWLINE ;

string_list       = STRING ( "," STRING )* ;

cors_block        = "cors" NEWLINE
                    INDENT cors_entry+ DEDENT ;

cors_entry        = "allow_origins" IDENT_LOWER string_list NEWLINE
                  | "allow_credentials" boolean NEWLINE
                  | "max_age" STRING NEWLINE ;

logging_block     = "logging" NEWLINE
                    INDENT logging_entry+ DEDENT ;

logging_entry     = "level" logging_level NEWLINE
                  | "format" log_format NEWLINE
                  | "redact" redact_policy NEWLINE
                  | "sample_rate" FLOAT NEWLINE ;

logging_level     = "debug" | "info" | "warn" | "error" ;
log_format        = "json" | "text" ;
redact_policy     = "pii" | "none" ;

tracing_block     = "tracing" NEWLINE
                    INDENT tracing_entry+ DEDENT ;

tracing_entry     = "propagate" boolean NEWLINE
                  | "sample_rate" FLOAT NEWLINE
                  | "exporter" IDENT_LOWER NEWLINE ;

encryption_block  = "encryption" NEWLINE
                    INDENT encryption_binding+ DEDENT ;

encryption_binding = "key" key_scope NEWLINE
                     INDENT encryption_body DEDENT ;

key_scope         = "@key." ( "app" | "tenant" | "user" | "record" ) ;

encryption_body   = ( "source" encryption_source NEWLINE
                    | "algorithm" encryption_algorithm NEWLINE
                    | "rotation" encryption_rotation NEWLINE
                    )+ ;

encryption_source = "env." IDENT_UPPER ( "{" template_axes "}" )?
                  | "secrets." IDENT_LOWER ( "{" template_axes "}" )? ;

template_axes     = "{" IDENT_LOWER "}" ( "_{" IDENT_LOWER "}" )* ;
encryption_algorithm = "aes_256_gcm" ;
encryption_rotation = "manual" ;
```

The `locale` block declares supported BCP-47 language tags and fallback edges. The `cors` block specifies per-environment origin allowlists. `logging` / `tracing` declare the observability contract (level, format, sampling rate, propagation). `encryption` binds `@key.<scope>` references to their source (env var or secrets backend) with an algorithm and rotation strategy.

Example (from `examples/full-capsule/app.lzi`):
```
locale
  default "pt-BR"
  supported "pt-BR", "en-US"
  fallback "en-US" -> "pt-BR"

cors
  allow_origins production "https://app.acme.example"
  allow_origins local "http://localhost:3000"
  allow_credentials true
  max_age "1h"

logging
  level info
  format json
  redact pii

tracing
  propagate true
  sample_rate 0.1

encryption
  key @key.tenant
    source env.CRYPT_KEY_TENANT_{tenant_id}
    algorithm aes_256_gcm
    rotation manual
```

## 11. Security and locality blocks

```ebnf
headers_block     = "headers" NEWLINE
                    INDENT headers_entry* DEDENT ;

headers_entry     = "csp" STRING NEWLINE
                  | "hsts" ( hsts_inline )? NEWLINE
                    ( INDENT hsts_child* DEDENT )?
                  | "x_frame_options" STRING NEWLINE
                  | "x_content_type_options" STRING NEWLINE
                  | "referrer_policy" STRING NEWLINE
                  | "permissions_policy" STRING NEWLINE ;

hsts_inline       = "max_age" INTEGER ( "include_subdomains" )? ( "preload" )? ;

hsts_child        = "max_age" INTEGER NEWLINE
                  | "include_subdomains" NEWLINE
                  | "preload" NEWLINE ;

cookie_block      = "cookie" NEWLINE
                    INDENT cookie_profile+ DEDENT ;

cookie_profile    = IDENT_LOWER NEWLINE
                    INDENT cookie_profile_entry* DEDENT ;

cookie_profile_entry = "signed" boolean NEWLINE
                     | "secure" boolean NEWLINE
                     | "http_only" boolean NEWLINE
                     | "same_site" ( "lax" | "strict" | "none" ) NEWLINE
                     | "max_age" STRING NEWLINE ;

proxy_block       = "proxy" NEWLINE
                    INDENT proxy_entry* DEDENT ;

proxy_entry       = "trusted" cidr_list NEWLINE
                  | "real_ip_header" STRING NEWLINE
                  | "forwarded_proto_header" STRING NEWLINE
                  | "forwarded_host_header" STRING NEWLINE ;

cidr_list         = IDENT_LOWER ( "," IDENT_LOWER )* ;

limits_block      = "limits" NEWLINE
                    INDENT limits_entry* DEDENT ;

limits_entry      = "body_size" STRING NEWLINE
                  | "header_size" STRING NEWLINE
                  | "upload_size" STRING NEWLINE
                  | "timeout" STRING NEWLINE ;

boolean           = "true" | "false" ;
```

HTTP security and request-shaping blocks declare the runtime's edge-layer policy for cookies (RFC 6265 with same-site/signed/secure attributes), trusted upstream proxies (real-IP header overrides), request ceilings (body/header/upload size + timeout), and HTTP security headers (CSP, HSTS, frame-options, etc.). All blocks are optional; `None` in any slot means "runtime default applies."

**Fixture example:**

```
app MyApp
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
    session
      same_site lax

  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto

  limits
    body_size "10mb"
    header_size "16kb"
    timeout "30s"

  headers
    csp "default-src 'self'"
    hsts max_age 31536000 include_subdomains preload
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy strict-origin-when-cross-origin
    permissions_policy "geolocation=()"
```

## 12. Validations not in this grammar

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

- `cookie` profile names must be valid identifiers (`[a-z_][a-z0-9_]*`);
- `cookie` `same_site` values are closed-catalog: `lax`, `strict`, `none`.
- `cookie` `signed`, `secure`, `http_only` accept boolean literals.
- `proxy` `trusted` entries are CIDR notation; doctor validates syntax.
- `limits` size/duration values are quoted strings; doctor validates
- `headers` `x_content_type_options` only admits `nosniff`.
- `headers` `x_frame_options` admits `DENY`, `SAMEORIGIN`, or
- `headers` `referrer_policy` is closed-catalog per W3C spec.
- `headers` `hsts` `max_age` must be a non-negative integer (seconds);

Doctor (`crates/lazuli_cli/src/doctor/mod.rs`) enforces these.
