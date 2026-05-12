# Proposal — Codegen Lazuli → Go

**Author**: lazuli-language-architect (codegen track)
**Date**: 2026-05-11
**Status**: Draft (audit pending)
**Tracks**: Phase Prep §1.1 of `docs/hostpoint-port-checklist.md` — 12-15 cells
**Depends on**: typed IR (Phase L Tier 1-4, done), Lazuli Go lib (Phase L Tier 3 contracts; gaps catalogued below)
**Out of scope**: Cut A agent dispatch (runtime team owns), TypeScript SDK (separate crate `lazuli_codegen_ts`), OpenAPI emission (already shipped via `lazuli_openapi`)

---

## §0. Founding principle (non-negotiable)

Lazuli is the abstraction; **Lazuli Go is wire** of mature Go libraries. The Lazuli → Go emitter is **THIN**: it produces Go user-code that `import "lazuli.dev/runtime/lazuli/<bucket>"` (canonical Go module path; see `docs/architecture.md` glossary) and *calls* abstractions. It does **not** emit implementations from scratch. The Lazuli Go library itself is **hand-written** — codegen never overwrites `runtime/go/lazuli/**`.

Negative reference: Aerocoding's heavy-codegen approach is explicitly **not** the model. Positive references: Encore.dev's "the framework writes the boring code, you write the body" + Rails generators + Vue→Nuxt's "the build emits a tiny shim, the runtime owns the engine".

---

## §1. CLI verb design

### 1.1 Surface

Mirror `lazuli generate openapi` (`crates/lazuli_cli/src/main.rs:84-98`). The closed-catalog `GenerateKind` (`main.rs:146-148`) gains exactly one variant:

```rust
#[derive(Debug, Clone, Copy, ValueEnum)]
enum GenerateKind {
    Openapi,
    Go,           // NEW
}
```

User surface:

```
lazuli generate go <input> --out dist/go/ [--module github.com/acme/<app>] [--lazuli-go-version v0.1.0]
```

| Flag | Required | Default | Purpose |
|---|---|---|---|
| `<input>` | yes | — | Path to `app.lzi` or directory containing one (mirrors `generate openapi`) |
| `--out` / `-o` | yes for `go` | — | Output directory (canonical: `dist/go/`); `generate openapi` lets stdout substitute, `go` does not (multi-file output, must hit disk) |
| `--module` | no | derived from `app.name` → `lazuli/<kebab-name>` | Go module path emitted in root `go.mod`. Project-scoped override. |
| `--lazuli-go-version` | no | crate-pinned constant `LAZULI_GO_VERSION` | Version constraint emitted in `require lazuli.dev/runtime/lazuli vX.Y.Z` |
| `--check` | no | false | Smoke-run the emitter without writing; surfaces undecidables (e.g. unresolved `@plugin/`) and exits non-zero. Mirrors `translate extract --check`. |

### 1.2 Wiring point in `crates/lazuli_cli/src/main.rs`

`generate_command` (`main.rs:400-409`) dispatches by kind. Add one arm:

```rust
match kind {
    GenerateKind::Openapi => generate_openapi(input, output, api_version),
    GenerateKind::Go => generate_go(input, output, module, lazuli_go_version, check),
}
```

`generate_go` calls `lazuli_codegen_go::generate_v1(&module, GoEmitOptions { … })` and writes the returned `Vec<GeneratedFile>` via the existing `write_generated_file` helper (`main.rs:1006-1015`).

`generate_v1` is the new entry point; the legacy `generate()` at `crates/lazuli_codegen_go/src/lib.rs:35` (which produces a hard-coded sample backend) is renamed to `generate_legacy_demo()` and kept for `lazuli spike-generate` parity until the spike CLI verb retires. No behavior change to existing callers.

### 1.3 Discoverability + ergonomics

- `lazuli generate --help` already enumerates kinds via clap derive — `go` lights up for free.
- `lazuli doctor` learns one new check: `CODEGEN-GO-001` "regenerate `dist/go/` after IR change" — diff `dist/go/<feature>/<feature>.gen.go` header banner against the input file's mtime hash. Out of scope for this proposal; gated until first Hostpoint port cell ships.

---

## §2. Emitter architecture

### 2.1 Choice: **handwritten printer**, mirroring `crates/lazuli_openapi/src/lib.rs`

The existing canonical emitter in `lazuli_openapi` (~620 LOC, no `serde_yaml`, purpose-built `YamlEmitter`) is the accepted pattern. The spike emitter at `crates/lazuli_codegen_go/src/runtime.rs:22-42` already follows the same shape (write banner, write imports, walk feature, write resources/commands/queries, write init). We keep it.

### 2.2 Trade-off table

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Handwritten printer** (`std::fmt::Write`, indent + `writeln!`) | Zero dep cost. Matches the openapi pattern. Column-aligned output (e.g. `db:"…" json:"…"` tags in `runtime.rs:159-160`) trivially expressed. Step-debuggable. Token-economy friendly for LLM reads. | More LOC per kind. Style drift risk between kinds (mitigated: extract shared helpers into one `printer` submodule). | **Chosen** |
| `text/template` (Go stdlib in build.rs?) | Familiar. | Wrong language (template engine in Rust ≠ Go templates). Needs a Rust port like `tera`. Adds dep. | Reject |
| `askama` (Rust compile-time templates) | Type-safe, fast. | Compile-time templates fight code review (template files split logic across `.rs`+`.tpl`). Doctrinal mismatch — IR walks are imperative; templates flatten that. Adds build-graph cost. | Reject |
| `tera` (runtime templates) | Flexible. | Runtime panics on missing keys. Two-language project. Loose typing. | Reject |
| Round-trip through `syn`/`quote` (Rust→Rust macro tooling) | None for Go output. | Wrong target language. | N/A |

### 2.3 Internal layout (`crates/lazuli_codegen_go/src/`)

```
lib.rs               # public API + GoEmitOptions; generate_v1() entry
emitter/
  printer.rs         # GoPrinter (mirrors YamlEmitter): line, indent, kv, banner, aligned_rows
  module.rs          # walks ir::Module → drives per-feature emission, owns go.mod + main.go assembly
  feature.rs         # walks ir::Feature → dispatches by kind to leaf emitters
  resource.rs        # ir::Resource + ir::Record → resource.gen.go
  command.rs         # ir::Command (Create/Update/Delete/Returns) → command.gen.go
  query.rs           # ir::Query (List/Lookup/Sql) → query.gen.go
  api.rs             # ir::Api → api.gen.go
  auth.rs            # ir::Auth → auth.gen.go
  job.rs             # ir::Job → job.gen.go
  webhook.rs         # ir::Webhook → webhook.gen.go
  notification.rs    # ir::Notification → notification.gen.go
  storage.rs         # FileCapability sites → storage.gen.go
  translation.rs     # ir::Translation → translation.gen.go
  tenant_migration.rs# ir::TenantMigration → migration.gen.go
  event_group.rs     # ir::EventGroup → events.gen.go
  types.rs           # TypeRef/BuiltinType/CapabilityRef → Go type strings (centralised mapping)
  imports.rs         # ImportSet (accumulates Go imports, dedups, emits sorted block)
  refs.rs            # @plugin / @runtime / @adapter / @semantic / @cap / @fn resolver
```

