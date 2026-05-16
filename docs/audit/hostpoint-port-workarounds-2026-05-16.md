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
| Vocab — operations | WAR-VOCAB-OPERATIONS-01..02 | denormalized agenda query, pending-reviews query |
| Runtime — ctx | WAR-RUNTIME-CTX-01 | ctx.SessionID exposure |
| Runtime — auth blocks | WAR-RUNTIME-AUTH-01 | password-reset / email-verification block declaration |
| Runtime — migrations | WAR-RUNTIME-MIGRATION-01..03 | CREATE TABLE IF NOT EXISTS (open); reserved-word columns (CLOSED); FK topo-sort (open) |
| Runtime — command routing | WAR-RUNTIME-COMMAND-01 | Register init blocks + Effect:Returns wiring missing |
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

- **STATUS:** open
- **Symptom:** when a feature's resource references an enum/record/type defined in another feature, `lazuli generate ts` does NOT emit the corresponding `import` line in the consumer feature's `.gen.ts`. Example: `traveler.gen.ts` declared `interface Traveler { gender: Gender; ... }` but never imported `Gender`. `tsc` fails with `Cannot find name 'Gender'`.
- **Workaround in place:** duplicate the enum locally in every feature that references it. For `Gender` this means defining it in `account`, `host`, AND `traveler` (3x DRY violation).
- **Annotated in:** `app/features/{account,host,traveler}/*.lzi` with `# WORKAROUND WAR-CODEGEN-TS-01` markers.
- **Removal criterion:** Lazuli TS codegen tracks cross-bucket type references and emits the matching `import { Gender } from '../account/account.gen'` (or equivalent) in the consumer.
- **Surfaced by:** Hostpoint Phase 1.3e onboarding screens (host BasicDetails + traveler BasicDetails both use `Gender`).

## WAR-CODEGEN-TS-02 — Redundant bucket prefix in SDK function names

- **STATUS:** open
- **Symptom:** `lazuli generate ts` emits commands with the bucket name twice (e.g. `saveHostHostBasicDetails`, `saveTravelerTravelerVehicle`, `completeHostHostOnboarding`). Expected: single bucket prefix (`saveHostBasicDetails`).
- **Workaround in place:** call sites use the verbose name (`saveHostHostBasicDetails`) — every call site repeats the bucket twice.
- **Annotated in:** `apps/hostpoint-app/src/routes/onboarding/host/BasicDetails.tsx` (and 11 other onboarding screens).
- **Removal criterion:** TS codegen deduplicates the bucket prefix when the command name already starts with the BC name.
- **Surfaced by:** Phase 1.3 onboarding agent (cosmetic but pervasive — every onboarding screen).

## WAR-CODEGEN-TS-03 — `dist/ts-web` design tokens not in Tailwind preset

- **STATUS:** open
- **Symptom:** doctor emits 20+ `design-token-undefined` warnings on migrated `shared/presentation/ui/*` files using Tailwind classes like `font-body`, `rounded-hp-sm`, `bg-surface-subtle`, `text-hp-cyan-700`. These classes are defined in `packages/design-tokens` (the Hostpoint workspace package) but not in `design.lzi` — so doctor doesn't know about them.
- **Workaround in place:** warnings accepted. Build is green because Tailwind preset resolves the classes at compile time.
- **Annotated in:** doctor output during Phase 2.3 commit.
- **Removal criterion:** Lazuli `design.lzi` either (a) supports importing tokens from external workspace packages (`@hostpoint/design-tokens`), OR (b) doctor accepts a `[design].extends = ["@hostpoint/design-tokens"]` opt-out for tokens managed outside the capsule.
- **Surfaced by:** Phase 1.3f UI primitives migration (commit `0fed5ad`).

---

## WAR-CODEGEN-XFEAT-01 — `record` types not reusable across features

- **STATUS:** open
- **Symptom:** `record Address` defined in `host.lzi` cannot be referenced from `catalog.lzi.Property` without unverified cross-feature emission semantics. Same issue likely affects `Geolocation`, `Money`, and any value type shared between features.
- **Workaround in place:** `catalog.Property` keeps flat address fields (8 columns: country, cep, street, address_number, complement, neighborhood, city, state). DRY violation across `host.Host` + `catalog.Property`.
- **Annotated in:** `app/features/catalog/catalog.lzi` (commit `b0040fe`); strategic-pivot memory `project_strategic_pivot_2026-05-15` L0 candidates.
- **Removal criterion:** Lazuli emits cross-feature `record` references with stable import + Postgres column emit semantics. Then `Address` lives in a shared feature (or `account`) and both `host` and `catalog` reference it via `Address` or `account.Address`.
- **Surfaced by:** cruel-review 2026-05-16 (`docs/audit` precedent).

