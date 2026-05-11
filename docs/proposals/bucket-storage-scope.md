# Storage Lowering Scope (pre-design)

**Status**: pre-design investigation. Resolves a side-quest blocker
discovered during the `bucket=storage` pipeline Stage 1+2 inventory,
so Stage 3 (design-language) runs against the correct scope.

**Audience**: language team, runtime team, anyone touching the storage
bucket cycle.

**Date**: 2026-05-10.

## Context

The `bucket=storage` inventory cataloged every storage-related
construct in the canonical fixture, IR, doctor, LSP, codegen, and
runtime. The picture is asymmetric — storage is at **L0 for capability
decoration only** and at **L1 for one file-local LSP check**, with
**zero IR carry-through, zero codegen, and zero runtime**. The
defining anchor:

- The fixture authors `file: @cap.File(max_size:25mb,accept:text/csv)
  required` (`examples/full-capsule/full-capsule.lzi:707`) and the
  symmetric `output @cap.File(max_size:100mb,accept:text/csv)` on the
  CSV export api (`examples/full-capsule/full-capsule.lzi:305`).
- The LSP harvests the arguments through a text-pattern walker
  (`crates/lazuli_lsp/src/lib.rs:2883-2957`, function
  `file_capability_contract_diagnostics`) and produces four shape-only
  warnings: missing `max_size`, missing `accept`, malformed size
  literal, malformed MIME.
- The analyzer **does not recognise `@cap.File` as a builtin type**.
  `type_ref_from_syntax` (`crates/lazuli_analyzer/src/lib.rs:486-501`)
  matches only `ID/Text/Boolean/Integer/Decimal/Date/DateTime/Json/
  Email`; everything else (including `@cap.File(...)`) falls through
  to `TypeRef::UserDefined { name: "@cap.File(max_size:25mb,...)" }`
  or `TypeRef::Unresolved`. The IR enum `BuiltinType::CapFile`
  (`crates/lazuli_ir/src/lib.rs:363`) exists but is never produced
  from authored source.
- `lazuli inspect --format=json examples/full-capsule/full-capsule.lzi`
  emits zero `@cap.File`-related projection: there is no `file_capability`
  key on `ResourceField`, no `accept`/`max_size` carried through, no
  api `output` projection of the file shape.
- Doctor has one cross-check
  (`crates/lazuli_cli/src/doctor.rs:1328-1336`, `APP-CAP-001`) that
  emits if any `@cap.File` text appears anywhere in the package and
  the app/registry doesn't declare `object_storage` or `storage`
  capability. The check uses a text walker
  (`doctor.rs:902-909`) that only records the line, never the
  parsed arguments. No cross-check exists between
  `@cap.File(accept:text/csv)` on an `input` field and `@cap.File(accept:text/csv)`
  on the corresponding `api` `output` — the contract is invisible
  to doctor.
- The runtime Go package (`runtime/go/lazuli/`) has **no storage
  helper at all**: `types.go:42` mentions `RetentionArchive` only as
  a doc comment; `command.go`, `event.go`, `query.go` contain only
  the substring "file" as parameter names; there is no upload helper,
  no signed-URL helper, no `lazuli.File` type, no
  `lazuli.ObjectStore` interface.
- The codegen (`crates/lazuli_codegen_go`) produces zero references
  to file/upload/cap.File in the generated package
  (`dist/go/customer/customer.gen.go`).

The Stage 1+2 inventory's recommendation was not to propose new
storage primitives (`storage`, `bucket`, `signed_url`,
`storage_quota`) until the lowering decision for the existing
`@cap.File` capability is made. Adding new kinds ahead of that
locks the new constructs into the same dead-on-arrival state that
`auth` was in before `docs/proposals/auth-lowering-scope.md`. This
proposal resolves the lowering route first, then names the primitive
subset that justifies a design pass.

## Why `@cap.File(...)` arguments do not reach IR