`runtime.rs` (the hand-rolled spike, `runtime.rs:22-42`) is folded into `emitter/feature.rs` once kinds beyond Resource/Command/Query land. Until then it stays as the proven feature-level emitter and is wrapped behind the new `generate_v1` for the green slice.

### 2.4 Determinism

Output must be byte-for-byte deterministic across runs. Concrete rules:

- Iteration order in maps: sort by feature/kind/short_name. Never `HashMap`.
- Import set: sorted; standard library and `lazuli.dev/runtime/lazuli/…` and third-party in three blank-line-separated groups (mirrors `goimports` default).
- `gofmt` is **not** invoked in-process. We emit `gofmt`-equivalent output by aligning struct tag columns in `printer::aligned_rows` (already proven at `runtime.rs:159-160`). Smoke test (§7) shells out to `gofmt -l <out>` as a post-condition.

---

## §3. Per-kind template mapping

Each row: IR shape → file emitted → imports → user-code shape (sample 5–15 LOC) → readiness.

Readiness key:
- **green** — IR exists + Lazuli Go bucket has the contract typed + spike codegen already exists (`dist/go/customer/customer.gen.go` proves this).
- **yellow** — IR exists, Lazuli Go contract types declared, but no spike emission yet OR contract has known gaps (catalogued in §4).
- **red** — IR has known partial coverage; emitter blocks on language-side work.

### 3.1 Resource (and Record)

- **IR**: `ir::Resource` (`crates/lazuli_ir/src/lib.rs:334`), `ir::Record` (`:381`).
- **File**: `dist/go/<feature>/resource.gen.go` (one file per feature, all resources of the feature)
- **Imports**: `lazuli.dev/runtime/lazuli` (`Resource[T]`, `ID`, `Time`, `TenancyOrg/None`, `RetentionSpec`)
- **Shape** (already proven in `dist/go/customer/customer.gen.go:22-41`):

```go
type Customer struct {
    ID        lazuli.ID    `db:"id"         json:"id"`
    OrgID     lazuli.ID    `db:"org_id"     json:"org_id"`
    Name      string       `db:"name"       json:"name"`
    Email     string       `db:"email"      json:"email"`
    CreatedAt lazuli.Time  `db:"created_at" json:"created_at"`
    DeletedAt *lazuli.Time `db:"deleted_at" json:"deleted_at,omitempty"`
}
var customerResource = lazuli.Resource[Customer]{ Name: "customer", Feature: "customer", Tenancy: lazuli.TenancyOrg, SoftDelete: true, Retention: &lazuli.RetentionSpec{ Window: lazuli.Duration("7y"), Then: lazuli.Anonymize } }
```

- **Readiness**: **green** (Tenancy, SoftDelete, Retention all in IR + lib).
- **Sub-shapes**:
  - `Record` → emit struct only, no `Resource[T]` var (it is a value type, no row identity).
  - `@cap.Hashed/Encrypted/Token` field → emit `lazuli.HashedRef` / `lazuli.EncryptedRef` / `lazuli.TokenRef` typed alias.
  - `@cap.File` field → emit `storage.FileRef` (see §3.10).
  - `@semantic.GeoPoint` (Hostpoint decision §5.1) → emit `postgis.Point` + `db:"… type:geography(point,4326)"` tag (see §9.1).

### 3.2 Command

- **IR**: `ir::Command` (`:639`), variants `Create/Update/Delete/Returns`.
- **File**: `dist/go/<feature>/command.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli` (`Command[I,O]`, `Bindings`, `FromInput`, `FromCtx`, `Creates/Updates/Deletes`, `Policy`, `AuditDefault`, `ValidatorRef`, `EventEmit`)
- **Shape** (already proven in `dist/go/customer/customer.gen.go:48-68`):

```go
type CreateCustomerInput struct { Name string `json:"name"`; Email string `json:"email"` }
var createCustomer = lazuli.Command[CreateCustomerInput, Customer]{
    Name: "customer.create", Resource: &customerResource,
    Policy: lazuli.Policy{Name: "@policy.create", Atoms: []lazuli.PolicyAtom{{Namespace: "role", Name: "admin"}}},
    RateLimit: "30 per hour per ip", Audit: lazuli.AuditDefault,
    Validators: []lazuli.ValidatorRef{lazuli.V("email_check")},
    Effect: lazuli.Creates(&customerResource, lazuli.Bindings{"name": lazuli.FromInput("name"), "email": lazuli.FromInput("email")}),
    Emits: []lazuli.EventEmit{{Name: "customer_created", From: lazuli.FromCreates}},
    Invalidates: []string{"customer.query.list"},
}
```

- **Readiness**: **green** for Create/Update/Delete + Emits + Invalidates. **yellow** for Tier 4 slots:
  - `Command.approval` → not in `lazuli.Command[I,O]` lib yet (gap §4.1).
  - `Command.external_calls` → not in lib (gap §4.1).
  - `Command.timeout/retry/idempotency` (Tier 4 mirrors of Job) → not in lib (gap §4.1).
  - `Command.deprecated` → not in lib (gap §4.1).

### 3.3 Query.List / Query.Lookup / Query.Sql

- **IR**: `ir::Query` enum (`:899`), variants `List(ListQuery)`, `Lookup(LookupQuery)`, `Sql(SqlQuery)`.
- **File**: `dist/go/<feature>/query.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli` (`Query[A,R]`, `QueryList/Lookup/SQL`, `FilterRule`, `OrderClause`, `LookupKey`, `SearchSpec`, `CacheSpec`)
- **Shape** (proven in `runtime.rs::write_query`):

```go
type ListCustomersArgs struct { LifecycleStage *string `json:"lifecycle_stage,omitempty"`; Search *string `json:"search,omitempty"` }
var listCustomers = lazuli.Query[ListCustomersArgs, Customer]{
    Name: "customer.query.list", Resource: &customerResource, Kind: lazuli.QueryList,
    Policy: lazuli.Policy{Name: "@policy.read"}, Filters: []lazuli.FilterRule{{Column: "lifecycle_stage", When: lazuli.FromInput("LifecycleStage")}},
    Order: []lazuli.OrderClause{{Column: "created_at", Desc: true}},
    Search: &lazuli.SearchSpec{When: lazuli.FromInput("Search"), Over: []string{"name", "email"}, Mode: lazuli.SearchContains},
    Paginate: 50, Cache: &lazuli.CacheSpec{TTL: 5 * time.Minute, Namespace: "customers"},
}
```

