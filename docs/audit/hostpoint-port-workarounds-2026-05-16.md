# Hostpoint Port — Workarounds Inventory

- **Date:** 2026-05-16
- **Source:** Hostpoint port (private repo). All entries are framework-level observations safe to commit to the public Lazuli repo.
- **Purpose:** running log of every workaround applied during the Hostpoint pilot. Each entry has an ID, severity, symptom, root cause hypothesis, the workaround in place, and the removal criterion. When Lazuli grows the canonical feature, the workaround entry is closed and the cell removes the duplicate / shim in the pilot.
- **Method:** entries are *appended* as new gaps surface during the port. Resolved entries are marked `STATUS: closed` with the commit/proposal that fixed them. **Do not edit historical entries** — they document framework velocity.
- **Updates accept new entries:** any agent or human committing a workaround in the Hostpoint port must add (or update) the corresponding `WAR-*` entry here.

---

## Index by category

| Category | IDs | Notes |
|---|---|---|
| Codegen Go | WAR-CODEGEN-GO-01..03 | go.mod require, go.work preserve, handler signature/cycle |
| Codegen TS | WAR-CODEGEN-TS-01..03 | cross-bucket imports, naming prefix, design tokens |
| Codegen — cross-feature | WAR-CODEGEN-XFEAT-01..02 | record reuse, enum reuse |
| Vocab — view kinds | WAR-VOCAB-VIEW-01 | auth-form view primitive |
| Vocab — collections | WAR-VOCAB-COLLECTIONS-01 | Set<Enum> typed collections |
| Vocab — semantic types | WAR-VOCAB-SEMANTIC-01..02 | Money, @semantic.Brazilian* |
| Vocab — constraints | WAR-VOCAB-CONSTRAINT-01 | cross-feature invariants |
| Vocab — webhook | WAR-VOCAB-WEBHOOK-01 | scope global syntax |
| Vocab — auth | WAR-VOCAB-AUTH-01..06 | sessions list, step-up, legal docs, settings updates (CLOSED), CNPJ, postal lookup |
| Vocab — notifications | WAR-VOCAB-NOTIFICATIONS-01 | inbox query + mark-read command |
| Vocab — host home | WAR-VOCAB-HOSTHOME-01..02 | my_host return type, account-pendings query |
| Vocab — host property detail | WAR-VOCAB-HOSTPROPDETAIL-01..03 | denormalized property-detail read, mutation return type, ID type mismatch |
| Vocab — host property create | WAR-VOCAB-PROPERTYCREATE-01..02 | catalog asset-upload commands not implemented, expo-image-picker web fallback |
| Vocab — host property edit | WAR-VOCAB-PROPERTYEDIT-01 | partial-update surface missing voltage/water_source/address/photos/notes |
| Vocab — operations | WAR-VOCAB-OPERATIONS-01..02 | denormalized agenda query, pending-reviews query |
| Vocab — payments | WAR-VOCAB-PAYMENTS-01 | MercadoPago integration end-to-end (CLOSED) |
| Vocab — messaging | WAR-VOCAB-MESSAGING-02..03 | full ChatExperience port + denormalized inbox shape (both CLOSED) |
| Vocab — operator OS | WAR-VOCAB-OPERATOR-01 | operator-only queries not authored (CLOSED) |
| Runtime — ctx | WAR-RUNTIME-CTX-01 | ctx.SessionID exposure |
| Runtime — auth blocks | WAR-RUNTIME-AUTH-01 | password-reset / email-verification block declaration |
| Runtime — migrations | WAR-RUNTIME-MIGRATION-01..03 | CREATE TABLE IF NOT EXISTS (open); reserved-word columns (CLOSED); FK topo-sort (open) |
| Runtime — command routing | WAR-RUNTIME-COMMAND-01 | Register init blocks + Effect:Returns wiring missing |
| Runtime — policy atoms | WAR-RUNTIME-POLICY-01 | Policy.Name without Atoms resolution at runtime |
| Runtime — API mount | WAR-RUNTIME-API-MUX-01 | Mux() doesn't iterate Apis() — /auth/* routes never mount (CLOSED) |
| Runtime — ctx expressions | WAR-RUNTIME-CTX-NOW-01 | ctx.now / ctx.actor unresolved in declarative `creates`/`updates` |
| Runtime — file naming | WAR-RUNTIME-FILENAME-01 | Underscore-prefix files silently excluded by go build |
| Runtime — multitenant | WAR-RUNTIME-MULTITENANT-01 | public-policy creates can't resolve tenant |
| Vocab — query enum | WAR-VOCAB-QUERY-ENUM-01 | query.list filters can't bind enum literals |
| Doctor — design tokens | WAR-DOCTOR-DESIGN-01..02 | hex-leak, undefined-token |
| Doctor — env | WAR-DOCTOR-ENV-01 | PUBLIC_ prefix false positive |
| Scaffold — gitignore | WAR-SCAFFOLD-GITIGNORE-01 | dist/ blanket-ignore vs user-authored handlers |

---

## WAR-CODEGEN-GO-01 — `dist/go/go.mod` emitted with fake `require lazuli.dev/runtime v0.1.0`

- **STATUS:** closed (lazuli commit `32fd8be`, Phase 1.2)
- **Symptom:** `lazuli generate go` emitted `require lazuli.dev/runtime v0.1.0` in `dist/go/go.mod`. `lazuli.dev/runtime` is not a published Go module — first `go mod tidy` failed with `404`.
- **Workaround at the time:** strip the `require` line by hand in `dist/go/go.mod`.
- **Fix:** codegen now skips the fake require when `workspace_mode = true`, relying on `go.work` to resolve the local runtime.

## WAR-CODEGEN-GO-02 — `lazuli generate go` overwrites project-root `go.work`

- **STATUS:** closed (lazuli commit `4f09b9c`, Phase 1.2)
- **Symptom:** every regen overwrote `go.work` with `use ( . ./dist/go )`, blowing away the `c:/Users/lucas/lazuli/runtime/go` workspace entry needed for in-tree dev.
- **Workaround at the time:** re-add the local runtime path manually after each generate.
- **Fix:** codegen now parses existing `go.work`, adds `./dist/go` idempotently, preserves all other entries.

## WAR-CODEGEN-GO-03 — Handler files emitted in wrong location (sub-package cycle)

- **STATUS:** closed (lazuli commit `32fd8be`, Phase 1.2; Option A in handler-architecture decision)
- **Symptom:** scaffold placed user-authored handler stubs at `app/features/<bc>/handlers/` while codegen emitted imports from `dist/go/<bc>/handlers/`. The handler signature emit referenced typed input structs (`LoginResultInput`) defined in the feature package, creating an import cycle if the handler `handlers/` sub-package imported the parent package.
- **Workaround at the time:** none — build broken until fix landed.
- **Fix:** handlers now live in the same package as the feature (`dist/go/<bc>/<name>.go` with `package <bc>`), side-by-side with `*.gen.go`. The `.gen.go` extension is the regen contract; files without it are sacred.

---

## WAR-CODEGEN-TS-01 — Cross-bucket type imports not emitted

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16)
- **Symptom:** when a feature's resource referenced an enum/record declared in another feature, `lazuli generate ts` didn't emit the corresponding `import`. `tsc` failed at every cross-feature site; authors worked around by duplicating the enum body in every consumer.
- **Fix:** `crates/lazuli_cli/src/main.rs` gains `write_cross_feature_imports` + `collect_cross_feature_refs` + `owner_feature_for_type`. For every enum/record referenced but not locally declared, the consumer's `.gen.ts` now emits both an `import type { X } from "../<owner>/<owner>.gen"` and a paired `export type { X } from ...` so existing consumer code that imports from the local feature still works after the duplicate is removed. The owner feature's emitter also widens to project enums consumed cross-feature.

## WAR-CODEGEN-TS-02 — Redundant bucket prefix in SDK function names

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16)
- **Symptom:** `lazuli generate ts` emitted commands with the bucket name twice (`saveHostHostBasicDetails`, `completeHostHostOnboarding`, etc.) when the authored command name already started with the bucket token.
- **Fix:** `command_ident` + `command_input_iface` skip the first command-name token that equals the feature name. `saveHostHostBasicDetails` becomes `saveHostBasicDetails`; `completeHostHostOnboarding` becomes `completeHostOnboarding`. Verified by regenerating Hostpoint's TS SDK + sed-rewriting 23 call sites + typecheck clean + 82/82 e2e green.

## WAR-CODEGEN-TS-03 — `dist/ts-web` design tokens not in Tailwind preset

- **STATUS:** open
- **Symptom:** doctor emits 20+ `design-token-undefined` warnings on migrated `shared/presentation/ui/*` files using Tailwind classes like `font-body`, `rounded-hp-sm`, `bg-surface-subtle`, `text-hp-cyan-700`. These classes are defined in `packages/design-tokens` (the Hostpoint workspace package) but not in `design.lzi` — so doctor doesn't know about them.
- **Workaround in place:** warnings accepted. Build is green because Tailwind preset resolves the classes at compile time.
- **Annotated in:** doctor output during Phase 2.3 commit.
- **Removal criterion:** Lazuli `design.lzi` either (a) supports importing tokens from external workspace packages (`@hostpoint/design-tokens`), OR (b) doctor accepts a `[design].extends = ["@hostpoint/design-tokens"]` opt-out for tokens managed outside the capsule.
- **Surfaced by:** Phase 1.3f UI primitives migration (commit `0fed5ad`).

