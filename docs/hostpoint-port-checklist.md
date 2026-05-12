# Hostpoint Port — Sub-Checklist

**Source analysis**: `docs/audit/framework-coverage-1400.md` + análise read-only de `c:/Users/lucas/hostpoint/` + `c:/Users/lucas/dev/flutter-hostpoint/`.
**Última revisão**: 2026-05-12.

> **Status tracking note (2026-05-12)**: este sub-checklist ainda não foi
> reconciliado linha-a-linha depois das batches c21-c150. Use
> `docs/next-checklist.md` rows 72-76 como ledger de execução e
> `docs/roadmap.md` §Status atual como fonte do "onde estamos". O blocker
> imediato para iniciar o port com confiança é fechar o smoke
> `lazuli generate go examples/full-capsule -> go build`, que hoje falha em
> `customer_auth/command.gen.go: undefined: AuthSession`.

## Princípio fundador

**Lazuli é abstração; Lazuli Go é wire.** O runtime Go **não reimplementa** primitivas que já existem em Go stdlib / extended / SDKs maduros. O trabalho de "Lazuli Go real" é:

1. **Codegen Lazuli → Go**: emitter pega o IR (tipado, Tier 1-4 done) e produz `dist/go/<feature>/*.gen.go` que **importa** e **chama** libs existentes.
2. **Adapter wiring**: cada "stub" em `runtime/go/lazuli/<bucket>/` vira wire de ~10-50 LOC para a lib Go correspondente.

**Não fazer**: reimplementar argon2, HMAC, OAuth2, pgx pool, JWT, OTEL exporter, S3 protocol, River dispatcher, chi router, slog handlers, sendgrid API. Tudo já existe — declarar + chamar.

---

## §1. Phase Prep — Pré-requisitos Lazuli antes do port iniciar

**Estimativa total: ~13-17 cells.** (Revisado pra baixo após correção: Lazuli Go wire ≠ implementação from scratch.)

### 1.1 Codegen Lazuli → Go (BLOCKER #1 — ~12-15 cells)

Emitter que consome IR (já tipado via Phase L Tier 1-4) e produz Go que compila.

- [ ] `lazuli generate go --out dist/go/` CLI verb funcional (hoje só `lazuli generate openapi` shipped)
- [ ] Templates por kind (consumir IR JSON via `lazuli inspect --format=json`):
  - [ ] `dist/go/<feature>/resource.gen.go` — structs + repository pattern via `pgx`
  - [ ] `dist/go/<feature>/command.gen.go` — handler + validation + auth middleware
  - [ ] `dist/go/<feature>/query.gen.go` — list/lookup com pgx + cache via `redis/go-redis`
  - [ ] `dist/go/<feature>/api.gen.go` — chi route + handler binding
  - [ ] `dist/go/<feature>/auth.gen.go` — login/logout + session middleware
  - [ ] `dist/go/<feature>/job.gen.go` — River worker registration
  - [ ] `dist/go/<feature>/webhook.gen.go` — chi receiver + HMAC verify
  - [ ] `dist/go/<feature>/notification.gen.go` — channel dispatcher
  - [ ] `dist/go/<feature>/storage.gen.go` — signed URL handler
  - [ ] `dist/go/<feature>/translation.gen.go` — catalog loader + format
- [ ] Build script roda `gofmt` no output
- [ ] Smoke test: codegen `examples/full-capsule/` → `go build ./dist/go/...` passa sem warning
- [ ] Integration test: codegen + Lazuli Go local + sqlite mock executa happy-path login

### 1.2 Lazuli Go wire (BLOCKER #2 — ~1.5-2 cells totais)

Cada item abaixo é **~10-50 LOC** de wire da lib pra dentro do runtime `runtime/go/lazuli/<bucket>/`. **Não é implementação** — é importar + chamar.

