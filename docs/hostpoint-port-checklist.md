# Hostpoint Port — Sub-Checklist

**Source analysis**: `docs/audit/framework-coverage-1400.md` + análise read-only de `c:/Users/lucas/hostpoint/` + `c:/Users/lucas/dev/flutter-hostpoint/`.
**Última revisão**: 2026-05-12.

> **Audit checkpoint (2026-05-12)**: este arquivo é o checklist de
> **portabilidade do Hostpoint em cima do Lazuli**, não uma lista de bugs do
> app Hostpoint original. Ele mistura pré-requisitos Lazuli (`Phase Prep`) e o
> port do produto Hostpoint (`Phase 1+`). Depois das batches c21-c180, a infra
> Lazuli avançou muito e o smoke do codegen Go está verde, mas o produto
> Hostpoint real ainda não foi portado. Use `docs/next-checklist.md` rows
> 60-81 como ledger de execução. Gate atual: `cargo test -p
> lazuli_codegen_go --features smoke` passa; próximo gate é rodar um happy
> path gerado de `examples/hostpoint-mini/` e iniciar Phase 1 no source real.
> Rows 78-81 registram 180 helpers adicionais de framework/Lazurite; continuam
> fora do escopo de port do produto Hostpoint.

## Princípio fundador

**Lazuli é abstração; Lazuli Go é wire.** O runtime Go **não reimplementa** primitivas que já existem em Go stdlib / extended / SDKs maduros. O trabalho de "Lazuli Go real" é:

1. **Codegen Lazuli → Go**: emitter pega o IR (tipado, Tier 1-4 done) e produz `dist/go/<feature>/*.gen.go` que **importa** e **chama** libs existentes.
2. **Adapter wiring**: cada "stub" em `runtime/go/lazuli/<bucket>/` vira wire de ~10-50 LOC para a lib Go correspondente.

**Não fazer**: reimplementar argon2, HMAC, OAuth2, pgx pool, JWT, OTEL exporter, S3 protocol, River dispatcher, chi router, slog handlers, sendgrid API. Tudo já existe — declarar + chamar.

---

## §1. Phase Prep — Pré-requisitos Lazuli antes do port iniciar

**Estimativa total: ~13-17 cells.** (Revisado pra baixo após correção: Lazuli Go wire ≠ implementação from scratch.)

**Status audit 2026-05-12**: Phase Prep está fechada para o gate de codegen
compilável/gofmt-clean e majoritariamente fechada no runtime Lazuli. Confirmado
verde: `go test ./...` em `runtime/go`, `cargo test -p lazuli_codegen_go`,
`cargo test -p lazuli_cli`, `cargo test -p lazuli_codegen_go --features smoke`
e `lazuli check examples/hostpoint-mini`. O port real do produto Hostpoint ainda
não começou; `hostpoint-mini` é playground de forma e smoke, não migração do app.
Depois das batches c181-c360, o foco continua sendo **framework Lazuli/Lazurite**:
HTTP/DB/testkit/security/cache/email/jobs/storage/search/realtime/OpenAPI/i18n/
reports/deploy/perf/authz/docgen/auth/events/admin/deploy/billing/views/rpc
hardening ganharam helpers, mas nenhum source real do Hostpoint foi portado.
Itens de produto real permanecem abertos quando só existe fixture/runtime.

### 1.1 Codegen Lazuli → Go (Gate fechado — ~12-15 cells)

Emitter que consome IR (já tipado via Phase L Tier 1-4) e produz Go que compila.

