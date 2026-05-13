# Bucket Cycle: Storage (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=storage`
pipeline. Implementation deferred to a separate run with
`mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-10.

## Contexto

The canonical fixture authors `@cap.File` on **both** sides of the
upload/download contract: as a resource field
(`examples/full-capsule/full-capsule.lzi:707` — `file:
@cap.File(max_size:25mb,accept:text/csv) required`) and as an api
output (`examples/full-capsule/full-capsule.lzi:305` — `output
@cap.File(max_size:100mb,accept:text/csv)`). The registry declares
`object_storage files`
(`examples/full-capsule/registry.lzi:15`). Doctor enforces that the
package declaring `@cap.File` also declares an `object_storage` /
`storage` capability (`crates/lazuli_cli/src/doctor.rs:1328-1336`,
diagnostic `APP-CAP-001`). The LSP file-local walker validates the
shape of `@cap.File(...)` arguments
(`crates/lazuli_lsp/src/lib.rs:2883-2957`).

What is missing is **typed lowering**: `@cap.File(max_size:25mb,
accept:text/csv)` lowers to either `TypeRef::UserDefined { name:
"@cap.File(max_size:25mb,accept:text/csv)" }` or
`TypeRef::Unresolved` because `type_ref_from_syntax`
(`crates/lazuli_analyzer/src/lib.rs:486-501`) does not recognise the
`@cap.<Name>(<args>)` shape. The IR enum `BuiltinType::CapFile`
(`crates/lazuli_ir/src/lib.rs:363`) exists but is never produced
from authored source, and even if it were, it has no slot for
`max_size` / `accept`. As a result, `lazuli inspect` cannot project
the contract, doctor cannot cross-check input/output symmetry,
codegen produces zero file/storage-related Go (confirmed by grepping
`crates/lazuli_codegen_go` and `dist/go/customer/` for `cap.File`),
and the Lazuli Go runtime ships no `lazuli.File` / `lazuli.ObjectStore`
helper at all (`runtime/go/lazuli/` has zero storage references).

The lowering route was decided in
`docs/proposals/bucket-storage-scope.md` (canonical input for this
run): **Route B** — add a typed `FileCapability` struct alongside the
existing `BuiltinType::CapFile`, wire lowering through the legacy
analyzer pipeline (the canonical-indent slice covers resource fields
only after Phase L row 24 lands), and keep other `@cap.*` decorators
text-pattern for now. Scope is the **4 typed children already
implied by the fixture** (`max_size`, `accept`, `signed`,
`public`/`private`); the speculative roadmap §1.26 kinds (`storage`,
`bucket`, `storage_quota`) are dropped from this design.

The closed-cycle criterion (3 new doctor diagnostics,
`--expand=storage` projection, LSP hover/completion on `@cap.File`
arguments, Go test for upload + signed URL) is the acceptance gate.
This proposal specifies the design for every stage of that gate so
the implementation run is mechanical.

## Baseline (Stages 1-2 inventory)

| Layer | Status | Anchor |
|---|---|---|
| Surface syntax (`.lzi`) | authored, 2 use sites | `examples/full-capsule/full-capsule.lzi:305` (api output), `:707` (resource field) |
| Registry capability | authored | `examples/full-capsule/registry.lzi:15` (`object_storage files`) |
| Grammar (`docs/grammar.app.md:202`, `docs/grammar.registry.md:69`) | `object_storage` listed as a capability kind | both grammars list it |
| IR (`crates/lazuli_ir`) | flat `BuiltinType::CapFile` enum variant; no args slot | `crates/lazuli_ir/src/lib.rs:363` |
| Analyzer lowering | broken: `@cap.File(...)` falls through to `TypeRef::UserDefined`/`Unresolved` | `crates/lazuli_analyzer/src/lib.rs:486-501` |
| Parser slice | not extended for resource fields (Phase L gap) | `crates/lazuli_syntax/src/parser.rs` (canonical slice covers `agent` only) |
| LSP (file-local text walk) | shape-only diagnostics: missing `max_size`, missing `accept`, malformed size literal, malformed MIME | `crates/lazuli_lsp/src/lib.rs:2883-2957` |
| Doctor cross-feature | one diagnostic (`APP-CAP-001`): package uses `@cap.File` but app/registry has no `object_storage`/`storage` capability | `crates/lazuli_cli/src/doctor.rs:1328-1336` |
| Doctor fact harvesting | text-pattern, plain-name (no args parsed) | `crates/lazuli_cli/src/doctor.rs:902-909` |
| Inspect projection | none — `lazuli inspect --format=json examples/full-capsule/full-capsule.lzi` emits zero `@cap.File` / file / accept / max_size keys | confirmed via probe |
| Codegen | none — `crates/lazuli_codegen_go` has zero references to `CapFile`/file/upload | confirmed via grep |
| Runtime (Lazuli Go) | none — `runtime/go/lazuli/` has zero storage helpers | confirmed via ls + grep |
| Highlighting | `@cap.File` colored by the generic capability scope; `max_size`/`accept` not specially highlighted | `editors/vscode/syntaxes/lazuli.tmLanguage.json` |
| Adapter slot | `object_storage` named in `is_allowed_capability_kind` closed set | `crates/lazuli_lsp/src/lib.rs:8670` |

**Cross-cutting fact**: `customer_export.output @cap.File(max_size:100mb,
accept:text/csv)` (`full-capsule.lzi:305`) and
`CustomerImportBatch.file: @cap.File(max_size:25mb,accept:text/csv)`
(`full-capsule.lzi:707`) form a **natural round-trip**: the export
emits CSV that, in principle, the import can re-ingest. Today nothing
in IR or doctor enforces that the accept lists agree, that the size
constraints are coherent, or that the export's output is signed/
visibility-tagged before bytes leave the runtime.

## Linguagem (Stage 3)

Surface is canonical for `@cap.File(max_size:..., accept:...)` —
already authored, already audited. Stage 3 is **documentation +
two additive arguments + one new decorator** to tighten the
contract.

### Formal grammar (EBNF, draft for `docs/grammar.lzi.md`)

```ebnf
cap_file              = "@cap.File" "(" cap_file_arg_list ")" ;