`@cap.File` is a **capability decorator with structured arguments**,
not a simple builtin type. The argument shape — `max_size:<size>`,
`accept:<mime>` — is canonical (documented at `docs/invariants.md:391-393`)
but the canonical-indent slice and the legacy pest pipeline both
treat capability decorators as opaque type strings:

1. The canonical-indent slice (`crates/lazuli_syntax/src/parser.rs`)
   only parses `feature` headers and the indented `agent` blocks
   inside them (commit `d2a6202`). Everything else, including
   `resource` field declarations, falls through to the legacy pipeline.
2. The legacy pipeline's `type_ref_from_syntax`
   (`crates/lazuli_analyzer/src/lib.rs:486`) is a flat string match.
   `@cap.File(max_size:25mb,accept:text/csv)` doesn't match any
   variant, so it lowers to `TypeRef::UserDefined { feature: None,
   name: "@cap.File(max_size:25mb,accept:text/csv)" }` — a synthetic
   "user-defined type" whose name happens to contain a paren.
3. The IR enum `BuiltinType::CapFile`
   (`crates/lazuli_ir/src/lib.rs:363`) is **only** used in
   `BuiltinType::CapFile => "@cap.File"`
   (`crates/lazuli_cli/src/main.rs:2176`) when formatting a TypeRef
   back to text. It is never produced from authored source. Whatever
   path produced `CapFile` historically has rotted — confirmed by
   grepping the analyzer and parser for `CapFile` (zero hits).

The pattern is identical to the `auth` block: a surface that authors
canonical syntax, an IR shape that exists, no lowering wire between
them. Storage is one step worse: the IR shape is **incomplete** —
`BuiltinType::CapFile` is a flat enum variant, but `@cap.File`
carries two structured arguments that the IR cannot represent. Just
producing `BuiltinType::CapFile` doesn't capture `max_size` or
`accept`; the IR needs a new struct.

This is the side-quest blocker. Until the IR struct exists and the
analyzer produces it from authored source, every downstream stage
(inspect projection, doctor cross-check, codegen, runtime, evals)
has no typed contract to read.

## Routes A vs B vs C

Three ways to close the storage lowering gap, each honouring the
Lazuli/Drusa boundary.

### Route A — extend the IR with `CapabilityRef` + extend the analyzer to recognise `@cap.File(args)`

Add a structured `CapabilityRef` shape to the IR (alongside the
existing `BuiltinType` enum) that carries the capability name plus
its parsed arguments. Extend `type_ref_from_syntax` (legacy pipeline)
and the canonical-indent slice (once it covers resource fields per
Phase L row 24) to recognise the `@cap.<Name>(<arg>:<val>,...)` shape
and emit a `TypeRef::Capability(CapabilityRef)` variant. Wire the
lowering. Inspect projection becomes mechanical because the IR shape
serialises directly.

Pros:
- Solves storage **and** every other `@cap.*` decorator with arguments
  (`@cap.Hashed`, `@cap.Encrypted`, `@cap.E2ee`, `@cap.Token`) in one
  cut. The LSP already validates these via text-pattern walkers
  (`capability_args` helper at `crates/lazuli_lsp/src/lib.rs:2960`);
  the IR side is the same kind of upgrade.
- One canonical home for the shape; doctor cross-checks (e.g.,
  `@cap.File(accept:text/csv)` on input vs `@cap.File(accept:text/csv)`
  on a referencing `api output`) become typed.
- Codegen consumes typed args directly — `dist/go/.../upload.gen.go`
  can declare `MaxSize: 25 * lazuli.MiB` from IR, not by re-parsing
  source.

Cons:
- Larger surface than the auth Route A. Touches every other `@cap.*`
  capability decorator with args (4 of them).
- The IR change is **schema-breaking** for the inspect contract if
  there are downstream JSON consumers reading `TypeRef`. Today
  there are none (the codegen consumes IR through internal API; no
  external SDK), but pin this fact in the implementation plan.