- **Sub-shapes**:
  - **Lookup**: emit `LookupBy []lazuli.LookupKey{…}` instead of Filters/Order. Body is a `RunLookup(ctx, args)`.
  - **Sql**: emit `SQL: "./queries/<name>.sql"` field; Lazuli Go lib reads + parameterises at boot. The `.sql` file is **copied** (not regenerated) into `dist/go/<feature>/queries/`. Hash-pinned so doctor catches drift.
- **Readiness**: List + Lookup **green**. Sql **yellow** — the runtime lib has the `SQL string` field (`query.go:64`) but no spike file uses it; first Hostpoint port (`reviews.list_by_target` SQL aggregate) exercises it.

### 3.4 Api

- **IR**: `ir::Api` (`:3145`) — read-style HTTP endpoint with method + path + typed output, distinct from Command.
- **File**: `dist/go/<feature>/api.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli` (`Api`, `HttpMethod`, `Handler[I,O]`), feature-local handler reference at `./handlers/<api_name>.go`
- **Shape**:

```go
var customerSummary = lazuli.Api[CustomerSummaryArgs, CustomerSummary]{
    Name: "customer_summary", Feature: "customer", Method: lazuli.MethodGet, Path: "/api/customer/{id}/summary",
    Policy: lazuli.Policy{Name: "@policy.read"}, RateLimit: "60 per minute per user",
    Handler: handlers.CustomerSummary,  // user-authored extension fn
}
```

- **Readiness**: **yellow** — `ir::Api` is typed and tested; Lazuli Go lib has no `Api[I,O]` value yet. Gap §4.2.

### 3.5 Auth

- **IR**: `ir::Auth` + `AuthIdentity/AuthPassword/AuthSessions/AuthMfa/AuthOAuthProvider` (`:2972-3044`).
- **File**: `dist/go/<feature>/auth.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli/auth` (`PasswordContract`, `SessionContract`, `MFAContract`, `OAuthContract`, `AlgoArgon2id`, `FieldRef`)
- **Shape**:

```go
var customerAuthPassword = auth.PasswordContract{
    Identity:  auth.FieldRef{Resource: "Customer", Field: "email"},
    Algorithm: auth.AlgoArgon2id, HashFn: "@fn.hash_customer_password", VerifyFn: "@fn.verify_customer_password",
    RateLimit: "5 per 10 minutes",
}
var customerAuthSessions = auth.SessionContract{ Resource: "CustomerSession", TTL: "7 days", Refresh: false }
var customerAuthOAuthGoogle = auth.OAuthContract{ Provider: "google", Adapter: "@adapter.google_oauth" }
```

- **Readiness**: **green** for declaration (`runtime/go/lazuli/auth/password.go:42-60`); **yellow** for runtime wire (argon2 import is the runtime team's job, not codegen's).

### 3.6 Job

- **IR**: `ir::Job` (`:2491`) + `JobTrigger/RetryPolicy/IdempotencyKey/ExternalCallRef`.
- **File**: `dist/go/<feature>/job.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli/jobs` (`JobContract`, `JobTrigger`, `RetryPolicy`, `BackoffExponential`, `IdempotencyKeySpec`)
- **Shape**:

```go
var customerOutreachWelcomeJob = jobs.JobContract{
    Feature: "customer_outreach", Name: "send_welcome_email",
    Trigger: jobs.JobTrigger{Kind: "event", Event: "customer.customer_created"},
    Retry: jobs.RetryPolicy{Count: 3, Backoff: jobs.BackoffExponential},
    Idempotency: &jobs.IdempotencyKeySpec{Path: "payload.customer_id"},
    Timeout: "30s", HandlerPath: "./jobs/send_welcome_email.go",
    ExternalCalls: []jobs.ExternalCallRef{{Slot: "email", Operation: "send"}},
}
```

- **Readiness**: **green** (`jobs/contract.go:1-100` has all the contract shapes).

### 3.7 Webhook

- **IR**: `ir::Webhook` (`:2594`) + `VerifySpec/ReplaySpec/DlqSpec/WebhookEventRef`.
- **File**: `dist/go/<feature>/webhook.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli/webhooks` (`WebhookContract`, `VerifySpec`, `VerifyHmac`, `TenantFromSpec`)
- **Shape**:

```go
var mercadopagoCallbackWebhook = webhooks.WebhookContract{
    Feature: "payments", Name: "mercadopago_callback",
    Route: "/webhooks/mercadopago", Verify: webhooks.VerifySpec{Scheme: webhooks.VerifyHmac, Algorithm: "sha256", SecretEnv: "MERCADOPAGO_HMAC_SECRET", Header: "X-Signature"},
    TenantFrom: &webhooks.TenantFromSpec{Path: "payload.external_reference"},
    IdempotencyBy: "payload.id", Policy: "@policy.public", HandlerPath: "./webhooks/mercadopago_callback.go",
}
```

- **Readiness**: **yellow** — IR has full Tier 4 (`replay/dlq/retry/payload`) but `webhooks/contract.go:69-80` only commits to the v0 spine. The expanded contract is enumerated as TODO comments at `webhooks/contract.go:17-30`. Gap §4.3.

### 3.8 Notification

- **IR**: `ir::Notification` (`:2772`) + `NotificationDigest/NotificationThrottle`.
- **File**: `dist/go/<feature>/notification.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli/notifications` (`NotificationContract`, `Channel`, `ChannelEmail/InApp/etc.`, `Digest`, `Throttle`)
- **Shape**:

```go
var bookingConfirmedNotification = notifications.NotificationContract{
    Feature: "payments", Name: "booking_confirmed",
    Trigger: notifications.Trigger{Kind: "event", Event: "payments.transaction_completed"},
    Channels:  []notifications.Channel{notifications.ChannelEmail, notifications.ChannelPush},
    Recipient: "target.user.email", Template: "./templates/booking_confirmed.<locale>.tmpl",
    Throttle:  &notifications.Throttle{MaxPer: "1 hour", PerRecipient: true, Burst: 3},
}
```

- **Readiness**: **yellow** — `notifications/contract.go:1-40` has channel enum + spec shapes; `Digest/Throttle` fields are catalogued at `notifications/contract.go` (TBD struct location); confirm before codegen wires. Gap §4.4.
- **Push channel** (Hostpoint §9.5): emitted same shape, adapter resolved through `@adapter.notification.push` → Expo Push plugin (see §6).

### 3.9 Storage (FileCapability sites)

- **IR**: `ir::CapabilityRef::File(FileCapability)` (`:489, :547`). Sites are field-level (Resource) or output-level (Api).
- **File**: `dist/go/<feature>/storage.gen.go` — collects every `@cap.File(...)` declaration in the feature into one file.
- **Imports**: `lazuli.dev/runtime/lazuli/storage` (`FileContract`, `MimeType`, `FileVisibility`, `VisibilityPublic/Private/Signed`)
- **Shape**:

```go
var customerProfilePhotoFile = storage.FileContract{
    Resource: "Customer", Field: "profile_photo",
    MaxSize: 5 * 1024 * 1024, Accept: []storage.MimeType{{Family: "image", Subtype: "*"}},
    Visibility: storage.VisibilityPublic,
}
```