cap_file_arg_list     = cap_file_arg ( "," cap_file_arg )* ;

cap_file_arg          = "max_size"   ":" file_size_literal
                      | "accept"     ":" mime_list
                      | "signed_ttl" ":" duration_literal     (* new in this cut *)
                      | "visibility" ":" file_visibility ;    (* new in this cut *)

file_size_literal     = INTEGER ( "kb" | "mb" | "gb" ) ;       (* closed unit catalog *)

mime_list             = mime_type ( "|" mime_type )* ;         (* `|` separates alternatives; existing single-MIME form parses as a 1-element list *)

mime_type             = mime_family "/" mime_subtype ;
mime_family           = "text" | "image" | "application" | "audio" | "video" | "font" | "*" ;
mime_subtype          = IDENT_LOWER ( ( "." | "+" | "-" ) IDENT_LOWER )* | "*" ;

duration_literal      = INTEGER ( "s" | "m" | "h" | "d" ) ;    (* mirrors @cap.Token.ttl literal *)

file_visibility       = "public" | "private" | "signed" ;      (* closed catalog *)
```

### Slot inventory (required/optional + type + closed catalog)

| Slot | Required | Type | Closed catalog | Fixture anchor |
|---|---|---|---|---|
| `@cap.File(max_size:<size>)` | yes (already LSP-warned today; **upgrade to required for typed IR**) | size literal | `kb`, `mb`, `gb` (existing LSP catalog at `crates/lazuli_lsp/src/lib.rs:is_file_size_literal`) | `full-capsule.lzi:707, :305` |
| `@cap.File(accept:<mime>)` | yes (already LSP-warned today) | MIME type or `\|`-separated list | family in `{text, image, application, audio, video, font, *}`; subtype `*` allowed | `full-capsule.lzi:707, :305` |
| `@cap.File(signed_ttl:<duration>)` | **new** — optional when `visibility:public`, required-or-default when `visibility:signed`, forbidden when `visibility:private` | duration literal | `s`, `m`, `h`, `d` (mirrors `@cap.Token`) | not in fixture; Stage 3 adds to `customer_export.output` |
| `@cap.File(visibility:<mode>)` | **new** — required for `@cap.File` on an `api output`, optional on a resource field (default `private`) | identifier | **closed**: `public`, `private`, `signed` | not in fixture; Stage 3 adds to `customer_export.output` |

### Closed-catalog rationale

- `max_size` units `{kb, mb, gb}` already enforced for the LSP
  literal check (`crates/lazuli_lsp/src/lib.rs` `is_file_size_literal`).
  The IR axis must share the catalog so codegen has typed bytes.
- `accept` MIME family is constrained to the IANA top-level types
  plus `*`. The pipe-separated list syntax mirrors how multiple
  accepts surface in HTTP `Accept` headers; allowing it lets one
  api accept `text/csv|application/vnd.ms-excel` without authors
  inventing a special separator.
- `visibility ∈ {public, private, signed}` — these are the three
  distinct contracts a file URL can have:
  - `public`: the URL is unguessable but anyone with it can fetch
    (think CDN-served static assets).
  - `private`: the URL is gated by a policy-checking handler — the
    runtime mounts a download endpoint that re-checks the command's
    `policy` per request.
  - `signed`: the URL carries a time-limited signature; once
    expired, the URL no longer works. Requires `signed_ttl`.
- `signed_ttl` literal `{s, m, h, d}` — same closed set as
  `@cap.Token(ttl:...)` (`docs/invariants.md:413-415`). Keeping the
  catalog uniform across capabilities reduces LLM hallucination cost.

### Example expansion in the fixture

Stage 3 extends `full-capsule.lzi:302-308` to make the visibility
contract explicit:

```lazuli
  api customer_export
    method GET
    path "/api/customers/export"
    output @cap.File(max_size:100mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    rate_limit "10 per hour per user"
    handler "./api/export_customers.go"
```

And `full-capsule.lzi:705-711` to make the input visibility
contract explicit:

```lazuli
    resource CustomerImportBatch
      uploaded_by: User required
      file: @cap.File(max_size:25mb,accept:text/csv,visibility:private) required
      status: ImportStatus = uploaded
      ...
```

The two new arguments are **additive** — every existing `@cap.File`
authoring without them keeps parsing, but the doctor diagnostic
`cap_file_visibility_undeclared_diagnostics` (Stage 8) warns when
the field is on an api output without `visibility`.

## IR (Stage 4)

The IR shape needs a new struct and a `TypeRef` variant carrying
the parsed options. Today's flat `BuiltinType::CapFile` enum variant
is insufficient because it has no slot for arguments.

### IR additions

Two additive types. Recommended placement: next to `BuiltinType` and
`TypeRef` in `crates/lazuli_ir/src/lib.rs:340-364`.

```rust
// crates/lazuli_ir/src/lib.rs — additive
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCapability {
    pub max_size: FileSize,
    /// At least one entry. Each entry is `family/subtype`; subtype
    /// `*` and family `*` are both valid wildcard markers.
    pub accept: Vec<MimeType>,
    /// `None` is the parse-time default (`private` on a field,
    /// **required** on an api output — analyzer raises if missing).
    pub visibility: Option<FileVisibility>,
    /// `Some` only when `visibility == Signed`. Mutually exclusive
    /// with `visibility == Private` (doctor enforces).
    pub signed_ttl: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSize {
    pub bytes: u64,
    pub literal: FileSizeLiteral, // preserved for inspect round-trip
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileSizeLiteral {
    Kb(u32),
    Mb(u32),
    Gb(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MimeType {
    pub family: String,   // "text" | "image" | ... | "*"
    pub subtype: String,  // e.g. "csv", "vnd.ms-excel", "*"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileVisibility {
    Public,
    Private,
    Signed,
}

// Existing TypeRef enum gains a new variant:
pub enum TypeRef {
    Builtin(BuiltinType),
    UserDefined(QualifiedName),
    EnumRef(QualifiedName),
    Many(Box<TypeRef>),
    Unresolved(String),
    Capability(CapabilityRef),  // new
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CapabilityRef {
    File(FileCapability),
    // future: Hashed(HashedCapability), Encrypted(EncryptedCapability), ...
}
```

`BuiltinType::CapFile` stays — Stage 4 deprecates it (analyzer no
longer emits it; format helper handles back-compat). Per Route B,
other `@cap.*` decorators (`Hashed`, `Encrypted`, `Token`) stay as
text-pattern in LSP for this cut; the `CapabilityRef` enum exists
so they can join later without another schema change.

### Surface → IR mapping

| Surface | IR field | Notes |
|---|---|---|
| `@cap.File(max_size:25mb,accept:text/csv)` on a resource field | `ResourceField.type_ref = TypeRef::Capability(CapabilityRef::File(FileCapability { max_size, accept, visibility: None, signed_ttl: None }))` | default `visibility = None` analyzer interprets as `Private` on field; doctor doesn't warn on omission. |
| `output @cap.File(max_size:100mb,accept:text/csv,visibility:signed,signed_ttl:1h)` on an `api` | `Api.output = ApiOutput::File(FileCapability { ... })` | requires new `ApiOutput` enum / variant; today `Api.output` is `TypeRef`, which Route B's `TypeRef::Capability` covers transparently. |

### Inspect JSON shape (`lazuli inspect --format=json --expand=storage`)

New top-level `--expand=storage` flag in `ExpandSet`
(`crates/lazuli_cli/src/main.rs:98-118`). Projection:

```json
{
  "features": [
    {
      "name": "customer_import",
      "storage": {
        "fields": [
          {
            "resource": "CustomerImportBatch",
            "field": "file",
            "file_capability": {
              "max_size": { "bytes": 26214400, "literal": "25mb" },
              "accept": [ { "family": "text", "subtype": "csv" } ],
              "visibility": "private",
              "signed_ttl": null
            },
            "origin": "examples/full-capsule/full-capsule.lzi:707"
          }
        ],
        "api_outputs": []
      }
    },
    {
      "name": "customer",
      "storage": {
        "fields": [],
        "api_outputs": [
          {
            "api": "customer_export",
            "file_capability": {
              "max_size": { "bytes": 104857600, "literal": "100mb" },
              "accept": [ { "family": "text", "subtype": "csv" } ],
              "visibility": "signed",
              "signed_ttl": "1h"
            },
            "origin": "examples/full-capsule/full-capsule.lzi:305"
          }
        ]
      }
    }
  ]
}
```

Normalisation rules:

- `accept` is always an array even with a single MIME (mirrors the
  `oauth` projection in the auth proposal).
- `signed_ttl` is `null` unless `visibility == "signed"`.
- `max_size.bytes` is canonicalised at lowering; `literal` is the
  authored form preserved for inspect round-trip.
- Features without any `@cap.File` usage have `storage` omitted
  from the projection (mirrors the agent/tools convention).
- Without `--expand=storage` the `storage` key is omitted entirely.

### Cross-refs the analyzer must register

| Edge | Source field | Target | Resolution scope |
|---|---|---|---|
| `field.file_capability` ↔ `app.capabilities.object_storage` | any resource field carrying `FileCapability` | the app/registry must declare an `object_storage` (or `storage`) capability | package-wide; existing `APP-CAP-001` upgraded to read typed IR |
| `api.output.file_capability` ↔ `command.input.file` | api whose output is a file capability and whose path/policy/handler dispatches a command consuming an input file | doctor checks that input/output `accept` lists intersect | feature-local, with cross-feature opt-in via fully-qualified command refs |
| `api.output.file_capability.visibility` | api `output` carries `@cap.File` | must declare `visibility` (one of `public`, `private`, `signed`) | api-local; doctor refuses absence |
| `field.file_capability.visibility = signed` | resource field marked as signed-URL backing store | doctor warns: signed visibility is an output contract, not a storage contract (fields are private at rest) | resource-local |

## Codegen (Stage 5)

Two new generated files per feature consuming `@cap.File`. Output is
skeletal — the Lazuli Go runtime supplies the body — and follows the existing
`dist/go/customer/customer.gen.go` style.

### `dist/go/customer_import/upload.gen.go`

Generated when a feature's resource declares a `@cap.File` field.

```go
// path: dist/go/customer_import/upload.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer_import

import (
    "github.com/lazuli-lang/runtime/go/lazuli"
    "github.com/lazuli-lang/runtime/go/lazuli/storage"
)

// UploadContract is the lowered `@cap.File` shape on
// CustomerImportBatch.file (examples/full-capsule/full-capsule.lzi:707).
var UploadContract = storage.FileContract{
    Resource:   "CustomerImportBatch",
    Field:      "file",
    MaxSize:    25 * lazuli.MiB,
    Accept:     []storage.MimeType{{Family: "text", Subtype: "csv"}},
    Visibility: storage.VisibilityPrivate,
}

// AcceptUpload is called by the generated `command upload` handler to
// stream the multipart body into the object_storage capability.
// Returns the storage-side key (opaque token) that gets persisted as
// the resource field value.
func AcceptUpload(ctx *lazuli.Ctx, body lazuli.MultipartBody) (storage.Key, error) {
    return storage.AcceptUpload(ctx, UploadContract, body)
}

// FetchUpload resolves a stored file by its storage-side key, gating
// on the declared visibility and the per-command policy.
func FetchUpload(ctx *lazuli.Ctx, key storage.Key) (storage.Stream, error) {
    return storage.FetchPrivate(ctx, UploadContract, key)
}
```

### `dist/go/customer/export.gen.go`

Generated when a feature's `api` declares a `@cap.File` output.

```go
// path: dist/go/customer/export.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer

import (
    "github.com/lazuli-lang/runtime/go/lazuli"
    "github.com/lazuli-lang/runtime/go/lazuli/storage"
)

// CustomerExportOutputContract is the lowered `@cap.File` shape on
// api customer_export (examples/full-capsule/full-capsule.lzi:305).
var CustomerExportOutputContract = storage.FileContract{
    Api:        "customer_export",
    MaxSize:    100 * lazuli.MiB,
    Accept:     []storage.MimeType{{Family: "text", Subtype: "csv"}},
    Visibility: storage.VisibilitySigned,
    SignedTTL:  1 * lazuli.Hour,
}

// CustomerExportSignedURL is invoked by the generated api handler. It
// asks the storage capability for a signed URL valid for the contract's
// TTL, and returns it to the client without serving bytes inline.
func CustomerExportSignedURL(ctx *lazuli.Ctx, exportKey storage.Key) (string, error) {
    return storage.SignedURL(ctx, CustomerExportOutputContract, exportKey)
}
```

### Types reused from `runtime/go/lazuli`

- `lazuli.Ctx` (`runtime/go/lazuli/ctx.go`) — request context, actor, tenant.
- `lazuli.MiB`, `lazuli.Hour` — size and duration literals (new
  constants; see Stage 6).
- `lazuli.Error` (`runtime/go/lazuli/error.go`) — typed error envelope
  used for the storage error types.
- `lazuli.MultipartBody` — new type, see Stage 6.

Boundary discipline: codegen never names a provider (`S3`, `GCS`,
`Minio`, `Azure Blob`). The generated code references
`runtime/go/lazuli/storage` capabilities only; provider selection is
adapter-level (`@runtime/<adapter>` / `@plugin/<publisher>/<name>` /
`@adapter.<local>` resolved from `registry.lzi`).

## Runtime (Stage 6)

Three new capability files under `runtime/go/lazuli/storage/`.
Boundary discipline: **the language references the capability by
name (`object_storage`); the runtime selects an adapter at boot via
`registry.lzi`'s capability binding**. Adapters (`@runtime/s3`,
`@runtime/local`, `@plugin/<publisher>/gcs`, `@plugin/<publisher>/azureblob`)
sit in their own packages and implement the
`storage.ObjectStore` interface.

### `runtime/go/lazuli/storage/contract.go`

- **Capability**: declare typed `FileContract`, `MimeType`, `Key`,
  `Stream`, `MultipartBody` types consumed by every generated file
  helper. Centralises the language-derived shape.
- **Lifecycle**: stateless types.
- **Config**: none.
- **Dependency**: none (stdlib types only).
- **Typed errors**: none (defined by consumers).

### `runtime/go/lazuli/storage/upload.go`

- **Capability**: `AcceptUpload` — stream a multipart body into the
  bound `object_storage` capability, validating `max_size` and
  `accept` against the typed `FileContract`. Returns an opaque
  `storage.Key` that the generated command persists as the resource
  field value.
- **Lifecycle**: per-request. Uses `withTx` from
  `runtime/go/lazuli/db.go` only when the calling command is the
  side of the multipart that persists; the bytes themselves stream
  to storage outside the transaction.
- **Config**: reads the bound `object_storage <name>` capability
  from `registry.lzi`. Adapter selects bucket / prefix / region /
  credentials from `registry.integrations` (provider details stay
  in the adapter).
- **Dependency**: `mime/multipart` (stdlib);
  `io` for the streaming body; adapter packages
  (`@runtime/local` for filesystem, `@runtime/s3` for S3) are
  selected at boot via `runtime/go/lazuli/register.go`.
- **Typed errors**:
  - `ErrFileTooLarge` → mapped to `expose client` status 413, code
    `storage.file_too_large`.
  - `ErrMimeNotAccepted` → 415, `storage.mime_not_accepted`.
  - `ErrStorageBackendUnavailable` → 503, `storage.backend_unavailable`.

### `runtime/go/lazuli/storage/signed.go`

- **Capability**: `SignedURL` — generate a signed URL for an existing
  storage key, valid for the contract's `SignedTTL`. Refuses if the
  contract's `Visibility != Signed`.
- **Lifecycle**: stateless dispatcher.
- **Config**: reads `FileContract.SignedTTL` and the bound adapter.
- **Dependency**: adapter packages. Each adapter
  (`@runtime/s3`, `@runtime/local`) owns its own signing scheme
  (HMAC for S3-compatible; URL token + filesystem TTL index for
  local).
- **Typed errors**:
  - `ErrVisibilityMismatch` → 500, `storage.visibility_mismatch`
    (compile-time prevented by `cap_file_visibility_signed_ttl_diagnostics`
    Stage 8; runtime safety net).
  - `ErrSignedTTLExpired` → 410, `storage.signed_ttl_expired`.

### `runtime/go/lazuli/storage/fetch_private.go`

- **Capability**: `FetchPrivate` — resolve a storage key through a
  policy-gated download endpoint mounted by the runtime. Refuses if
  the contract's `Visibility != Private`. The endpoint inherits the
  policy of the command that originally accepted the upload.
- **Lifecycle**: per-request. Reads the resource row to confirm the
  caller's tenant/actor matches; streams bytes through the adapter.
- **Config**: reads `FileContract` + the bound adapter.
- **Dependency**: adapter packages.
- **Typed errors**:
  - `ErrUnauthorizedDownload` → 403, `storage.unauthorized_download`.
  - `ErrFileNotFound` → 404, `storage.file_not_found`.

### Adapter contract

The `object_storage` capability binds to an `ObjectStore` interface:

```go
// runtime/go/lazuli/storage/adapter.go (NOT codegen-generated)
type ObjectStore interface {
    Put(ctx context.Context, key Key, body io.Reader, contentType string) error
    GetStream(ctx context.Context, key Key) (io.ReadCloser, error)
    Sign(ctx context.Context, key Key, ttl time.Duration) (url string, err error)
    Delete(ctx context.Context, key Key) error
}
```

Adapter packages (`@runtime/local`, `@runtime/s3`, `@plugin/.../gcs`)
implement this interface. Lazuli core never names any of them.

## Evals/Testes (Stage 7)

### Doctor fixture — visibility undeclared

`crates/lazuli_cli/tests/fixtures/storage/visibility_undeclared.lzi`:

```lzi
feature x_export
  domain
    resource Export
      id: ID required
  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv)
    policy @policy.global_read
    handler "./api/x/download.go"
```

Asserts that doctor emits **exactly one**
`cap_file_visibility_undeclared_diagnostics` at the `output @cap.File`
line.

### Doctor fixture — input/output mismatch

`crates/lazuli_cli/tests/fixtures/storage/accept_mismatch.lzi`:
authors an api `output @cap.File(accept:application/json)` whose
handler dispatches a command whose `input file: @cap.File(accept:text/csv)`.
Asserts `cap_file_accept_input_output_mismatch_diagnostics` fires.

### Go integration test — round-trip via local adapter

`runtime/go/lazuli/storage/storage_test.go`:

```go
// Behaviour:
// 1. Bind `object_storage files` to @runtime/local.
// 2. Upload a 1KB CSV body through AcceptUpload.
// 3. Read back through FetchPrivate; assert bytes match.
// 4. Switch visibility to Signed, call SignedURL, follow URL,
//    assert bytes match.
// 5. Advance synthetic clock past SignedTTL; assert
//    ErrSignedTTLExpired.
```

Uses `testing/synctest` for the TTL expiry step.

### LSP test — hover + completion on `@cap.File` arguments

`crates/lazuli_lsp/tests/storage.rs`:

- Hover on `max_size` keyword inside `@cap.File(...)` shows the unit
  catalog (`kb | mb | gb`) and the lowered byte count for the
  authored literal.
- Hover on `accept` shows the MIME family catalog and the parsed
  list.
- Hover on `visibility` shows `public | private | signed`.
- Completion at `@cap.File(visibility:|` offers exactly those three.
- Completion at `@cap.File(max_size:25` offers `kb`, `mb`, `gb`.

### Inspect contract test

`crates/lazuli_cli/tests/inspect_storage.rs`: runs
`lazuli inspect --format=json --expand=storage examples/full-capsule`
and asserts the `storage` projection matches the JSON shape in
Stage 4 (typed args, normalisation rules, omission of features
without `@cap.File`).

## Doctor/LSP (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `cap_file_visibility_undeclared_diagnostics` | error | "api output `@cap.File` must declare `visibility:` (`public`, `private`, or `signed`); ambiguous visibility on a file URL is a security contract gap." | api `output @cap.File(...)` parses with `visibility = None` after lowering | `visibility_undeclared.lzi` above |
| `cap_file_accept_input_output_mismatch_diagnostics` | error | "api `<name>` output declares `@cap.File(accept:<X>)` but its dispatched command's `input` declares `@cap.File(accept:<Y>)`; accept lists must intersect for the round-trip to be valid." | api with file output dispatches a command whose file input has a disjoint MIME family/subtype | `accept_mismatch.lzi` above |
| `cap_file_visibility_signed_ttl_mismatch_diagnostics` | error | "`@cap.File(visibility:signed)` requires `signed_ttl:<duration>`; `visibility:public`/`private` forbid `signed_ttl`." | typed IR shows `visibility = Signed` with `signed_ttl = None`, or `visibility != Signed` with `signed_ttl = Some(_)` | minimal `.lzi` per direction |
| `cap_file_size_unit_invalid_diagnostics` (upgraded) | error | "`@cap.File(max_size:<literal>)` must use a positive integer with unit `kb`, `mb`, or `gb`." | typed lowering rejects the literal (previously a text-pattern LSP warning) | minimal `.lzi` with `max_size:large` |
| `cap_file_mime_family_unknown_diagnostics` | warning | "`@cap.File(accept:<family>/<subtype>)` uses unknown family `<family>`; known families: `text`, `image`, `application`, `audio`, `video`, `font`, `*`." | parsed MIME has a family outside the closed catalog | minimal `.lzi` with `accept:gibberish/csv` |

All five codes register under `is_security_enforcement_code`
(`crates/lazuli_lsp/src/lib.rs:9527`) so the strict + production
profiles upgrade severity uniformly when a profile cuts in.
`APP-CAP-001` (`crates/lazuli_cli/src/doctor.rs:1334`) keeps its
text-pattern fallback so that even pre-lowered packages emit it; it
upgrades to consume typed IR once Route B lands.

### Diagnostic anchors (where to add)

- `cap_file_visibility_undeclared_diagnostics` — `crates/lazuli_cli/src/doctor.rs`
  cross-feature pass once IR carries `FileCapability.visibility`.
- `cap_file_accept_input_output_mismatch_diagnostics` — same pass;
  uses the new api → command edge tracked through `api.handler`
  dispatch (today the handler is opaque; this diagnostic kicks in
  when the api directly references a generated command, which is
  the documented pattern).
- `cap_file_visibility_signed_ttl_mismatch_diagnostics` — file-local
  in LSP (typed shape rule) and cross-feature in doctor (same check
  to catch packaged code that bypassed LSP).
- `cap_file_size_unit_invalid_diagnostics` — promoted from
  `crates/lazuli_lsp/src/lib.rs:2935` text-pattern warning to typed
  doctor error. Keep the LSP version for live editing.
- `cap_file_mime_family_unknown_diagnostics` — file-local LSP +
  cross-feature doctor (warning only — pilot evidence may grow the
  family list before promotion to error).

### LSP hovers (new entries)

Add to `KEYWORD_HOVER` in `crates/lazuli_lsp/src/lib.rs`:

| Keyword (in `@cap.File(...)` context) | Hover summary |
|---|---|
| `@cap.File` | "File capability: `max_size:<size>` + `accept:<mime>` + `visibility:<mode>` (+ `signed_ttl:<duration>` if signed). Used on resource fields and api outputs. Requires `object_storage` capability." |
| `max_size` | "Maximum upload size. Closed unit catalog: `kb`, `mb`, `gb`." |
| `accept` | "Accepted MIME types; pipe-separated for alternatives, e.g. `text/csv\|application/vnd.ms-excel`. Families: `text`, `image`, `application`, `audio`, `video`, `font`, `*`. Subtype `*` is also valid." |
| `visibility` | "URL visibility. `public` = unguessable but un-gated; `private` = policy-gated download handler; `signed` = time-limited signed URL (requires `signed_ttl`)." |
| `signed_ttl` | "Signed URL TTL. Closed unit catalog: `s`, `m`, `h`, `d`. Only valid when `visibility:signed`." |

Closed-catalog completions to add:

- `@cap.File(visibility:` → `public`, `private`, `signed`.
- `@cap.File(max_size:<int>` → `kb`, `mb`, `gb`.
- `@cap.File(signed_ttl:<int>` → `s`, `m`, `h`, `d`.
- `@cap.File(accept:` → suggest the 7 MIME families.

### Namespaces (`is_allowed_reference_namespace`)

No new namespace required. The `@cap.*` namespace is already in the
closed catalog (`crates/lazuli_lsp/src/lib.rs:2114-2135` and
`docs/invariants.md:233-249`). `object_storage` is already in the
capability kind closed set (`crates/lazuli_lsp/src/lib.rs:8670`).

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

`@cap.File` already colored as a capability decorator. Add
`max_size | accept | visibility | signed_ttl | signed | public |
private` to the capability-argument scope. Size literals (`25mb`)
and duration literals (`1h`) hit existing literal scopes. MIME types
inside `accept:` follow the string-content scope.

## Critério de "ciclo fechado"

- [ ] Fixture exercises typed `@cap.File` with `visibility:signed` +
  `signed_ttl` on the export api output and `visibility:private`
  on the import resource field (Stage 3 extends `full-capsule.lzi`
  per the inline examples above).
- [ ] `lazuli check examples/full-capsule` accepts the syntax after
  Route B lands (no regression on existing pre-typed `@cap.File`
  declarations — additive arguments only).
- [ ] `lazuli inspect --format=json --expand=storage examples/full-capsule`
  shows the IR shape described in Stage 4 for `customer_import`
  and `customer`.
- [ ] `lazuli doctor` emits the 5 named diagnostics on the matching
  fixtures.
- [ ] `lazuli generate` produces `dist/go/customer_import/upload.gen.go`
  and `dist/go/customer/export.gen.go` that compile under
  `runtime/go/lazuli/storage`.
- [ ] Lazuli Go exposes upload (multipart) + signed download
  (round-trip) end-to-end (runtime-team deliverable).
- [ ] `runtime/go/lazuli/storage/storage_test.go` synctest test
  passes for round-trip + signed TTL expiry.
- [ ] LSP hovers + completion cover the 5 keywords + 4 closed
  catalogs from Stage 8.

## Próximo passo

Human approval of this proposal **and** the scope proposal
(`docs/proposals/bucket-storage-scope.md`) + a separate
`mode=implement` run that lands Route B: add the
`FileCapability` / `CapabilityRef` / `FileSize` / `MimeType` /
`FileVisibility` types to `crates/lazuli_ir/src/lib.rs`, extend
`type_ref_from_syntax` (`crates/lazuli_analyzer/src/lib.rs:486`) to
recognise `@cap.File(args)` (reusing the existing
`capability_args` helper from
`crates/lazuli_lsp/src/lib.rs:2960`), wire lowering, add
`ExpandSet.storage` (`crates/lazuli_cli/src/main.rs:98-118`), and
ship the five doctor diagnostics + LSP entries. The Lazuli Go
runtime team owns
`runtime/go/lazuli/storage/{contract,upload,signed,fetch_private}.go`
and the local + S3 adapters in parallel.

## Rows sugeridas para `docs/next-checklist.md`

Three additions, formatted to match the existing table:

```
| 29 | Storage bucket cycle Route B — typed `@cap.File` lowering | planned | Add `FileCapability`/`CapabilityRef`/`FileSize`/`MimeType`/`FileVisibility` to IR. Extend `type_ref_from_syntax` to recognise `@cap.File(args)` via the existing `capability_args` helper. New `--expand=storage` projection. Two new args: `visibility`, `signed_ttl`. See `docs/proposals/bucket-storage-cycle.md` §Linguagem/§IR + `docs/proposals/bucket-storage-scope.md`. |
| 30 | Storage bucket cycle — 5 doctor diagnostics + LSP coverage | planned | `cap_file_visibility_undeclared_diagnostics`, `cap_file_accept_input_output_mismatch_diagnostics`, `cap_file_visibility_signed_ttl_mismatch_diagnostics`, `cap_file_size_unit_invalid_diagnostics` (typed promotion), `cap_file_mime_family_unknown_diagnostics`. LSP hovers for 5 keywords + closed-catalog completions for `visibility`/`max_size`/`signed_ttl`/`accept`. Depends on row 29. See `docs/proposals/bucket-storage-cycle.md` §Doctor/LSP. |
| 31 | Storage bucket cycle — Lazuli Go runtime + local/S3 adapters | planned | `runtime/go/lazuli/storage/{contract,upload,signed,fetch_private}.go`. Local adapter (`@runtime/local`) writes to filesystem; S3 adapter (`@runtime/s3`) uses `aws-sdk-go-v2`. Doctor test for round-trip + signed TTL expiry via `testing/synctest`. The runtime team owns. Depends on row 29. See `docs/proposals/bucket-storage-cycle.md` §Runtime/§Evals. |
```