---

## WAR-CODEGEN-XFEAT-01 — `record` types not reusable across features

- **STATUS:** **closed (TS side)** — Go side already supported cross-feature record refs.
- **Symptom:** `record Address` defined in one feature couldn't be referenced from another without breaking TS codegen.
- **Fix:** same `write_cross_feature_imports` mechanism that closes TS-01 also handles records (the type collector walks both enum and record refs).
- **Note:** authors moving an `Address` record from a feature's local declaration to a shared feature still need to update the call sites (`Address` ⇄ `Account.Address`).

## WAR-CODEGEN-XFEAT-02 — `enum` cross-feature reuse same gap as TS-01

- **STATUS:** **closed** (same fix as WAR-CODEGEN-TS-01)
- **Symptom:** mirror of TS-01; Go-side already handled this correctly (verified `Gender account.Gender` in `dist/go/host/resource.gen.go`).

---

## WAR-VOCAB-VIEW-01 — `.lzx` view kinds don't cover auth-form screens

- **STATUS:** open
- **Symptom:** Lazuli `.lzx` supports `view list`, `view detail`, `view create` (and L0 #6 added cells / drawer / filters / search / sort / selection / bulk_actions / settings). None of these cover *form-mode* views like SignIn / SignUp / Welcome (a marketing landing) / ChooseRole (a 2-option picker) / ForgotPassword (single-input form).
- **Workaround in place:** auth screens are hand-written TSX in `apps/hostpoint-app/src/routes/*.tsx` consuming the Lazuli SDK via `useLazuliCommand`. `.lzx` only declares `view list` against `account.query.mine_sessions` to make `[frontends.*]` declarations satisfy `FRONTEND-AUDIENCE-UNKNOWN-001` (semantically meaningless view).
- **Annotated in:** `c:/Users/lucas/hostpoint/docs/lazuli-port-roadmap.md` Phase 1.3 entry; account.lzx + account.web.lzx in Hostpoint.
- **Removal criterion:** Lazuli adds `view form`, `view landing`, or equivalent primitives expressive enough for auth + onboarding-step screens. Per L0 candidate: `view form <name> { source <command>; fields <list> }` would suffice.
- **Surfaced by:** Phase 1.3e auth screens (6 screens + 12 onboarding screens, all handwritten).

---

## WAR-VOCAB-COLLECTIONS-01 — `Set<Enum>` / typed collections not first-class

- **STATUS:** **closed (basic enum arrays)** — exclusive-sentinel sub-gap tracked separately.
- **Symptom:** `Property.amenities`, `Property.rules`, `Property.accepted_vehicles` etc. were all typed `JSON required = "[]"` because authors believed Lazuli lacked typed collections. The cruel-review 2026-05-16 flagged this as "JSON soup".
- **Discovery + fix:** Lazuli already had `[]` array form (`type_ref_from_syntax` at `crates/lazuli_analyzer/src/lib.rs:1247` lifts `Type[]` to `TypeRef::Many(inner)`). The codegen for Go emits `[]Type` slices, and TS emits `Type[]`. The WAR was a discoverability bug, not a language gap.
- **Hostpoint applied:** `Property.amenities: Amenity[]`, `Property.rules: RuleType[]`, `Property.accepted_vehicles: AcceptedVehicleType[]`. New `enum AcceptedVehicleType` added to `catalog.lzi`. PropertyDetailView record + UpdateCatalogPropertyInput aligned. Front-end casts at the boundary (`as never`) because UI union types carry extras the backend enum doesn't list yet (`adaptedCar` vs `adapted_car`, custom amenities) — a separate Hostpoint vocab sweep would align them but isn't blocking.
- **Open sub-gap (tracked separately)**: `Traveler.pets` exclusive-sentinel (`none` mutually exclusive with others) — Lazuli still lacks `exclusive_sentinel` annotation. UI's `togglePet` helper stays. Add to `docs/next-checklist.md` polish.
- **Note for future authors**: when in doubt about typed collections, just try `EnumType[]`. Lazuli supports it.

## WAR-VOCAB-SEMANTIC-01 — `@semantic.Money` missing

- **STATUS:** **closed** (Lazuli + Hostpoint commits follow-up 2026-05-16; ships per proposal `docs/proposals/semantic-types-money-brazilian.md` v0.3 PASS 8.89/10)
- **Symptom:** `Charge.amount_cents` + `currency: Text = "BRL"` (three columns × `amount + platform_fee + net_to_host`) had no DSL surface saying "this is money". Handler-side `/100` formatting repeated across features; currency was implicit in storage and explicit on display.
- **Fix:** five-layer landing:
  - **Analyzer** (`crates/lazuli_analyzer/src/lib.rs:1301`): bare `Money` keyword resolves to `BuiltinType::SemanticMoney` (was `Decimal`).
  - **Go codegen** (`crates/lazuli_codegen_go/src/emitter/types.rs:160`): emits `lazuli.MoneyValue` (rich struct) instead of legacy `lazuli.Money` (int64 alias preserved for backward compat).
  - **Go migration DDL** (`crates/lazuli_codegen_go/src/emitter/migration_ddl.rs:515`): `NUMERIC(20,4)` per audit removal criterion. Every Money field auto-emits a paired `<field>_currency TEXT` column.
  - **TS codegen** (`crates/lazuli_cli/src/main.rs:1976`): emits `Money` interface from `@lazuli/runtime`.
  - **Go runtime** (`runtime/go/lazuli/money.go`): new `MoneyValue { Amount, Currency }` struct + `BRL/USD/EUR` constructors + `ParseMoneyLiteral` + pgx `Scanner/Valuer` + JSON marshal contracts. Existing `Money = int64` alias preserved.
  - **TS runtime** (`runtime/web/lazuli/src/types.ts`): new `Money` interface + `formatMoney(m, locale)` helper via `Intl.NumberFormat`.
  - **Hostpoint**: `Charge.amount_cents Integer + currency Text` → `Charge.amount Money` (× 3 fields). Migration codegen emits 3 NUMERIC + 3 TEXT currency columns. `CreateCheckoutPreference` handler converts legacy cents to decimal string via `formatBRL` helper. e2e 82/82 still green.
- **Tier 2 (`@plugin/scalars-br` for CPF/CNPJ/CEP/Phone)**: deferred to companion proposal `semantic-types-plugin-locales.md` (per architect grading split). WAR-VOCAB-SEMANTIC-02 remains open until the generic plugin-type-contribution mechanism is graded.

## WAR-VOCAB-SEMANTIC-02 — Brazilian semantic types missing (`@semantic.Brazilian{CPF,CNPJ,CEP,Phone}`)

- **STATUS:** open
- **Symptom:** `Host.cpf: Text required unique` is a raw 11-digit string with no Lazuli-level validation, checksum verification, format constraint, or PII tag. Same for `Host.phone`, `Property.cep`. CPF is regulated PII under LGPD; representing it as `Text` means compliance evidence has to be retrofitted via handler-side validation.
- **Workaround in place:** `Text` fields throughout. Validation/format would happen in handler if implemented at all today (handlers are stubs).
- **Annotated in:** `app/features/host/host.lzi` (Host.cpf); `app/features/catalog/catalog.lzi` (Property.cep).
- **Removal criterion:** `@plugin/scalars-br` plugin ships `@semantic.BrazilianCPF` / `@semantic.BrazilianCNPJ` / `@semantic.BrazilianCEP` / `@semantic.BrazilianPhone` per scope-discipline (per-locale scalars belong in `@plugin/scalars-<locale>`). Lazuli core stays locale-agnostic; the plugin provides validation + PII tagging + format helpers.
- **Surfaced by:** cruel-review 2026-05-16; PRODUCT.md domain rules (CPF, CNPJ for host-only).

---

## WAR-VOCAB-CONSTRAINT-01 — Cross-feature invariants missing

- **STATUS:** open
- **Symptom:** `host.Host` has `user: User required unique` where `user.role` must equal `Role.host`. Nothing in `host.lzi` declares this invariant. A `Host` row can be created referencing a User whose role is `traveler` — silent data corruption.
- **Workaround in place:** convention only. Handlers TRUST the caller's role check (which is enforced via `policy @policy.host_only` on commands but NOT at the data layer).
- **Annotated in:** `app/features/host/host.lzi` commit `b0040fe` deferred list.
- **Removal criterion:** Lazuli adds `constraint` or `rule` vocabulary that expresses cross-feature invariants like `host.Host.user.role = account.Role.host`. Doctor enforces at lowering; Postgres emit becomes `CHECK` constraint or trigger.
- **Surfaced by:** cruel-review 2026-05-16.

## WAR-VOCAB-WEBHOOK-01 — `scope global` not accepted as `webhook` child

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16)
- **Symptom:** `webhook-tenant-from` lint asked for `tenant_from payload.<axis>_id` OR `scope global` + `reason`. The parser rejected the second form, leaving payments-like webhooks (provider doesn't send tenant key) with no audited escape hatch.
- **Fix:** `crates/lazuli_syntax/src/ast.rs` gains `WebhookScopeGlobal { reason, span }` and `Webhook.scope_global: Option<WebhookScopeGlobal>`. `crates/lazuli_syntax/src/parser.rs` accepts the two-line form:
  ```lzi
  webhook mp_payment_event
    ...
    scope global
      reason "MercadoPago does not include a tenant key in payment notifications;
              handler reconciles org from provider_external_reference lookup"
  ```
  `reason` is REQUIRED (parser diagnostic if missing) so the audit-of-record surfaces why this webhook escapes tenant_from. The LSP lint at `crates/lazuli_lsp/src/lib.rs:10720` already detected `scope global` and suppressed the warning — only the parser was the blocker. Hostpoint's `payments.lzi` now declares the scope_global with a precise reason; `lazuli check` passes clean.

## WAR-VOCAB-AUTH-01 — Sessions-list query not auto-emitted by `auth sessions` block

- **STATUS:** open
- **Symptom:** Settings → ContaSeguranca screen needs `account.list_sessions() -> [Session]` with device, location, last_seen, is_current fields. The `auth sessions { resource UserSession; ttl "7 days" }` block declares the contract but does not auto-emit a `query.list sessions` for the authenticated actor.
- **Workaround in place:** not yet implemented — settings screens are deferred. When authored, will need a hand-rolled `query.list mine_sessions` plus extending `UserSession` with `device_label`, `location`, `last_seen` fields (not in current schema).
- **Removal criterion:** Lazuli `auth sessions` block auto-emits the canonical sessions-list query + adds device/location/last_seen as standard session columns.
- **Surfaced by:** AccountFlows.ContaSeguranca storybook pattern.

## WAR-VOCAB-AUTH-02 — Step-up auth / sensitive-change flow not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 1.3g)
- **Symptom:** ContaSeguranca screen requires a 6-digit confirmation code on sensitive change (`update_credentials`). Two-command flow: `account.request_step_up_code()` then `account.update_credentials(..., confirmation_code)`. Today this would be handler-side improvisation.
- **Workaround in place:** `routes/account/Security.tsx` keeps the code field local-state only and fires the confirmation Dialog → success Dialog without a backend call. Save handlers are stubs with `// TODO: wire account.request_step_up_code() / account.update_credentials(...)` markers.
- **Annotated in:** `apps/hostpoint-app/src/routes/account/Security.tsx`.
- **Removal criterion:** Lazuli adds `step_up` or `@hook.requires_step_up` vocabulary so sensitive commands can declare their freshness window + verification requirement.
- **Surfaced by:** AccountFlows.ContaSeguranca + Settings sub-panels.

## WAR-VOCAB-AUTH-03 — `platform.get_legal_doc(kind)` query + `platform.request_data_action()` command not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 1.3g)
- **Symptom:** Privacy / Terms screens display a versioned legal document (sections + last_updated) and the privacy screen exposes a DSAR ("Solicitar dados ou exclusão") fire-and-forget request. Neither the read query nor the write command exist in `platform.lzi`.
- **Workaround in place:** `routes/account/PrivacyPolicy.tsx` and `routes/account/TermsOfUse.tsx` hard-code the article sections + last-updated label in module-local constants. The DSAR button toggles a local Dialog without backend interaction.
- **Annotated in:** `apps/hostpoint-app/src/routes/account/{PrivacyPolicy,TermsOfUse}.tsx`.
- **Removal criterion:** Lazuli `platform` BC ships `LegalDoc` record (sections array + last_updated + version), `platform.get_legal_doc(kind: privacy | terms)` query, and `platform.request_data_action()` command. Screens then consume via `useLazuliQuery` / `useLazuliCommand`.
- **Surfaced by:** AccountFlows.PoliticaPrivacidade / AccountFlows.TermosDeUso.

## WAR-VOCAB-AUTH-04 — Settings update commands (host.update_*, traveler.update_*, account.update_*) not authored

- **STATUS:** **closed** (Hostpoint commits `699696c` + `9afc4d5`, Phase 4.2 / 1.3h, 2026-05-16) — 17 new commands authored across account/host/traveler; 13 panels wired to canonical update commands; setTimeout stubs removed.
- **Symptom:** Settings sub-panels need granular updates: `traveler.update_basic_details / update_contact / update_vehicle / update_family / update_pets / update_languages / update_health_notes`, `host.update_personal / update_contact / update_address / update_languages`, `account.update_email(new_email, current_password)` (two-phase), `account.update_notifications_pref(enabled)`. None of these commands existed in the corresponding `.lzi` files. Only the onboarding-step `save_*` commands shipped (designed for first-time entry, not partial updates).
- **Workaround at the time:** sub-panels re-used the onboarding `save_*` commands as proxies, or `setTimeout` stubs with `// TODO: wire <command>` markers.
- **Resolution:** authored 17 canonical update commands across the three features. 12 are declarative `updates X` (zero handler code). 3 require @fn handlers (`update_credentials`, `revoke_session`, `revoke_other_sessions`) and ship with hand-rolled implementations in `dist/go/account/`. 2 new User columns added (`notifications_enabled`) and 4 new UserSession columns added (`device_label`, `location_label`, `last_seen_at`, `revoked_at`) to support the Security sessions dialog.
- **Lesson:** the `updates <Resource>` declarative form covered 12 of 17 commands with zero handler authoring. Only commands that need conditional logic (password verification, scoped DELETE) need @fn handlers. Lazuli's declarative bias paid off — most of the new vocabulary is just SDL.

## WAR-VOCAB-AUTH-05 — Host "CNPJ" field mismatch with Host.cpf SDK field

- **STATUS:** **closed (storage side)** — semantic typing (`@semantic.BrazilianCNPJ`) deferred to companion proposal alongside scalars-br.
- **Symptom:** Storybook host-personal panel showed CNPJ; SDK only had `Host.cpf: Text required unique`. Front-end cached CNPJ as fixture and forwarded as `cpf` to satisfy the SDK contract.
- **Fix:** `host.lzi` Host resource now declares BOTH:
  - `cpf: Text optional unique` (was `required unique`)
  - `cnpj: Text optional unique` (new)
  Authors choose: natural-person hosts populate `cpf`; legal-entity hosts populate `cnpj`. Both fields are `Text` until `@semantic.BrazilianCPF` / `@semantic.BrazilianCNPJ` ship via `@plugin/scalars-br` (WAR-VOCAB-SEMANTIC-02, separate proposal). The `save_host_basic_details` command accepts both fields as optional input. Front-end HostPersonal panel can drop the fixture-as-cpf forwarding once it wires the cnpj input via `updateHostPersonal`.
- **Note**: a future `host.request_cnpj_change()` command for support-mediated alteration is tracked in next-checklist.md alongside the audit-trail invariants.

## WAR-VOCAB-AUTH-06 — CEP autofill (`platform.lookup_postal_code`) not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 1.3g)
- **Symptom:** Host onboarding Address step and Settings host-address panel both include a CEP field with the helper "Vamos preencher seu endereço automaticamente." Storybook implies a `platform.lookup_postal_code(country: Country, postal_code: Text) -> Address` query that prefills street/neighborhood/city/state from a CEP. No query authored yet.
- **Workaround in place:** CEP field is plain Input. No autofill on blur. Users type the address manually.
- **Annotated in:** `apps/hostpoint-app/src/routes/settings/panels/HostAddress.tsx` (and original onboarding `Address.tsx`).
- **Removal criterion:** `platform.lzi` (or `@plugin/scalars-br`) ships the lookup query backed by Correios/ViaCEP. Address panels wire the lookup on CEP blur.
- **Surfaced by:** Settings host-address panel + onboarding Host.Address.

---

## WAR-RUNTIME-CTX-01 — `ctx.SessionID` / `ctx.SessionToken` not exposed to handlers

- **STATUS:** open
- **Symptom:** `command logout` handler needs to invalidate the current session (`auth.InvalidateSession(ctx, contract, token)`). The raw session token is extracted from the cookie by middleware and used to populate `ctx.User`, but the token itself is not exposed to the handler.
- **Workaround in place:** `Logout()` deletes ALL sessions of the actor instead of just the current one. Semantically "log out of every device" — acceptable for MVP but not the storybook UX.
- **Annotated in:** `dist/go/account/logout.go` inline comment; commit `c6897ee`.
- **Removal criterion:** `lazuli.Ctx` exposes `SessionID lazuli.ID` or `SessionToken string` populated by the middleware after `auth.ResolveSession`. Logout handler then revokes just the current session.
- **Surfaced by:** Phase 4.1 real handler implementations.

## WAR-RUNTIME-AUTH-01 — Email-verification / password-reset blocks need vocab + handler glue

- **STATUS:** partial workaround applied (2026-05-16 — Hostpoint Phase 4.2)
- **Symptom:** Lazuli runtime has `auth.RequestPasswordReset`, `auth.ConfirmPasswordReset`, `auth.IssueEmailVerificationToken`, `auth.VerifyEmailToken` with `PasswordResetContract` / `EmailVerificationContract` types. But these contracts must be DECLARED in `.lzi` (via `auth password_reset` / `auth email_verification` blocks?) for codegen to emit them. Account.lzi only has `command request_password_reset` / `command verify_email` with `handler @fn.X` — bypasses the canonical auth block path.
- **Workaround applied 2026-05-16:** Hostpoint authors token resources directly in `account.lzi` (`PasswordResetToken`, `EmailVerificationToken`) with hand-rolled column layout matching the framework helpers' expectations. Handler files at `dist/go/account/{request_password_reset,reset_password,verify_email}.go` implement the SHA-256 + argon2id flow directly without calling the framework's `auth.*` helpers (the helpers assume the canonical `auth password_reset` block, which would conflict with our hand-rolled resources). Dev-mode delivers tokens via `slog.Info("auth: …", "link", …)` — production wires `@plugin/smtp` for email and `@plugin/sms-twilio` for SMS.
- **Removal criterion:** documented grammar for `auth password_reset { resource <X>; ttl <Y>; identity <field> }` and `auth email_verification { ... }` blocks that emit the contract + canonical command + canonical route + delivery-side hook. Then the handler shrinks to just calling `@plugin/smtp` / `@plugin/sms-twilio` for the actual send.
- **Surfaced by:** Phase 4.2 (commit `723065d` in Hostpoint).

## WAR-RUNTIME-COMMAND-01 — Commands not registered to HTTP Mux + handler-based Effect:nil

- **STATUS:** workaround applied (Hostpoint commit `723065d`, 2026-05-16, Phase 4.2)
- **Symptom:** two compound framework gaps that together mean ALL handler-based commands silently 404 (or 500) when called via the typed SDK:
  1. **Registration missing.** `dist/go/<feature>/command.gen.go` declares `var <cmd> = lazuli.Command[...]{...}` but emits NO `func init() { lazuli.Register(&cmd, ...) }` block. Without `Register`, the command is not in `lazuli.Commands()` and `lazuli.Mux()` skips it → HTTP 404 at `/api/v1/c/<command-name>`.
  2. **Effect:nil on @fn handlers.** Commands declared with `handler @fn.X` (vs `returns X`) emit `Effect: nil` instead of `Effect: lazuli.Returns(X)`. Even when the command IS registered, `Command.Handle()` returns HTTP 500 `"command has no effect"` from `applyEffect` (`runtime/go/lazuli/handle.go:269-274`) because the dispatcher can't find anything to invoke.
- **Discovery:** every `useLazuliCommand(updateAccountCredentials)` call from the panel agent was hitting a 404 backend. Verified by reading the .gen.go files, the `Mux()` function (`runtime/go/lazuli/http.go:27`), and the `Register*` chain (`runtime/go/lazuli/register.go:69` → `registry_typed.go:168`). NO `.gen.go` in Hostpoint or `examples/full-capsule/` calls `lazuli.Register` for commands.
- **Workaround applied:** one `_register.go` per feature (`dist/go/<feature>/_register.go`) that:
  - patches `<cmd>.Effect = lazuli.Returns(<UserFn>)` for every command whose user handler exists,
  - calls `lazuli.Register(&cmd1, &cmd2, ...)` for every command in the feature.
  The leading underscore in `_register.go` sorts it ahead of `command.gen.go` alphabetically, but since Go runs all package-level `var` initializers before any `init()`, ordering doesn't matter — the patch just needs to run before the first HTTP request.
  Files: `dist/go/{account,host,traveler,catalog,messaging,operations,payments,platform,trust}/_register.go` (9 features, 64 commands registered, 13 Effect wires in account, 1 in host).
- **Annotated in:** each `_register.go` carries a doc comment pointing at this WAR entry.
- **Removal criterion:** Lazuli `lazuli generate go` codegen emits the matching init blocks per feature:
  ```go
  func init() {
      <cmd>.Effect = lazuli.Returns(<UserFn>) // for `handler @fn.X` commands when the user fn exists
      lazuli.Register(&cmd1, &cmd2, …)
  }
  ```
  Open design question: how does codegen know whether a `@fn.X` handler exists (vs. has not been authored yet)? Options: (a) emit the Effect patch unconditionally and rely on Go's package-init-order to fail loudly if the symbol is undefined; (b) `@fn.X at "./handlers/X.go"` triggers an emit of a `func X(...) {…}` stub in `dist/go/<feature>/X.gen.go` that the user `replace`s with a hand-written file (the gen contract via filename); (c) emit a registration registry-side helper `lazuli.WireHandler[I,O](name, func(...))` that user code calls at init time. Option (b) preserves the current "sacred user file" contract and is the lowest-friction.
- **Impact**: BLOCKER for every typed SDK command. Without this fix, the entire panel/onboarding/settings/auth surface 404s at runtime in spite of clean typecheck + build.
- **Surfaced by:** Phase 4.2 / Phase 1.3h integration verification on 2026-05-16.

## WAR-RUNTIME-POLICY-01 — Policy `Name` references emitted without resolved `Atoms`

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint dist/go/<feature>/register.go)
- **Symptom:** Lazuli codegen emits `lazuli.Policy{Name: "@policy.<name>"}` with an empty `Atoms` slice. The runtime's `EvalPolicy` (`runtime/go/lazuli/policy.go:127`) returns HTTP 500 `"command/query registered with empty policy: @policy.<name>"` because it expects `Atoms` to be populated. The codegen comment (`crates/lazuli_codegen_go/src/emitter/command.rs:1137-1138`) acknowledges this gap: "let the Lazuli Go lib's registry walk resolve at boot — matching what the spike does for…", but no such registry exists.
- **Discovery:** end-to-end smoke test on 2026-05-16 — every authenticated command returned 500 on the first request.
- **Workaround in place:** each feature's `register.go` (Hostpoint commit `700e95b`) ships a `policyAtoms(name) []lazuli.PolicyAtom` lookup table covering the canonical catalog (`@policy.public` / `@policy.authenticated` / `@policy.host_only` / `@policy.traveler_only` / `@policy.operator_only`) + a generic `patchPolicy[I,O](*lazuli.Command[I,O])` helper that assigns `cmd.Policy.Atoms` in `init()`. After patching, unauthenticated requests correctly return 403 `policy_denied`.
- **Removal criterion:** Lazuli codegen resolves `@policy.<name>` references from the feature's `policies` block at codegen time and emits `lazuli.Policy{Name: "@policy.<name>", Atoms: []lazuli.PolicyAtom{...}}` populated. OR the runtime ships a `RegisterPolicy(name, atoms)` registry + `EvalPolicy` lookup-by-name when `Atoms` is empty.
- **Surfaced by:** Phase 4.2 e2e smoke test 2026-05-16.

