# Bucket Cycle: Field-Level Encryption (L0→L2)

**Status**: design proposal v0.1. Stages 3–9 of the `bucket=encryption`
pipeline. Pairs with the AES-256-GCM runtime being shipped in parallel
(`cdx-encryption-aes` cell → `runtime/go/lazuli/encryption/aes_gcm.go`).

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-14.

**Cell**: #2 under `docs/proposals/corbanx-class-readiness.md`. Closes
gap row 2 (field-level encryption / secret at-rest).

## Contexto

The canonical fixture authors `@cap.Encrypted(key:@key.tenant)` on
**three** distinct field sites today:

- `examples/full-capsule/full-capsule.lzi:53` — `external_id:
  @cap.Encrypted(key:@key.tenant) @pii.external optional` on
  `Customer`.
- `examples/full-capsule/full-capsule.lzi:495` —
  `provider_access_token: @cap.Encrypted(key:@key.tenant)
  @pii.credential optional` on `CustomerSession`.
- `examples/field-permissions.lzi:19-21` — three back-to-back encrypted
  fields (`tax_id`, `annual_revenue`, `internal_notes`).
- `examples/user-auth.lzi:39` — `provider_access_token:
  @cap.Encrypted(key:@key.tenant)` on `UserSession`.

The surface is **canonical and stable** (`docs/invariants.md:423-433`):
`@cap.Encrypted(key:@key.<scope>)` declares server-readable
encrypted material with an explicit key scope; `@cap.E2ee(key:@key.<scope>)`
declares ciphertext the server stores but never reads. The key-scope
catalog is closed (`docs/canonical-semantics.md:1173-1178`):
`@key.app`, `@key.tenant`, `@key.user`, `@key.record`.

The IR layer already carries the typed shape: `EncryptedCapability {
key: String }` at `crates/lazuli_ir/src/lib.rs:598-604`, lowered from
the analyzer via `parse_cap_encrypted_type` at
`crates/lazuli_analyzer/src/lib.rs:1249-1257`. Codegen already emits
column types as `lazuli.EncryptedRef` (a string typedef) at
`crates/lazuli_codegen_go/src/emitter/resource.rs:897-911`. The
runtime placeholder type lives at `runtime/go/lazuli/types.go:67`
with a comment referencing the missing `@adapter.encryption`.

**What is missing is the bridge between surface and bytes.** Today:

1. `@key.tenant` is a symbolic scope with no declared **source**. The
   resource field says "encrypt this with the tenant key" but the
   capsule has no place to declare *where* the tenant key lives
   (KMS slot? env var? per-tenant value in a secrets table?).
2. The runtime has no AES-256-GCM helper (`runtime/go/lazuli/encryption/`
   is empty; Codex cell `cdx-encryption-aes` ships
   `aes_gcm.go` in parallel with this proposal).
3. The codegen emits `EncryptedRef` (a typedef) but never wires
   `enc.Encrypt(...)` before INSERT or `enc.Decrypt(...)` after SELECT
   on any encrypted field. Confirmed by grepping
   `crates/lazuli_codegen_go/src/emitter/` for `encrypt` / `cipher`
   (zero hits beyond the `EncryptedRef` typedef).
4. Doctor has zero `ENC-*` diagnostic codes
   (`grep -rn "ENC-" crates/lazuli_cli/src/doctor` returns nothing).
   A field can declare `@cap.Encrypted(key:@key.nope)` and pass
   `lazuli doctor` today.
5. The `hostpoint-port-gap-2026-05-14.md:56-62` audit explicitly
   flags `@key.tenant` envelope as missing — "no L0 proposal and no
   codegen handling".

This proposal closes that bridge. It adds **one new authored
construct** (the app-level `encryption` block declaring how each
`@key.*` scope resolves to bytes), **one new IR shape**
(`EncryptionBinding`), the codegen plan that injects
`encryption.Cipher` calls at the resource repository boundary, and
the doctor diagnostics that refuse the "encryption-declared,
source-undeclared" gap.

The runtime side (`runtime/go/lazuli/encryption/{aes_gcm,resolver}.go`)
is wired but the typed crypto primitive lives in the runtime package
only — codegen never emits homegrown crypto. This honours the
wire-thin principle (`CLAUDE.md:8-35`): the language declares the
contract; the runtime supplies the bytes.

## Baseline (Stages 1-2 inventory)

| Layer | Status | Anchor |
|---|---|---|
| Surface `@cap.Encrypted` | canonical, 5+ use sites | `examples/full-capsule/full-capsule.lzi:53,495`; `examples/field-permissions.lzi:19-21` |
| Surface `@cap.E2ee` | declared in invariants, **zero use sites** in fixtures | `docs/invariants.md:425` |
| Surface `@key.*` catalog | closed: `app`, `tenant`, `user`, `record` | `docs/canonical-semantics.md:1173-1178` |
| Grammar | parses both as type decorators | `docs/grammar.lzi.md:34,258` |
| IR `EncryptedCapability` | typed `{ key: String }` | `crates/lazuli_ir/src/lib.rs:598-604` |
| IR `E2eeCapability` | **missing** — `CapabilityRef` enum has no `E2ee` variant | `crates/lazuli_ir/src/lib.rs:565-580` |
| Analyzer lowering | works for `@cap.Encrypted`, validates `@key.` prefix | `crates/lazuli_analyzer/src/lib.rs:1249-1257` |
| Codegen Go column type | emits `lazuli.EncryptedRef` typedef | `crates/lazuli_codegen_go/src/emitter/resource.rs:897-911` |
| Codegen Go encrypt/decrypt wiring | **missing** — no `Cipher.Encrypt`/`Decrypt` call sites generated | confirmed via grep |
| Codegen TS shape | **missing** — TS SDK treats encrypted fields as opaque strings; no client-side noise | confirmed |
| Runtime `encryption.Cipher` | shipping in parallel via `cdx-encryption-aes` | `runtime/go/lazuli/encryption/aes_gcm.go` (Codex cell) |
| Runtime `encryption.Registry` | **missing** — no per-tenant/global resolver | to ship under this proposal |
| Runtime `EncryptedRef` typedef | placeholder string alias mentioning missing adapter | `runtime/go/lazuli/types.go:55-72` |
| App `encryption { ... }` block | **missing** — no syntax to declare key sources | new construct (this proposal) |
| Doctor diagnostics | **zero** `ENC-*` codes | confirmed via grep |
| LSP hover/completion on `@cap.Encrypted` | text-pattern warning when `key:` missing | `crates/lazuli_lsp/src/lib.rs:2868,2889` |
| Inspect `--expand=security` projection | surfaces `@key.*` markers as pass-through strings | `docs/canonical-semantics.md:1024` |
| Migration DDL | emits encrypted column as `BYTEA` placeholder | `crates/lazuli_codegen_go/src/emitter/migration_ddl.rs:957` |
| Highlighting | `@cap.Encrypted` + `@key.*` colored | `editors/vscode/syntaxes/lazuli.tmLanguage.json` |