#### Auth
- [ ] `golang.org/x/crypto/argon2` — wire em `auth/password.go` (~10 LOC: Argon2id IDKey com parâmetros canônicos)
- [ ] `golang.org/x/crypto/bcrypt` — fallback para migração de legacy (~5 LOC)
- [ ] Session store: Postgres table via pgx (~30 LOC; pode usar `riverqueue` lib do River pra inspiração de schema-managed)
- [ ] `golang.org/x/oauth2` + `golang.org/x/oauth2/google` — OAuth Google wire (~20 LOC)
- [ ] `github.com/pquerna/otp/totp` — TOTP MFA wire (~10 LOC)

#### Storage
- [ ] `github.com/aws/aws-sdk-go-v2/service/s3` + `s3/v1` presigner — wire upload/signed URL (~30 LOC)
- [ ] Local filesystem adapter (~20 LOC)
- [ ] (futuro) `github.com/minio/minio-go/v7` se quiser MinIO support

#### Jobs
- [ ] `github.com/riverqueue/river` + `riverpgxv5` — wire dispatcher + retry policy (~40 LOC)
- [ ] DLQ handler hook (River nativo via `Worker.NextRetry`)

#### DB
- [ ] `github.com/jackc/pgx/v5` + `pgxpool` — wire connection pool em `db.go` (~30 LOC)
- [ ] `withTx` helper já existe em `runtime/go/lazuli/db.go` — completar com retry on serialization failure

#### HTTP
- [ ] `github.com/go-chi/chi/v5` — wire router em `http.go` (já parcial; substitui hardcoded `/healthz`)
- [ ] Middleware: recover, request-id, otel, csrf via Go 1.26 `net/http.CrossOriginProtection`

#### Observability
- [ ] `log/slog` (stdlib) — wire JSON handler em `observability/logging.go` (~15 LOC)
- [ ] `go.opentelemetry.io/otel/sdk/trace` + `otlptracehttp` exporter — wire em `observability/tracing.go` (~25 LOC)
- [ ] `getsentry/sentry-go` — wire panic handler (~15 LOC)

#### Email
- [ ] `github.com/sendgrid/sendgrid-go` — primary (~20 LOC)
- [ ] SMTP stdlib (`net/smtp`) — fallback (~15 LOC)

#### Webhooks
- [ ] HMAC verify usando `crypto/hmac` + `crypto/sha256` (stdlib) — wire genérico (~10 LOC)

### 1.3 Codegen consume Tier 4 IR completo

- [ ] `Command.audit/approval/invalidates/external_calls` typed (já no IR) → codegen emite middleware + Postgres audit insert
- [ ] `Resource.retention` → codegen emite cron job de anonymization
- [ ] `Field.derived_from` → codegen emite computed column ou GENERATED ALWAYS AS
- [ ] `CapabilityRef::Hashed/Encrypted/Token` → codegen emite import + chamada da lib correspondente

---

## §2. Features-core do Hostpoint mapeadas

### 2.1 Auth & User (Phase 1 port — 6-8 cells)

- [ ] `feature auth_hostpoint`:
  - [ ] `auth identity Customer.email`
  - [ ] `auth password algorithm argon2id hash @fn.hash_password verify @fn.verify_password rate_limit "5 per 10 minutes per ip"`
  - [ ] `auth oauth google adapter @adapter.google_oauth`
  - [ ] `auth sessions resource Session ttl "7 days" refresh false`
- [ ] `resource User` (mapeado de Firestore `auth_users`):
  - [ ] `email: @semantic.Email @pii.contact required unique`
  - [ ] `name: Text required`
  - [ ] `role: UserRole required` (enum traveler|host)
  - [ ] `fcm_token: Text optional` (mobile push)
  - [ ] `profile_photo: @cap.File(visibility=public, max_size=5mb, accept=image/*) optional`
  - [ ] `tenancy by user_id` (single-tenant — sem org)
- [ ] `resource Session` (replace SharedPreferences + token refresh loop):
  - [ ] `user: User required on_delete cascade`
  - [ ] `expires_at: DateTime required`
  - [ ] `refresh_token_hash: @cap.Hashed(algorithm:argon2id) required`