## WAR-CODEGEN-XFEAT-02 — `enum` cross-feature reuse same gap as TS-01

- **STATUS:** open (same root as WAR-CODEGEN-TS-01)
- **Symptom:** mirror of WAR-CODEGEN-TS-01 but also affects Go codegen consumers via the resource type signatures.
- **Workaround in place:** duplicate enums in every consuming feature.
- **Removal criterion:** same fix as WAR-CODEGEN-TS-01 + corresponding Go-side import emit.

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

- **STATUS:** open
- **Symptom:** properties like `Property.amenities`, `Property.rules`, `Property.accepted_vehicles`, `Service.options`, `Service.schedule`, `Host.languages`, `Traveler.pets`, `Traveler.languages` semantically should be `Set<Amenity>`, `Set<RuleType>`, `Set<TravelerPet>` etc. Lazuli vocabulary doesn't yet have first-class typed collections, so all 8+ fields are typed as `JSON` (opaque payload).
- **Workaround in place:** `JSON required = "[]"` everywhere. UI deserializes JSON, codegen treats as `unknown` in TS, Go side gets `[]byte`/`pgx.JSONB`. No type safety on what enum variants live inside the set.
- **Bonus exclusive-sentinel gap:** `Traveler.pets` has a "none" sentinel that's mutually exclusive with the others. UI implements toggle logic (`togglePet` helper) manually; Lazuli can't express "set with one exclusive sentinel" yet.
- **Annotated in:** 6+ `.lzi` files (catalog/host/traveler); cruel-review memory entry.
- **Removal criterion:** Lazuli adds `<EnumType>[]` or `Set<EnumType>` first-class. Optionally with `exclusive_sentinel: <variant>` annotation for the Pets case.
- **Surfaced by:** cruel-review 2026-05-16 (6+ JSON fields flagged as soup).

## WAR-VOCAB-SEMANTIC-01 — `@semantic.Money` missing

- **STATUS:** open
- **Symptom:** `Service.price` and `Charge.amount_cents` are typed as `Integer` (cents) + `currency: Text = "BRL"`. No `@semantic.Money(currency: BRL)` semantic carrying decimal-precision intent + currency-locked formatting.
- **Workaround in place:** Integer cents + Text currency. Handler-side math + formatting bear the burden.
- **Annotated in:** `app/features/catalog/catalog.lzi` (Service.price_amount_cents); `app/features/payments/payments.lzi` (Charge.amount_cents + platform_fee_cents + net_to_host_cents).
- **Removal criterion:** Lazuli ships `@semantic.Money(currency: BRL)` semantic type. Postgres emit becomes `NUMERIC(20,4)` with currency constraint; TS becomes a branded Money type with formatting helpers.
- **Surfaced by:** Hostpoint payments BC vocab (commit `612334a`).

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

- **STATUS:** open
- **Symptom:** webhook `mp_payment_event` in a tenant-scoped feature (`payments`) triggers `webhook-tenant-from` warning asking for `tenant_from payload.org_id` OR `scope global` with reason. Trying `scope global / reason "..."` as a webhook child fails parser: `webhook children are path, verify, tenant_from, idempotency by, policy, handler, emits, payload from, replay, retry, dlq, gate behind/quota plan.*`.
- **Workaround in place:** webhook has neither `tenant_from` nor `scope global`. The `webhook-tenant-from` warning persists. Handler must reconcile tenant from `provider_external_reference` lookup at runtime.
- **Annotated in:** `app/features/payments/payments.lzi` commit `612334a` deferred list.
- **Removal criterion:** Lazuli accepts `scope global` (with required `reason`) as a webhook child for cases where the provider does not send a tenant key.
- **Surfaced by:** Hostpoint payments BC (MercadoPago doesn't send tenant context in webhook payload).

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

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 1.3g)
- **Symptom:** Storybook host-personal panel displays a read-only "CNPJ" field with the helper "Dado de verificação. Para alterar, fale com o suporte." But the SDK resource has only `Host.cpf: Text required unique` (cf. `host.lzi`). Brazilian hosts are typically registered as legal entities (PJ) and identified by CNPJ, not CPF — the storybook is the source of truth for the PRODUCT, and `host.lzi` underspecifies.
- **Workaround in place:** `routes/settings/panels/HostPersonal.tsx` displays "CNPJ" with a hard-coded fixture value `32.184.770/0001-58` (storybook fixture). The cached value is forwarded as `cpf` to `saveHostHostBasicDetails` to keep the SDK contract happy. The screen does NOT let the user edit it (read-only per storybook).
- **Annotated in:** `apps/hostpoint-app/src/routes/settings/panels/HostPersonal.tsx`.
- **Removal criterion:** `host.lzi` adds `Host.cnpj: @semantic.BrazilianCNPJ required unique` (depends on WAR-VOCAB-SEMANTIC-02 = `@plugin/scalars-br`) and a `host.request_cnpj_change()` command for the support-mediated alteration flow. Optionally model legal-entity vs natural-person hosts (PJ has CNPJ, PF has CPF — different identity scalars).
- **Surfaced by:** Settings host-personal panel.

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