## WAR-RUNTIME-API-MUX-01 — `Mux()` doesn't auto-mount API registrations

- **STATUS:** **closed** (lazuli commit `0798932`, 2026-05-16)
- **Symptom:** `lazuli.Mux()` walked `Commands()` + `Queries()` + `report.Mount(mux)` but skipped `Apis()`. APIs registered via `lazuli.RegisterApi(&Api{Path: "/auth/login", Handler: auth.LoginHandler})` from the generated `auth.gen.go` were stored in the registry but never bound to HTTP routes — every call to `/auth/*` returned 404.
- **Fix:** two cooperating changes:
  1. `apiRegistration` extended to carry `Method HttpMethod` + `Dispatch func(*Ctx, []byte) (any, error)`. The Dispatch closure captures the typed `Api[I, O]`, unmarshals JSON body to I, calls `api.Invoke` (runs plan-gate prelude + user Handler), returns marshaled output.
  2. `Mux()` added a loop over `Apis()` that mounts `<METHOD> <PATH>` via the new `handleApiRequest` helper.
- After this fix, `/auth/login` + `/auth/signup` + `/auth/logout` mount; they now return 500 with `auth: <X>Handler not implemented` because the framework auth stubs still need real implementations (tracked as WAR-RUNTIME-AUTH-01), but the routing layer no longer 404s.
- **Note:** Hostpoint's user-side login/register still works via `POST /api/v1/c/account.login` (command path) per the user-authored Login() func.