- [ ] Commands:
  - [ ] `register` (email+password+role)
  - [ ] `login` (email+password → session)
  - [ ] `logout` (invalidate session)
  - [ ] `request_password_reset` (magic link via email)
  - [ ] `reset_password` (token + new password)
  - [ ] `enable_mfa` (TOTP setup)
- [ ] Policies:
  - [ ] `@policy.same_user` (user can only modify own profile)
  - [ ] `@role.traveler` / `@role.host` (role-based dispatch)

### 2.2 Property & Service (Phase 2 port — 17-21 cells)

- [ ] `resource Property`:
  - [ ] `host: User required on_delete cascade`
  - [ ] `name: Text required`
  - [ ] `description: Text optional`
  - [ ] `address: Text required`
  - [ ] `coordinates: GeoPoint required` ← **NEW: precisa de `@semantic.GeoPoint` ou `Latitude`+`Longitude` no IR**
  - [ ] `photos: many Photo` (cap_file array)
  - [ ] `amenities: Text[]` (array column)
  - [ ] `rules: Text optional`
  - [ ] `tenancy by host_id`
  - [ ] `soft_delete; retention 7y then anonymize`
- [ ] `resource Service`:
  - [ ] `property: Property required on_delete cascade`
  - [ ] `host: User required on_delete cascade`
  - [ ] `category: ServiceCategory required` (enum)
  - [ ] `name: Text required`
  - [ ] `price_cents: Integer required` (não Float — sempre cents)
  - [ ] `currency: @semantic.Currency required` (default BRL)
  - [ ] `photos: @cap.File(visibility=public, max_size=3mb, accept=image/*)[3]` (max 3)
  - [ ] `available_hours: TimeRange[]`
- [ ] Commands CRUD pra ambos (create/update/list/delete/detail)
- [ ] Queries:
  - [ ] `properties.list filter by_amenity, by_price_range search params.q over name, description` (com cache `5 minutes` namespace `properties`)
  - [ ] `properties.lookup by_id`
  - [ ] `properties.search_by_radius(lat, lng, radius_km)` ← **PostGIS ou Haversine**
- [ ] Indexes (mapeado de `firestore.indexes.json`):
  - [ ] `(host_id, created_at desc)`
  - [ ] `(category, price_cents)`
  - [ ] `GIST (coordinates)` para PostGIS radius search

### 2.3 Geolocation (NEW — bucket separado)

**Decisão estratégica**: maps + geocoding ficam em **adapter packs**, não Lazuli core. Geo primitives em IR são closed-set.

- [ ] IR: adicionar `BuiltinType::GeoPoint` (lat+lng tuple) e/ou `@semantic.Latitude` + `@semantic.Longitude`
- [ ] Storage: PostGIS extension nativa (geo queries no Postgres direto, sem provider externo)
  - [ ] Codegen emite `CREATE EXTENSION IF NOT EXISTS postgis;` em migration
  - [ ] Codegen emite `GEOGRAPHY(POINT, 4326)` para `GeoPoint` columns
  - [ ] Codegen emite `ST_DWithin` para `search_by_radius` queries
- [ ] Adapter `@runtime/google_maps` (geocoding endereço → coords; uma operation): wire `googlemaps.github.io/maps-services-go` (~20 LOC)
- [ ] Adapter alternativo `@runtime/mapbox` (mesmo contract)
- [ ] Adapter alternativo `@runtime/nominatim` (OpenStreetMap, gratuito; ~30 LOC)
- [ ] `requires integration maps: MapsProvider` em features que precisam geocoding
- [ ] **Não** wirar map rendering — isso é client-side (Flutter `google_maps_flutter` ou Expo `react-native-maps`); Lazuli core não toca UI rendering

### 2.4 Transactions & Payment (Phase 4 port — 7 cells)

- [ ] `resource ServiceTransaction`:
  - [ ] `traveler: User required on_delete restrict`
  - [ ] `host: User required on_delete restrict`
  - [ ] `service: Service required on_delete restrict`
  - [ ] `status: TransactionStatus required` (enum pending|completed|cancelled|refunded)
  - [ ] `amount_paid_cents: Integer required`
  - [ ] `mercadopago_payment_id: Text optional unique`