**Cross-cutting fact**: every existing `@cap.Encrypted(key:@key.tenant)`
use site is on a multi-tenant feature (`customer`, `customer_auth`,
`user_auth`, `field-permissions`). The dominant pressure case is
**per-tenant key isolation** — each tenant's data must be encrypted
under a key the tenant alone controls. A single global key is the
degenerate case; the design must support both, with multi-tenant as
canonical.

## Design (Stage 3)

The gap is the **key source declaration**. The capability decorator
already names the scope (`@key.tenant`); the missing piece is the
binding between that abstract scope and an actual byte resolver. Two
candidates considered.

### Candidate A — Capability decorator-only (extend `@cap.Encrypted` args)

Add `provider:` and `key_name:` arguments to `@cap.Encrypted`:

```lazuli
field external_id: @cap.Encrypted(key:@key.tenant,
                                  provider:env,
                                  key_name:CRYPT_KEY_TENANT_{tenant_id}) optional
```

Pros:
- Local: everything about the field's encryption lives on the field.
- Consistent with `@cap.File(max_size:25mb,accept:text/csv)` — the
  pattern is "capability decorator carries all args".

Cons:
- **Repetition**: every `@cap.Encrypted` site in a capsule repeats
  the same `provider:` + `key_name:` template. The `field-permissions.lzi`
  fixture would gain 3 identical 30-token decorator suffixes per
  resource. Conservative count: 50+ tokens of pure boilerplate per
  multi-field resource × N resources per feature × M features per app.
  Rule Zero red flag: this is mechanism leaking when vocabulary
  should carry the contract.
- **Cold-read friction**: a reader scanning a 20-field resource
  sees the same `provider:env, key_name:CRYPT_KEY_TENANT_{tenant_id}`
  decorator 6 times and has to mentally diff each one looking for
  the deviation that isn't there. The LLM-cold-read test fails: the
  author's intent ("encrypt these under the tenant key, all the
  same way") is buried in repeated syntax.
- **Drift surface**: nothing stops a typo making one field's
  `key_name:` differ from its siblings. Doctor would need
  cross-field consistency checks per capsule — a complicated lint
  for a problem that vanishes if the source is declared once.
- **Provider names leak into source**: `provider:env` is a runtime
  mechanism (where the key bytes come from), not a contract. The
  language already separates contracts from mechanisms — capability
  arguments should be product-meaningful (sizes, MIME types, TTLs),
  not infrastructure-meaningful.

### Candidate B — App-level `encryption` block + unchanged field surface

Keep the field surface exactly as today (`@cap.Encrypted(key:@key.tenant)`)
and add a new app-level block declaring how each `@key.*` scope
resolves to bytes:

```lazuli
# app.lzi (or registry.lzi when the capsule prefers)
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm

    key @key.app
      source env.CRYPT_KEY_APP
      algorithm aes_256_gcm
```

Pros:
- **One declaration per scope, not per field**. The cold-read story
  is correct: a reader sees that `@key.tenant` is sourced from a
  per-tenant env-var template, and every field tagged
  `@cap.Encrypted(key:@key.tenant)` inherits that decision.
- **Provider neutrality stays in the field**: the field says "this
  is encrypted under the tenant scope". The app says "the tenant
  scope is backed by an env var (or a KMS slot, or a secrets
  table)". This is the same separation as `requires integration
  crm: CRMProvider` (feature declares the slot) vs `bindings
  customer_import.crm = integrations.crm` (app resolves the slot).
- **Consistent with existing app blocks**: `app.lzi` already owns
  `cors`, `logging`, `tracing`, `deploy`, `runtime` — all
  cross-cutting operational concerns declared once and referenced
  everywhere. `encryption` fits naturally.
- **Multi-tenant template syntax**: `source env.CRYPT_KEY_TENANT_{tenant_id}`
  uses the same template-literal shape as `tenant_from
  payload.org_id` and gives doctor a tractable cross-check (the
  template must include `{tenant_id}` when the scope is
  `@key.tenant`).
- **Adapter swap is one-line**: switching from env-var-per-tenant
  to a KMS-backed binding is one block change. With Candidate A,
  every `@cap.Encrypted` site needs editing.
- **AI-first**: the LLM author writes `@cap.Encrypted(key:@key.tenant)`
  by pattern, then declares the source once. The LLM cold-reader
  sees the scope and looks up the binding in one place.

Cons:
- Two declarations (field + app) instead of one. Mitigated by the
  point above: the second declaration is shared.
- New top-level block grows the `app.lzi` schema. Acceptable —
  this is exactly how cross-cutting concerns are supposed to
  grow.

### Candidate C — Hybrid: short form `encrypted` + app block (proposed compromise considered)

Add a short form `field external_id: Text encrypted` that desugars to
`@cap.Encrypted(key:@key.tenant)` using an app-level
`encryption.default_scope @key.tenant` default. The app-level
`encryption` block from Candidate B remains.

Rejected:
- The short form duplicates the capability decorator (two ways to
  express the same intent — Rule Zero violation per
  `docs/grading-rubric.md` criterion 5 "Determinism").
- The default-scope mechanism hides the key scope from cold-readers;
  the LLM can no longer tell from one line which key blast radius
  applies.
- Existing fixtures already use the long form ubiquitously; the
  short form would require either rewriting them all (churn for no
  semantic gain) or maintaining two equivalent forms forever (more
  Rule Zero pain).
- The 30-token "savings" per field is illusory — the field already
  carries `@pii.<class>` markers next to the capability, so the line
  is rarely tight.

### Recommendation

**Candidate B**. Adopt the app-level `encryption` block; leave the
field surface unchanged. This:

1. Honours Rule Zero (one canonical form per intent).
2. Honours the runtime-wire boundary (provider mechanics declared
   at app level, not at field level).
3. Matches the precedent of every other cross-cutting operational
   concern in `app.lzi`.
4. Makes the per-tenant key isolation case the natural default.
5. Demands no rewrite of existing fixtures.

### Formal grammar (EBNF, draft for `docs/grammar.app.md`)

```ebnf
encryption_block    = "encryption" NEWLINE INDENT
                      key_binding+
                      DEDENT ;