---

## WAR-DOCTOR-DESIGN-01 — `design-token-hex-leak` on inline-style placeholders

- **STATUS:** partially resolved (commit `0fed5ad` removed most inline hex by migrating to NativeWind classes consuming design tokens; some `_auth-styles.ts`-era inline hex may persist until full storybook fidelity sweep)
- **Symptom:** doctor warned `hex literal '28bbdd' used in an inline style prop. Declare the color in design.lzi and use tokens.color.<name>.base.` 20+ times across early auth screens (before UI primitive migration).
- **Workaround in place:** migration to NativeWind / shared/presentation/ui primitives that consume the `@hostpoint/design-tokens` Tailwind preset.
- **Removal criterion:** all screens consume tokens via the preset; doctor stays silent.
- **Surfaced by:** Phase 1.3e initial auth screens (commit `0c539e6` / `6a3d617`).

## WAR-DOCTOR-DESIGN-02 — `design-token-undefined` on NativeWind tokens not in `design.lzi`

- **STATUS:** open
- **Symptom:** doctor flags `Tailwind class 'font-body' uses prefix 'font' with suffix 'body' not declared in design.lzi`. The token IS declared in `@hostpoint/design-tokens` workspace package (Tailwind preset), but Lazuli doesn't know about external token packages.
- **Workaround in place:** warnings accepted.
- **Removal criterion:** same as WAR-CODEGEN-TS-03 — Lazuli accepts external design-token packages OR provides a doctor opt-out.
- **Surfaced by:** Phase 1.3f UI primitives migration.

## WAR-DOCTOR-ENV-01 — `env-client-exposure` false positive on names already prefixed `PUBLIC_`

- **STATUS:** open
- **Symptom:** `client PUBLIC_MERCADOPAGO_KEY: Text required ...` triggers `client env names should use a 'PUBLIC_' prefix` warning. The name already starts with `PUBLIC_`. Maybe the rule expects an exact prefix match counting underscore separators differently.
- **Workaround in place:** warning accepted.
- **Removal criterion:** doctor rule recognizes `PUBLIC_*` prefix correctly; warning fires only on names lacking the prefix (e.g. `MERCADOPAGO_PUBLIC_KEY` — `PUBLIC` in the middle).
- **Surfaced by:** Phase 2.2-4.1 registry.lzi commit `612334a`.

---

## WAR-VOCAB-NOTIFICATIONS-01 — Inbox query + read-state command not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.1)
- **Symptom:** Storybook `Notifications.Viajante` + `Notifications.Anfitriao` show a per-actor inbox with unread badges + category filters + tap-to-read. `messaging.lzi` has `NotificationDelivery` resource (channel/template_key/payload/status) — sender-side delivery tracking — but no actor-side `query.list mine_notifications` returning a denormalized view (icon, tone, title, body, time, unread) and no `command mark_notification_read(id)` to toggle the unread state.
- **Workaround in place:** `routes/Notifications.tsx` hard-codes 4 traveler + 4 host notification fixtures matching the storybook bytes-for-bytes. `markRead` is local-state-only.
- **Annotated in:** `apps/hostpoint-app/src/routes/Notifications.tsx`.
- **Removal criterion:** `messaging.lzi` adds `query.list mine_notifications` (denormalized actor-side view, with role-gated `@policy.authenticated`) + `command mark_notification_read(id)` declarative `updates NotificationDelivery`. Screen then consumes via `useLazuliQuery / useLazuliCommand`.
- **Surfaced by:** Notifications storybook pattern (Viajante + Anfitriao).