- **Readiness**: **green** (`storage/contract.go:18-80`).

### 3.10 Translation

- **IR**: `ir::Translation` (`:1741`) + `TranslationKey/Variant/PluralArm`.
- **File**: `dist/go/<feature>/translation.gen.go` + sibling JSON catalogs at `dist/go/<feature>/i18n/<locale>.json`
- **Imports**: `lazuli.dev/runtime/lazuli/i18n` (`Catalog`, `LocaleContract`)
- **Shape**:

```go
//go:embed i18n/*.json
var translationFS embed.FS
var customerOutreachTranslations = i18n.Catalog{ Name: "customer_outreach.messages", FS: translationFS, BasePath: "i18n" }
```

- The actual translation strings ship as JSON inside `dist/go/<feature>/i18n/<locale>.json` (reuses existing `lazuli translate extract` output shape; codegen does **not** duplicate translation values into Go literals).
- **Embed contract**: the `//go:embed i18n/*.json` directive is part of what codegen emits, but codegen never reads or rewrites the JSON catalogs themselves — they are sacred (extension files, see §5.3). This split is locked in §11 (codegen owns the directive; catalogs are user-territory).
- **Readiness**: **yellow** — `i18n/contract.go:13-30` has `LocaleContract`; `Catalog` value type still TBD. Gap §4.5.

### 3.11 TenantMigration

- **IR**: `ir::TenantMigration` (`:2940`) + `TenantMigrationTarget`.
- **File**: `dist/go/<feature>/migration.gen.go`
- **Imports**: `lazuli.dev/runtime/lazuli/migrations` (`MigrationContract`, `BackoffStrategy`)
- **Shape**:

```go
var customerBackfillMigration = migrations.MigrationContract{
    Feature: "customer", Name: "backfill_lifecycle_stage",
    Target: migrations.Target{Axis: "org_id"},
    Idempotency: "envelope.tenant_id", Timeout: "10m",
    Retry: migrations.RetryPolicy{Count: 3, Backoff: migrations.BackoffExponential},
    HandlerPath: "./migrations/backfill_lifecycle_stage.go",
}
```

- **Readiness**: **green** (`migrations/contract.go:1-30`).

### 3.12 EventGroup

- **IR**: `ir::EventGroup` (`:2894`).
- **File**: `dist/go/<feature>/events.gen.go` — single file aggregating all groups + concrete events of the feature.
- **Imports**: `lazuli.dev/runtime/lazuli` (`EventGroup`, `EventDescriptor`)
- **Shape**:

```go
var customerLifecycleEvents = lazuli.EventGroup{
    Pattern: "customer_*", Resource: "Customer",
    Events: []lazuli.EventDescriptor{
        {Name: "customer_created", PayloadType: "CustomerCreatedPayload"},
        {Name: "customer_archived", PayloadType: "CustomerArchivedPayload"},
    },
}
type CustomerCreatedPayload struct { CustomerID lazuli.ID `json:"customer_id"`; Email string `json:"email"` }
```

- **Readiness**: **yellow** — IR exists (Phase L Tier 3); Lazuli Go lib `EventGroup` value type TBD. Gap §4.6.

### 3.13 Root-level files

| File | Purpose | Cardinality |
|---|---|---|
| `dist/go/go.mod` | Module declaration; pin `lazuli.dev/runtime/lazuli vX.Y.Z` and third-party deps | One per app |
| `dist/go/go.sum` | Generated by `go mod tidy` post-codegen | One per app |
| `dist/go/main.go` | `func main()` boots the Lazuli runtime (`lazuli.Boot(ctx)`); imports each feature package for its `init()` registrations | One per app |
| `dist/go/lazuli_app.gen.go` | App-level contract: `app.locale`, `app.logging`, `app.tracing`, `app.cors`, `app.routes` lowered to `lazuli.AppContract{…}` | One per app |
| `dist/go/migrations/<NNN>_<name>.sql` | DDL migrations derived from `ir::Resource` field set; PostGIS extension toggled by Hostpoint §9.1 | N |

### 3.13.1 `app.routes` lowering shape

The top-level `.lzx route` declarations (typed `RouteSlot.from: Option<String>`
since Phase L Tier 4 second wave) lower to a `lazuli.AppRoutes` slice on
`lazuli.AppContract`:

```go
var lazuliApp = lazuli.AppContract{
    Name:    "hostpoint",
    Locale:  lazuli.AppLocale{Default: "pt-BR", Supported: []string{"pt-BR", "en-US"}},
    CORS:    lazuli.AppCors{Allow: []string{"https://hostpoint.com.br"}, Credentials: true},
    Logging: lazuli.AppLogging{Level: lazuli.LogLevelInfo, Format: lazuli.LogFormatJSON},
    Tracing: lazuli.AppTracing{SampleRate: 0.05, Exporter: "@adapter.otlp"},
    Routes: []lazuli.AppRoute{
        {Name: "customer_detail", Path: "/customers/{id}", PathType: "lazuli.ID",
         From: lazuli.StringPtr("customer.query.lookup")},
        {Name: "customer_list", Path: "/customers", From: nil},
    },
}
```

`AppRoute.From` resolves the typed binding-from-context (Tier 4 W2A). `PathType`
emits the slot's `TypeRef` rendered via `types.rs` (e.g. `lazuli.ID`,
`lazuli.UUID`). The emitter walks `module.app.routes`; falls through gracefully
when no routes are declared (no `Routes` field is emitted, no slice allocated).
Adding a new route at the language level requires zero codegen change.

### 3.14 Coverage matrix summary

| Kind | IR | Lazuli Go lib | Spike emission | Readiness |
|---|---|---|---|---|
| Resource | done | done | done (`runtime.rs`) | green |
| Record | done | done | partial | green |
| Command (Create/Update/Delete) | done | done | done | green |
| Command (Returns, approval, external_calls, deprecated) | done | partial | none | yellow (§4.1) |
| Query.List / Lookup | done | done | done (`runtime.rs`) | green |
| Query.Sql | done | done (`query.go:64`) | none | yellow |
| Api | done | none | none | yellow (§4.2) |
| Auth (identity/password/sessions/oauth/mfa) | done | done | none | yellow |
| Job | done | done | none | yellow |
| Webhook (v0 spine) | done | done | none | yellow |
| Webhook (replay/dlq/retry/payload) | done | none | none | yellow (§4.3) |
| Notification (v0 spine) | done | done | none | yellow |
| Notification (digest/throttle) | done | partial | none | yellow (§4.4) |
| Storage / FileCapability | done | done | none | green |
| Translation | done | partial | none | yellow (§4.5) |
| TenantMigration | done | done | none | green |
| EventGroup | done | partial | none | yellow (§4.6) |

---

## §4. Lazuli Go lib import shape — gaps codegen will discover

### 4.1 `lazuli.Command[I, O]` Tier 4 slots