## WAR-RUNTIME-CTX-NOW-01 — `ctx.now` and `ctx.actor` not resolved in declarative effects

- **STATUS:** open
- **Symptom:** `.lzi` declarative `creates` / `updates` clauses commonly reference `ctx.now` (e.g., `created_at = ctx.now`) and `ctx.actor.org_id` / `ctx.actor.user_id` for tenancy + ownership. The runtime fails with `"unknown ctx path: now"` when applying the effect, returning HTTP 500. Tested with `POST /api/v1/c/account.register` which has `creates User { ... created_at = ctx.now }` in account.lzi.
- **Workaround in place:** none yet. Workaround options: (a) replace `ctx.now` with hand-rolled `@fn.X` handler that builds the INSERT manually; (b) make `register` command use a `handler @fn.register` and author the handler.
- **Removal criterion:** runtime's `applyCreates` / `applyUpdates` recognises `ctx.now` / `ctx.actor.*` / `input.*` value-source paths and resolves them at handler time. Today only `input.*` paths work via `FromInput`.
- **Surfaced by:** Phase 4.2 e2e smoke test 2026-05-16.

## WAR-RUNTIME-FILENAME-01 — Underscore-prefix files silently excluded by `go build`

- **STATUS:** closed (renamed in Hostpoint commit `700e95b`)
- **Symptom:** files named `_register.go` (workaround init blocks for WAR-RUNTIME-COMMAND-01) were silently excluded by `go build`. Go's build system ignores any source file whose name starts with `_` or `.`. The init() blocks never compiled, so the HTTP Mux had 0 routes even though `dist/go/<feature>/_register.go` existed and `cargo run --quiet -- generate go` succeeded.
- **Discovery:** boot log showed `commands=0` despite 9 register.go files existing in the working tree.
- **Fix:** renamed all 9 files from `_register.go` to `register.go`.
- **Lesson for future:** Lazuli codegen should NEVER emit filenames starting with `_` or `.`. Reserve `dist/<feature>/<name>.gen.go` for regen-overwritable and `dist/<feature>/<name>.go` for user-authored sacred files.
- **Surfaced by:** Hostpoint e2e smoke test 2026-05-16.

