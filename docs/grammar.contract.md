# Lazuli `contract.lzi` Grammar — External Service Contracts

**Status**: Reference grammar for `contract.lzi` (canonical indent).
Sibling of `docs/grammar.lzi.md`; shares lexical layer
(`docs/grammar.lzi.md §1`).

`contract.lzi` declares the typed contract that a non-Lazuli service
exposes to a Lazuli application: records, operations (HTTP/RPC/event
transports), and events. It also imports schemas from external
schema languages (OpenAPI, AsyncAPI, Proto, JSON Schema, Avro).

The runtime materializes typed Go transport bindings from this
contract; adapters do the actual HTTP/RPC/broker work. SDK exports
for client languages are publication artifacts, not core language.

## 1. Lexical layer

Identical to `docs/grammar.lzi.md §1`. Reserved words specific to
`contract.lzi`:

```
asyncapi auth avro backward backoff compatibility contract error
event events exponential expose forward http idempotency import
input json_schema method none openapi operation output path
payload proto record records require_approval required restore
retry returns rpc service status stream timeout topic transport
upstream version
```

## 2. File-level structure

```ebnf
file              = "contract" qualified_contract_name NEWLINE
                    INDENT contract_body DEDENT ;

qualified_contract_name = IDENT_LOWER ( "." IDENT_LOWER )* ;

contract_body     = ( meta_stmt
                    | import_stmt
                    | record_decl
                    | operation_decl
                    | event_decl
                    )+ ;

meta_stmt         = "version" STRING NEWLINE
                  | "compatibility" compatibility_kind NEWLINE
                  | "purpose" STRING NEWLINE ;

compatibility_kind = "backward" | "forward" | "none" ;
```

## 3. Imports (external schema languages)

```ebnf
import_stmt       = "import" import_format STRING NEWLINE ;
import_format     = "openapi" | "asyncapi" | "proto"
                  | "json_schema" | "avro" ;
```

`import openapi "./contracts/ai.openapi.json"` makes the imported
schema's records and operations available in the same namespace
as authored declarations. Naming conflicts are doctor errors.

## 4. Records

```ebnf
record_decl       = "record" IDENT_UPPER NEWLINE
                    INDENT record_field+ DEDENT ;

record_field      = IDENT_LOWER ":" type_ref field_marker*
                    ( "required" | "optional" ) NEWLINE ;

type_ref          = scalar_type
                  | semantic_type
                  | "@cap." IDENT_UPPER ( "(" cap_args ")" )?
                  | record_ref ;

scalar_type       = "ID" | "Text" | "Boolean" | "Integer"
                  | "Decimal" | "Date" | "DateTime" | "JSON" ;
semantic_type     = "@semantic." IDENT_UPPER ;
record_ref        = IDENT_UPPER ;

field_marker      = "@pii." IDENT_LOWER
                  | "@key." IDENT_LOWER
                  | "@adapter." IDENT_LOWER ;

cap_args          = cap_arg ( "," cap_arg )* ;
cap_arg           = IDENT_LOWER ":" cap_arg_value ;
cap_arg_value     = STRING | INTEGER | DURATION | namespace_ref
                  | IDENT_LOWER ;
namespace_ref     = "@" IDENT_LOWER "." IDENT_LOWER ;
```

## 5. Operations (HTTP/RPC transports)

```ebnf
operation_decl    = "operation" IDENT_LOWER NEWLINE
                    INDENT operation_body DEDENT ;

operation_body    = ( "transport" transport_kind NEWLINE
                    | "method" http_method NEWLINE
                    | "path" STRING NEWLINE
                    | "input" record_ref NEWLINE
                    | "output" output_decl NEWLINE
                    | "auth" auth_kind NEWLINE
                    | "timeout" STRING NEWLINE
                    | "retry" INTEGER ( "backoff" backoff_strategy )? NEWLINE
                    | "idempotency" "by" idempotency_path
                      ( "," idempotency_path )* NEWLINE
                    | error_decl
                    )+ ;

transport_kind    = "http" | "rpc" | "grpc" | IDENT_LOWER ;
http_method       = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" ;
output_decl       = ( "stream" )? record_ref ;
auth_kind         = "service" | "user" | "none" | IDENT_LOWER ;
backoff_strategy  = "exponential" | "linear" | "constant" ;
idempotency_path  = ( "input" | "ctx" ) "." IDENT_LOWER
                    ( "." IDENT_LOWER )* ;

error_decl        = "error" IDENT_UPPER
                    ( "status" INTEGER )?
                    ( "expose" ident_list )? NEWLINE ;
ident_list        = IDENT_LOWER ( "," IDENT_LOWER )* ;
```

Per `docs/invariants.md §line 140-146`: error fields inside
`contract` operations expose schema-defined keys, not the
`message|code|data` envelope used by feature commands.

## 6. Events (broker-shaped contracts)

```ebnf
event_decl        = "event" IDENT_LOWER NEWLINE
                    INDENT event_body DEDENT ;

event_body        = ( "topic" STRING NEWLINE
                    | "transport" transport_kind NEWLINE
                    | "payload" NEWLINE INDENT record_field+ DEDENT
                    )+ ;
```

## 7. Validations not in this grammar

- `import openapi|asyncapi|...` paths must resolve to a real file
  in the package. Doctor checks.
- Imported records / operations conflict-checked against authored
  ones. Conflicts are doctor errors.
- `output stream <Record>` requires the transport to support
  streaming (HTTP+SSE, gRPC streaming). Doctor warns when transport
  is `rpc` and stream is declared.
- `auth service` requires the consuming app to declare a service
  identity in `app.lzi`.
- `error status <int>` must be a valid HTTP status when transport
  is `http`.
- `retry` with `backoff exponential` requires `timeout` to be
  declared.
- `idempotency by input.X` requires `X` to exist in the operation's
  input record.

`crates/lazuli_cli/src/doctor/mod.rs` covers cross-file validation.
LSP `lzx_contract_diagnostics`-pattern checks live in
`crates/lazuli_lsp/src/lib.rs`.

## 8. Relationship to other contracts

- `contract.lzi` declares **upstream** services consumed by Lazuli
  apps. Authored alongside the app or imported via
  `workspace.lzi external <app> contract "./..."`.
- `app.lzi services exposes` declares **downstream** operations
  the Lazuli app exposes to other services. The shape is similar
  but the file is different — `app.lzi` is the producer, contracts
  are the consumer.
- `feature.api` and `feature.command` and `feature.query` declare
  the operations a Lazuli **feature** exposes inside the app. They
  generate the typed Go transport bindings the runtime uses.

## 9. Out of scope

- SDK / client library generation. Optional publication artifact;
  not a language concern.
- Concrete transport configuration (TLS, headers, compression).
  Adapter / runtime concerns.
- Service discovery. Adapter / runtime / mesh concerns.
- Schema evolution beyond the `compatibility backward|forward|none`
  hint. Per-record migration semantics belong in the schema
  language being imported (Proto, Avro, etc.).