- [x] `lazuli generate go --out dist/go/` CLI verb funcional
- [x] Templates v0 por kind (gerados a partir do IR; smoke verde):
  - [x] `dist/go/<feature>/resource.gen.go` — structs + resource contract + db/json tags
  - [x] `dist/go/<feature>/command.gen.go` — typed input/output contract + handler stubs
  - [x] `dist/go/<feature>/query.gen.go` — list/lookup/sql contracts + cache metadata
  - [x] `dist/go/<feature>/api.gen.go` — route contract + handler placeholder binding
  - [x] `dist/go/<feature>/auth.gen.go` — identity/password/oauth/session/MFA contracts
  - [x] `dist/go/<feature>/job.gen.go` — River-compatible job contracts + registration surface
  - [x] `dist/go/<feature>/webhook.gen.go` — receiver/HMAC/retry/DLQ contract emission
  - [x] `dist/go/<feature>/notification.gen.go` — channel/digest/throttle contract emission
  - [x] `dist/go/<feature>/storage.gen.go` — file contract + signed/direct upload metadata
  - [x] `dist/go/<feature>/translation.gen.go` — embed catalog loader + placeholder catalog file
- [x] Build script/harness roda `gofmt` no output — `full_capsule_generated_go_is_gofmt_clean`
- [x] Smoke test: codegen `examples/full-capsule/` → `go build ./dist/go/...` passa
- [ ] Integration test: codegen + Lazuli Go local + sqlite mock executa happy-path login

### 1.2 Lazuli Go wire (Quase fechado — ~1.5-2 cells totais)

Cada item abaixo é **~10-50 LOC** de wire da lib pra dentro do runtime `runtime/go/lazuli/<bucket>/`. **Não é implementação** — é importar + chamar.

#### Auth
- [x] `golang.org/x/crypto/argon2` — wire em `auth/password.go` (~10 LOC: Argon2id IDKey com parâmetros canônicos)
- [x] `golang.org/x/crypto/bcrypt` — fallback para migração de legacy (~5 LOC)
- [x] Session store: Postgres table via pgx (~30 LOC; pode usar `riverqueue` lib do River pra inspiração de schema-managed)
- [x] `golang.org/x/oauth2` + `golang.org/x/oauth2/google` — OAuth Google wire (~20 LOC)
- [x] `github.com/pquerna/otp/totp` — TOTP MFA wire (~10 LOC)

#### Storage
- [x] `github.com/aws/aws-sdk-go-v2/service/s3` + `s3/v1` presigner — wire upload/signed URL (~30 LOC)
- [x] Local filesystem adapter (~20 LOC)
- [x] (futuro) `github.com/minio/minio-go/v7` se quiser MinIO support

#### Jobs
- [x] `github.com/riverqueue/river` + `riverpgxv5` — wire dispatcher + retry policy (~40 LOC)
- [x] DLQ handler hook (River nativo via `Worker.NextRetry`)

#### DB
- [x] `github.com/jackc/pgx/v5` + `pgxpool` — wire connection pool em `db.go` (~30 LOC)
- [x] `withTx` helper já existe em `runtime/go/lazuli/db.go` — completar com retry on serialization failure

#### HTTP
- [ ] `github.com/go-chi/chi/v5` — wire router em `http.go` (runtime usa `net/http` `ServeMux` hoje; decisão de chi ainda aberta)
- [x] Middleware: recover, request-id, otel, csrf via Go 1.26 `net/http.CrossOriginProtection`

#### Observability
- [x] `log/slog` (stdlib) — wire JSON handler em `observability/logging.go` (~15 LOC)
- [x] `go.opentelemetry.io/otel/sdk/trace` + `otlptracehttp` exporter — wire em `observability/tracing.go` (~25 LOC)
- [x] `getsentry/sentry-go` — wire panic/error reporter

#### Email
- [x] `github.com/sendgrid/sendgrid-go` — primary (~20 LOC)
- [x] SMTP stdlib (`net/smtp`) — fallback (~15 LOC)

#### Webhooks
- [x] HMAC verify usando `crypto/hmac` + `crypto/sha256` (stdlib) — wire genérico (~10 LOC)

### 1.3 Codegen consume Tier 4 IR completo

- [x] `Command.audit/approval/invalidates/external_calls` typed (já no IR) → codegen emite middleware + Postgres audit insert
- [ ] `Resource.retention` → codegen emite cron job de anonymization — metadata/intent now emitted; cron job still open
- [ ] `Field.derived_from` → codegen emite computed column ou GENERATED ALWAYS AS — DDL intent comments emitted; generated column still open
- [x] `CapabilityRef::Hashed/Encrypted/Token` → codegen emite import + chamada da lib correspondente