---

## WAR-DOCTOR-DESIGN-01 — `design-token-hex-leak` on inline-style placeholders

- **STATUS:** partially resolved (commit `0fed5ad` removed most inline hex by migrating to NativeWind classes consuming design tokens; some `_auth-styles.ts`-era inline hex may persist until full storybook fidelity sweep)
- **Symptom:** doctor warned `hex literal '28bbdd' used in an inline style prop. Declare the color in design.lzi and use tokens.color.<name>.base.` 20+ times across early auth screens (before UI primitive migration).
- **Workaround in place:** migration to NativeWind / shared/presentation/ui primitives that consume the `@hostpoint/design-tokens` Tailwind preset.
- **Removal criterion:** all screens consume tokens via the preset; doctor stays silent.
- **Surfaced by:** Phase 1.3e initial auth screens (commit `0c539e6` / `6a3d617`).

## WAR-DOCTOR-DESIGN-02 — `design-token-undefined` on NativeWind tokens not in `design.lzi`

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16) — extension-allowlist mechanism.
- **Symptom:** doctor flagged Tailwind classes (`font-body`, `rounded-hp-sm`, `bg-surface-subtle`) coming from external workspace packages (`@hostpoint/design-tokens`) because Lazuli's `design.lzi` emitter doesn't see them. Build was green (Tailwind preset resolved them) but doctor noise persisted.
- **Fix:** `crates/lazuli_doctor/src/design/helpers.rs` `read_allowlist` now also merges `dist/ts-web/design/allowlist.extension.json` when present. The extension file is hand-authored by the capsule owner to declare tokens that come from external packages — same JSON shape as the canonical allowlist; entries append to per-prefix buckets. Hostpoint creates this file once listing tokens from `@hostpoint/design-tokens` (`font: ["body", "display", "mono"]`, `rounded: ["hp-sm", "hp-md", ...]`, etc.); doctor stays silent on those without affecting the rules for in-`design.lzi` tokens.
- **Pattern**: same as `# doctor:allow` per-field opt-outs — the extension file is the per-project escape hatch for design tokens that legitimately come from outside the capsule.
- **Surfaced by:** Phase 1.3f UI primitives migration.

## WAR-DOCTOR-ENV-01 — `env-client-exposure` false positive on vendor-style PUBLIC names

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16)
- **Symptom:** the original lint required `name.starts_with("PUBLIC_")`, which rejected vendor-imposed shapes like `MERCADOPAGO_PUBLIC_KEY` or `STRIPE_PUBLIC_KEY` where `PUBLIC` appears as a mid-name token rather than the leading prefix. Vendor SDKs impose the latter shape; the lint forced authors to either rename (incompatible with SDK) or accept noise.
- **Fix:** new `has_public_token(name)` helper accepts `PUBLIC` as any `_`-delimited token in the env name. Both `PUBLIC_API_KEY` and `MERCADOPAGO_PUBLIC_KEY` are now treated as intentional public-exposure declarations. Both LSP and doctor instances of the lint share the helper for consistency. Message updated to: "client env names should contain a `PUBLIC` token (e.g. `PUBLIC_MERCADOPAGO_KEY` or vendor-style `MERCADOPAGO_PUBLIC_KEY`)".

---

## WAR-VOCAB-NOTIFICATIONS-01 — Inbox query + read-state command not modeled

- **STATUS:** **closed** (Hostpoint commit follow-up 2026-05-16)
- **Symptom:** Storybook `Notifications.Viajante` + `Notifications.Anfitriao` show a per-actor inbox with unread badges + category filters + tap-to-read. `messaging.lzi` had `NotificationDelivery` (sender-side delivery tracking) but no actor-side denormalized query and no read-state command.
- **Fix:** two cooperating additions to `messaging.lzi`:
  - `record NotificationListEntry` (8 fields) — delivery_id + template_key + rendered title/body + tone + icon_key + unread + created_at.
  - `command list_my_notifications returns JSON handler @fn.list_my_notifications` — single query over `notification_delivery`, projects rows to the inbox shape via `renderNotification` / `toneForTemplate` / `iconForTemplate` (template-aware label/icon picker).
  - `command mark_notification_read input { delivery_id } handler @fn.mark_notification_read` — updates `notification_delivery.status` to `read` for the actor's own row.
  Front-end `routes/Notifications.tsx` wires `useLazuliCommand(listMessagingMyNotifications)` + `useLazuliCommand(markMessagingNotificationRead)`. When the back-end returns rows they replace fixtures via `normalizeNotificationEntries`; empty DB keeps the storybook visual. Category derived from `template_key` prefix (reservation / proposal / message / payment / system).

## WAR-VOCAB-HOSTHOME-01 — `host.query.my_host` SDK return type is wrong

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16 — `pick_query_resource_ts` heuristic)
- **Symptom:** `lookupHostByMyHost` returned `IntermediationTermsAcceptance` because the TS codegen unconditionally picked `feature.resources.first()` as the lookup-query return type. The host feature happens to declare `IntermediationTermsAcceptance` before `Host`, so every `query.lookup` / `query.list` in `host.lzi` got the wrong type.
- **Fix:** `crates/lazuli_cli/src/main.rs` adds `pick_query_resource_ts(feature, query_name)` — finds the resource whose snake-cased name appears as a substring of the query name (or matches the last token for compound names like `ServiceTransaction`). Falls back to `feature.resources.first()` when no match. Verified: `lookupHostByMyHost` → `Host`, `lookupCatalogByPropertyDetail` → `Property`, `listOperationsMineTransactionsAsHost` → `ServiceTransaction[]`.

## WAR-VOCAB-HOSTHOME-02 — Account-pendings query not modeled

- **STATUS:** **closed** (Hostpoint commit follow-up 2026-05-16)
- **Symptom:** Storybook host-home "Pendencias" section needed a cross-feature synthesis (MP-account flag from `payments`, profile-completion flag from `account`, has-published-properties flag from `catalog`). No single query expressed this.
- **Fix:** `host.lzi` declares two new records + one command:
  - `record HostHomePending` — keyed pending tile (title/subtitle/tone/icon/cta_label/cta_target).
  - `record HostHomeSnapshot` — identity snapshot (display_name, avatar, unread count, property count, MP-connected).
  - `command get_host_home returns JSON handler @fn.get_host_home` — single handler queries User + Host + counts on NotificationDelivery + Property + MercadoPagoAccount, then `buildHostHomePendings` synthesizes the cross-feature pendings list. Returns `{snapshot, pendings}` JSON.
  Front-end `HostHome.tsx` consumes via `useLazuliCommand(getHostHostHome)`; live data overrides fixture, empty/loading keeps the storybook visual. The cross-feature aggregation primitive design space (`union`, `derived` view) is left as future Lazuli work — the handler-based synthesis works because Hostpoint owns all three contributing features.
- **Surfaced by:** Phase 3.2 host-home port (this entry).

---

## WAR-RUNTIME-MIGRATION-01 — `lazuli generate go` emits `CREATE TABLE IF NOT EXISTS` instead of ALTER TABLE for added columns

- **STATUS:** open
- **Symptom:** when a resource gains a new field after the initial migration is generated, the subsequent regen emits a NEW migration file (`003_account_user_session.sql`) with `CREATE TABLE IF NOT EXISTS user_session (...)` including the new column. Against a fresh database the new migration is a no-op (the table already exists), so the new column is never added. Hostpoint hit this when adding `notifications_enabled` to User and `device_label/location_label/last_seen_at/revoked_at` to UserSession.
- **Workaround in place:** none yet — accepted for fresh-DB deploys (Phase 5 VPS will provision a fresh Postgres). For existing DBs, hand-roll `ALTER TABLE` migrations until the framework supports incremental diffs.
- **Annotated in:** Hostpoint commit `699696c` (added columns); no inline annotation in migrations.
- **Removal criterion:** Lazuli migration emit uses an Atlas-style diff (compare against `migrations/.lazuli-snapshot.json` or similar) and emits `ALTER TABLE … ADD COLUMN` for new fields, drops + adds for renames, etc. Architecturally this is the `atlas` integration target documented in `docs/architecture.md` technology picks.
- **Surfaced by:** Phase 4.2 / 1.3h schema additions (commit `699696c`).