Already in IR (`ir::Command` `:639-690`), missing in `runtime/go/lazuli/command.go:9-53`:

| IR slot | Add to Command[I,O] | Acceptance |
|---|---|---|
| `Command.approval: ApprovalSpec` | `Approval *ApprovalSpec` | typed `then/by/reason` fields |
| `Command.external_calls: Vec<ExternalCallRef>` | `ExternalCalls []ExternalCallRef` | same shape as `jobs.ExternalCallRef` — share the type |
| `Command.timeout/retry/idempotency` | `Timeout string; Retry *RetryPolicy; Idempotency *IdempotencyKey` | mirror Job spine; share types with `jobs` package |
| `Command.deprecated: Option<Deprecation>` | `Deprecation *Deprecation` | `Since/Replacement/Sunset` |

### 4.2 `lazuli.Api[I, O]`

The IR ships `ir::Api` (`:3145`) but `runtime/go/lazuli/` has no Api value type. Required addition:

```go
type Api[I, O any] struct {
    Name, Feature, Path string
    Method  HttpMethod
    Policy  Policy
    RateLimit RateLimit
    Handler func(ctx *Ctx, input I) (O, error)
}
```

### 4.3 `webhooks.WebhookContract` expanded fields

Per the TODO at `runtime/go/lazuli/webhooks/contract.go:17-30`, the runtime team is on the hook for `Retry`, `Replay`, `DLQ`, `PayloadType`. Codegen blocks on this for full Webhook Tier 4 coverage; spike emission for the v0 spine can proceed in parallel.

### 4.4 `notifications.NotificationContract` digest/throttle

`ir::NotificationDigest` / `ir::NotificationThrottle` exist; verify the matching Go structs in `runtime/go/lazuli/notifications/`. If absent, runtime team adds:

```go
type Digest struct{ Every string; GroupBy string; MaxSize uint32; TemplateStrategy DigestStrategy }
type Throttle struct{ MaxPer string; PerRecipient, PerChannel bool; Burst uint32 }
```

### 4.5 `i18n.Catalog`

Required: a value type that pairs `LocaleContract` with an `embed.FS` carrying per-locale JSON. The contract currently stops at `LocaleContract`.

### 4.6 `lazuli.EventGroup` and `lazuli.EventDescriptor`

Phase L Tier 3 IR exists; the runtime currently models events via `EventEmit` on Commands but lacks a value type for the declarative `event_group` block. Add both, plus payload-type registration so the dispatcher can decode events into typed structs.

### 4.7 Plugin and adapter resolver in Go

For `@adapter.<name>` and `@plugin/<name>` references emitted as strings today (`auth.PasswordContract.HashFn = "@fn.…"`), the runtime needs a registry resolver: a `RegisterAdapter("@adapter.google_oauth", impl)` API called from `main.go`. Codegen emits the string references; the runtime resolves at boot. This is **already partial** (see `registry.go` for resources/commands/queries) — extend the same pattern.

---

## §5. Go module layout

### 5.1 Per-feature directories

```
dist/go/
  go.mod
  go.sum
  main.go                              # boots lazuli.Boot(ctx); imports each feature pkg for side-effects
  lazuli_app.gen.go                    # app.locale, app.logging, app.tracing, app.cors → lazuli.AppContract
  migrations/
    001_customer.sql                   # DDL derived from resources
    002_postgis.sql                    # CREATE EXTENSION IF NOT EXISTS postgis (Hostpoint §9.1)
  customer/                            # one Go package per feature
    customer.gen.go                    # OR split: resource.gen.go, command.gen.go, query.gen.go, …
    extensions.go                      # user-authored fns (hash_customer_password, etc.) — NOT generated
    queries/
      rating_aggregate.sql             # copied from feature dir, hash-pinned
  customer_outreach/
    notification.gen.go
    templates/
      booking_confirmed.pt-BR.tmpl     # copied; not regenerated
      booking_confirmed.en-US.tmpl
  payments/
    webhook.gen.go
    command.gen.go
    resource.gen.go
```

**Decision: per-kind file vs one-file-per-feature.** Both are tenable. Recommendation: **per-kind files** (one `resource.gen.go`, one `command.gen.go`, etc.) because (a) it scales to features with 5+ commands without forcing scroll, (b) diff churn localises by kind, (c) matches Hostpoint checklist row §1.1's enumeration of `<feature>/<kind>.gen.go`. The spike's monolithic `customer.gen.go` (`dist/go/customer/customer.gen.go`) stays as a transitional fixture and is split on the first emitter cell.

### 5.2 `go.mod` shape

Emitted **once at root**, regenerated each `lazuli generate go` run. Idempotent.

```go-mod
module github.com/acme/hostpoint

go 1.24

require (
    lazuli.dev/runtime/lazuli v0.1.0
)
```

User customisation:
- `--module github.com/acme/hostpoint` overrides the derived module name.
- `lazuli generate go` does **not** run `go mod tidy` — that is a build-step concern, called out in the dist `README.md` as a one-liner.
- Hostpoint plugin dependencies (e.g. `@plugin/mercadopago` → `github.com/lazurite/lazuli-plugin-mercadopago`) appear automatically when used. See §6 for resolution.

### 5.3 Incrementality

`generate go` is a **full rewrite** of `dist/go/**.gen.go` files plus root `go.mod`/`main.go`/`lazuli_app.gen.go`. It does **not** touch:

- `dist/go/<feature>/extensions.go` (user-authored)
- `dist/go/<feature>/queries/*.sql` (copied, hash-pinned for drift detection)
- `dist/go/<feature>/templates/*.tmpl` (copied)
- `dist/go/<feature>/handlers/*.go` (user-authored extension points; resolved by `@fn`/`@hook` namespaces per `docs/extension-points.md`)

`.gen.go` extension is the contract: any file with that suffix is "free game". Files without it are sacred.

---

## §6. Plugin namespace resolution

Rules from `c:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_plugin_namespace_policy.md` (2026-05-11). The `@plugin/<product>/<name>` form is **retired** — never emit it.

### 6.1 Reference → import resolution table

| Lazuli reference | Resolved Go import | Notes |
|---|---|---|
| `@runtime/postgres` | `lazuli.dev/runtime/lazuli/db` | Commodity platform adapter; ships in core repo |
| `@runtime/s3` | `lazuli.dev/runtime/lazuli/storage/s3` | Commodity; lives in core |
| `@runtime/google_oauth` | `lazuli.dev/runtime/lazuli/auth/oauth/google` | Commodity adapter for OAuth providers |
| `@plugin/mercadopago` | `github.com/lazurite/lazuli-plugin-mercadopago` (private repo) | Proprietary; opinionated provider; private repo per policy |
| `@plugin/google-maps` | `github.com/lazurite/lazuli-plugin-google-maps` (private repo) | Hostpoint §9.3 — proprietary SLA provider |
| `@plugin/expo-push` | `github.com/lazurite/lazuli-plugin-expo-push` (private repo) | Hostpoint §9.5 |
| `@adapter.<name>` | string literal at codegen time | Resolved at runtime via `RegisterAdapter(name, impl)` from user-authored `main.go` extensions |
| `@fn.<name>` | string literal `"@fn.<name>"` | Resolved at runtime via extension-points registry |
| `@semantic.<Type>` | maps to Go type (see §6.3) | Closed catalog; some semantics need plugin imports (GeoPoint → postgis) |

