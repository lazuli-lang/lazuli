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
| Vocab — auth | WAR-VOCAB-AUTH-01..02 | sessions list, step-up, terms-versioning |
| Runtime — ctx | WAR-RUNTIME-CTX-01 | ctx.SessionID exposure |
| Runtime — auth blocks | WAR-RUNTIME-AUTH-01 | password-reset / email-verification block declaration |
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

- **STATUS:** open
- **Symptom:** ContaSeguranca screen requires a 6-digit confirmation code on sensitive change (`update_credentials`). Two-command flow: `account.request_step_up_code()` then `account.update_credentials(..., confirmation_code)`. Today this would be handler-side improvisation.
- **Workaround in place:** deferred — settings screens not started.
- **Removal criterion:** Lazuli adds `step_up` or `@hook.requires_step_up` vocabulary so sensitive commands can declare their freshness window + verification requirement.
- **Surfaced by:** AccountFlows.ContaSeguranca + Settings sub-panels.

---

## WAR-RUNTIME-CTX-01 — `ctx.SessionID` / `ctx.SessionToken` not exposed to handlers

- **STATUS:** open
- **Symptom:** `command logout` handler needs to invalidate the current session (`auth.InvalidateSession(ctx, contract, token)`). The raw session token is extracted from the cookie by middleware and used to populate `ctx.User`, but the token itself is not exposed to the handler.
- **Workaround in place:** `Logout()` deletes ALL sessions of the actor instead of just the current one. Semantically "log out of every device" — acceptable for MVP but not the storybook UX.
- **Annotated in:** `dist/go/account/logout.go` inline comment; commit `c6897ee`.
- **Removal criterion:** `lazuli.Ctx` exposes `SessionID lazuli.ID` or `SessionToken string` populated by the middleware after `auth.ResolveSession`. Logout handler then revokes just the current session.
- **Surfaced by:** Phase 4.1 real handler implementations.

## WAR-RUNTIME-AUTH-01 — Email-verification / password-reset blocks need vocab + handler glue

- **STATUS:** open
- **Symptom:** Lazuli runtime has `auth.RequestPasswordReset`, `auth.ConfirmPasswordReset`, `auth.IssueEmailVerificationToken`, `auth.VerifyEmailToken` with `PasswordResetContract` / `EmailVerificationContract` types. But these contracts must be DECLARED in `.lzi` (via `auth password_reset` / `auth email_verification` blocks?) for codegen to emit them. Account.lzi only has `command request_password_reset` / `command verify_email` with `handler @fn.X` — bypasses the canonical auth block path.
- **Workaround in place:** handlers stub `ErrNotImplemented`. Real flow needs either declaring the canonical auth blocks (whose grammar/codegen status is unclear) OR hand-rolling the resource + token gen + email sending in the handler.
- **Removal criterion:** documented grammar for `auth password_reset { resource <X>; ttl <Y>; identity <field> }` and `auth email_verification { ... }` blocks that emit the contract + canonical command + canonical route. Then the handler shrinks to just sending the email.
- **Surfaced by:** Phase 4.2 remaining handlers (request_password_reset, reset_password, verify_email).

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

## WAR-SCAFFOLD-GITIGNORE-01 — `lazuli new` `.gitignore` blanket-ignores `dist/`

- **STATUS:** open (workaround applied at pilot)
- **Symptom:** scaffolded `.gitignore` line `dist/` ignored EVERYTHING including user-authored handler files at `dist/go/<bc>/<name>.go`. Per Lazuli regen contract (`docs/proposals/codegen-lazuli-go.md`), `.gen.go` files are regen-overwritable but non-`.gen.go` files (handlers) are sacred. Blanket-ignore violates the contract.
- **Workaround in place:** Hostpoint `.gitignore` rewritten to ignore only `*.gen.go`, `*.gen.ts`, `*.zod.ts`, `dist/go/{main.go,go.mod,go.sum,migrations/}`, `dist/{ts-web,ts-mobile}/design/`. See Hostpoint commit `1c03f30`.
- **Removal criterion:** `lazuli new` scaffold ships the granular .gitignore by default. Reference Hostpoint commit `1c03f30` for the canonical pattern.
- **Surfaced by:** Phase 1.2 commit `1c03f30`.

---

## Meta — appended after this doc was authored

Future workarounds add an entry here following the same shape. If a workaround is removed from the pilot (because Lazuli grew the canonical feature), update STATUS to `closed` + cite the lazuli commit / proposal that fixed it. Do not delete the entry — the history is the framework's velocity log.
