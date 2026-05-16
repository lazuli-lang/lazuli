# Lazuli `registry.lzi` Grammar — Package Registry

**Status**: Reference grammar for `registry.lzi` (canonical indent).
Sibling of `docs/grammar.lzi.md`; shares lexical layer
(`docs/grammar.lzi.md §1`).

`registry.lzi` is the package-level catalog for env groups,
capabilities, integrations, packs, adapters, and other global
bindings. It is discovered automatically alongside `app.lzi`.

When the package layout is non-standard, `app.lzi` may import
specific entries; the canonical assumption is that `registry.lzi`
sits next to `app.lzi` and is loaded by convention.

## 1. Lexical layer

Identical to `docs/grammar.lzi.md §1`. Reserved words specific to
`registry.lzi`:

```
adapter adapters capabilities capability client credentials
environments env group integration integrations mobile name
optional packs pack provider provides registry required server
type version
```

## 2. File-level structure

```ebnf
file              = "registry" NEWLINE
                    INDENT registry_body DEDENT ;

registry_body     = ( env_block
                    | capabilities_block
                    | integrations_block
                    | packs_block
                    | adapters_block
                    )+ ;
```

## 3. Env declaration

```ebnf
env_block         = "env" NEWLINE
                    INDENT env_decl+ DEDENT ;

env_decl          = "group" IDENT_LOWER NEWLINE
                      INDENT env_var_decl+ DEDENT
                  | env_var_decl ;

env_var_decl      = ( "server" | "client" | "mobile" )
                    IDENT_UPPER ":" type_ref
                    ( "required" | "optional" )
                    ( "in" IDENT_LOWER )? NEWLINE ;

type_ref          = "Text" | "URL" | "Secret" | "Integer"
                  | "Boolean" | "JSON" | IDENT_UPPER ;
```

## 4. Capabilities

```ebnf
capabilities_block = "capabilities" NEWLINE
                     INDENT capability_decl+ DEDENT ;

capability_decl    = capability_kind IDENT_LOWER NEWLINE
                     INDENT capability_body DEDENT ;

capability_kind    = "database" | "queue" | "object_storage"
                   | "mailer" | "event_bus" | "tracing"
                   | "cache" | "search" | IDENT_LOWER ;

capability_body    = ( "provider" IDENT_LOWER NEWLINE
                     | "adapter" adapter_ref NEWLINE
                     | "optional" boolean NEWLINE
                     | "environments" env_list NEWLINE
                     )+ ;

adapter_ref        = "@runtime/" IDENT_LOWER
                   | "@plugin/" IDENT_LOWER "/" IDENT_LOWER
                   | "@adapter." IDENT_LOWER
                   | STRING ;

env_list           = IDENT_LOWER ( "," IDENT_LOWER )* ;
boolean            = "true" | "false" ;
```

## 5. Integrations

Integrations are provider-neutral catalog entries. They name a
capability slot that features may bind to via
`requires integration <slot>: <Capability>`.

```ebnf
integrations_block = "integrations" NEWLINE
                     INDENT integration_decl+ DEDENT ;

integration_decl   = "integration" IDENT_LOWER NEWLINE
                     INDENT integration_body DEDENT ;

integration_body   = ( "type" IDENT_UPPER NEWLINE
                     | "adapter" adapter_ref NEWLINE
                     | "environments" env_list NEWLINE
                     | "credentials" NEWLINE
                       INDENT credential_binding+ DEDENT
                     )+ ;

credential_binding = IDENT_LOWER "=" "env." IDENT_UPPER NEWLINE ;
```

## 6. Packs