- [ ] `requires integration payment_gateway: PaymentGateway`
- [ ] App binding `payment_gateway = integrations.mercadopago` em `app.lzi`
- [ ] `registry.lzi`: `integrations.mercadopago` typed
- [ ] Adapter pack `@runtime/mercadopago`:
  - [ ] OAuth wire (~30 LOC)
  - [ ] Token refresh wire (~15 LOC)
  - [ ] Webhook signature verify (~20 LOC; usa HMAC SHA256)
  - [ ] Create preference API call (~25 LOC)
- [ ] `webhook mercadopago_callback` com `verify @validator.mercadopago_hmac tenant_from payload.external_reference`
- [ ] `workflow transaction_lifecycle` no resource:
  - [ ] pending → completed (em webhook approved)
  - [ ] pending → cancelled (timeout 24h ou user cancel)
  - [ ] completed → refunded (admin command)

### 2.5 Reviews (Phase 4 port — 2 cells)

- [ ] `resource Review`:
  - [ ] `reviewer: User required on_delete cascade`
  - [ ] `target_id: ID required`
  - [ ] `target_type: ReviewTargetType required` (enum property|service|host)
  - [ ] `stars: Integer required` (1-5)
  - [ ] `text: Text optional max 1000`
- [ ] Commands:
  - [ ] `create_review` com policy `@policy.one_review_per_target_per_user`
- [ ] Queries:
  - [ ] `reviews.list_by_target(target_id, target_type)`
  - [ ] `query.sql ./queries/rating_aggregate.sql` (avg stars + count, scope_by target)

### 2.6 Chat & Messaging (Phase 3 port — 6 cells)

- [ ] `resource Chat`:
  - [ ] `participants: User[]` (array — non-canonical em SQL; usar JSONB ou tabela junction)
  - [ ] `last_message_at: DateTime optional`
- [ ] `resource Message`:
  - [ ] `chat: Chat required on_delete cascade`
  - [ ] `sender: User required on_delete restrict`
  - [ ] `body: Text required`
  - [ ] `sent_at: DateTime required defaults now`
  - [ ] `read_at: DateTime optional`
- [ ] Commands:
  - [ ] `send_message` emits `event message_sent`
  - [ ] `mark_message_read`
- [ ] Queries:
  - [ ] `messages.list_by_chat(chat_id)` (paginated, com cache 30s)
- [ ] `event_group message_*` (replicar pattern Phase L Tier 3)
- [ ] **MVP**: polling — Expo app re-fetch `messages.list_by_chat` every 2s
- [ ] **Future (Cut realtime gated)**: `channel chat_messages` + `subscription` + WebSocket (proposal pronto)

### 2.7 Notifications (Phase 3 port — 2 cells)

- [ ] `notification new_message_email`:
  - [ ] `trigger event chat.message_sent`
  - [ ] `channel email`
  - [ ] `recipient input.recipient_email`
  - [ ] `template "./templates/new_message.<locale>.tmpl"`
  - [ ] `throttle max_per "1 hour" per_recipient burst 3` (decisão B — convivendo com rate_limit)
- [ ] `notification booking_confirmed`:
  - [ ] `trigger event payment.transaction_completed`
  - [ ] `channel email, push`
  - [ ] `template "./templates/booking_confirmed.<locale>.tmpl"`
- [ ] Channels:
  - [ ] Email: Sendgrid adapter
  - [ ] Push: FCM adapter (`firebase.google.com/go/v4/messaging`)

### 2.8 i18n (já mostly done — 1 cell pra port)

- [ ] `app.locale default "pt-BR" supported "pt-BR", "en-US"` (legacy hostpoint só pt-BR; novo Expo bilíngue)
- [ ] `translation hostpoint_messages` em `customer_auth` feature
- [ ] `locale_negotiate source accept_language strategy best_match` em runtime unit
- [ ] Templates email com `<locale>` token