## WAR-RUNTIME-MIGRATION-02 — `lazuli generate go` emits unquoted column names that collide with SQL reserved words

- **STATUS:** **closed** (lazuli main branch, 2026-05-16 — `crates/lazuli_codegen_go/src/emitter/migration_ddl.rs` `is_sql_reserved_word`).
- **Symptom:** Postgres migration files included columns named `user`, a SQL reserved word, causing `ERROR: syntax error at or near "user"` on every CREATE TABLE. Affected 16+ tables across all features (host, traveler, phone_otp, user_session, password_reset_token, email_verification_token, service_transaction, web_push_subscription, notification_delivery, data_request, review, reputation_snapshot, chat, chat_message, intermediation_terms_acceptance).
- **Fix:** `sql_ident()` now consults a Postgres reserved-words list (`is_sql_reserved_word`) covering `user`, `from`, `to`, `select`, `where`, `order`, … and quotes any column or FK constraint identifier that collides. Verified by clean-regenerating migrations + applying to Postgres: all 28 tables created successfully.

## WAR-RUNTIME-MIGRATION-03 — Migration files not topologically sorted by FK dependency

- **STATUS:** open
- **Symptom:** migration files are named `NNN_<feature>_<resource>.sql` where N comes from a per-feature counter; applied alphabetically by filename, cross-feature FKs land out of order. Example: `010_host_host.sql` requires `org` (created at `019_org_org.sql`), so single-pass apply fails with `relation "org" does not exist` on every host/traveler/catalog/operations table.
- **Discovery:** end-to-end smoke test on 2026-05-16.
- **Workaround in place:** apply migrations in N passes — each pass re-applies failed tables until all FK dependencies resolve. In practice 2-3 passes suffice. `CREATE TABLE IF NOT EXISTS` makes successful passes idempotent. The `pnpm db:migrate` script in `package.json` needs updating to retry until the failure count converges.
- **Removal criterion:** Lazuli emits a topologically-sorted single `0001_init.sql` (or numerically-prefixed files in dependency order), so any single-pass apply succeeds. Alternative: a `lazuli migrate` Go runner that respects FK dependencies via Tarjan-style ordering before delegating to `pgx.Conn.Exec`.
- **Surfaced by:** Hostpoint end-to-end provisioning 2026-05-16.

---

## WAR-SCAFFOLD-GITIGNORE-01 — `lazuli new` `.gitignore` blanket-ignores `dist/`

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16)
- **Symptom:** scaffolded `.gitignore` line `dist/` ignored EVERYTHING including user-authored handler files at `dist/go/<bc>/<name>.go`. Violated Lazuli's regen contract (`.gen.go` overwritable, non-`.gen.go` sacred).
- **Fix:** `GITIGNORE_TEMPLATE` at `crates/lazuli_cli/src/main.rs:38` now ignores ONLY regen-overwritable artifacts:
  - `dist/**/*.gen.go`, `dist/**/*.gen.ts`, `dist/**/*.zod.ts` (codegen outputs)
  - `dist/go/{main.go,go.mod,go.sum,migrations/}` (full-rewrite slots)
  - `dist/{ts-web,ts-mobile}/design/` (design-token snapshots)
  - `.lazuli/` (internal cache)
  User-authored files (handler `.go`/`.ts` at `dist/go/<bc>/<name>.go`) stay tracked. Hostpoint's hand-rewritten `.gitignore` at `1c03f30` was the design source; the scaffold now matches.

---

## WAR-VOCAB-HOSTPROPDETAIL-01 — Denormalized property-detail read not modeled

- **STATUS:** **closed (read side)** (Hostpoint commit follow-up 2026-05-16) — services-projection follow-up tracked separately.
- **Symptom:** host-property-detail screen needed joined display fields per property: formatted address, cover URL, photo gallery URLs, amenities/rules/accepted-vehicles enum arrays. Existing `lookupCatalogByPropertyDetail` returned wrong type (UploadedAsset, cf. HOSTPROPDETAIL-02). Even with correct type, raw Property had JSON columns + FK ids only.
- **Fix:** `catalog.lzi` declares `record PropertyDetailView` (15 fields) + `command get_property_detail_view input { property_id } returns JSON handler @fn.get_property_detail_view`. Handler joins `uploaded_asset` for cover URL, composes `formatted_address` from CEP/street/number/complement/neighborhood/city/state, projects the JSON amenities/rules/accepted-vehicles columns as-stored. Front-end `HostPropertyDetail.tsx` fires `useLazuliCommand(getCatalogPropertyDetailView)` on mount when `property_id` is numeric, then `mergeFixtureWithDetail(FIXTURE_PROPERTY, live)` merges live data over the fixture for displayed fields. `filterEnum` guards against unknown enum values.
- **Open follow-up (smaller scope):** `PropertyDetailView` doesn't yet project the linked-services list; the fixture's `LinkedService[]` is preserved for the services section. A future cycle can add `services[]{id, name, summary, price_formatted, icon_key, accent_key, is_active}` to the record. The amenity/rule/vehicle display catalogs (label + icon per enum value) remain inline in `_META` constants — moving them server-side via `@semantic.LabeledEnum` is a vocabulary cycle, not a port blocker.

## WAR-VOCAB-HOSTPROPDETAIL-02 — Catalog command return type is `UploadedAsset`

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16 — `command_output_ts_type` fix)
- **Symptom:** every catalog command was typed `defineCommand<Input, UploadedAsset>` because the TS codegen for `CommandEffect::None` (handler-only commands without `creates`/`updates`/`deletes`/`returns`) fell back to `feature.resources.first()`. UploadedAsset was the first resource declared, so it leaked to every fire-and-forget command.
- **Fix:** `crates/lazuli_cli/src/main.rs` `command_output_ts_type` now returns `"void"` for `CommandEffect::None` instead of the first resource. This is the right shape — `@fn.*` handlers without a declared effect produce `struct{}` on the Go side. Verified: all `publish_*`/`unpublish_*`/`delete_*` commands now type as `defineCommand<I, void>`. The `void` propagates through `useLazuliCommand` so callers cannot accidentally read a meaningful response payload.

## WAR-VOCAB-HOSTPROPDETAIL-03 — Route param type mismatch with `ID = number`

- **STATUS:** **closed** (Lazuli commit follow-up 2026-05-16 — typed coercion helpers `toID` + `tryID`)
- **Symptom:** URL params are typed `string` per tanstack-router; the SDK's `ID = number` rejected them. Every call site did `Number(params.id)` which silently produced `NaN` for non-numeric placeholders.
- **Fix:** `runtime/web/lazuli/src/types.ts` exports two helpers re-exported from `@lazuli/runtime`:
  - `toID(value: string | number | undefined | null): ID` — throws on non-numeric input; call this when you require a real ID.
  - `tryID(value: string | number | undefined | null): ID | null` — returns null on non-numeric input; call this when storybook fixture ids may legitimately appear (`"pousada"`).
  HostPropertyDetail.tsx demonstrates the closure pattern: `const propertyId = tryID(propertyIdRaw)`, then skip the live query when `propertyId === null`. Type-safe, intent-revealing, and the `NaN` payload risk is gone.

---

## WAR-VOCAB-OPERATIONS-01 — Agenda + reservations denormalized actor-side queries

- **STATUS:** **closed** (Hostpoint commit follow-up 2026-05-16)
- **Symptom:** the storybook host-operations agenda + traveler-reservations cards needed joined traveler / host / property / service display attributes per row. The generated `listMineTransactionsAsHostOperationss` / `listMineTransactionsAsTravelerOperationss` queries returned the raw `ServiceTransaction` resource — FK ids only, no joined names / photos / display fields. Client-side N+1 lookups per row would have been the only path.
- **Fix:** two new denormalized records + JSON-returning commands in `operations.lzi`:
  - `record AgendaEntry` (16 fields) — transaction id + status + joined traveler/property/service display attrs + total + lifecycle timestamps.
  - `record ReservationEntry` (17 fields) — same shape from the traveler perspective with `host_*` + `property_city` + `property_state` for the reservation card.
  - `command list_host_agenda returns JSON handler @fn.list_host_agenda` — single SQL with JOINs on traveler/host/property/service + LEFT JOIN LATERAL for cover photo.
  - `command list_traveler_reservations returns JSON handler @fn.list_traveler_reservations` — symmetric.
  Front-end normalizers (`HostOperations.tsx` `normalizeAgendaEntries` + `TravelerReservations.tsx` `normalizeReservationEntries`) map SDK rows to the storybook display shape — status enum → display enum, ISO timestamp → `dayKey`/`fullDate`/`time` labels, cents → `R$ X,YY`. Live rows replace fixtures when present; empty DB keeps the storybook visual.

## WAR-VOCAB-OPERATIONS-02 — Pending-reviews query not modeled