## WAR-VOCAB-HOSTHOME-01 — `host.query.my_host` SDK return type is wrong + missing `full_name`

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.2)
- **Symptom:** the generated `lookupHostByMyHost` query in `dist/ts-web/host/host.gen.ts` is declared with the wrong response type — it returns `IntermediationTermsAcceptance` instead of `Host`. As a result, consuming the query gives the caller a record with `version` / `accepted_at` fields but no access to `Host.full_name`, the field the host-home greeting (`Ola, <first-name>`) needs. Likely related to WAR-CODEGEN-TS-02 (bucket-prefix issue) but the broken return type is a separate codegen bug.
- **Workaround in place:** `apps/hostpoint-app/src/routes/HostHome.tsx` keeps `hostName` as a module-local fixture matching storybook (`'Lucas Silva'`). Same pattern as `routes/settings/HostAccountHome.tsx` (host settings home, which already uses a `MOCK` constant per WAR-VOCAB-AUTH-04).
- **Annotated in:** `apps/hostpoint-app/src/routes/HostHome.tsx`.
- **Removal criterion:** fix `lazuli generate ts` to emit the correct return type for `host.query.my_host` (should be `Host`, not `IntermediationTermsAcceptance`). Add the same fix-path to the lookup-query codegen logic so all `query.lookup` declarations resolve their actual resource type. Then call sites can use `useLazuliQuery(lookupHostByMyHost, {})` and read `data.full_name`.
- **Surfaced by:** Phase 3.2 host-home port (this entry).

## WAR-VOCAB-HOSTHOME-02 — Account-pendings query not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.2)
- **Symptom:** Storybook host-home shows a "Pendencias" section listing global account blockers (configure-receivables, complete-profile, first-property nudge, payments-review). Each item has a typed tone (`brand` / `success` / `warning` / `danger`), an icon, title, subtitle, and a CTA label. No corresponding `account.query.list_mine_pendings` or `account.query.account_health` is authored in `account.lzi` / `host.lzi` — the pendings are a synthesis of multiple feature signals (MP-account-connected flag from `payments`, profile-completion flag from `account`, has-published-properties flag from `catalog`).
- **Workaround in place:** `routes/HostHome.tsx` inlines a `FIXTURE_DATA` constant matching the storybook `activeData` state byte-for-byte. CTAs route to `/account/host` as a safe destination until the underlying flows are ported.
- **Annotated in:** `apps/hostpoint-app/src/routes/HostHome.tsx`.
- **Removal criterion:** Lazuli adds a cross-feature aggregation primitive (`derived` view or `query.list mine_pendings` with `union` over feature-local pendings sources) capable of expressing "list of typed action items derived from feature flags". Alternatively each feature emits its own pendings query (`payments.query.mine_pendings_payments`, `account.query.mine_pendings_account`, etc.) and the host-home screen merges them client-side — heavier-handed but unblocks the port.
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

- **STATUS:** open (workaround applied at pilot)
- **Symptom:** scaffolded `.gitignore` line `dist/` ignored EVERYTHING including user-authored handler files at `dist/go/<bc>/<name>.go`. Per Lazuli regen contract (`docs/proposals/codegen-lazuli-go.md`), `.gen.go` files are regen-overwritable but non-`.gen.go` files (handlers) are sacred. Blanket-ignore violates the contract.
- **Workaround in place:** Hostpoint `.gitignore` rewritten to ignore only `*.gen.go`, `*.gen.ts`, `*.zod.ts`, `dist/go/{main.go,go.mod,go.sum,migrations/}`, `dist/{ts-web,ts-mobile}/design/`. See Hostpoint commit `1c03f30`.
- **Removal criterion:** `lazuli new` scaffold ships the granular .gitignore by default. Reference Hostpoint commit `1c03f30` for the canonical pattern.
- **Surfaced by:** Phase 1.2 commit `1c03f30`.

---