### Route B — narrow Route A to `@cap.File` only, leave other `@cap.*` decorators alone

Add a `FileCapability { max_size: FileSize, accept: Vec<MimeType> }`
struct to `ResourceField` as a sibling slot (`file_capability: Option<FileCapability>`),
populated only when the field's `TypeRef` is exactly `BuiltinType::CapFile`.
Keep other `@cap.*` capabilities text-pattern in LSP as today.

Pros:
- Smaller patch surface; touches only the storage path.
- Compatible with the current `BuiltinType::CapFile` enum — just adds
  the sidecar struct.

Cons:
- Creates a special case. The four other `@cap.*` decorators with
  arguments stay text-pattern, building friction for the eventual
  Phase L migration that wants to typify them uniformly. This is
  the same "third text-pattern fact family" anti-pattern the auth
  scope proposal called out (`docs/proposals/auth-lowering-scope.md:133`).
- Doesn't unblock symmetric cross-checks like
  `@cap.Encrypted(key:@key.tenant)` on a field vs `@cap.Encrypted`
  on the api `output`, because the other capabilities stay opaque.

### Route C — text-pattern fact extraction (the `CommandApprovalFact` shape)

Add `collect_file_capability_facts` in `crates/lazuli_cli/src/doctor.rs`
next to `file_capabilities: Vec<SourceFact>`
(`crates/lazuli_cli/src/doctor.rs:511`), upgrading the existing
plain-name fact to a structured `FileCapFact { max_size: String,
accept: String, qualified_field: FieldRef }`. Surface
`registry_file_defects: Vec<FileCapFact>` slot on `DoctorPackage`.
Inspect projection stays text-derived.

Pros:
- Smallest possible patch — extends the existing harvester.

Cons:
- Adds a fourth text-pattern fact family (after `CommandApprovalFact`,
  `collect_feature_symbols`, and the existing `file_capabilities`
  facts). Same drift risk the auth scope flagged.
- Diverges from auth Route A's precedent. The repo's stated direction
  (Phase L, `docs/next-checklist.md:60`) is to shrink text-pattern
  facts, not grow them.
- Codegen still can't consume typed args; needs its own re-parse.

### Comparison

| Axis | Route A (full `@cap.*` typing) | Route B (file-only sidecar) | Route C (text-pattern) |
|---|---|---|---|
| Upfront cost | ~3 cells: new IR variant + 5 capability recognisers + lowering. | ~2 cells: one new struct + recogniser for `CapFile` only. | ~1 cell: extend the existing harvester. |
| Maintenance cost | Lowest long-term — one canonical home for all `@cap.*` decorators. | Medium — file path typed, others stay text-pattern. | Highest — every new `@cap.*` argument grows the harvester. |
| Cross-checks possible | All: input/output symmetry, retention vs `@pii.*`, generated SDK upload shape, key scope consistency. | File-shape only (max_size + accept). | Same as B, but bound to text patterns. |
| Codegen consumption | Typed — `dist/go/.../upload.gen.go` reads IR args directly. | Typed for file only; other `@cap.*` need re-parse. | Untyped — codegen must re-parse source. |
| LSP coverage | Slice → typed AST → hover + completion + closed-catalog enforcement on `accept:`. | Same as A for file only. | Stays at the current file-local text-walk level. |
| Compat with Phase L | Aligned — promotes capability decorators out of text-pattern territory. | Half-aligned — file done; others still text-pattern. | Misaligned — grows the backlog. |
| Risk | Schema change in `TypeRef`; no external consumers yet, but pin. | Localised; no `TypeRef` change. | Drift risk between LSP walker and doctor walker. |

### Recommendation

**Route B** for the storage bucket cycle, **with Route A as the
explicit follow-up Phase L work**.

Rationale:

1. Storage's authoring pressure today is `@cap.File` only. The
   fixture's other capability decorators (`@cap.Hashed`,
   `@cap.Encrypted`) are file-adjacent (session tokens, encrypted
   blobs) but their arguments aren't the bottleneck — algorithm
   names and key scopes are already validated by closed-catalog
   LSP rules (`crates/lazuli_lsp/src/lib.rs:2729-2876`). The auth
   bucket cycle is the natural pressure point for typing
   `@cap.Hashed.algorithm`, not the storage cycle.
2. Route A's surface is large and touches IR-wide enums. Doing it
   inside the storage bucket cycle inflates the cycle's risk and
   delays the storage-specific work (signed URLs, retention,
   storage class). Route B carries the storage-specific gain
   (typed `max_size`/`accept` cross-checks, codegen consumption)
   without the cross-bucket fanout.
3. The auth bucket cycle's precedent is Route A scoped to one
   block (`auth`). Route B mirrors that discipline scoped to one
   capability decorator (`@cap.File`).
4. Phase L row 24 (`docs/next-checklist.md:60`) is the documented
   home for the broader cleanup. When commands/resources/records
   land in the canonical-indent slice, lifting other `@cap.*`
   decorators to typed IR is a small follow-up cut.

Route B also matches the boundary discipline: the typed
`FileCapability` shape is a contract that codegen and doctor read
directly; the runtime's `lazuli.File` type maps 1:1 from it; no
adapter or transport detail leaks into the language.

## Pilot-needed vs Speculative