- **STATUS:** **closed** (Hostpoint commit follow-up 2026-05-16)
- **Symptom:** "Avaliacoes apos a estadia" card on host-operations was a fixture single-review; no SDK existed to list pending reviews nor to write a host reply. Review resource lacked `host_reply` / `host_replied_at` columns.
- **Fix:** trust.lzi gains:
  - `Review.host_reply: Text optional` + `Review.host_replied_at: DateTime optional` (schema additions; new CREATE TABLE emits the columns).
  - `record PendingReviewEntry` (review_id, transaction_id, author display, rating, comment, created_at).
  - `command list_my_pending_reviews_as_host returns JSON handler @fn.list_my_pending_reviews_as_host` — selects unreplied submitted reviews where target = actor.
  - `command leave_host_reply input { review_id, reply } handler @fn.leave_host_reply` — actor-scoped update; only the review target can reply, and only once.
  Front-end `HostOperations.tsx` consumes via `useLazuliCommand(listTrustMyPendingReviewsAsHost)` + `normalizePendingReviews`; live rows replace the storybook fixture when present.
- **Migration note:** existing-DB deployments need an ALTER TABLE for the two new Review columns (framework's CREATE TABLE IF NOT EXISTS doesn't auto-migrate column additions — cf. WAR-RUNTIME-MIGRATION-01). Fresh DBs include them automatically.

## WAR-VOCAB-PROPERTYCREATE-01 — Catalog asset-upload commands not implemented

- **STATUS:** **closed** (Hostpoint commits `b5e3ad4` + `630adf1`, 2026-05-16)
- **Symptom:** host property-create wizard prompted for photos but `request_asset_upload` / `confirm_asset_upload` were unimplemented stubs (HTTP 500). The command output type was also `struct{}` — even when the handler ran it couldn't return a presigned URL via the canonical SDK shape.
- **Fix:** two cooperating changes:
  1. **Handler implementations** (commit `b5e3ad4`): `RequestAssetUpload` inserts an UploadedAsset row in `uploading` status with a deterministic object_key + bucket; `ConfirmAssetUpload` flips the row to `ready` after the browser PUT. Object key path: `<kind>/<yyyy>/<mm>/<random>` so retention can prune by month.
  2. **Typed return record** (commit `630adf1`): catalog.lzi declares `record AssetUploadIntent { asset_id, url, method, headers_content_type, expires_at }` and the command `returns AssetUploadIntent`. The codegen now emits the Go struct, the TS interface, and a typed SDK shape. The handler populates the record so the front-end consumes it directly.
- **Note:** real S3/MinIO presigning needs the `@runtime/storage` PresignedURLWriter adapter to be bound (env-driven via `OBJECT_STORE_ENDPOINT` + bucket + creds). Until then the URL is a dev-mode placeholder so local integration tests flow against the same shape. This is configuration, not a code gap.

## WAR-VOCAB-PAYMENTS-01 — MercadoPago integration not implemented end-to-end

- **STATUS:** **closed** (Hostpoint commits `f50fa5f` + `d984bd5` + `6a372a6` + this cycle, 2026-05-16)
- **Symptom:** the payments BC's three commands (`connect_mercadopago`, `create_checkout_preference`, `refund_charge`) plus the `mp_payment_event` webhook all needed real HTTP integration with MercadoPago. Additionally `create_checkout_preference` returned `struct{}` — the SDK had no `checkout_url` field for the front-end to redirect to. The end-to-end checkout flow could not be exercised in dev mode either: there was no equivalent of MP's redirect lifecycle.
- **Fix:** four cooperating changes:
  1. **Real HTTP client** (commit `f50fa5f`): `dist/go/payments/mp_client.go` (190 LOC) wraps the three MP REST endpoints (`POST /oauth/token`, `POST /checkout/preferences`, `POST /v1/payments/{id}/refunds`) plus an HMAC SHA-256 webhook signature verifier. Constructor `newMpClient()` returns `(client, ok)`; `ok=false` when `MERCADOPAGO_ACCESS_TOKEN` env is unset so consumers branch on env-driven prod/dev selection (no code flag).
  2. **Webhook handler** (commit `6a372a6`): `OnMpPaymentEvent` parses MP's payment envelope, maps the provider status enum to `ChargeStatus`, updates `paid_at` / `refunded_at` columns. Auto-mounted via `webhooks.Register` (lazuli runtime ca7bdf1).
  3. **Real prod calls wired** (commit `d984bd5`): `CreateCheckoutPreference` calls `mp.CreatePreference(...)` in prod path and `RefundCharge` calls `mp.RefundPayment(...)`; both fall through to deterministic dev placeholders otherwise.
  4. **Typed return record + dev-mode parity** (this cycle): `payments.lzi` declares `record PaymentPreference { external_reference, checkout_url }` and `create_checkout_preference returns PaymentPreference`. In prod the URL is MP's `init_point`; in dev the URL is `<app>/payment/dev-checkout?ref=<X>`, a PWA React route (`apps/hostpoint-app/src/routes/PaymentDevCheckout.tsx`) that mimics MP's redirect. A new dev-only command `payments.dev_auto_approve_charge` (handler `dist/go/payments/dev_auto_approve_charge.go`) refuses to run when `MERCADOPAGO_ACCESS_TOKEN` is set — so the bypass cannot reach production. The dev page calls this command on "Pay" to synthesize the same `charge.status = 'approved' + paid_at` transition the real signed webhook would perform.
- **Note:** real-money traffic still needs the four env vars (`MERCADOPAGO_ACCESS_TOKEN`, `MERCADOPAGO_CLIENT_ID`, `MERCADOPAGO_CLIENT_SECRET`, `MERCADOPAGO_REDIRECT_URI`) populated. This is normal SaaS configuration — equivalent to setting `DATABASE_URL` or `SMTP_HOST`. The code path is complete; the dev simulator gives an end-to-end test path without an MP account.

## WAR-VOCAB-MESSAGING-02 — Full ChatExperience component port

- **STATUS:** **closed** (Hostpoint commit `ee2eef8`, 2026-05-16)
- **Symptom:** the storybook `messaging-chat.stories.tsx` rendered a 2654 LOC `ChatExperience` composite component depending on `react-native-svg`. The PWA build didn't carry the SVG dependency and the minimal port (`MessagingChat.tsx` v0) wrapped just an inbox + thread without the rich storybook surface (audio bubbles, reservation proposal cards, presence dots, receipt marks, conversation actions menu, block/delete dialogs, etc.).
- **Fix:** ported the full 2654 LOC ChatExperience with all 38 sub-components into `apps/hostpoint-app/src/features/messaging/presentation/components/ChatExperience.tsx`. Added a 34 LOC SVG shim (`apps/hostpoint-app/src/shared/presentation/lib/svg.tsx`) that re-exports `Svg` + `Path` as plain HTML `<svg>` + `<path>` with the camelCase → kebab-case prop translation React already does for SVG attrs. Zero new npm dependencies. The model files (chat, chat-message, chat-participant, message-status) ported alongside. MessagingChat route rewrite uses `ChatInboxView` + `ChatThreadView` directly.

## WAR-VOCAB-MESSAGING-03 — Denormalized inbox shape not modeled in the SDK

- **STATUS:** **closed** (Hostpoint commit `bf6d2ce`, 2026-05-16)
- **Symptom:** the SDK `Chat` shape returned by `listMineChatsMessagings` was the raw resource (org_id + transaction + participant_a + participant_b + last_message_at + created_at). The ChatExperience UI needs joined counterpart name + avatar + property name + last-message-preview + unread-count to render a usable inbox card. Doing the join client-side would require N+1 round-trips per inbox row.
- **Fix:** messaging.lzi declares `record ChatListEntry` with the denormalized inbox-card shape, plus `command list_chat_inbox returns JSON handler @fn.list_chat_inbox`. Handler issues a single SQL with `LEFT JOIN LATERAL` for last-message-preview + unread-count + counterpart user lookup. JSON-typed because `returns X` in Lazuli is per-record (no list-of-record yet); front-end TS casts to `ChatListEntry[]` using the SDK-emitted interface.

## WAR-VOCAB-OPERATOR-01 — Operator-only queries not authored

- **STATUS:** **closed** (Hostpoint commits `bfcba86` + `8a513fb`, 2026-05-16)
- **Symptom:** Hostpoint OS (operator audience PWA) dashboard had no real data — fixtures only — because no operator-only `query.list` blocks existed.
- **Fix:** 4 new operator-only `query.list` blocks authored across host / catalog / trust: `pending_intermediation_hosts`, `pending_basic_details_hosts`, `pending_property_publications`, `flagged_reviews`. All declarative with `@policy.operator_only` + `paginate 50` + tenancy-scoped `org_id = ctx.actor.org_id`. Operator OS dashboard rewired via `useLazuliQuery`, normalizing the SDK response through a `normalizeQueueItems(unknown)` helper since the SDK return types are still `UploadedAsset[]` per the SDK type drift gap (WAR-VOCAB-HOSTPROPDETAIL-02).

## WAR-VOCAB-PROPERTYEDIT-01 — `UpdateCatalogPropertyInput` only models the create-wizard subset