## WAR-VOCAB-HOSTPROPDETAIL-01 — Denormalized property-detail read not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.4)
- **Symptom:** the storybook host-property-detail screen renders, for a single property: name, type-label, cover photo, gallery thumbnails (with `+N` overflow), street address, description, chip groups for amenities / house rules / accepted vehicles, plus a denormalized list of linked services (icon, name, summary, price, active flag). The generated SDK exposes `lookupCatalogByPropertyDetail` returning `UploadedAsset` (clearly the wrong type — likely the same bug class as WAR-VOCAB-HOSTHOME-01 where lookup-style queries lose their resource type during codegen). Even if the type were fixed to `Property`, the raw resource exposes `amenities` / `rules` / `accepted_vehicles` as `unknown` JSON columns plus FK ids for photos and services. Mapping each id-shaped amenity / rule / vehicle to its `{ label, icon, ink }` display tuple requires a static catalog that does not exist server-side; rendering the photo gallery needs the joined `UploadedAsset.public_url` set; rendering the linked services list needs each `Service` joined with its `category` / `cover_photo` / first-option `price_amount_cents`.
- **Workaround in place:** `apps/hostpoint-app/src/routes/HostPropertyDetail.tsx` inlines a `FIXTURE_PROPERTY` constant matching the storybook `publishedData` byte-for-byte. The amenity / rule / vehicle catalogs (label + icon for each enum value) are inlined as module-local `*_META` constants until Lazuli grows either a `@semantic.LabeledEnum` primitive or a per-feature `display_catalog` block authored alongside the enum.
- **Annotated in:** `apps/hostpoint-app/src/routes/HostPropertyDetail.tsx` header comment.
- **Removal criterion:** `catalog.lzi` adds a `query.lookup property_detail(id) -> PropertyDetail` returning a denormalized record with: name, type_label, cover_url, photo_urls[], address (street-formatted), description, amenities[]{id, label, icon}, rules[]{id, label, icon}, accepted_vehicles[]{id, label, icon}, services[]{id, name, summary, price_formatted, icon_key, accent_key, is_active}, status. Companion `display_catalog` per enum to source labels/icons declaratively; LSP knows how to surface them.
- **Surfaced by:** Phase 3.4 host-property-detail port (this entry).

## WAR-VOCAB-HOSTPROPDETAIL-02 — Catalog command return type is `UploadedAsset`

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.4)
- **Symptom:** every command in `dist/ts-web/catalog/catalog.gen.ts` is typed as `defineCommand<*Input, UploadedAsset>` — including `publishCatalogProperty`, `unpublishCatalogProperty`, `deleteCatalogProperty`, `createCatalogProperty`, `updateCatalogProperty`, `createCatalogService`, `publishCatalogService`, `unpublishCatalogService`, `deleteCatalogService`, `updateCatalogService`, `createCatalogCustomServiceCategory`, `deleteCatalogCustomServiceCategory`, and the asset-upload command pair. The mutation success payload should be the affected resource (`Property` for property commands, `Service` for service commands, `CustomServiceCategory` for category commands, `UploadedAsset` for the upload pair only). Looks like the codegen path that resolves the return type for commands collapses to the bucket's last-defined record across the bucket, picking up `UploadedAsset` as the universal answer.
- **Workaround in place:** mutation callers ignore the success payload (or do not consume `data` from the `useLazuliCommand` result). For `HostPropertyDetail.tsx` the `onSuccess` callbacks only flip local UI state — no read from the response.
- **Annotated in:** `apps/hostpoint-app/src/routes/HostPropertyDetail.tsx` header comment.
- **Removal criterion:** TS codegen resolves the per-command success-payload type from the declared `command` block's `effect: returns <Resource>` annotation (or the implicit `Resource` from `updates`/`creates`). Then `publishCatalogProperty` returns `Property`, `createCatalogService` returns `Service`, etc.
- **Surfaced by:** Phase 3.4 host-property-detail port (this entry).

## WAR-VOCAB-HOSTPROPDETAIL-03 — Route param type mismatch with `ID = number`

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.4)
- **Symptom:** `@lazuli/runtime` declares `type ID = number` (numeric primary keys, Postgres `serial`/`bigserial`). URLs carry IDs as strings (`/host/properties/:id`). When a route component reads the param via `useParams` and passes it to a mutation typed `{ property_id: ID }`, TypeScript correctly rejects the string. Today every call site does `Number(params.id)` which loses the type contract (non-numeric strings become `NaN`).
- **Workaround in place:** explicit `Number(...)` coercion at the call site. The numeric-string contract is enforced only at runtime.
- **Annotated in:** `apps/hostpoint-app/src/routes/HostPropertyDetail.tsx` inline comment.
- **Removal criterion:** either (a) Lazuli ships a branded `BrandedID` (string-typed for URL safety + opaque-int for storage) that the SDK accepts uniformly, OR (b) `useLazuliCommand` accepts a string and the wire layer coerces; OR (c) `tanstack-router` route definitions get a typed `parseParams` hook the SDK can hook into. Either path closes the leak.
- **Surfaced by:** Phase 3.4 host-property-detail port (this entry).

---