---

## §2. Features-core do Hostpoint mapeadas

**Leitura dos checkboxes desta seção**: itens marcados como feitos são
capacidades/modelos já representados em Lazuli ou em `examples/hostpoint-mini/`.
Eles não significam que os dados/telas/serviços do Hostpoint real foram
migrados. A migração real começa em §3 Phase 1.

### 2.1 Auth & User (Phase 1 port — 6-8 cells)

- [x] `feature auth_hostpoint` shape in `hostpoint-mini` (`feature account`):
  - [x] `auth identity User.email`
  - [x] `auth password algorithm argon2id hash @fn.hash_password verify @fn.verify_password rate_limit "5 per 10 minutes per ip"`
  - [ ] `auth oauth google adapter @adapter.google_oauth` — runtime Google OAuth exists; mini fixture still password-first
  - [x] `auth sessions resource Session ttl "30 days" refresh true`
- [ ] `resource User` production migration (Firestore `auth_users` still not ported):
  - [x] `email: @semantic.Email @pii.contact required unique`
  - [x] `name: Text required`
  - [x] `role: UserRole required` (enum guest|host|admin in mini; traveler/host final naming pending)
  - [ ] `fcm_token: Text optional` (mobile push)
  - [ ] `profile_photo: @cap.File(visibility=public, max_size=5mb, accept=image/*) optional`
  - [ ] `tenancy by user_id` (single-tenant — sem org)
- [x] `resource Session` shape (replace SharedPreferences + token refresh loop):
  - [x] `user: User required on_delete cascade`
  - [x] `expires_at: DateTime required`
  - [x] `refresh_token_hash: @cap.Hashed(algorithm:argon2id) required`
- [ ] Commands:
  - [x] `register` (email+password+role)
  - [x] `login` (email+password → session)
  - [x] `logout` (invalidate session)
  - [x] `request_password_reset` (magic link via email)
  - [x] `reset_password` (token + new password)
  - [ ] `enable_mfa` (TOTP setup)
- [ ] Policies:
  - [x] `@policy.same_user` equivalent (`@scope.self`) for profile fields
  - [x] `@role.traveler` / `@role.host` role-based dispatch shape (`guest`/`host` in mini)

### 2.2 Property & Service (Phase 2 port — 17-21 cells)

- [ ] `resource Property` production migration:
  - [x] `host: User required on_delete cascade` shape (`owner: User`) in mini
  - [x] `name: Text required` shape (`title: Text`) in mini
  - [x] `description: Text optional`
  - [x] `address: Text required` shape (`address` optional + `city`/`country`) in mini
  - [x] `coordinates: GeoPoint required` via `@semantic.GeoPoint`
  - [ ] `photos: many Photo` (cap_file array)
  - [x] `amenities: Text[]` shape present as `amenities: Text optional`; array cardinality still open
  - [ ] `rules: Text optional`
  - [ ] `tenancy by host_id`
  - [ ] `soft_delete; retention 7y then anonymize` — `soft_delete` present; retention metadata path exists, anonymization job open
- [ ] `resource Service` production migration:
  - [x] `property: Property required on_delete cascade`
  - [x] `host: User required on_delete cascade`
  - [ ] `category: ServiceCategory required` (enum)
  - [ ] `name: Text required` — mini models booking service rather than sellable service listing
  - [x] `price_cents: Integer required` (não Float — sempre cents) shape via `amount: @semantic.Money`
  - [x] `currency: @semantic.Currency required` (default BRL)
  - [ ] `photos: @cap.File(visibility=public, max_size=3mb, accept=image/*)[3]` (max 3)
  - [ ] `available_hours: TimeRange[]`