- **STATUS:** **closed** (Hostpoint commit follow-up 2026-05-16) — `accepted_families` / `accepted_pets` / `house_rules_notes` / `photos[]` / `promotions[]` are deferred to a follow-up that requires schema additions.
- **Symptom:** host-property-edit only round-tripped 7 fields. Address / voltage / water_source / lat-lng were captured in the UI but silently dropped on save. The Property storage schema already had columns for most of those — the gap was only on the input shape.
- **Fix:** `catalog.lzi` extends `command update_property input` with the 11 missing optional fields (`cep`, `street`, `address_number`, `complement`, `neighborhood`, `city`, `state`, `latitude`, `longitude`, `voltage`, `water_source`). The Go handler (`dist/go/catalog/update_property.go`) appends 11 new `if input.* != nil` branches to the partial-UPDATE SQL builder. Front-end `handleSaveCore` now sends the full payload covering all storage-modeled fields. Voltage maps `'both'` → SDK `'v110_220'` per the `VoltageType` enum.
- **Deferred follow-up:** `accepted_families` / `accepted_pets` / `house_rules_notes` need new Property columns; `photos[]` depends on the catalog asset-upload pair flow; `promotions[]` needs a new `Promotion` resource. All of these are catalog-vocab cycles, not edit-form gaps.

## WAR-VOCAB-PROPERTYCREATE-02 — `expo-image-picker` not available on PWA target

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.5; PWA-pivot related per memory `project_hostpoint_pwa_pivot_2026-05-15`)
- **Symptom:** the legacy storybook used `import * as ImagePicker from 'expo-image-picker'` + `requestMediaLibraryPermissionsAsync` + `launchImageLibraryAsync` for the photo picker. On the PWA target (`tanstack-vite` frontend per `Lazurite.toml`) the expo native modules are not built; even when bundled via `react-native-web`, `expo-image-picker` requires `EXImagePicker` native runtime. The UI primitives `CoverPhotoSlot` / `ExtraPhotosSlot` were deleted from the PWA build for the same reason (cf. `apps/hostpoint-app/src/shared/presentation/ui/index.ts` comment block).
- **Workaround in place:** wizard inlines a `pickPropertyPhotosWeb()` helper (`apps/hostpoint-app/src/routes/HostPropertyCreate.tsx`) that creates a hidden `<input type="file" multiple>` via `document.createElement('input')`, fires `.click()`, and resolves a `Promise<PropertyPhoto[]>` from the change event using `URL.createObjectURL(file)` for preview thumbnails. Cover-photo slot accepts 1 file; extras slot accepts up to 9. The `CoverPhotoSlot` / `ExtraPhotosSlot` presentation is also inlined locally (`CoverPhotoSlotWeb` / `ExtraPhotosSlotWeb`) since the canonical primitives were deleted. `Platform.OS === 'web'` guard short-circuits the helper to no-op on non-web targets, which means the wizard will not pick photos when the Expo client is restored.
- **Annotated in:** `apps/hostpoint-app/src/routes/HostPropertyCreate.tsx` (`pickPropertyPhotosWeb`, `CoverPhotoSlotWeb`, `ExtraPhotosSlotWeb`).
- **Removal criterion:** EITHER (a) Lazuli ships a `@runtime/image-picker` or `@plugin/image-picker` cross-platform adapter that abstracts file-input vs `expo-image-picker` vs CameraRoll behind a shared TS interface, OR (b) `CoverPhotoSlot` / `ExtraPhotosSlot` are re-introduced in `shared/presentation/ui/forms/` with platform-specific implementations (`*.web.tsx` vs `*.native.tsx`) the way react-native-web typically splits primitives. Path (b) is more aligned with the existing `react-native-web` layering; path (a) is more aligned with the framework's "wire-thin runtime" thesis. Closing this entry retires both inline `*Web` slots + the inline helper.
- **Surfaced by:** Phase 3.5 host-property-create port (this entry).

## WAR-RUNTIME-MULTITENANT-01 — `@policy.public` commands cannot resolve tenant for `creates`

- **STATUS:** open (workaround in place — Hostpoint Phase 1.3 / 2026-05-16; surfaced in `apps/hostpoint-app/src/routes/SignUp.tsx` + `dist/go/account/register_user.go`)
- **Symptom:** the declarative form `command register_user { policy @policy.public; creates User { ... } }` lowers a generated handler that reads `ctx.Actor` for tenant resolution. Under `@policy.public` there is no authenticated actor at request time, so `ctx.Actor.OrgID` is the zero value; the generated INSERT carries `org_id = 0` and either fails the FK constraint or silently writes to a non-existent org. Every public sign-up path hits this.
- **Workaround in place:** Hostpoint replaces the framework-generated `register_user.go` with a hand-rolled `RegisterUserHandler` (`dist/go/account/register_user.go`) that (a) opens a transaction; (b) `SELECT id FROM org WHERE slug = 'default' LIMIT 1`; (c) `INSERT INTO org` with the default slug if no row found, capturing the new id; (d) issues the `INSERT INTO "user"` with `org_id = <resolved>`. The handler is registered via `account.RegisterUserHandler` in `dist/go/main.go` ahead of the framework-generated stub.
- **Annotated in:** `dist/go/account/register_user.go` (head-of-file comment naming the warrant); `apps/hostpoint-app/src/routes/SignUp.tsx` (UI assumes a single org is fine for the MVP).
- **Removal criterion:** Lazuli grows one of these (cells deferred to the next framework wave, design call pending):
  - **Path A (declarative tenant-resolution on public commands):** new keyword on `command.policy @policy.public` — `resolve tenant via @fn.<name>` or `tenant_from input.<axis>_id` (mirroring the inbound-webhook `tenant_from`/`scope global` pattern that landed 2026-05-16). The framework injects the resolver into the generated `creates` lowering.
  - **Path B (runtime-supplied tenant context for public commands):** add a `WithDefaultTenant` middleware to `lazuli.Mux()` that resolves a fallback `Org` per `request.Header` / `Host` / app config. Less explicit than A; risks silent tenancy bugs.
  - **Path C (require pre-auth for `creates`):** make `@policy.public` + `creates X` a doctor `BLOCK` (analogous to `WEBHOOK-SCOPE-001` for inbound webhooks lacking `tenant_from`). Forces the author to choose A or escalate the command to authenticated.
- **Surfaced by:** Hostpoint Phase 1.3e (sign-up flow); see `docs/port-status-2026-05-16.md` open-workarounds list.

## WAR-VOCAB-QUERY-ENUM-01 — `query.list filters <enum_field> = <literal>` does not bind under codegen

- **STATUS:** open (workaround in place — Hostpoint Phase 3.7 onwards; per `docs/port-status-2026-05-16.md`)
- **Symptom:** declarative form `query.list pending_intermediation_hosts { filters { status = pending } }` parses cleanly under the canonical grammar — the analyzer accepts an enum literal on the RHS of `=` in filter clauses — but the generated pgx query parameter binding does NOT carry the enum-literal-to-text conversion that the column requires. End result: the SQL builds, but the binding emits the unquoted identifier or the wrong type, and the query returns zero rows. Worked-around by either (a) returning the unfiltered set and filtering client-side, or (b) writing a `handler @fn.<name> returns JSON` that does the typed enum filter server-side. The Hostpoint port relies on path (b) for `list_host_agenda` / `list_traveler_reservations` / `list_my_pending_reviews_as_host` / `list_chat_inbox`.
- **Workaround in place:** four `command list_*` blocks (`operations.list_host_agenda`, `operations.list_traveler_reservations`, `trust.list_my_pending_reviews_as_host`, `messaging.list_chat_inbox`) return rich denormalized `JSON` projections via hand-rolled Go handlers; the declarative `query.list` form is intentionally avoided for any list that needs status-filtering. Client-side, the storybook screens that need "pending vs active vs completed" buckets receive ALL transactions and split by status in the React layer.
- **Annotated in:** `app/features/operations/operations.lzi` (record `AgendaEntry` + `ReservationEntry` carry status as a denormalized field); `dist/go/operations/list_host_agenda.go` + sibling handlers (head-of-file comments naming the warrant).
- **Removal criterion:** Lazuli codegen wires enum-literal filter clauses through the IR lowering, the pgx parameter binding, AND the analyzer (must validate the enum literal is a known variant of the field's enum type at compile time, not at runtime). Concretely: `crates/lazuli_codegen_go/src/emitter/query.rs` (or equivalent) must (a) detect `FilterEq { lhs: FieldRef(<enum_field>), rhs: EnumLiteral(<variant>) }`, (b) emit the runtime conversion `pgtype.Text{String: <variant_string>, Valid: true}` for the placeholder bind, (c) ensure the analyzer rejects unknown variants at compile time. Doctor lint `VOCAB-QUERY-ENUM-UNKNOWN-001` candidate post-fix to catch typos.
- **Surfaced by:** Hostpoint Phase 3.7 (host-operations agenda); see `docs/port-status-2026-05-16.md` open-workarounds list.

---

## Meta — appended after this doc was authored

Future workarounds add an entry here following the same shape. If a workaround is removed from the pilot (because Lazuli grew the canonical feature), update STATUS to `closed` + cite the lazuli commit / proposal that fixed it. Do not delete the entry — the history is the framework's velocity log.