## WAR-VOCAB-OPERATIONS-01 — Agenda needs a denormalized actor-side query

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.7)
- **Symptom:** the storybook host-operations agenda renders each reservation as a card with: traveler name + photo, property name, formatted date + time slot, service-level breakdown (icon, name, amount per service), and an aggregate total amount. The generated SDK query `listMineTransactionsAsHostOperationss` (cf. `dist/ts-web/operations/operations.gen.ts`) returns the raw `ServiceTransaction` resource (FK ids, JSON `proposed_options`, `total_amount_cents`) — none of the joined display attributes the storybook card needs. Translating the raw shape into the card would require additional N+1 lookups per row (traveler/property/service) plus client-side formatting.
- **Workaround in place:** `apps/hostpoint-app/src/routes/HostOperations.tsx` inlines a fixture array matching the storybook `agendaItems` constant byte-for-byte (4 reservations × pending/proposal/confirmed/declined statuses). The traveler-side surface `apps/hostpoint-app/src/routes/TravelerReservations.tsx` (Phase 3.3, 2026-05-16) inlines its own fixture array of 5 reservations × pending/proposal/confirmed/cancelled statuses for the same reason — `listMineTransactionsAsTravelerOperationss` returns the raw resource without joined property + service display fields (property name + photo, service breakdown with icon/accent/option-name, formatted dates).
- **Annotated in:** `apps/hostpoint-app/src/routes/HostOperations.tsx` header comment; `apps/hostpoint-app/src/routes/TravelerReservations.tsx` header comment (traveler-side surface, same denormalization gap).
- **Removal criterion:** `operations.lzi` adds a `query.list mine_agenda_as_host()` (or equivalent) that returns a denormalized agenda row type with traveler/property/service display attributes already joined and pre-formatted: `traveler_name`, `traveler_photo_url`, `property_name`, `formatted_date` (e.g. `"Hoje"`), `formatted_full_date` (`"02/05/2026"`), `formatted_time` (`"11:00"`), `service_breakdown: [{ name, amount_cents, icon_key, accent_key }]`, `total_amount_cents`, plus `status` and `hours_until_start`. Companion `query.list mine_reservations_as_traveler()` ships the traveler-side counterpart with `property_name`, `property_photo_url`, `property_location`, and per-service `option_name` in addition. Both queries are actor-side denormalized views of the same `ServiceTransaction` stream the sender-side `listMineTransactionsAsHostOperationss` / `listMineTransactionsAsTravelerOperationss` queries already expose. Same architectural shape as the request for `query.list mine_notifications` in WAR-VOCAB-NOTIFICATIONS-01.
- **Surfaced by:** Phase 3.7 host-operations port (this entry); Phase 3.3 traveler-reservations port (2026-05-16, traveler-side surface).

## WAR-VOCAB-OPERATIONS-02 — Pending-reviews query not modeled

- **STATUS:** open (workaround applied 2026-05-16 — Hostpoint Phase 3.7)
- **Symptom:** the storybook host-operations surface includes a "Avaliacoes apos a estadia" card prompting the host to respond to a review left after a completed transaction. The trigger is conceptually tied to operations (it's the post-completion follow-up) and the storybook surfaces it inline with the agenda. There is no `query.list mine_pending_reviews_as_host()` (or equivalent) authored — no Review/Rating resource exists in the current Lazuli capsule at all, and the operations BC only models the transaction lifecycle (request → proposal → accepted → paid → completed → cancelled).
- **Workaround in place:** `apps/hostpoint-app/src/routes/HostOperations.tsx` inlines a single pending-review fixture matching the storybook reviews data (Maria Costa, 5 stars). The "Responder avaliacao" button is non-functional.
- **Annotated in:** `apps/hostpoint-app/src/routes/HostOperations.tsx` header comment.
- **Removal criterion:** a Review BC (or `trust.Review`) ships with `Review` resource (transaction reference, rating, comment, host_reply, status) + `query.list mine_pending_reviews_as_host()` actor-side query + `command leave_host_reply(review_id, reply)`. The operations agenda then consumes the query via `useLazuliQuery` to surface the prompt. Architecturally this overlaps with the `@trust` bucket (existing scaffold) but no resources are currently authored there.
- **Surfaced by:** Phase 3.7 host-operations port (this entry).

---

## Meta — appended after this doc was authored

Future workarounds add an entry here following the same shape. If a workaround is removed from the pilot (because Lazuli grew the canonical feature), update STATUS to `closed` + cite the lazuli commit / proposal that fixed it. Do not delete the entry — the history is the framework's velocity log.