- [x] Commands CRUD/lifecycle shape pra ambos in mini (`create`, `update_listing`, `archive`, booking commands)
- [ ] Queries:
  - [x] `properties.list filter ... search params.q over name, description` shape via mini list/search queries
  - [x] `properties.lookup by_id`
  - [ ] `properties.search_by_radius(lat, lng, radius_km)` ← **PostGIS ou Haversine**
- [ ] Indexes (mapeado de `firestore.indexes.json`):
  - [ ] `(host_id, created_at desc)`
  - [ ] `(category, price_cents)`
  - [x] `GIST (coordinates)` para PostGIS radius search — codegen emits GiST for `@semantic.GeoPoint`

### 2.3 Geolocation (NEW — bucket separado)

**Decisão estratégica**: maps + geocoding ficam em **adapter packs**, não Lazuli core. Geo primitives em IR são closed-set.

- [x] IR: adicionar `BuiltinType::GeoPoint` (lat+lng tuple) e/ou `@semantic.Latitude` + `@semantic.Longitude`
- [x] Storage: PostGIS extension nativa (geo queries no Postgres direto, sem provider externo)
  - [x] Codegen emite `CREATE EXTENSION IF NOT EXISTS postgis;` em migration
  - [x] Codegen emite `GEOGRAPHY(POINT, 4326)` para `GeoPoint` columns
  - [x] Runtime helper emite `ST_DWithin` predicate; query.codegen dedicado `search_by_radius` ainda fica para Phase 2
- [x] Adapter `@runtime/google_maps` (geocoding endereço → coords; uma operation): wire `googlemaps.github.io/maps-services-go` (~20 LOC)
- [ ] Adapter alternativo `@runtime/mapbox` (mesmo contract) — request/normalization helpers exist; full adapter wiring remains open
- [ ] Adapter alternativo `@runtime/nominatim` (OpenStreetMap, gratuito; ~30 LOC) — request/normalization helpers exist; full adapter wiring remains open
- [x] `requires integration maps: MapsProvider` em features que precisam geocoding
- [x] **Não** wirar map rendering — isso é client-side (Flutter `google_maps_flutter` ou Expo `react-native-maps`); Lazuli core não toca UI rendering

### 2.4 Transactions & Payment (Phase 4 port — 7 cells)

- [ ] `resource ServiceTransaction`:
  - [ ] `traveler: User required on_delete restrict`
  - [ ] `host: User required on_delete restrict`
  - [ ] `service: Service required on_delete restrict`
  - [ ] `status: TransactionStatus required` (enum pending|completed|cancelled|refunded)
  - [ ] `amount_paid_cents: Integer required`
  - [ ] `mercadopago_payment_id: Text optional unique`
- [x] `requires integration payment_gateway: PaymentGateway`
- [x] App binding `payment_gateway = integrations.mercadopago` em `app.lzi`
- [x] `registry.lzi`: `integrations.mercadopago` typed
- [ ] Adapter pack `@runtime/mercadopago`:
  - [ ] OAuth wire (~30 LOC)
  - [ ] Token refresh wire (~15 LOC)
  - [x] Webhook signature verify (~20 LOC; usa HMAC SHA256)
  - [x] Create preference API call (~25 LOC)
- [x] `webhook mercadopago_callback` com `verify @validator.mercadopago_hmac tenant_from payload.external_reference`
- [ ] `workflow transaction_lifecycle` no resource:
  - [ ] pending → completed (em webhook approved)
  - [ ] pending → cancelled (timeout 24h ou user cancel)
  - [ ] completed → refunded (admin command)

### 2.5 Reviews (Phase 4 port — 2 cells)

- [ ] `resource Review` production migration:
  - [x] `reviewer: User required on_delete cascade`
  - [x] target fields shape (mini uses `service` + `property` refs instead of polymorphic `target_id`/`target_type`)
  - [x] `stars: Integer required` (1-5)
  - [x] `text: Text optional max 1000`
- [x] Commands:
  - [x] `create_review` shape
- [x] Queries:
  - [x] `reviews.list_by_target(target_id, target_type)` shape via property/status list query
  - [ ] `query.sql ./queries/rating_aggregate.sql` (avg stars + count, scope_by target)