key_binding         = "key" key_scope NEWLINE INDENT
                      source_clause
                      algorithm_clause
                      [ rotation_clause ]
                      DEDENT ;

key_scope           = "@key.app"
                    | "@key.tenant"
                    | "@key.user"
                    | "@key.record" ;

source_clause       = "source" source_expr NEWLINE ;

source_expr         = env_template
                    | secrets_template ;

env_template        = "env." IDENT_UPPER ( "{" template_axis "}" )* ;

secrets_template    = "secrets." IDENT_LOWER ( "{" template_axis "}" )* ;

template_axis       = "tenant_id" | "user_id" | "record_id" ;

algorithm_clause    = "algorithm" enc_algorithm NEWLINE ;

enc_algorithm       = "aes_256_gcm" ;    (* closed catalog v0 *)

rotation_clause     = "rotation" rotation_strategy NEWLINE ;

rotation_strategy   = "manual"            (* v0 default *)
                    | "kms_managed" ;     (* speculative; deferred *)
```

### Slot inventory

| Slot | Required | Type | Closed catalog | Fixture anchor (after Stage 3 lands) |
|---|---|---|---|---|
| `encryption.key <scope>` | yes (one per scope referenced by `@cap.Encrypted` / `@cap.E2ee`) | `@key.<closed catalog>` | `app`, `tenant`, `user`, `record` | `app.lzi` |
| `encryption.key.source` | yes | env or secrets template | `env.<UPPER>` / `secrets.<lower>` | `app.lzi` |
| `encryption.key.algorithm` | yes | identifier | `aes_256_gcm` (v0; pairs with `runtime/go/lazuli/encryption/aes_gcm.go`) | `app.lzi` |
| `encryption.key.rotation` | optional | identifier | `manual` (v0); `kms_managed` deferred | `app.lzi` |

Template-axis requirements (doctor-enforced):

| Scope | Required template axis | Rationale |
|---|---|---|
| `@key.app` | none | global key; same bytes for the whole app |
| `@key.tenant` | `{tenant_id}` | per-tenant isolation; one cipher per tenant |
| `@key.user` | `{user_id}` | per-user isolation; rare; e.g. private notes |
| `@key.record` | `{record_id}` | per-row keys; the strictest; envelope-encryption pattern |

### Example expansion in the fixture

Stage 3 extends `examples/full-capsule/app.lzi` to declare the
binding consumed by the existing `@cap.Encrypted` use sites:

```lazuli
app AcmeCRM
  title "Acme CRM"
  ...

  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
```

And extends `examples/full-capsule/registry.lzi` so the env var is
schema-validated:

```lazuli
registry
  env
    group encryption
      server CRYPT_KEY_TENANT_{tenant_id}: Secret required
    ...
```

The `{tenant_id}` template marker in an `env` schema entry is a new
**registry pattern**, not a new env variable per tenant. The runtime
substitutes `{tenant_id}` at resolution time against the current
request's tenant scope.

### Closed-catalog rationale

- `algorithm = aes_256_gcm` — single canonical algorithm for v0,
  paired 1:1 with the runtime cell (`aes_gcm.go`). New algorithms
  (XChaCha20-Poly1305, AES-256-SIV) wait for pilot pressure.
  Same closed-catalog discipline as `@cap.Hashed(algorithm:argon2id)`.
- `rotation = manual` — v0 ships rotation as an operational
  procedure (rewrite env var, re-encrypt rows via job). KMS-managed
  rotation requires a key-version field on every encrypted column
  and a much heavier IR. **Deferred to Wave 2** per
  `corbanx-class-readiness.md`.
- `source = env.* | secrets.*` — two prefixes only. `env.*` resolves
  through the env var schema declared in `registry.lzi`. `secrets.*`
  resolves through `@adapter.secrets` (canonical secret store
  adapter; not designed in this proposal). The two surfaces
  separate "key bytes live in env" from "key bytes live in a secret
  manager" without each adapter inventing its own template syntax.

## Lowering (Stage 4)

The IR carries the binding alongside the existing app-level shapes
(cors, deploy, logging, tracing). Two additive IR types.

### IR additions (`crates/lazuli_ir/src/lib.rs`)

```rust
// additive — placed alongside existing app-level IR (logging, tracing, deploy)