### 6.2 Resolver module

`crates/lazuli_codegen_go/src/emitter/refs.rs` owns the closed-catalog mapping:

```rust
pub enum ResolvedRef {
    CoreRuntime { go_path: &'static str },           // lazuli.dev/runtime/lazuli/...
    PluginPrivate { go_module: String },             // github.com/lazurite/lazuli-plugin-<name>
    AdapterString { literal: String },                // @adapter.* — emitted as Go string
    FnString { literal: String },                     // @fn.* — emitted as Go string
    Unresolved { name: String },                      // surfaces a codegen error (or --check warning)
}
```

The mapping is **data, not code** — a static table keyed by reference prefix. Adding a new core runtime adapter (e.g. `@runtime/valkey`) extends the table; private plugins are user-configurable through `registry.lzi` once that surface lands. Until then, every `@plugin/<name>` resolves through a hard-coded `LAZURITE_PRIVATE_HOST = "github.com/lazurite/"` constant and codegen errors if the plugin isn't declared in `app.lzi` `registry`.

### 6.2.1 Error code catalog

Closed catalog mirroring the `doctor.rs` convention. Every `Unresolved` variant
hits exactly one code; never silent string pass-through.

| Code | Condition | Exit |
|---|---|---|
| `CODEGEN-GO-PLUGIN-001` | `@plugin/<name>` reference present but no matching `integrations.<name>` entry in `app.lzi`'s `registry` | non-zero |
| `CODEGEN-GO-UNRESOLVED-002` | `@runtime/<name>` references a key not in the closed catalog | non-zero |
| `CODEGEN-GO-ADAPTER-003` | `@adapter.<name>` emitted but no matching `RegisterAdapter` site projected (warn-only at v0; promotable to error once extension discovery lands per §10.5) | warn |
| `CODEGEN-GO-SEMANTIC-004` | `@semantic.<Type>` outside the §6.3 closed table (e.g. an authored semantic not yet wired) | non-zero |
| `CODEGEN-GO-CAP-005` | `@cap.<Capability>` outside `Hashed/Encrypted/Token/File` | non-zero |
| `CODEGEN-GO-FN-006` | `@fn.<name>` reference with no extension stub discoverable in `features/<feature>/domain/` (warn-only until §10.5 settles) | warn |

`--check` mode prints the full catalog of unresolved refs and exits 1 when any
`non-zero` code fires. Smoke harness (§7.4) treats `non-zero` codes as test
failures; `warn` codes surface in stdout but do not fail.

### 6.3 @semantic.* → Go type table (excerpt)

| `@semantic.<Type>` | Go type | Import |
|---|---|---|
| `@semantic.Email` | `string` (validated by `lazuli.SemanticEmail`) | `lazuli.dev/runtime/lazuli` |
| `@semantic.Phone` | `string` | same |
| `@semantic.Url` | `string` | same |
| `@semantic.Uuid` | `lazuli.UUID` | same |
| `@semantic.Money` | `lazuli.Money` (typed int cents) | same |
| `@semantic.GeoPoint` | `postgis.Point` | `github.com/twpayne/go-geom` or `github.com/cridenour/go-postgis` (decide in §9.1) |
| `@semantic.Currency` | `lazuli.Currency` (ISO 4217 string) | same |

### 6.4 Privacy / repo policy

Codegen NEVER emits a plugin import nested under the core module path (no `lazuli.dev/runtime/lazuli/plugins/<name>`, no `github.com/lazuli/lazuli/plugins/<name>`). The core repo (`github.com/lazuli/lazuli`) and core module (`lazuli.dev/runtime`) hold **only** the language + `@runtime/` commodity adapters. Every `@plugin/` reference resolves to a **separate private repo** owned by Lazurite or the plugin author (canonical example: `github.com/lazurite/lazuli-plugin-<name>`).

`--check` mode: if any `@plugin/<name>` lacks a corresponding entry in `app.lzi`'s `registry.integrations.<name>`, codegen exits non-zero with `CODEGEN-GO-PLUGIN-001: plugin reference @plugin/<name> not declared in app.lzi registry`.

---

## §7. Smoke test plan

### 7.1 Acceptance goal

```
lazuli generate go examples/full-capsule/full-capsule.lzi --out dist/go/
cd dist/go && go mod tidy && go build ./...
```

succeeds with zero warnings on the **green slice** of kinds (§3.14).

### 7.2 Per-bucket readiness for `examples/full-capsule/`

The canonical fixture has 5 features (`customer`, `customer_auth`, `customer_tags`, `customer_import`, `customer_outreach` — see `examples/full-capsule/full-capsule.lzi`).

| Feature | Kinds present | Smoke status |
|---|---|---|
| `customer` | Resource, Record, Command (Create/Update/Delete), Query.List, Query.Lookup, EventGroup, Translation, Workflow | **green** for first 4 kinds (covered by existing spike `dist/go/customer/customer.gen.go`); **yellow** for Translation + EventGroup |
| `customer_auth` | Auth (full), Command (login/logout) | **yellow** — Auth gen file shape new; Lazuli Go contracts exist |
| `customer_tags` | Resource, Command, Query | **green** — same green set as `customer` |
| `customer_import` | Job, Webhook (v0 spine), Notification (v0 spine) | **yellow** — first emission of these kinds |
| `customer_outreach` | Notification (with throttle), Translation | **yellow** — exercises throttle + per-locale templates |

### 7.3 Cell budget to close yellows in `full-capsule/`

| Cell | Scope |
|---|---|
| **G1** | Emitter slice: Auth (covers `customer_auth`) — 1 cell |
| **G2** | Emitter slice: Job + Webhook v0 spine + Notification v0 spine — 1 cell (shared scaffolding for `JobContract`/`WebhookContract`/`NotificationContract` value emission) |
| **G3** | Emitter slice: Translation (catalog file + `i18n.Catalog` value) + EventGroup — 1 cell |
| **G4** | Emitter slice: per-kind file split (move from monolithic `<feature>.gen.go` to per-kind) — 1 cell |
| **G5** | Emitter slice: Api kind — 1 cell |
| **G6** | Emitter slice: Webhook expanded (replay/dlq/retry/payload) once `webhooks/contract.go` gaps close — 1 cell (depends on §4.3 runtime work) |

Plus integration:

| Cell | Scope |
|---|---|
| **I1** | CLI wiring (`lazuli generate go` verb + `GenerateKind::Go` + entry in `generate_command`) — 1 cell |
| **I2** | Module-level emission: root `go.mod`, `main.go`, `lazuli_app.gen.go` — 1 cell |
| **I3** | Smoke harness: `tests/smoke_go_build.rs` shells out to `go build`; gated by `LAZULI_GO_SMOKE=1` so CI without Go skips — 1 cell |
| **I4** | Plugin resolver (§6) + `--check` mode + error catalog (`CODEGEN-GO-PLUGIN-001`, `CODEGEN-GO-UNRESOLVED-002`) — 1 cell |

### 7.4 Smoke acceptance criteria

1. `cargo test -p lazuli_codegen_go` passes (unit tests on emitter, no Go toolchain needed)
2. `LAZULI_GO_SMOKE=1 cargo test -p lazuli_codegen_go --features smoke` invokes `go build` over `dist/go/` produced from `examples/full-capsule/`
3. `gofmt -l dist/go/` returns empty (no reformat needed)
4. `go vet ./...` clean on the green slice
5. Output is byte-identical across two consecutive runs (determinism check)

---

## §8. Cell budget breakdown — total 12-15 cells

Ordered for mergeability. Each row is 1–2 cells.

| Order | Cell | Scope | Depends on |
|---|---|---|---|
| 1 | I1 | CLI verb + `GenerateKind::Go` + plumbing | — |
| 2 | E1 | Emitter scaffold: `printer.rs`, `imports.rs`, `types.rs`, `module.rs` shell; produces empty but valid Go pkg for one feature | I1 |
| 3 | E2 | Resource + Record kind (incl. capability typed mapping `@cap.Hashed/Encrypted/Token/File`) | E1 |
| 4 | E3 | Command (Create/Update/Delete) — proven shape from `runtime.rs` ported to new emitter | E2 |
| 5 | E4 | Query.List + Query.Lookup + Query.Sql (with `.sql` file copy + hash pin) | E3 |
| 6 | I2 | Module-level: `go.mod`, `main.go`, `lazuli_app.gen.go` (covers `app.locale`, `app.cors`, `app.logging`) | E2 |
| 7 | G1 | Auth emission (5 sub-blocks: identity, password, sessions, mfa, oauth) | E2 |
| 8 | G2 | Job + Webhook v0 spine + Notification v0 spine (shared bucket emission pattern) | E2 |
| 9 | G3 | Translation (catalog FS embed) + EventGroup | E2, §4.5/§4.6 runtime gaps |
| 10 | G4 | Storage / FileCapability emission (resource fields + api outputs) | E2 |
| 11 | I4 | Plugin namespace resolver + `--check` mode + error catalog | E1 |
| 12 | I3 | Smoke harness (`LAZULI_GO_SMOKE=1`) — `go build` over `examples/full-capsule/` | E2–G4 |
| 13 | G5 | Api kind | §4.2 runtime addition |
| 14 | G6 | Command Tier 4 slots (approval / external_calls / timeout / retry / idempotency / deprecated) | §4.1 runtime addition |
| 15 | G7 | Webhook Tier 4 slots (replay / dlq / retry / payload typed decode) | §4.3 runtime addition |

Cells 1–6 are independently mergeable in order; produce a green-slice MVP at cell 6 that already builds (a single feature `dist/go/customer/customer.gen.go` byte-equivalent to the spike). Cells 7–12 fan out and any subset may merge. Cells 13–15 block on runtime team's bucket completion (§4) and are last.

---

## §9. Hostpoint decisions (§5 of checklist) — codegen materialisation

The 5 decisions resolved 2026-05-11. Each row shows how the emitter wires it.

### 9.1 GeoPoint as single `@semantic.GeoPoint`

- **IR side** (language team, not codegen): add `BuiltinType::SemanticGeoPoint` mirroring `SemanticEmail` shape (`lib.rs:445-487`).
- **Codegen side**: `types.rs` maps the variant to `postgis.Point` Go type with import `github.com/cridenour/go-postgis` (or chosen lib — TBD in §10).
- **Field tag**: `db:"coordinates,type:geography(point,4326)"` so pgx scan + ddl-tool both pick up the PostGIS type.
- **DDL emission**: `dist/go/migrations/002_postgis.sql` carries `CREATE EXTENSION IF NOT EXISTS postgis;` whenever any resource references the semantic.

### 9.2 PostGIS as the geo backend (no Algolia GeoSearch)

- **Codegen side**: Query.List with `filter by_radius(params.lat, params.lng, params.radius_km)` and a column typed `@semantic.GeoPoint` emits a `lazuli.FilterRule` whose `When` source is a `FromInput` triple. The runtime `lazuli/db` adapter recognises the geo-filter shape and emits the corresponding `ST_DWithin(coordinates, ST_MakePoint($1,$2)::geography, $3 * 1000)`. Codegen does **not** emit raw SQL — it emits the filter rule and lets the lib's geo adapter build the WHERE.
- **Constraint emission**: `GIST (coordinates)` index emitted in migration when codegen sees the GeoPoint column.

### 9.3 Google Maps for production geocoding

- Reference shape: `requires integration maps: MapsProvider` in feature → `app.lzi` binds `maps = integrations.google_maps` → `registry.lzi` declares `integrations.google_maps adapter @plugin/google-maps`.
- **Codegen side**: `@plugin/google-maps` resolves to `github.com/lazurite/lazuli-plugin-google-maps` (private repo per §6). Emitted as `import` in `main.go` + a `RegisterAdapter("@plugin/google-maps", googleMaps.New(env.GOOGLE_MAPS_API_KEY))` line.
- **Dev / MVP** uses `@plugin/nominatim` instead, same shape, different repo. Switch is a one-line app.lzi binding change — no code change.

### 9.4 MercadoPago as `@plugin/mercadopago`

- Webhook + payment-gateway integration. Per `c:/Users/lucas/.claude/projects/c--Users-lucas-lazuli/memory/project_plugin_namespace_policy.md`, MercadoPago is `@plugin/mercadopago` (NOT `@plugin/hostpoint/mercadopago` — that nested form is retired).
- **Codegen side**: Webhook emission for `mercadopago_callback` produces the `webhooks.WebhookContract` value (shape in §3.7); the `verify @validator.mercadopago_hmac` reference resolves to a string the runtime adapter dereferences. The HMAC verifier itself ships in `github.com/lazurite/lazuli-plugin-mercadopago` and registers via `RegisterValidator("@validator.mercadopago_hmac", impl)` in plugin init.
- Codegen NEVER emits HMAC verification code inline; it emits the reference string + the contract.

### 9.5 Expo Push Notifications (replaces FCM)

- **Notification block** with `channel push` resolves the push adapter through `@adapter.notification.push` → `@plugin/expo-push` binding in `app.lzi`.
- **Codegen side**: identical `notifications.NotificationContract` shape (§3.8). The `Channels: []notifications.Channel{notifications.ChannelPush}` enum value is the same regardless of provider. Provider swap is an `app.lzi` binding change.
- **Plugin import**: `github.com/lazurite/lazuli-plugin-expo-push` in `main.go` boot block. Same resolution rule as §9.3.