### 2.6 Chat & Messaging (Phase 3 port — 6 cells)

- [ ] `resource Chat` production migration:
  - [x] participants shape via `guest` + `host` refs
  - [ ] `last_message_at: DateTime optional`
- [x] `resource Message`:
  - [x] `chat: Chat required on_delete cascade`
  - [x] `sender: User required on_delete restrict` shape (`author: User`)
  - [x] `body: Text required`
  - [x] `sent_at: DateTime required defaults now`
  - [ ] `read_at: DateTime optional`
- [x] Commands:
  - [x] `send_message` emits `event message_sent`
  - [ ] `mark_message_read`
- [x] Queries:
  - [x] `messages.list_by_chat(chat_id)` (paginated, com cache 30s)
- [ ] `event_group message_*` (replicar pattern Phase L Tier 3) — `message_sent` event exists; explicit group still open
- [ ] **MVP**: polling — Expo app re-fetch `messages.list_by_chat` every 2s
- [ ] **Future (Cut realtime gated)**: `channel chat_messages` + `subscription` + WebSocket (proposal pronto)

### 2.7 Notifications (Phase 3 port — 2 cells)

- [ ] `notification new_message_email` production template:
  - [x] `trigger event chat.message_sent`
  - [x] `channel email`
  - [x] `recipient input.recipient_email` shape via notification queue recipient
  - [ ] `template "./templates/new_message.<locale>.tmpl"`
  - [x] `throttle max_per "1 hour" per_recipient burst 3` (runtime throttle helpers exist)
- [ ] `notification booking_confirmed` production template:
  - [x] `trigger event payment.transaction_completed` shape via payment events
  - [x] `channel email, push`
  - [ ] `template "./templates/booking_confirmed.<locale>.tmpl"`
- [ ] Channels:
  - [x] Email: Sendgrid adapter
  - [x] Push: Expo Push adapter (`runtime/go/lazuli/notifications/expo.go`)

### 2.8 i18n (já mostly done — 1 cell pra port)

- [ ] `app.locale default "pt-BR" supported "pt-BR", "en-US"` (legacy hostpoint só pt-BR; novo Expo bilíngue)
- [ ] `translation hostpoint_messages` em `customer_auth` feature
- [x] `locale_negotiate source accept_language strategy best_match` em runtime unit
- [x] Templates email com `<locale>` token

### 2.9 Observability (já done na Lazuli — 0 cells port)

- [x] Built-in trace events `command_run`/`job_run`/`webhook_run`/`agent_run` (Lazuli já entrega)
- [x] `app.logging level info format json redact pii.*`
- [x] `app.tracing propagate true sample_rate 0.1 exporter otlp`
- [x] Sentry adapter wire (~15 LOC em Lazuli Go)

### 2.10 File Storage (já mostly done — wire S3 em Lazuli Go)

- [ ] Property/Service photos: `@cap.File(visibility=public, max_size=5mb, accept=image/*)`
- [ ] User profile photo: idem
- [ ] Optional docs privadas: `@cap.File(visibility=signed, signed_ttl="24h", max_size=10mb)`
- [x] Runtime expõe helper HTTP para direct upload S3/local; codegen endpoint específico por feature ainda é Phase 2

### 2.11 Search (MVP simples, avançado deferred)

- [x] **MVP**: `query.list filter search params.q over name, description` (LIKE SQL — já cobre L1)
- [ ] **Phase 6+ (Cut search gated)**: Meilisearch adapter quando volume justificar (~10K+ properties)

---

## §3. Roadmap concreto de port

### Phase Prep (framework antes do port)

- [x] **Codegen real** emite `dist/go/<feature>/*.gen.go` que compila
- [ ] **Lazuli Go wire** (10+ libs Go): argon2 + pgx + chi + River + S3 + slog + OTEL + sendgrid + oauth2 + totp — all except chi decision are wired/tested
- [ ] **Smoke test**: `examples/full-capsule/` → `go build` → roda em Docker compose local — `go build` smoke green; Dockerfile/Compose helpers exist; local app run still open
- [x] **Decision gate**: codegen + Lazuli Go runnable até 2026-06-01? Proceed for framework structuring; product port remains deferred.