```ebnf
packs_block       = "packs" NEWLINE
                    INDENT pack_decl+ DEDENT ;

pack_decl         = "pack" IDENT_LOWER NEWLINE
                    INDENT pack_body DEDENT ;

pack_body         = ( "name" pack_ref NEWLINE
                    | "version" STRING NEWLINE
                    | "provides" provides_kind feature_or_anchor_ref NEWLINE
                    | "requires" requires_decl NEWLINE
                    )+ ;

pack_ref          = "@runtime/" IDENT_LOWER
                  | "@plugin/" IDENT_LOWER "/" IDENT_LOWER ;

provides_kind     = "feature" | "anchor" | "view" | "validator"
                  | "client" ;
feature_or_anchor_ref = IDENT_LOWER
                      | "@anchor." IDENT_LOWER
                      | "@client." IDENT_LOWER
                      | "@validator." IDENT_LOWER ;

requires_decl     = "integration" IDENT_LOWER ":" IDENT_UPPER ;
```

## 7. Adapters

```ebnf
adapters_block    = "adapters" NEWLINE
                    INDENT adapter_decl+ DEDENT ;

adapter_decl      = "adapter" IDENT_LOWER ":" capability_kind NEWLINE
                    INDENT adapter_body DEDENT ;

adapter_body      = ( "name" adapter_ref NEWLINE
                    | "environments" env_list NEWLINE
                    | "version" STRING NEWLINE
                    | "credentials" NEWLINE
                      INDENT credential_binding+ DEDENT
                    )+ ;
```

## 8. Tools registry (Cut A)

This section reflects the proposal in
the `ai-primitives-v0` proposal (operational archive) Cut A (registry-side IR
extension). It will move from "proposal" to "shipped" when Cut A
implementation lands.

```ebnf
tools_block       = "tools" NEWLINE
                    INDENT tool_decl+ DEDENT ;

tool_decl         = "tool" tool_dotted_path NEWLINE
                    INDENT tool_body DEDENT ;

tool_dotted_path  = IDENT_LOWER ( "." IDENT_LOWER )* ;

tool_body         = ( "effect" tool_effect NEWLINE
                    | "pii_class" pii_class_list NEWLINE
                    | "adapter" adapter_ref NEWLINE
                    | "result" result_decl
                    )+ ;

tool_effect       = "read" | "write" ;
pii_class_list    = "@pii." IDENT_LOWER ( "," "@pii." IDENT_LOWER )* ;

result_decl       = "Record" NEWLINE
                    INDENT result_field+ DEDENT ;
result_field      = IDENT_LOWER ":" type_ref
                    ( "required" | "optional" ) NEWLINE ;
```

## 9. Validations not in this grammar

- `capability_kind` extends a closed catalog (`database`, `queue`,
  `object_storage`, `mailer`, `event_bus`, `tracing`, `cache`,
  `search`); other identifiers parse but doctor warns. Registry
  authors should not invent new capability kinds without a language
  cut.
- `pack_ref` adapter sources are validated against the closed
  provenance set: `@runtime/...`, `@plugin/<publisher>/<name>`,
  `@adapter.<local>`, or local path. Anything else is rejected by
  `lazuli check` with `pack_ref_provenance_unknown_diagnostics`.
- `provides feature <name>` requires the named feature to actually
  ship inside the pack. `lazuli doctor` checks the package set.
- `tool effect` is required (per the Cut A proposal). Adapters
  whose operations have both effects must register two named tools.
- `pii_class @pii.<name>` references must use the closed `@pii.*`
  catalog declared in `docs/canonical-semantics.md
  §Reference Namespaces`.
- `env required in <environment>` references must match the
  environment list in `app.lzi environments` once the app loads.

Cross-file checks live in `crates/lazuli_cli/src/doctor.rs`.

## 10. Out of scope

- Provider-specific configuration. `registry.lzi` declares *what
  exists* and *which adapter binds to it*; the adapter implements
  the provider details.
- Operation-level schemas for integrations. Operation contracts
  live in `contract.lzi` (see `docs/grammar.contract.md`).
- DI mechanics. `registry.lzi` is a catalog, not a wiring graph.
  The runtime materializes the actual injection.