---

## §10. Open questions

Honest list of items this proposal did not close. Each must be resolved before its dependent cell ships.

### 10.1 PostGIS Go binding library

Three candidates: `github.com/cridenour/go-postgis` (lightweight, ~200 LOC, pgx-native scan), `github.com/twpayne/go-geom` (broader feature set, OGC/WKT/WKB roundtrip, larger dep graph), `github.com/paulmach/orb` (popular but pgx integration weaker). Pick one **before** cell G4-related geo work; locks the import constant in `types.rs`. Recommendation: **`cridenour/go-postgis`** for the MVP given the wire-thin philosophy (§0); revisit if Hostpoint surfaces a need orb covers better.

### 10.2 Per-kind file split vs single-file-per-feature — when?

§5.1 chose per-kind. Spike emission today is monolithic. **Open**: do we ship E2-E4 as monolithic + split in G4, or split from day one? Argument for monolithic-first: byte-equivalent with `runtime.rs` output → trivial to verify against the existing fixture. Argument for split-first: every subsequent cell touches one file instead of one giant file. **Lean**: split-first; the file boundary is cheap. Decide at I2 review.

### 10.3 How does codegen learn about `@plugin/<name>` declared in `registry.lzi`?

`ir::AppRegistry` (`:1785`) is the natural home, but the resolver in `crates/lazuli_codegen_go/src/emitter/refs.rs` needs to walk it. **Open**: does codegen take the IR `Module` only, or `Module` + the resolved `registry.lzi` IR side-by-side? The former is cleaner; the latter is what the openapi emitter does today (it walks `module.app` directly). Recommendation: walk `module.app.registry` (already part of `AppManifest`) and surface unresolved refs via `CODEGEN-GO-PLUGIN-001`.

### 10.4 Should `lazuli generate go` invoke `gofmt`?

The openapi emitter doesn't shell out. Our column-alignment in `printer.rs` matches `gofmt` for struct tags but not for other constructs (long arg lists, multi-line function calls). **Open**: trust our alignment 100% (and CI gate via `gofmt -l`), or shell out as a post-step (loses determinism control + adds Go toolchain dependency to the Rust crate). Recommendation: **trust our alignment**, gate via CI smoke test; the spike already proves this works for the green slice.

**Revisit trigger (locked)**: if `gofmt -l dist/go/` returns a non-empty list during the smoke harness (§7.4 criterion #3) for any kind covered by the green slice (cells E1–E4, G1–G4), the emitter has drifted from `gofmt` semantics. The fix is to extend `printer.rs` alignment rules; shell-out remains the last resort. The smoke harness's `gofmt -l` step is the deterministic trigger, not a periodic review.

### 10.5 How are user-authored Go extension files (`@fn`/`@hook`) discovered?

Per `docs/extension-points.md`, `@fn.hash_customer_password` resolves to `features/customer/domain/hash_customer_password.go`. **Open**: does codegen *generate* the file stub the first time (Rails-generator-style) or just emit the reference and assume the user wrote it? **Lean**: emit stubs with `//go:generate` comments and a `// IMPLEMENT ME` body on first run only (idempotency: never overwrite). But that conflicts with the "extensions.go is sacred" rule in §5.3. Defer to a follow-up proposal (`codegen-lazuli-go-extension-stubs.md`); first cell-set ships **without** stub generation.

### 10.6 `--check` mode reach

Does `--check` validate plugin registry coverage only, or also (a) IR Tier 4 slot completeness, (b) target Lazuli Go lib version compatibility, (c) `.sql` file existence for `Query.Sql`, (d) extension file existence for `@fn`/`@hook`? Decide before I4 implementation. **Lean**: minimal scope (plugin + unresolved refs) at I4; defer the rest to an explicit `lazuli doctor codegen` verb so the language-team-owned `doctor.rs` carries integrity checks, not the codegen crate.

### 10.7 Multi-app workspaces

`docs/grammar.workspace.md` describes multi-app workspaces. **Open**: does `lazuli generate go` accept a workspace input and emit one `dist/go/<app>/` per app? Or is it strictly single-app for now? **Lean**: single-app at v0, mirrors `lazuli generate openapi`. Workspace-level codegen is a follow-up cycle.

### 10.8 Migration ordering and naming

Migration files at `dist/go/migrations/<NNN>_<name>.sql`. **Open**: how is `<NNN>` derived? Resource declaration order in the IR is unstable across feature reorderings. Three options: (1) content-hash prefix (immutable but ugly), (2) sequential by feature emission order (stable if features sort lexically), (3) timestamp-based (Rails style — but then `lazuli generate go` is non-idempotent). **Lean**: option 2, with the doctor surfacing reorder warnings. Defer specifics to a `bucket-migrations-cycle.md` follow-up.

### 10.9 How does the agent dispatch land?

Per `crates/lazuli_codegen_go/src/lib.rs:1-21`, Cut A's agent block is acknowledged but **not yet generated**. The runtime team owns this. **Open for this proposal**: should the codegen crate ship a stub `agent.gen.go` emitter that hand-shakes with the runtime team's dispatch implementation, or wait until Cut A's runtime side stabilises? Recommendation: **wait**; the runtime hand-off (`docs/runtime-handoff.md`) already routes this work elsewhere.

### 10.10 Codegen + LSP feedback loop

When the user edits `.lzi`, does the LSP suggest "regenerate Go"? Or is regeneration explicit (`lazuli generate go` on save)? **Open**. Recommendation: explicit at v0; revisit once the editor extension matures (separate cycle).

---

## §11. Boundary discipline reaffirmed

The emitter MUST NOT:

- Implement algorithms (argon2, HMAC, JWT signing, etc.) — those are runtime-side, wire-only (§0).
- Touch `runtime/go/lazuli/**` — that tree is hand-written.
- Emit code for `@plugin/<product>/<name>` — that namespace form is retired (§6, plugin policy memory 2026-05-11).
- Generate type-erased `map[string]any` shapes for typed surfaces — typed all the way down.
- Read `serde_yaml`, `serde_json::Value`, or other dynamically-typed bridges to reach into IR — consume `lazuli_ir::Module` directly.
- Invoke any Go toolchain at codegen time (only during smoke tests, gated by `LAZULI_GO_SMOKE=1`).

The emitter MUST:

- Be byte-deterministic.
- Produce `gofmt`-equivalent output without invoking `gofmt`.
- Resolve every reference (`@runtime/`, `@plugin/`, `@adapter.`, `@fn.`, `@semantic.`, `@cap.`) through the closed-catalog resolver in `emitter/refs.rs`.
- Surface unresolved references as errors in `--check`; never as silent panics or `string`-typed pass-throughs without a flagged TODO.
- Emit a Code Connect-style banner on every `.gen.go` file (`// Code generated by lazuli; DO NOT EDIT.`) so `gopls` / linters skip them.