The 5 "missing" constructs in the Stage 1+2 inventory map to roadmap
§1.26 (`docs/roadmap.md:239-245`). Classified against fixture
evidence and the §0 ciclo L0→L2 pilot bucket
(`docs/roadmap.md:23-45` — storage is bucket-piloto #2):

### PILOT-NEEDED — exercised by the canonical fixture today

| Construct | Fixture evidence | Justification |
|---|---|---|
| `@cap.File` typed args (`max_size`, `accept`) | `full-capsule.lzi:305` (output), `full-capsule.lzi:707` (input) | Already authored on **both** an api `output` and a resource field. The contract is canonical (`docs/invariants.md:391-393`) but invisible to IR/doctor/codegen. Resolving Route B is exactly the lowering this scope proposal recommends. |
| `signed` decorator (file URL signing) | Not authored, but **strongly implied** by the symmetric input/output use of `@cap.File`: the export api at `full-capsule.lzi:302-308` cannot return a file body inline (`100mb`) — the generated runtime needs a signed URL. The decorator's absence forces the runtime to invent the policy. | Without a typed contract, every adapter (`@runtime/s3`, `@runtime/local`, `@plugin/...`) chooses its own signing policy and TTL. Promote to a typed decorator (`@cap.File(max_size:25mb,accept:text/csv,signed_ttl:1h)` or a sibling `signed_url` block) so the contract is visible. |
| `public`/`private` visibility decorator | Not authored. Default behaviour today is undefined: the fixture's `customer_export` api at `full-capsule.lzi:302-308` has `policy @policy.global_read` but the resulting file URL has no declared visibility — anyone with the URL? signed-only? policy-gated download endpoint? | Concrete pilot evidence: every product authoring `@cap.File` on an api output will hit this. Storage cycle must land at least the binary decision (`public`/`private`) so the runtime can refuse to generate ambiguous code. |
| `retention <duration> then delete\|anonymize\|archive` on file-carrying resources | Authored for PII resources (`full-capsule.lzi:60`, `full-capsule.lzi:455`) but not on `CustomerImportBatch.file` (`full-capsule.lzi:707`). The retention contract exists in the language; storage extends it. | Files are physical objects whose retention contract (delete the bytes? archive to cold storage? anonymize the filename?) needs typed semantics distinct from the row-level retention. Today's retention contract on a resource doesn't say what `delete` means for the underlying bytes. |

### SPECULATIVE — not in the fixture; defer until a real pilot exercises them

| Construct | Status | Why defer |
|---|---|---|
| `storage` kind (top-level, per roadmap §1.26) | Not authored; not referenced; only in `docs/roadmap.md:241`. | Pilot needed: a product that needs to declare **multiple** storage backends per feature (e.g., one bucket for receipts, another for avatars, a third for legal exports). Today's single-capability `object_storage files` in registry covers the 1-bucket case. A `storage` kind introduces a separate dispatch surface — only justified if the product needs more than one. |
| `bucket` kind | Same as `storage` kind. The two are near-synonyms in the roadmap; pilot will tell which name is canonical. | Pilot needed: a product that authors per-bucket policy (lifecycle, encryption-at-rest scope, region pinning). Until that, the `object_storage <name>` capability slot in registry carries the contract. |
| `storage_quota` decorator on tenant | Not authored. Roadmap §1.26 lists it; framework-coverage audit §28 names it under DL. | Pilot needed: a product that enforces per-tenant file quotas and bills on usage. The decorator would interact with `defaults.tenancy` and existing `@scope.same_org` policy; designing it without pilot pressure invents shape. |
| Direct uploads (`presigned PUT`) | Not authored. Customer CSV upload at `full-capsule.lzi:751-758` uses `input file` (multipart body), not a presigned URL flow. | Pilot needed: a product where files exceed memory limits (>100MB) and the runtime must hand the client a presigned URL. Today's `input file` covers the multipart case canonically. |
| Multipart / resumable uploads | Same as direct uploads — framework feature, not surface authoring pressure. Belongs to Drusa runtime (§2.22). | Pilot needed: a product authoring file uploads >5GB. The contract is adapter-specific (S3 multipart vs GCS resumable vs local stream-to-disk); Lazuli should not pre-declare it. |
| File versioning / deduplication / lifecycle policies | Roadmap §2.22 lists these as DF (Drusa runtime concerns). | These are runtime/adapter, not language. The language might declare `lifecycle archive_after 90d` as a sugar over `retention`, but that's a promotion candidate, not pre-pilot. |
| Image / video / audio processing, thumbnail generation, metadata extraction, virus scanning, CDN integration, backup integration | Framework-coverage audit §28: "**F**: image processing, thumbnail generation, video/audio processing, metadata extraction, virus scanning, CDN integration, backup integration (cuts media gated)." | **F-class** (pilot-gated). Strictly outside the storage bucket cycle scope. Don't promote any of these without a media pilot. |

The pilot-needed subset is the four entries that already appear or
are strongly implied by the canonical fixture. Speculative
additions wait for §0 bucket-piloto storage to surface real
authoring pressure.

## Closed-cycle criterion for the storage bucket

Adapted from `docs/roadmap.md:34-43` (8-item §0 checklist) to the
specific shape of bucket-piloto #2:

- [ ] **Fixture authors the full surface.** The canonical fixture
  already exercises `@cap.File(max_size:25mb,accept:text/csv)`
  (`full-capsule.lzi:707`) as a resource field and
  `output @cap.File(max_size:100mb,accept:text/csv)`
  (`full-capsule.lzi:305`) on an api. Stage 3 should extend this
  with **one** symmetric pair (api output → command input) plus
  the binary `signed`/`public`/`private` decision on the export
  api so the doctor diagnostic from Stage 8 has a positive
  fixture case.
- [ ] **`lazuli check` accepts the syntax.** Already true — the
  text-pattern path accepts `@cap.File(...)`. After Route B lowering,
  the typed path accepts it too.
- [ ] **`lazuli inspect --expand=storage` projects the IR.** New
  projection; required deliverable. Surfaces per-feature file
  capability usage (`field` references + api `output` references)
  with parsed `max_size` / `accept` / `signed` / `visibility`.
- [ ] **`lazuli doctor` carries ≥3 cross-feature diagnostics for
  storage.** Concrete proposals:
  - `cap_file_max_size_invalid_diagnostics` — Already today as a
    text-pattern in LSP; this promotes it to typed cross-feature.
  - `cap_file_accept_input_output_mismatch_diagnostics` — A command
    whose `input file: @cap.File(accept:text/csv)` is consumed by
    an api whose `output @cap.File(accept:application/json)` — the
    file shapes disagree and the contract leaks.
  - `cap_file_visibility_undeclared_diagnostics` — A `@cap.File` on
    an api `output` without `signed`/`public`/`private` decoration
    is a security ambiguity; doctor refuses.
  - `app_missing_storage_capability` (already exists as `APP-CAP-001`
    at `crates/lazuli_cli/src/doctor.rs:1334`) — keep, upgrade to
    consume typed IR.
- [ ] **`lazuli generate` produces Go that compiles.** Runtime-team
  deliverable (parallel Drusa work). Consumed via stable IR JSON
  through `lazuli inspect --format=json --expand=storage`.
- [ ] **Drusa executes end-to-end upload + download + signed URL.**
  Runtime-team deliverable. Outside language scope.
- [ ] **`eval`/test coverage.** A doctor test fixture exercising
  each diagnostic; a Go integration test in Drusa for upload +
  signed URL; a golden eval covering the symmetric
  `customer_export` round-trip is optional (no LLM in the loop).
- [ ] **LSP hover/completion on `@cap.File` arguments.** Today the
  LSP carries shape-only diagnostics. Hover on `max_size` shows the
  unit literal grammar; hover on `accept` shows the MIME closed-set
  (mimicking the `algorithm`-on-`@cap.Hashed` pattern, scoped to
  the family of MIME types the runtime can validate); completion
  on `signed` offers `true|false` or duration literal.

The first four items are language-team Stage 3 deliverables. Items
5-7 are Drusa-team. Item 8 is language-team but small.

This list is attainable in a single Stage 3 design cut once Route
B lands; nothing on it depends on speculative primitives.

## Recommendation

1. **Take Route B** (add a typed `FileCapability` struct alongside
   the existing `BuiltinType::CapFile` and wire lowering through
   the legacy pipeline). Estimated scope: ~2 cells of analyzer +
   IR work. Add `ResourceField.file_capability: Option<FileCapability>`
   (or a `TypeRef::CapFile { options: FileCapabilityOptions }`
   inline variant — Stage 4 decides). Mechanical because the LSP
   already extracts the same args via `capability_args`
   (`crates/lazuli_lsp/src/lib.rs:2960`); the analyzer can call
   the same helper.
2. **Scope Stage 3 design to the PILOT-NEEDED subset only.** Four
   children — typed `max_size`/`accept`, `signed`,
   `public`/`private`, file-aware `retention`. Stage 3's job is to
   tighten the contract (closed catalogs for MIME families, typed
   size literals, visibility decision), not invent new top-level
   kinds.
3. **Defer SPECULATIVE additions** until the bucket cycle surfaces
   real pilot pressure. `storage`/`bucket` top-level kinds,
   `storage_quota`, direct/multipart/resumable uploads, file
   versioning are catalog noise without a pilot exercising them.
   The roadmap §1.26 list stays as is; this proposal does not
   promote any of those items.
4. **Run Stage 3 with the closed-cycle criterion above as the
   acceptance gate.** Anything that doesn't shrink the gate counts
   as speculative and goes to backlog.
5. **Update `docs/next-checklist.md` row 24** (`Phase L`) **only
   after** Route B lands, to reflect that `@cap.File` joins
   `agent` in the canonical-indent slice's typed coverage. Do not
   edit row 24 as part of this proposal.

When Route B is implemented, Stage 3 (design-language) runs on the
shipped substrate and produces a focused proposal covering at most
the three doctor diagnostics named in the closed-cycle criterion
plus the `--expand=storage` projection. Stage 4 (Drusa codegen)
then has a stable IR JSON to consume.