### Phase 1 — Auth Port (deferred até Lazurite/framework coeso)

- [ ] Port firebase_auth → Lazuli `auth` block — modeled in `hostpoint-mini`; real source migration open
- [ ] Port Firestore `auth_users` → `resource User` + `Session` — modeled in `hostpoint-mini`; data migration open
- [ ] Migrate rules → `@policy.same_user` + `@role.traveler/host`
- [ ] Commands: register/login/logout/reset/mfa
- [ ] Golden eval login_password + login_oauth
- [ ] Lazuli Go executa login real (argon2id wire) end-to-end — password/session helpers exist; generated happy path still open

### Phase 2 — Data Port (5-6 semanas, 17-21 cells)

- [ ] `resource Property` + `Service` + indexes — modeled in `hostpoint-mini`; source/data migration open
- [ ] Geolocation: PostGIS column + radius search query — IR + DDL + runtime predicate exist; generated query open
- [x] Maps adapter wire (Google Maps geocoding inicial)
- [ ] Commands CRUD + queries
- [x] File upload S3 (Lazuli Go S3 wire)
- [ ] Soft delete + retention 7y — metadata emitted; anonymization job open
- [ ] Doctor schema drift + integrity checks

### Phase 3 — Chat + Events (2 semanas, 6 cells)

- [x] Chat + Message resources modeled in `hostpoint-mini`
- [ ] send_message command + event_group — command/event modeled; explicit event_group open
- [x] Notifications (Sendgrid wire)
- [ ] **MVP polling**, realtime flagged como Phase 6 — realtime pubsub/presence/backpressure helpers now exist, but no product/client wiring

### Phase 4 — Payment + Reviews (2 semanas, 7-9 cells)

- [ ] ServiceTransaction + workflow lifecycle — modeled in `hostpoint-mini` as `PaymentTransaction`; real port open
- [ ] MercadoPago adapter pack (`@runtime/mercadopago`) — client + webhook verify exist; OAuth/token refresh open
- [x] Webhook signature verify
- [ ] Review CRUD + rating aggregation — Review CRUD modeled; SQL aggregate query open

### Phase 5 — UI + e2e (3-4 semanas, 11 cells)

- [ ] Port Storybook telas (29 telas Flutter → Expo)
- [ ] TypeScript SDK via `lazuli generate --openapi` + SDK gen
- [ ] Design tokens (já em `@hostpoint/design-tokens`)
- [ ] Maestro e2e: signup → browse → book → pay
- [ ] Deploy staging (Fly.io ou AWS Lightsail)

### Phase 6+ (deferred)

- [ ] Realtime chat (Cut realtime gated; proposal done)
- [ ] Meilisearch (Cut search gated; proposal done)
- [ ] Admin dashboards (Cut admin gated; proposal done)

---

## §4. Estimativa revisada total

| Bloco | Cells | Semanas |
|---|---|---|
| Phase Prep | 13-17 | 3 |
| Phase 1 (Auth) | 6-8 | 3 |
| Phase 2 (Data + Geo) | 17-21 | 5-6 |
| Phase 3 (Chat+Events) | 6 | 2 |
| Phase 4 (Payment+Reviews) | 7-9 | 2 |
| Phase 5 (UI+e2e) | 11 | 3-4 |
| Contingency | 8-12 | distribuído |
| **TOTAL** | **68-84 cells** | **18-23 semanas** |

**Revisão pra baixo significativa vs. estimativa anterior (86-132)**: o entendimento correto de que Lazuli Go = wire (~1.6 cells totais para todas as libs combinadas) em vez de reimplementação (35-44 cells) elimina ~30 cells de fantasma.