/// Phase L Tier 5 — app-level encryption binding catalog. One entry
/// per `@key.<scope>` referenced anywhere in the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionBinding {
    /// `@key.<scope>` reference, stored verbatim with the `@key.` prefix.
    pub scope: String,
    /// Where the key bytes live. Closed catalog: env template or
    /// secrets template; resolver template axes preserved verbatim.
    pub source: EncryptionSource,
    /// Symmetric algorithm. v0: `Aes256Gcm`.
    pub algorithm: EncryptionAlgorithm,
    /// v0: `Manual`; `KmsManaged` deferred.
    pub rotation: EncryptionRotation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EncryptionSource {
    /// `env.<UPPER>{...}` — resolves via the env var schema.
    Env(EncryptionTemplate),
    /// `secrets.<lower>{...}` — resolves via `@adapter.secrets`.
    Secrets(EncryptionTemplate),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionTemplate {
    /// The literal name with template axes preserved
    /// (e.g. `"CRYPT_KEY_TENANT_{tenant_id}"`).
    pub literal: String,
    /// Parsed template axes; closed catalog `{tenant_id, user_id, record_id}`.
    pub axes: Vec<EncryptionTemplateAxis>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionTemplateAxis {
    TenantId,
    UserId,
    RecordId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionRotation {
    Manual,
}

// AppManifest gains the catalog:
pub struct AppManifest {
    // ... existing fields ...
    pub encryption_bindings: Vec<EncryptionBinding>,
}
```

`@cap.E2ee` joins `CapabilityRef` as a sibling variant so doctor can
cross-check it against the same `EncryptionBinding` catalog. Its IR
shape mirrors `EncryptedCapability` exactly:

```rust
pub enum CapabilityRef {
    File(FileCapability),
    Hashed(HashedCapability),
    Encrypted(EncryptedCapability),
    E2ee(E2eeCapability),       // new — additive
    Token(TokenCapability),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct E2eeCapability {
    pub key: String,
}
```

### Analyzer (`crates/lazuli_analyzer/src/lib.rs`)

Two additive lowering paths:

1. **`@cap.E2ee` parser**: clone of `parse_cap_encrypted_type` at
   `lib.rs:1249-1257`, gated on `@cap.E2ee(` prefix.
2. **`encryption` block parser**: new visitor on the `app` block,
   emits `Vec<EncryptionBinding>`. Template parsing reuses the
   existing identifier lexer; `{tenant_id}` etc. detected via a
   small `parse_template_axes` helper. Doctor enforces template
   axis required-ness per scope.

The legacy `@cap.Secret` form remains deprecated (`docs/invariants.md:423`)
and continues to emit the existing warning.

### Surface → IR mapping

| Surface | IR field | Notes |
|---|---|---|
| `encryption.key @key.tenant.source env.CRYPT_KEY_TENANT_{tenant_id}.algorithm aes_256_gcm` | `AppManifest.encryption_bindings.push(EncryptionBinding { scope: "@key.tenant", source: Env(EncryptionTemplate { literal: "CRYPT_KEY_TENANT_{tenant_id}", axes: [TenantId] }), algorithm: Aes256Gcm, rotation: Manual })` | template axes parsed from the literal |
| `field external_id: @cap.Encrypted(key:@key.tenant) optional` | unchanged: `Field.type_ref = TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability { key: "@key.tenant" }))` | already lowered; this proposal only adds the binding lookup at codegen time |
| `field message: @cap.E2ee(key:@key.user) required` | `Field.type_ref = TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability { key: "@key.user" }))` | new variant; mirror of Encrypted |

### Inspect JSON shape (`lazuli inspect --format=json`)

Top-level `app.encryption_bindings` carries the catalog:

```json
{
  "app": {
    "name": "AcmeCRM",
    "encryption_bindings": [
      {
        "scope": "@key.tenant",
        "source": {
          "kind": "env",
          "value": {
            "literal": "CRYPT_KEY_TENANT_{tenant_id}",
            "axes": ["tenant_id"]
          }
        },
        "algorithm": "aes_256_gcm",
        "rotation": "manual",
        "origin": "examples/full-capsule/app.lzi:121"
      }
    ]
  }
}
```

Under `--expand=security` the existing per-field projection gains a
`bound_to` pointer so cold-readers see which binding any
`@cap.Encrypted` field resolves through:

```json
{
  "feature": "customer",
  "fields": [
    {
      "resource": "Customer",
      "field": "external_id",
      "capability": {
        "kind": "encrypted",
        "key": "@key.tenant",
        "bound_to": "app.encryption_bindings[@key.tenant]"
      }
    }
  ]
}
```

`bound_to` is `null` when the doctor diagnostic `ENC-KEY-MISSING-001`
would fire (no binding declared) so inspect remains coherent on
broken capsules.

### Cross-refs the analyzer must register

| Edge | Source | Target | Resolution scope |
|---|---|---|---|
| `field.@cap.Encrypted(key:@key.<scope>)` ↔ `app.encryption_bindings[@key.<scope>]` | any field carrying `EncryptedCapability` or `E2eeCapability` | the app must declare an `encryption.key @key.<scope>` entry | package-wide; doctor diagnostic `ENC-KEY-MISSING-001` |
| `encryption.key.source.template.axes` ↔ scope | scope `@key.tenant` requires `{tenant_id}`; `@key.user` requires `{user_id}`; `@key.record` requires `{record_id}`; `@key.app` forbids any axis | template literal | per-binding; doctor diagnostic `ENC-TEMPLATE-AXIS-001` |
| `encryption.key.source.env.<NAME>` ↔ `registry.env.*.<NAME>` | env-prefix source | the env var (or templated env var) must exist in the registry schema | package-wide; doctor diagnostic `ENC-SOURCE-ENV-001` |
| `defaults.tenancy = org` ↔ presence of any `@key.tenant` usage | feature defaults | tenant-scoped encryption requires the feature to be tenant-scoped | feature-local; doctor diagnostic `ENC-TENANCY-001` |
| `@cap.E2ee` ↔ `event_group.payload` | E2ee fields cannot leak through unredacted event payloads | payload schema | feature-local; doctor diagnostic `ENC-E2EE-EVENT-001` |

## Codegen plan (Stage 5)

Three changes; **no homegrown crypto** in any of them.

### Boot wiring — `dist/go/main.go`

The generator adds a per-binding `encryption.Resolver` registration
at boot. Skeletal output (one block per binding declared in
`app.lzi`):

```go
// path: dist/go/main.go (generated)
// Code generated by Lazuli. DO NOT EDIT.
package main

import (
    "github.com/lazuli-lang/runtime/go/lazuli"
    "github.com/lazuli-lang/runtime/go/lazuli/encryption"
)

func init() {
    encryption.Register(encryption.Binding{
        Scope:     "@key.tenant",
        Source:    encryption.SourceEnv,
        Template:  "CRYPT_KEY_TENANT_{tenant_id}",
        Axes:      []encryption.TemplateAxis{encryption.AxisTenantID},
        Algorithm: encryption.AlgorithmAES256GCM,
        Rotation:  encryption.RotationManual,
    })
}
```

The `encryption.Register` call is the only generated code that knows
about the binding; everything else flows through `encryption.For(ctx,
"@key.tenant")` which returns a `*encryption.Cipher` resolved
against the current tenant.

### Resource repository — `dist/go/<feature>/<resource>_repo.gen.go`

Today the generated repo emits direct INSERT/SELECT. The codegen
extension wraps every `EncryptedCapability` / `E2eeCapability`
field with `enc.Encrypt` / `enc.Decrypt` calls at the repository
boundary. Skeletal output for the `Customer.external_id` field:

```go
// path: dist/go/customer/customer_repo.gen.go (generated)
// Code generated by Lazuli. DO NOT EDIT.

func (r *CustomerRepo) Insert(ctx *lazuli.Ctx, row Customer) error {
    cipher, err := encryption.For(ctx, "@key.tenant")
    if err != nil {
        return err
    }
    encExternalID, err := cipher.Encrypt(row.ExternalID)
    if err != nil {
        return err
    }
    _, err = r.db.Exec(ctx,
        `INSERT INTO customers (id, ..., external_id) VALUES ($1, ..., $N)`,
        row.ID, ..., encExternalID,
    )
    return err
}

func (r *CustomerRepo) ByID(ctx *lazuli.Ctx, id lazuli.ID) (Customer, error) {
    var row Customer
    var encExternalID lazuli.EncryptedRef
    err := r.db.QueryRow(ctx,
        `SELECT id, ..., external_id FROM customers WHERE id = $1`, id,
    ).Scan(&row.ID, ..., &encExternalID)
    if err != nil {
        return Customer{}, err
    }
    cipher, err := encryption.For(ctx, "@key.tenant")
    if err != nil {
        return Customer{}, err
    }
    row.ExternalID, err = cipher.Decrypt(encExternalID)
    if err != nil {
        return Customer{}, err
    }
    return row, nil
}
```

Key codegen rules:

1. **Boundary**: encrypt happens between the in-memory struct and
   the DB driver. Decrypt happens between DB rows and the in-memory
   struct. No business logic ever sees ciphertext.
2. **Cipher lookup is per-request**: `encryption.For(ctx, "@key.tenant")`
   reads the tenant from `ctx` (already canonical via
   `runtime/go/lazuli/ctx.go`) and returns the resolved cipher. The
   cipher is cached on the registry; the request never paid the
   key-derivation cost twice.
3. **E2ee fields**: codegen for `@cap.E2ee` is identical to
   `@cap.Encrypted` on the write path but **omits the decrypt call**
   on the read path. The server stores the ciphertext, returns it
   to clients (web/mobile) unchanged, and never holds plaintext.
   The runtime `Cipher.E2eeEncrypt` is a distinct method whose
   contract is "encrypt with a key the server may not later
   decrypt" (typically the user's derived key, not stored
   server-side).
4. **Optional fields**: when the field is `optional`, codegen emits
   `if row.ExternalID != "" { ... }` guards so empty values bypass
   the cipher.
5. **No `Cipher` symbol in the generated code's import path other
   than `encryption.For`**: every concrete cipher implementation
   (AES-256-GCM today, future XChaCha) lives behind the
   `encryption.Cipher` interface in the runtime package. Codegen
   never names AES.

### Migration DDL — `crates/lazuli_codegen_go/src/emitter/migration_ddl.rs`

The existing emitter writes `BYTEA` columns for encrypted fields
(`migration_ddl.rs:957`). Stage 5 extends the column comment to
embed the bound `@key.<scope>` and algorithm so DBAs cold-reading
the schema see the contract:

```sql
CREATE TABLE customers (
  id UUID PRIMARY KEY,
  ...
  external_id BYTEA, -- @cap.Encrypted(key:@key.tenant) algorithm=aes_256_gcm
  ...
);
```

No SQL-level change beyond the comment.

### Types reused from `runtime/go/lazuli`

- `lazuli.Ctx` — already canonical.
- `lazuli.EncryptedRef` — already defined as a string typedef at
  `runtime/go/lazuli/types.go:67`. Stage 6 promotes it to a typed
  envelope (see Runtime).
- `encryption.Cipher`, `encryption.Binding`, `encryption.Registry`,
  `encryption.For` — new. See Runtime.

Boundary discipline: codegen never names `aes`, `gcm`, `nonce`,
`tag`, or any concrete cipher detail. The generated code calls
`cipher.Encrypt` / `cipher.Decrypt` and lets the runtime supply
the algorithm.

## Runtime (Stage 6)

Three new files under `runtime/go/lazuli/encryption/`. The
AES-256-GCM primitive (`aes_gcm.go`) ships in parallel via Codex
cell `cdx-encryption-aes`; this proposal designs the resolver and
binding registry around it.

### `runtime/go/lazuli/encryption/aes_gcm.go` (parallel Codex cell)

- **Capability**: `Cipher` interface plus AES-256-GCM
  implementation backed by `crypto/aes` + `crypto/cipher` (stdlib).
  Single struct, ~50-80 LOC of stdlib wiring.
- **Methods**: `NewCipher(key []byte) (*AESGCMCipher, error)`,
  `Encrypt(plaintext []byte) ([]byte, error)`,
  `Decrypt(ciphertext []byte) ([]byte, error)`.
- **Format**: nonce || ciphertext || tag (96-bit nonce, 128-bit tag —
  GCM standard). The runtime never invents a wire format.
- **Wire-thin verdict**: this is ~30 LOC wrapping stdlib. Honours
  `CLAUDE.md` line 8 ("Lazuli is abstraction; the runtime is wire").

### `runtime/go/lazuli/encryption/resolver.go` (this proposal)

- **Capability**: `Binding`, `TemplateAxis`, `Source`, `Algorithm`,
  `Rotation` types matching the IR; `Registry` struct mapping
  `@key.<scope>` → per-tenant `*Cipher`; `Register(Binding)` global
  registration; `For(ctx, scope) (*Cipher, error)` per-request
  resolver.
- **Lifecycle**: registry initialized at boot (`init()` blocks in
  generated `main.go`). Per-tenant ciphers are lazily derived on
  first request and cached for the process lifetime.
- **Config**: reads env values via the existing `lazuli.Env(name)`
  accessor (`runtime/go/lazuli/env.go`); reads secrets via the
  pending `@adapter.secrets` interface (deferred to Wave 2;
  for v0, `SourceSecrets` errors with `ErrSecretsAdapterMissing`).
- **Dependency**: none beyond `aes_gcm.go` + `lazuli.Env`.
- **Typed errors**:
  - `ErrBindingMissing` → "no encryption binding declared for
    `@key.<scope>`; declare it in `app.lzi encryption` block."
    Maps to runtime-internal 500 (should be doctor-caught).
  - `ErrEnvKeyMissing` → "env var `<resolved-template>` is empty
    or not set". Maps to 500.
  - `ErrKeyDecodeFailed` → "env value is not a base64-encoded
    32-byte key". Maps to 500.
  - `ErrEncryptFailed`, `ErrDecryptFailed` → "cipher operation
    failed". Maps to 500 (with redacted detail).

### `runtime/go/lazuli/encryption/types.go` (this proposal)

- **Capability**: re-exports / promotes `lazuli.EncryptedRef` from
  the placeholder string alias (`runtime/go/lazuli/types.go:67`) to
  a thin typed envelope alias. Keeps it `[]byte`-compatible so
  existing pgx scanners work without changes.
- **Lifecycle**: stateless.
- **Dependency**: none.

### Adapter contract

`encryption.Source = Env | Secrets`. For v0 only `SourceEnv` is
implemented (calls `lazuli.Env(template_resolved)`). `SourceSecrets`
is reserved for the Wave 2 secrets adapter; the registry rejects
`SourceSecrets` bindings with `ErrSecretsAdapterUnavailable` until
that adapter ships.

Adapter packages (`@plugin/aws-kms`, `@plugin/gcp-kms`, `@plugin/vault`)
would later implement an `encryption.SecretsAdapter` interface; this
proposal does not design that interface. Lazuli core never names any
of them.

## Doctor diagnostics (Stage 7)

Six new diagnostic codes, scoped per the closed-cycle gate.

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `ENC-KEY-MISSING-001` | error | "field `<resource>.<field>` declares `@cap.Encrypted(key:@key.<scope>)` but the app declares no `encryption.key @key.<scope>` binding. Add a binding in `app.lzi` (or `registry.lzi`)." | any `EncryptedCapability` / `E2eeCapability` whose `key` has no matching `AppManifest.encryption_bindings` entry | `tests/fixtures/encryption/key_missing.lzi` |
| `ENC-TEMPLATE-AXIS-001` | error | "`encryption.key @key.<scope>` source template `<literal>` is missing required axis `{<axis>}`; tenant-scoped keys must include `{tenant_id}`, user-scoped `{user_id}`, record-scoped `{record_id}`. App-scoped keys forbid all axes." | template literal axis set disagrees with the scope catalog | `tests/fixtures/encryption/template_axis_mismatch.lzi` |
| `ENC-SOURCE-ENV-001` | error | "`encryption.key @key.<scope>` references env var `<NAME>` but no `env <NAME>` schema entry exists in `registry.lzi` (or `app.lzi`)." | source `env.<NAME>` doesn't resolve in the env schema | `tests/fixtures/encryption/source_env_missing.lzi` |
| `ENC-TENANCY-001` | warning | "feature `<feature>` uses `@cap.Encrypted(key:@key.tenant)` but its `defaults.tenancy` is not `org` (or equivalent tenant axis). Tenant-scoped encryption without tenant-scoped data is a contract gap." | a tenant-scoped capability lives on a feature whose `defaults.tenancy` is none/global | `tests/fixtures/encryption/tenancy_mismatch.lzi` |
| `ENC-E2EE-EVENT-001` | error | "field `<resource>.<field>` is `@cap.E2ee` but its enclosing event payload `<event>` exposes `payload.<field>` to consumers. E2ee fields must not appear in event payloads — the server cannot decrypt them and consumers see ciphertext." | any `event_group` payload includes a field carrying `E2eeCapability` | `tests/fixtures/encryption/e2ee_event_leak.lzi` |
| `ENC-ROTATION-001` | warning | "`encryption.key @key.<scope>` declares no `rotation` strategy; defaulting to `manual`. Document the rotation procedure in the capsule's runbook." | binding lacks `rotation` child and scope is `@key.tenant` / `@key.user` / `@key.record` (the cases where rotation matters operationally) | `tests/fixtures/encryption/rotation_undeclared.lzi` |

`ENC-KEY-MISSING-001`, `ENC-TEMPLATE-AXIS-001`, `ENC-SOURCE-ENV-001`,
and `ENC-E2EE-EVENT-001` register under `is_security_enforcement_code`
(`crates/lazuli_lsp/src/lib.rs:9527`) so the strict + production
profiles upgrade severity uniformly.

### Diagnostic anchors

- `ENC-KEY-MISSING-001` — cross-feature pass in
  `crates/lazuli_cli/src/doctor.rs` once IR carries
  `AppManifest.encryption_bindings`.
- `ENC-TEMPLATE-AXIS-001` — file-local in LSP (typed shape) + cross-
  feature in doctor (analyzer parses template axes from literal).
- `ENC-SOURCE-ENV-001` — cross-feature pass; reuses the existing
  env-var harvester (`registry.lzi` env schema is already
  cross-checked for `env.NAME` reads elsewhere).
- `ENC-TENANCY-001` — feature-local pass joining `defaults.tenancy`
  to capability scope.
- `ENC-E2EE-EVENT-001` — cross-feature pass over the event payload
  graph.
- `ENC-ROTATION-001` — file-local on `app.lzi` `encryption` block.

### LSP hovers (new entries)

Add to `KEYWORD_HOVER` in `crates/lazuli_lsp/src/lib.rs`:

| Keyword | Hover |
|---|---|
| `encryption` (top-level in `app.lzi`) | "App-level encryption key binding catalog. One `key @key.<scope>` child per `@cap.Encrypted` / `@cap.E2ee` scope referenced in the capsule. Required scopes: any declared by feature surfaces. Closed catalog: `@key.app`, `@key.tenant`, `@key.user`, `@key.record`." |
| `source` (inside `key` child) | "Key source. `env.<NAME>{...}` resolves via the env var schema in `registry.lzi`. `secrets.<name>{...}` resolves via `@adapter.secrets` (Wave 2). Template axes: `{tenant_id}`, `{user_id}`, `{record_id}` per scope." |
| `algorithm` (inside `key` child) | "Encryption algorithm. v0: `aes_256_gcm` (paired with `runtime/go/lazuli/encryption/aes_gcm.go`)." |
| `rotation` (inside `key` child) | "Key rotation strategy. v0: `manual` (rewrite env, re-encrypt rows via a job). `kms_managed` is deferred to a future cut." |

Closed-catalog completions:

- `encryption.key ` → `@key.app`, `@key.tenant`, `@key.user`, `@key.record`.
- `encryption.key.source ` → `env.`, `secrets.`.
- `encryption.key.algorithm ` → `aes_256_gcm`.
- `encryption.key.rotation ` → `manual`.

### Namespaces (`is_allowed_reference_namespace`)

No new namespace required. `@cap.Encrypted`, `@cap.E2ee`, and
`@key.*` are already in the closed catalog
(`crates/lazuli_lsp/src/lib.rs:2114-2135`,
`docs/invariants.md:251-265`).

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

Add `encryption` as an app-block keyword scope. Add `source`,
`algorithm`, `rotation` to the encryption-argument scope. `env.*`,
`secrets.*` resolve through the existing namespace scope. Template
braces `{tenant_id}` etc. use the existing string-interpolation
scope.

## Acceptance gate (closed-cycle criterion)

Adapted from `docs/roadmap.md` §0 8-item checklist to the encryption
bucket:

- [ ] **Fixture authors the full surface.** `examples/full-capsule/app.lzi`
  declares the `encryption` block binding `@key.tenant` to
  `env.CRYPT_KEY_TENANT_{tenant_id}` with `algorithm aes_256_gcm`.
  `registry.lzi` declares the templated env var. Existing field
  use sites stay unchanged.
- [ ] **`lazuli check examples/full-capsule` accepts the syntax**
  after the analyzer adds the `encryption` block parser. Existing
  fixtures without an `encryption` block produce
  `ENC-KEY-MISSING-001` on every `@cap.Encrypted` field (forcing
  the migration).
- [ ] **`lazuli inspect --format=json --expand=security examples/full-capsule`**
  shows `app.encryption_bindings` and per-field `bound_to`
  references. Inspect schema matches the JSON shape in Stage 4.
- [ ] **`lazuli doctor` emits the 6 named diagnostics** on matching
  fixtures with zero false positives on the canonical fixture.
- [ ] **`lazuli generate` produces Go that compiles.** The
  `customer_repo.gen.go` file under `dist/go/customer/` carries
  `encryption.For(ctx, "@key.tenant")` calls wrapping `external_id`
  reads and writes. `dist/go/main.go` registers the binding at boot.
- [ ] **Lazuli Go round-trip test passes.** A new
  `runtime/go/lazuli/encryption/resolver_test.go` covers:
  (a) `Register` + `For` returns a stable cipher per tenant,
  (b) per-tenant key derivation yields distinct ciphertexts for
  the same plaintext under different tenants,
  (c) `ErrBindingMissing` fires when no binding exists,
  (d) tamper detection (changing a byte in the ciphertext yields
  `ErrDecryptFailed` — GCM auth tag check).
- [ ] **LSP hover/completion** covers the 4 new keywords + 4 new
  closed catalogs.
- [ ] **Highlighting** colors the new tokens correctly.

When all eight items are green, the encryption bucket cycle is
closed.

## Cells

Implementation split into single-file Codex cells where possible
(L.A.* = analyzer/IR, L.B.* = doctor, L.C.* = codegen). Wire-up
happens in the orchestrator post-merge per `CLAUDE.md` Codex
discipline.

### L.A.* — IR + analyzer

- **L.A.1** `crates/lazuli_ir/src/encryption.rs` — new file declaring
  `EncryptionBinding`, `EncryptionSource`, `EncryptionTemplate`,
  `EncryptionTemplateAxis`, `EncryptionAlgorithm`,
  `EncryptionRotation`, `E2eeCapability`. Re-exported from
  `crates/lazuli_ir/src/lib.rs`. Pure type definitions; no logic.
- **L.A.2** `crates/lazuli_analyzer/src/encryption_parse.rs` — new
  file with `parse_encryption_block(&AppBlockAst)
  -> Vec<EncryptionBinding>` and `parse_cap_e2ee_type(&str)
  -> Option<E2eeCapability>` (mirror of `parse_cap_encrypted_type`).
  Wire-up into `lib.rs:486-501` (`type_ref_from_syntax`) and the app
  manifest builder is the orchestrator's one-line edit.
- **L.A.3** `crates/lazuli_syntax/src/parser.rs` — extend the
  canonical-indent slice to parse `encryption` as a top-level block
  inside `app`. Single block kind, three required children, one
  optional child. ~40 LOC patch.

### L.B.* — doctor + LSP

- **L.B.1** `crates/lazuli_cli/src/doctor/encryption/key_missing.rs`
  (new module file) — implements `ENC-KEY-MISSING-001`. Single
  pass over `AppManifest.encryption_bindings` joined against every
  feature's `EncryptedCapability` / `E2eeCapability` field set.
- **L.B.2** `crates/lazuli_cli/src/doctor/encryption/template_axis.rs`
  — implements `ENC-TEMPLATE-AXIS-001`. Pure per-binding check.
- **L.B.3** `crates/lazuli_cli/src/doctor/encryption/source_env.rs`
  — implements `ENC-SOURCE-ENV-001`. Joins binding sources against
  the env-var schema.
- **L.B.4** `crates/lazuli_cli/src/doctor/encryption/tenancy.rs` —
  implements `ENC-TENANCY-001`. Feature-local pass.
- **L.B.5** `crates/lazuli_cli/src/doctor/encryption/e2ee_event.rs`
  — implements `ENC-E2EE-EVENT-001`. Cross-feature pass over event
  payload graph.
- **L.B.6** `crates/lazuli_cli/src/doctor/encryption/rotation.rs` —
  implements `ENC-ROTATION-001`. Per-binding pass.
- **L.B.7** `crates/lazuli_lsp/tests/encryption.rs` — LSP test
  covering hover + completion for the 4 new keywords.

### L.C.* — codegen + runtime

- **L.C.1** `crates/lazuli_codegen_go/src/emitter/encryption.rs` —
  new emitter module producing the `encryption.Register` block for
  `dist/go/main.go`. Reads `AppManifest.encryption_bindings`.
- **L.C.2** `crates/lazuli_codegen_go/src/emitter/repo_encryption.rs`
  — extends the resource repo emitter to wrap encrypted-field reads
  and writes in `encryption.For` + `Encrypt` / `Decrypt` calls.
  Single new file; wire-up into the existing `resource.rs` is a
  small additive edit by the orchestrator.
- **L.C.3** `runtime/go/lazuli/encryption/resolver.go` — registry +
  `For` + binding registration. Consumes the `aes_gcm.go` cipher
  from the parallel Codex cell.
- **L.C.4** `runtime/go/lazuli/encryption/resolver_test.go` —
  per-tenant cipher derivation + tamper detection + binding-missing
  errors.
- **L.C.5** `crates/lazuli_codegen_go/src/emitter/migration_ddl.rs`
  — small patch: encrypted column comments now include the bound
  `@key.<scope>` and `algorithm`.

### Orchestrator wire-up (post-merge, NOT a Codex cell)

- Add 6 doctor diagnostic codes to the codes table in
  `crates/lazuli_cli/src/doctor.rs`.
- Add the `encryption` keyword to the LSP `KEYWORD_HOVER` table.
- Add the new tokens to `editors/vscode/syntaxes/lazuli.tmLanguage.json`.
- Extend `examples/full-capsule/app.lzi` and `registry.lzi` with the
  new block (one positive fixture so doctor doesn't regress).
- Add the 6 fixture `.lzi` files under
  `crates/lazuli_cli/tests/fixtures/encryption/` (one per
  diagnostic) and the corresponding `tests/encryption_doctor.rs`.

## Open questions

1. **`encryption` block placement: `app.lzi` or `registry.lzi`?**
   Both are defensible. `app.lzi` matches `cors`/`logging`/`tracing`
   (operational cross-cutting concerns); `registry.lzi` matches
   `capabilities`/`integrations` (provider-neutral resolution).
   Recommendation: `app.lzi` is canonical; `registry.lzi` accepts
   the same block by alias to mirror the existing optional split
   (`docs/invariants.md:33-35`). Decision punt to grading.
2. **Should `@key.app` be permitted at all?** Multi-tenant SaaS is
   the dominant pressure case; a global app-wide key is rare and
   risky. Option: keep `@key.app` legal but doctor warns `Unusual
   global key scope — verify intent`. Recommendation: keep legal,
   add advisory warning. Punt the lint codification to Wave 2.
3. **Key rotation in v0: how much do we promise?** This proposal
   declares `rotation: manual` as the only legal value and adds a
   doctor warning when omitted. The full rotation contract
   (column-level key version, dual-cipher decrypt during transition)
   is a heavier IR and explicitly deferred. Should we ship even the
   `rotation` slot in v0, or hold it back to keep the IR minimal?
   Recommendation: ship the slot; it's one closed-catalog field and
   pins the contract for future work.
4. **`@cap.E2ee` codegen on the read path: what does the API
   return?** The server holds ciphertext and cannot decrypt. The
   generated query handler can either return the raw ciphertext
   (clients decrypt with the user's derived key) or refuse to
   serialize. Recommendation: return the ciphertext as a typed
   `lazuli.E2eeRef` opaque string; clients implement the user-side
   derive themselves (out of Lazuli scope). Reserve the formal
   client-side wire for a future bucket.
5. **Per-record envelope encryption (`@key.record`)** is in the
   closed catalog but unused in any fixture today. Should v0 even
   wire it through codegen, or stub it out with
   `ENC-SCOPE-UNSUPPORTED-001` until a pilot exercises it?
   Recommendation: stub. Avoids designing a per-row key column
   without pilot pressure.
6. **Migration story for existing `@cap.Encrypted` fixtures
   without an `encryption` block**: every fixture in `examples/`
   becomes broken until each declares the binding. This is a hard
   break. Options:
   a. Doctor downgrades `ENC-KEY-MISSING-001` to a warning when
      `--security-profile prototype` is set, so legacy fixtures
      survive.
   b. Auto-migration script writes a default binding pointing at
      a placeholder env var.
   c. Hold the diagnostic as error from day 1; bulk-update the
      fixtures in the same PR.
   Recommendation: (c). The fixture set is small (4 files); the
   migration is mechanical; the strict-by-default story is the
   one we want long-term.

## Rows sugeridas para `docs/next-checklist.md`

```
| N+1 | Encryption bucket cycle — IR + analyzer | planned | Add `EncryptionBinding`/`EncryptionSource`/`EncryptionTemplate`/`EncryptionAlgorithm`/`EncryptionRotation` to IR. Add `E2eeCapability` to `CapabilityRef`. Extend analyzer with `parse_encryption_block` and `parse_cap_e2ee_type`. Extend canonical-indent slice for `app.encryption`. See `docs/proposals/encryption-vocab.md` §Lowering. |
| N+2 | Encryption bucket cycle — 6 doctor diagnostics + LSP | planned | `ENC-KEY-MISSING-001`, `ENC-TEMPLATE-AXIS-001`, `ENC-SOURCE-ENV-001`, `ENC-TENANCY-001`, `ENC-E2EE-EVENT-001`, `ENC-ROTATION-001`. LSP hover/completion for `encryption`, `source`, `algorithm`, `rotation`. Depends on N+1. See `docs/proposals/encryption-vocab.md` §Doctor. |
| N+3 | Encryption bucket cycle — codegen + Lazuli Go runtime | planned | Codegen `encryption.Register` in `dist/go/main.go` and `encryption.For` + `Encrypt`/`Decrypt` calls in resource repos. Runtime `encryption/{resolver,types}.go` + `resolver_test.go`. Consumes the AES-256-GCM cipher from `cdx-encryption-aes`. Depends on N+1. See `docs/proposals/encryption-vocab.md` §Codegen/§Runtime. |
```