### 2.9 Observability (já done na Lazuli — 0 cells port)

- [ ] Built-in trace events `command_run`/`job_run`/`webhook_run`/`agent_run` (Lazuli já entrega)
- [ ] `app.logging level info format json redact pii.*`
- [ ] `app.tracing propagate true sample_rate 0.1 exporter otlp`
- [ ] Sentry adapter wire (~15 LOC em Lazuli Go)

### 2.10 File Storage (já mostly done — wire S3 em Lazuli Go)

- [ ] Property/Service photos: `@cap.File(visibility=public, max_size=5mb, accept=image/*)`
- [ ] User profile photo: idem
- [ ] Optional docs privadas: `@cap.File(visibility=signed, signed_ttl="24h", max_size=10mb)`
- [ ] Codegen emite presigned URL endpoint para direct upload S3

### 2.11 Search (MVP simples, avançado deferred)

- [ ] **MVP**: `query.list filter search params.q over name, description` (LIKE SQL — já cobre L1)
- [ ] **Phase 6+ (Cut search gated)**: Meilisearch adapter quando volume justificar (~10K+ properties)

---

## §3. Roadmap concreto de port

### Phase Prep (3 semanas, 13-17 cells)

- [ ] **Codegen real** emite `dist/go/<feature>/*.gen.go` que compila
- [ ] **Lazuli Go wire** (10+ libs Go): argon2 + pgx + chi + River + S3 + slog + OTEL + sendgrid + oauth2 + totp
- [ ] **Smoke test**: `examples/full-capsule/` → `go build` → roda em Docker compose local
- [ ] **Decision gate**: codegen + Lazuli Go runnable até 2026-06-01? Proceed.

### Phase 1 — Auth Port (3 semanas, 6-8 cells)

- [ ] Port firebase_auth → Lazuli `auth` block
- [ ] Port Firestore `auth_users` → `resource User` + `Session`
- [ ] Migrate rules → `@policy.same_user` + `@role.traveler/host`
- [ ] Commands: register/login/logout/reset/mfa
- [ ] Golden eval login_password + login_oauth
- [ ] Lazuli Go executa login real (argon2id wire) end-to-end

### Phase 2 — Data Port (5-6 semanas, 17-21 cells)

- [ ] `resource Property` + `Service` + indexes
- [ ] Geolocation: PostGIS column + radius search query
- [ ] Maps adapter wire (Google Maps geocoding inicial)
- [ ] Commands CRUD + queries
- [ ] File upload S3 (Lazuli Go S3 wire)
- [ ] Soft delete + retention 7y
- [ ] Doctor schema drift + integrity checks

### Phase 3 — Chat + Events (2 semanas, 6 cells)

- [ ] Chat + Message resources
- [ ] send_message command + event_group
- [ ] Notifications (Sendgrid wire)
- [ ] **MVP polling**, realtime flagged como Phase 6

### Phase 4 — Payment + Reviews (2 semanas, 7-9 cells)

- [ ] ServiceTransaction + workflow lifecycle
- [ ] MercadoPago adapter pack (`@runtime/mercadopago`)
- [ ] Webhook signature verify
- [ ] Review CRUD + rating aggregation

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

**MVP realista (2026-07-15)** com 1 dev: Phase Prep + Phase 1 + Phase 2 lite + Phase 4 webhook test. ~25-30 cells, 6-7 semanas.

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

**HOJE** (decisão de continuação):

1. Aprovar (ou recusar) as 5 decisões pendentes §5
2. Lançar agent pra implementar **Phase Prep §1.1 (Codegen Lazuli→Go)** — esse é o real blocker (12-15 cells)
3. Lançar agent paralelo pra implementar **Phase Prep §1.2 (Lazuli Go wire)** — pode rodar em paralelo (~1.5-2 cells totais)

Quando Phase Prep completar (~3 semanas), kick Phase 1 (Auth port) imediatamente. Decision gate 2026-06-01.