**MVP realista (2026-07-15)** com 1 dev: Phase Prep + Phase 1 + Phase 2 lite + Phase 4 webhook test. ~25-30 cells, 6-7 semanas. A estimativa restante caiu porque Phase Prep/codegen smoke e vários adapters Hostpoint-needed já estão prontos; a incerteza agora está mais em migração de produto/dados/UI do que em infraestrutura Lazuli.

**Produção completa (2026-09-30)** com 1-2 devs: tudo até Phase 5.

---

## §5. Decisões resolvidas (2026-05-11)

Todas as 5 decisões pendentes foram aprovadas pelo owner. Phase Prep
desbloqueada.

1. **GeoPoint shape**: `@semantic.GeoPoint { lat, lng }` (tipo semântico único,
   simétrico com `@semantic.Email`/`Phone`/etc). Validador embarcado;
   `lat ∈ [-90,90]`, `lng ∈ [-180,180]`. Codegen Go projeta como
   `postgis.Point` ou shape equivalente.
2. **Geospatial search**: **PostGIS no Postgres** (extensão oficial, sem
   dependency externa). Wire em `@runtime/postgres` (~30 LOC).
   `ST_DWithin` / `ST_Distance` / GiST index. Algolia descartado: drift
   de schema vs Postgres adiciona risk sem ganho de MVP.
3. **Maps adapter primário**: **Google Maps direto** (sempre paid, sem fallback
   Nominatim). Trade-off explícito: custo no dev/staging em troca de zero
   gotchas de provider switch entre ambientes. Adapter em
   `@plugin/google-maps` (repo separado, ver §4 abaixo para policy).
4. **MercadoPago adapter**: **`@plugin/mercadopago`** (genérico — não é
   hostpoint-specific, qualquer app brasileiro de pagamento usa). Vive em
   repo privado, NÃO no core `lazuli/lazuli`. Descartado `@runtime/`
   (não é commodity de plataforma; é provider opinativo) e descartado
   `@plugin/hostpoint/mercadopago` (escopo errado — adapter é genérico,
   não product-specific). Ver memória `project_plugin_namespace_policy`
   para a regra completa de namespacing de plugins.
5. **Push notifications**: **Expo Push** (built-in no Expo SDK, abstrai FCM
   Android + APNs iOS). Adapter em `@plugin/expo-push` (proprietary,
   repo separado). FCM direto descartado: o novo app é Expo-based, dual-channel
   adiciona código sem ganho.

---

## §6. Riscos atualizados

| Risco | P | Impacto | Mitigação |
|---|---|---|---|
| Codegen Lazuli→Go é mais complexo que esperado | 30% | Atrasa Phase Prep 2-3 sem | Começar AGORA; templates simples primeiro |
| Lazuli Go wire descobre incompatibilidade de version (e.g., River API change) | 20% | 1 sem por lib | Pin versions em `go.mod`; CI smoke test |
| Firestore→Postgres data migration tem gotchas (loose schema → strict) | 35% | 1-2 sem | Audit schema semana 1; testes de migration data |
| Geolocation accuracy/performance issues com PostGIS | 15% | 1 sem | Benchmark cedo (10K properties mock) |
| Hostpoint payment webhook race conditions | 25% | 1 sem | `idempotency by mercadopago_payment_id` |

**Overall apetite**: ✅ Moderate. Timeline elástico; MVP sempre deliverable.

---

## §7. Próximo passo concreto

**HOJE** (continuação após c316-c360):

1. Estruturar o framework Lazuli/Lazurite: quais helpers viram codegen contracts, quais ficam runtime-only e quais pertencem a adapter packs.
2. Criar/rodar happy path gerado de `examples/hostpoint-mini`: register/login/session + property list/search + MercadoPago webhook verify.
3. Manter o port real do Hostpoint pausado até o framework estar coeso; este checklist só guia capacidades necessárias.
4. Manter este arquivo e `docs/roadmap.md` atualizados a cada wave, além de `docs/next-checklist.md`.

Phase Prep/codegen não é mais o gargalo principal; o gargalo agora é consolidar o framework e só depois migrar produto real + dados + UI.
