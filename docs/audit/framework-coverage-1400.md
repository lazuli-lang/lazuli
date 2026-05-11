# Lazuli × Framework Go Completo — Audit de Cobertura (1.400 features)

**Data**: 2026-05-10
**Baseline Go**: 1.26.3 (linha principal mais recente; recursos como `testing/synctest`, `encoding/json/v2`, GC Green Tea, `runtime/trace.FlightRecorder`, `crypto/hpke`, `runtime/secret`, `errors.AsType`, `log/slog.NewMultiHandler` etc.)
**Companion roadmap**: [`docs/roadmap.md`](../roadmap.md)
**Near-term execution**: [`docs/next-checklist.md`](../next-checklist.md)

## Objetivo

Mapear ~1.400 features que um "framework Go completo" da era 1.26 entregaria contra o estado real do Lazuli (linguagem `.lzi` + runtime Drusa + adapters). O exercício responde:

1. Quantas features Lazuli **já cobre**?
2. Quantas **deveria** cobrir?
3. Quais são **linguagem** (`.lzi`) vs **framework** (runtime Drusa)?
4. Quais **não devem ter** (anti-Lazuli, fora de escopo)?

Este documento mantém **a auditoria completa**, incluindo as ~260 categorizadas como out-of-scope, para preservar a justificativa de cada corte. O [`roadmap.md`](../roadmap.md) é o derivado prático, sem os N's.

---

## Legenda

> **Revisão 2026-05-10** (após crítica): a tag `L` é genérica demais. Foi quebrada em **L0/L1/L2** porque "cobre" significa coisas muito diferentes para fixture, parser e runtime. Os números na coluna `L` deste documento são **L0** (expressível em fixture/IR/gramática). A coluna estrita L2 (parser+IR+codegen+runtime executando) é **substancialmente menor** — provavelmente <50, dado o estado de Drusa.

| Tag | Significado | Onde mora |
|----|---|---|
| **L0** | **Expressível** em fixture canônico, gramática, ou IR shipado (a surface declarativa aceita) | `.lzi` source |
| **L1** | **Parser/IR aceita** — `lazuli check` e `lazuli inspect` reconhecem; doctor lints estão wired | Parser + IR |
| **L2** | **Runtime executa** — `lazuli generate` produz Go válido + Drusa roda em produção | Runtime Drusa |
| **DL** | **Deve ter na linguagem** — vira primitiva/decoração declarável no `.lzi` | `.lzi` |
| **DF** | **Deve ter no framework/runtime** — Drusa entrega; `.lzi` referencia sem declarar mecânica | Runtime Go |
| **DA** | **Deve ter como adapter plugável** — provider-specific, registrado em `registry.lzi` | Adapter pack |
| **F** | **Futuro / pilot-gated** — só após pilot validar necessidade (cuts B/D-H, capsule initiative) | Espera evidência |
| **N** | **Não deve ter / fora de escopo por design** — anti-Lazuli ou redundante a Go nativo | — |

**Sub-distinção N**:
- **N (estrito)** = quebra a tese se entrar. Ex: routers paralelos (gin/echo/fiber), ORMs ativos em runtime (GORM/Ent), layout Rails-style, magic superadmin, cascade/herança em config.
- **N (DA baixo-prioridade)** = pode existir fora do core como adapter comunitário sem ferir a tese. Ex: Heroku adapter, FIPS mode, Nomad, brotli/zstd, libs alternativas. Distintos de "proibido" — apenas não-prioritários.

**Princípios de classificação** (extraídos de [`docs/design-principles.md`](../design-principles.md) + [`docs/grading-rubric.md`](../grading-rubric.md)):
- **Linguagem** = muda **prova de segurança, shape de API, identidade de migração, alcance de policy, tenancy, ou semântica de eval/agent**.
- **Framework** = mecânica reutilizável que **não muda prova nem shape**, só execução.
- **Adapter** = escolha de **provider** (Stripe vs Paddle, Redis vs Valkey, OpenAI vs Anthropic).
- **Out-of-scope** = quebra **Rule Zero** (vocabulário > mecanismo), introduz cascade/herança, vaza Go-ism para `.lzi`, ou já é resolvido por Go stdlib sem ergonomia adicional.

---

## Sumário executivo

| Métrica | Valor |
|---|---|
| Features totais inventariadas | **1.400** |
| **L0** — Expressíveis em fixture/IR/gramática | **~155 (11%)** |
| **L1/L2 estrito** — parser+IR+runtime executando | **<<155** (subset; provavelmente <50 hoje) |
| **DL** — Devem virar primitiva `.lzi` | **~95 (7%)** |
| **DF** — Devem ser entregues pelo runtime Drusa | **~485 (35%)** |
| **DA** — Devem ser adapters plugáveis | **~290 (21%)** |
| **F** — Pilot-gated / futuro evidence-based | **~115 (8%)** |
| **N** — Não devem ter / fora de escopo (estrito + DA baixo-prioridade) | **~260 (19%)** |

**Leituras corrigidas** (após revisão de números):

- **Terreno legítimo (futuro completo)**: L+DL+DF+DA+F = **~1.145 features ≈ 82%** do inventário.
- **Core desejável sem pilot-gated**: L+DL+DF+DA = **~1.025 features ≈ 73%** — esse é o "tamanho final realista" se Drusa amadurecer.
- **Out-of-scope (N)**: **~19%**.
- **Maioria das lacunas (~485, 35%) é runtime/codegen**, não linguagem.
- **DL ≈ 95** primitivas adicionais: tamanho final da gramática `.lzi`. Cut A já trouxe ~10; sobram ~85 (auth, storage, cache, notification expandidas, etc.).

**Cuidado com "Lazuli já cobre 155"**: na maioria dos casos isso significa **L0** (a surface declarativa aceita), não **L2** (Drusa executa). O próprio `full-capsule.lzi` declara: *"Design fixture only; parser and codegen coverage may lag this surface."* Para auditoria de implementação real, ler L como L0; L2 honesto é muito menor.

---

## Distribuição por seção

**Aviso**: a coluna `L` é **L0** (expressível). L1/L2 estrito por seção exigiria audit caso-a-caso contra parser+IR+runtime; reservado para um doctor automatizado futuro.

| # | Seção (faixa) | Total | L0 | DL | DF | DA | F | N |
|--:|---|--:|--:|--:|--:|--:|--:|--:|
| 1 | Core / arquitetura (1–60) | 60 | 26 | 14 | 10 | 0 | 4 | 6 |
| 2 | CLI / DX (61–130) | 70 | 8 | 4 | 38 | 0 | 8 | 12 |
| 3 | Estrutura de projeto (131–175) | 45 | 4 | 6 | 22 | 0 | 0 | 13 |
| 4 | HTTP / servidor (176–242) | 67 | 4 | 12 | 38 | 4 | 4 | 5 |
| 5 | Controllers / handlers (243–290) | 48 | 18 | 14 | 12 | 0 | 2 | 2 |
| 6 | Views / templates / frontend (291–340) | 50 | 4 | 8 | 14 | 6 | 6 | 12 |
| 7 | Banco de dados (341–430) | 90 | 12 | 18 | 32 | 14 | 4 | 10 |
| 8 | Migrations / schema / seeds (431–470) | 40 | 4 | 10 | 22 | 0 | 2 | 2 |
| 9 | Models / domínio (471–520) | 50 | 22 | 18 | 6 | 0 | 0 | 4 |
| 10 | Autenticação (521–560) | 40 | 4 | 8 | 16 | 10 | 2 | 0 |
| 11 | Autorização (561–590) | 30 | 16 | 8 | 4 | 2 | 0 | 0 |
| 12 | Segurança (591–635) | 45 | 8 | 6 | 24 | 2 | 2 | 3 |
| 13 | APIs (636–675) | 40 | 8 | 8 | 14 | 2 | 4 | 4 |
| 14 | JSON / serialização (676–700) | 25 | 6 | 2 | 12 | 0 | 0 | 5 |
| 15 | Jobs / filas (701–740) | 40 | 8 | 6 | 16 | 8 | 2 | 0 |
| 16 | Eventos (741–765) | 25 | 8 | 4 | 8 | 2 | 3 | 0 |
| 17 | Cache (766–790) | 25 | 2 | 4 | 12 | 5 | 0 | 2 |
| 18 | Sessões (791–810) | 20 | 2 | 2 | 12 | 2 | 0 | 2 |
| 19 | Email / notificações (811–845) | 35 | 4 | 6 | 12 | 11 | 2 | 0 |
| 20 | Realtime (846–870) | 25 | 0 | 4 | 12 | 4 | 5 | 0 |
| 21 | Observabilidade (871–925) | 55 | 4 | 4 | 30 | 14 | 2 | 1 |
| 22 | Testes (926–975) | 50 | 4 | 6 | 28 | 0 | 4 | 8 |
| 23 | Configuração (976–1005) | 30 | 14 | 6 | 4 | 4 | 0 | 2 |
| 24 | Internacionalização (1006–1025) | 20 | 0 | 6 | 10 | 2 | 2 | 0 |
| 25 | Admin (1026–1050) | 25 | 0 | 4 | 8 | 0 | 13 | 0 |
| 26 | SaaS / multi-tenant (1051–1075) | 25 | 12 | 6 | 4 | 0 | 3 | 0 |
| 27 | Pagamentos (1076–1100) | 25 | 0 | 4 | 4 | 13 | 4 | 0 |
| 28 | Arquivos / storage (1101–1125) | 25 | 2 | 2 | 8 | 8 | 5 | 0 |
| 29 | Busca (1126–1145) | 20 | 0 | 4 | 4 | 10 | 2 | 0 |
| 30 | Relatórios (1146–1165) | 20 | 0 | 2 | 6 | 4 | 8 | 0 |
| 31 | Deploy (1166–1200) | 35 | 0 | 0 | 20 | 6 | 6 | 3 |
| 32 | Performance (1201–1230) | 30 | 0 | 0 | 18 | 0 | 6 | 6 |
| 33 | Compat Go recente (1231–1280) | 50 | 0 | 0 | 38 | 0 | 6 | 6 |
| 34 | Integrações libs (1281–1370) | 90 | 0 | 0 | 8 | 80 | 0 | 2 |
| 35 | Qualidade (1371–1400) | 30 | 6 | 6 | 10 | 0 | 4 | 4 |
| **Σ** | | **1.400** | **~155** | **~95** | **~485** | **~290** | **~115** | **~260** |

(Números arredondados; features que tangem duas categorias foram atribuídas pela primária.)

---

## Análise por seção

### 1. Core / arquitetura (1–60)

- **L**: `net/http` como base, `http.Handler` compatível, route groups (via API + features), middleware pipeline (auth/policy/idempotency), graceful shutdown (declarado em `app.lzi`), lifecycle hooks (commands têm `before`/`after` via hooks), embedded assets (Go embed via codegen), single-binary deploy, modular monolith, microservice mode (`workspace.lzi`), typed config/routes/params/responses/errors/events/jobs/mailers/forms/policies/settings/feature flags — **toda a família "typed-*" é onde Lazuli brilha** porque é a tese da DSL.
- **DL** (com ressalva — evitar mecanismo demais): `boot_hook`/`shutdown_hook` como kinds, `pack` namespace amadurecido. `engines/modular apps` e `service container` são **N** estritos se virarem mecânica imperativa estilo Laravel/Nest — Lazuli já expressa composição via `uses` + `registry.lzi` + `requires integration` + `bindings`. Um `bind CRMProvider to FooCRMProvider scoped singleton` seria Go/DI vazando para DSL: **não fazer**.
- **DF**: adapter `chi` (escolhido em [`docs/target-stack.md`](../target-stack.md)), worker mode, dev server, hot reload, multi-binary workspace mechanics.
- **F**: hot reload de `.lzi` (via doctor watch), pluggable transport (gRPC). Capsules cross-platform = F.
- **N**: adapters paralelos para gin/echo/fiber (Rule Zero: um router); CLI mode/cron mode como "modos do framework" (já são `command`/`job` kinds).

### 2. CLI / DX (61–130)

- **L**: `lazuli check`, `lazuli doctor`, `lazuli inspect`, `lazuli generate`, `lazuli grade`, `go run ./cmd/app` (gerado).
- **DL**: `app routes` (inspect de IR já dá metade), `app make <kind>` scaffolders, shell completions.
- **DF (grande gap)**: `app new`, `app serve`, `app console`, `app db {create,drop,migrate,rollback,seed,reset}`, `app make controller/model/migration/job/mailer/command/policy/middleware/request/resource/serializer/test/admin/scaffold/crud`, `app destroy scaffold`, `app doctor`, `app upgrade`, `app fmt`, `app lint`, `app test`, `app bench`, `app profile`, `app trace`, `app env`, `app secrets`, `app deploy`, `app plugins`, route/middleware/dependency-graph/config/health/build-info inspection.
- **F**: `app make admin` (Cut admin gated), `openapi/graphql generation`, `client SDK generation`, `mock generation`.
- **N**: `go new app` como template (use `lazuli new`); reverse engineering de schema (oposto da tese — schema vem do source); vendoring/`go.work` aware (Go já cobre); módulo template/vendoring mode/reproducible builds são propriedades do Go toolchain.

**Destaque**: o CLI Lazuli precisa expandir muito — é onde o "Rails feeling" mora.

### 3. Estrutura de projeto (131–175)

- **L**: `cmd/`, `internal/` (codegen Go), `database/migrations|seeds|factories` (planejado), `config/`, `routes/`, `public/`, `storage/`, `tmp/`, `test/`, `docs/`, environment-specific config/boot/routes/providers (via [`profiles.lzi`](../../examples/full-capsule/profiles.lzi)).
- **DL**: layout package-by-feature (já é a tese — `features/customer/`, `features/billing/`). Internal package enforcement (linter rule).
- **DF**: gerador de scaffold com pastas; module aliases.
- **N**: `app/controllers`, `app/models`, `app/views`, `app/jobs`, `app/mailers`, `app/policies`, `app/services`, `app/events`, `app/listeners`, `app/serializers`, `app/resources`, `app/forms`, `app/validators`, `app/middleware`, `app/commands`, `app/tasks` — **Lazuli é package-by-feature por design**; a estrutura Rails-style por tipo é anti-Lazuli. `spec/` redundante a `test/`. `locales/` cobertos por i18n (DL futura).

### 4. HTTP / servidor (176–242)

- **L**: TLS via `app.lzi`, request ID (correlation), upload limits (`@cap.File(max_size:)`), CORS (A.11), rate limiting (`rate_limit` em agent/command), idempotency middleware (`idempotency by ...`).
- **DL**: signed/encrypted cookies (decorador), trusted proxy config, real IP detection, request body limits global, header limits, secure cookie defaults declarados, session middleware (declarável), custom 404/405/500/maintenance, panic recovery, Problem Details. Authorization/authentication middleware **já é** Lazuli — só falta enumerar.
- **DF**: HTTP/1.1, HTTP/2, mTLS, ACME/Let's Encrypt, HSTS, keep-alive, timeouts, streaming/chunked/SSE/WebSocket, multipart, static files, reverse proxy, forwarded headers, compressão (gzip native, brotli/zstd adapter), ETag, conditional requests, Cache-Control, Range, CSRF (Go 1.26 `CrossOriginProtection`), circuit breaker, retry middleware, body parser, validation middleware, observability middleware, slow request logging, timeout middleware.
- **DA**: brotli adapter, zstd adapter, bot protection hooks (Cloudflare/recaptcha).
- **F**: HTTP/3 via adapter (quic-go), WebSocket rooms/channels/presence (Cut realtime).
- **N**: directory listing opt-in (insegurança); IP allowlist/denylist como features primárias (preferir middleware adapter); long polling (legacy).

### 5. Controllers / handlers (243–290)

- **L**: typed request structs (`input`), typed response structs (`returns`), JSON binding, form binding, multipart binding (`@cap.File`), path variable binding (`route id: Customer.ID`), query binding (em `query.list`), header binding, redirect/URL helpers (declarável), URL helpers, JSON/HTML/file/stream/no-content render, error mapping, status mapping (`expose client 4xx/5xx`), API resources, serializers, transformers (records), pagination/sorting/filtering helpers, include relations, sparse fieldsets.
- **DL**: cookie binding decorator, XML binding (raro), before/after/around actions = lifecycle hooks declaráveis, controller filters/concerns ≈ composições via `uses`.
- **DF**: mecânica de binding/render por trás dos kinds; flash messages (atrás de `session`).
- **N**: controller base class/struct (anti-Lazuli); function vs method handlers (Lazuli não escreve handlers à mão); dependency-injected handlers (DI é implícita).

### 6. Views / templates / frontend (291–340)

- **L**: `.lzx` (web/mobile) é a camada de view; cobertura inicial.
- **DL**: components, slots, view helpers, form builders, HTML builders, asset helpers, URL helpers (já existem em `.lzx`), CSRF helpers, flash helpers, validation error helpers, i18n helpers.
- **DF**: template inheritance/layouts/partials (atrás de `.lzx`); markdown rendering, syntax highlighting, asset fingerprinting/manifest, source maps.
- **DA**: React (target principal), Expo (target principal), Tailwind, esbuild/Vite, CDN asset host.
- **F**: PDF/RSS/sitemap templates (cut reports), fragment caching, view caching.
- **N**: native `text/template` (oposto da tese), HTMX/Alpine/Vue/Svelte/Solid/Turbo paralelos a React (Rule Zero — pick one stack); template hot reload (`.lzx` re-lower); PostCSS (Tailwind cobre). Import maps (target é bundler-based).

### 7. Banco de dados (341–430)

- **L**: `database/sql`, `pgx`, PostgreSQL, transactions, prepared statements (planejados), repository pattern (`query.*`), data mapper (resources são entidades), raw SQL (`query.sql`), typed SQL, named queries, query scopes (`policy` em query), eager loading (`includes`), joins (no `query.sql`), has-one/has-many/belongs-to/many-to-many (relations), self-referential, UUID keys (`@semantic.UUID`), JSON columns, enum mapping, soft deletes, timestamps, slugs, audit columns, audit events (cuts shipados).
- **DL**: row-level locking, optimistic locking (decorador), value objects (records), counter cache, materialized/database views (kinds `view`), outbox table (kind `outbox`), inbox table, event store (proposto), read models, CQRS hooks, full-text search (decorador no field), composite keys, polymorphic relations, single-table inheritance, sharding hooks, per-tenant database/schema, multi-database routing, primary/replica routing.
- **DF**: connection pooling, read replicas, health checks, query logging, slow query logging, query comments, query tracing, nested transactions, savepoints, unit of work, lazy loading, preloading, cursor/offset pagination, bulk insert/update, batch queries, upsert, pessimistic locking, ULID/Snowflake IDs, advisory locks.
- **DA**: MySQL, MariaDB, SQLite, SQL Server, CockroachDB, ClickHouse, Oracle, MongoDB, Redis (todos adapters opt-in; **só PostgreSQL é primário**).
- **F**: stored procedures (gated).
- **N**: GORM/Ent/SQLC/Bun/SQLBoiler/Squirrel paralelos (Rule Zero — `query.sql` é a saída de fuga, codegen direto sobre pgx); Active Record-style API (anti-Lazuli); database fixtures separadas de seeds.

### 8. Migrations / schema / seeds (431–470)

- **L**: schema dump (IR), schema diff (via doctor), reversible migrations (re-lower from source), migration codegen.
- **DL**: schema drift detection (doctor rule), tenant migrations (kind `tenant_migration`), index/foreign-key/constraint/enum/extension/trigger/view/generated column/partition helpers como decoradores em resources.
- **DF**: SQL/Go migrations execution, transactional/non-transactional, online migrations, zero-downtime helpers (atlas), migration locking/status/rollback/redo/squashing, database creation/dropping/reset/truncate, seed loading.
- **F**: irreversible migration markers, schema snapshots.
- **N**: separate "Go migrations" file format (Lazuli re-lowera do source).

### 9. Models / domínio (471–520)

- **L**: struct tags (`@semantic.*`, `@pii.*`, `@cap.*`), field metadata, validations (`@validator.*`), custom validators, cross-field validation, conditional validation, sanitization, normalization, type coercion (no IR), attribute API, virtual attributes (computed via formula — Cut C?), defaults, before/after callbacks, domain events, model policies, model serializers, model resources, form objects (`input` records), command objects (`command`), query objects (`query`), service objects (commands), value objects (records), entities (resources), repositories (queries), domain errors (typed enum), token generation, secure random helpers (`@cap.Token`).
- **DL**: dirty tracking (kind hook), change sets, model observers (via `event`), aggregates (kind `aggregate`), specifications (kind `spec`), invariants (rule + doctor), slug generation (decorador), async validation.
- **DF**: callback execution mechanics (after_commit/after_rollback).
- **N**: model base class/struct (anti-Lazuli); explicit Argon2/Bcrypt como model concern (move to auth).

**Destaque**: Models/domínio é onde Lazuli **já é o mais maduro** — `~44/50 features` cobertas.

### 10. Autenticação (521–560)

- **L**: password auth (estrutura), session/cookie auth, OAuth/OIDC client (decorador).
- **DL**: declarável: `auth password`, `auth passkeys`, `auth magic_link`, `auth oauth provider: github`, `auth saml`, `auth ldap`, MFA (`mfa totp`), recovery codes, device sessions (kind `device_session`), remember me, account lockout (rate_limit existente), login throttling, login audit (event), impersonation (kind), API tokens (`@cap.Token`), personal access tokens, service accounts (kind `service_account`), token scopes, rotation, revocation, refresh tokens.
- **DF**: hashing mechanics (Argon2, Bcrypt), JWT/PASETO encode/decode, OAuth2 server flow, WebAuthn cerimony, SAML handshake.
- **DA**: GitHub/Google/Microsoft/Apple/Discord/Slack social providers.
- **F**: SAML/LDAP (gated por demanda enterprise).

### 11. Autorização (561–590)

- **L**: RBAC (`@role.*`), ABAC (`@scope.*`), policy-based (`rule`), resource policies, route policies, controller policies, field-level permissions (audience/policy), row-level permissions (tenant_from, scope), tenant-level, organization-level (uses org), team permissions, group permissions, permission caching (na codegen), policy DSL (`rule`), policy testing (via `case` em rules), authorization middleware, scoped queries (`scope` em query), secure default deny, deny reasons (`rule message`).
- **DL**: permission inheritance (kind `role_inherits`), policy generators, admin permission UI (gated), permission audit log (via `event`), authorization helpers/decorators, impersonation policies (kind futuro).
- **DF**: avaliação de policy em runtime.
- **DA**: Casbin adapter, OPA/Rego adapter, Zanzibar-style (Spicedb).
- **N**: superadmin mode "magic" (anti-Lazuli — declare role explícita).

**Destaque**: Autorização é a **outra coluna mais madura** (~24/30 cobertas) — policy reachability é diferencial.

### 12. Segurança (591–635)

- **L**: CSRF, CORS, secure headers (declaráveis), cookie signing/encryption, session encryption, secrets manager (`registry.lzi`), `@cap.Encrypted`, PII masking (`@pii.*`), sensitive log redaction, upload MIME validation (`@cap.File(accept:)`), rate limiting, security audit logs (audit events).
- **DL**: CSP nonce helpers, HSTS toggle, X-Frame-Options/X-Content-Type-Options/Referrer-Policy/Permissions-Policy (decoradores em `app.lzi`), secret rotation policy, field encryption (`@cap.Encrypted`), at-rest encryption helpers, SSRF guards (decorador em integration), open redirect guards, host authorization, path traversal guards (decorador em storage), upload virus scan hook, brute-force protection (já é `rate_limit`), abuse detection hooks.
- **DF**: native crypto helpers, `crypto/hpke`, `crypto/tls`, post-quantum awareness, FIPS mode, `runtime/secret`, password pepper, key derivation, envelope encryption, request body redaction, HTML escaping, SQL injection safeguards (codegen automático), safe file serving, dependency vulnerability scanning hook, SBOM generation, SLSA/provenance hooks.
- **DA**: cloud KMS (AWS/GCP/Azure/Vault), SOPS.
- **N**: supply-chain policy hooks como feature framework (CI já cobre).

### 13. APIs (636–675)

- **L**: REST, JSON API (shape padrão), webhooks (kind `webhook`), Problem Details, API versioning (via profile/feature), API pagination/filtering/sorting/includes/sparse fields, API rate limits, API auth scopes (`@scope.*`), API keys, request validation, response validation, idempotency keys (`idempotency by`), webhook signing (HMAC verify), webhook retries.
- **DL**: OpenAPI generation (a partir do IR — gap claro), API deprecation headers (decorador em api), webhook event registry (kind `webhook_event`), webhook replay/DLQ, API changelog (auto a partir do IR diff).
- **DF**: OpenAPI validation/UI, server stubs, gRPC adapter, JSON-RPC, ConnectRPC, contract tests, HATEOAS helpers, API analytics.
- **DA**: GraphQL adapter, gRPC-Gateway.
- **F**: HAL, Siren.
- **N**: XML API (legacy), separate "JSON API" spec dialect.

### 14. JSON / serialização (676–700)

- **L**: `encoding/json` base, custom marshaling (via `@semantic` rules), JSON schema generation (do IR), JSON schema validation, serializer groups (audience), field masking (`@pii`), case conversion, error serialization.
- **DL**: empty/null policy (decorador), date/time formatting (já é semantic).
- **DF**: `encoding/json/v2`, `jsontext`, NDJSON streaming, streaming JSON, fast JSON mode.
- **N**: XML/YAML/TOML/CSV/MsgPack/Protobuf/Avro/Parquet como first-class (são adapters; **só JSON é primário**); field aliasing.

### 15. Jobs / filas (701–740)

- **L**: background jobs (kind `job`), delayed jobs, scheduled jobs, recurring jobs, cron jobs (`trigger schedule`), unique jobs (`idempotency by`), retried jobs (decorador), job priorities, job queues, worker pools, job timeout, job middleware, job tracing (event), idempotent jobs, transactional event dispatch (outbox planejado).
- **DL**: job batches, job chains (kind `chain`), distributed locks (kind `lock`), leader election (kind `leader`).
- **DF**: exponential backoff, dead-letter queue, concurrency limits, job cancellation, job progress, job metrics, job dashboard, graceful worker shutdown.
- **DA**: Redis/PostgreSQL/NATS/Kafka/RabbitMQ/SQS/Pub-Sub/Temporal/Asynq/River/Machinery (só **River** é primário).
- **F**: outbox-driven jobs (gated).

### 16. Eventos (741–765)

- **L**: event bus, domain events, integration events, event listeners (via job/notification), event middleware (já é `safety`/policy), transactional event dispatch, async/sync event dispatch, event schemas (payload typed), webhook events, audit events, system events, user events, model events.
- **DL**: event subscribers (kind `subscriber`), event versioning (semver no kind), event upcasters (kind `upcaster`).
- **DF**: event replay mechanics, event store backend.
- **DA**: event store implementation (Postgres, EventStoreDB).
- **F**: event sourcing, CQRS, outbox/inbox pattern (cuts gated).

### 17. Cache (766–790)

- **L**: in-memory cache (runtime), cache TTL/invalidation declarados em queries.
- **DL**: cache tags (decorador), cache namespaces, cache stampede protection (decorador `coalesce`), stale-while-revalidate decorator, sliding expiration, cache locks.
- **DF**: HTTP cache, fragment/query/model/view/template cache, cache warming, cache metrics, two-level (local+remote), ETag integration.
- **DA**: Redis, Memcached, Valkey.
- **N**: asset cache como feature framework (CDN cobre); database cache adapter (já é PostgreSQL).

### 18. Sessões (791–810)

- **L**: cookie sessions (planejado em runtime), encrypted/signed sessions.
- **DL**: rotating session IDs, session fixation protection, per-device sessions, session metadata, session impersonation support, flash messages (kind).
- **DF**: Redis/database/memory sessions, session invalidation, session audit, session cleanup, session TTL/renewal.
- **DA**: Redis/database session store.
- **F**: wizard state (kind `wizard`), cart state (gated por demanda e-commerce).
- **N**: anonymous sessions como kind separado (já cobre via `session.user_id null`).

### 19. Email / notificações (811–845)

- **L0**: notification channel (kind `notification`), notification trigger event, templated mail. Fixture canônico só mostra **email** e **email, in_app** (`full-capsule.lzi:818,829`).
- **DL**: SMS/push/Slack/Discord/webhook channels (mencionar no kind `notification`; hoje não estão no fixture), notification preferences, notification templates por canal, notification localization, notification digest, notification throttling, delivery receipts, read receipts.
- **DF**: SMTP, transactional mail, bulk mail, attachments, inline attachments, mail previews (dev), mail sandbox, mail queues (já é job), mail retries, bounce handling, unsubscribe links.
- **DA**: Mailgun, SendGrid, SES, Postmark, Resend, Twilio (SMS), Firebase (push), Slack webhook, Discord webhook.

### 20. Realtime (846–870)

- **L**: nenhum.
- **DL**: kind `channel`, kind `presence`, kind `broadcast`, kind `subscription`.
- **DF**: WebSocket server, SSE server, pub/sub, presence tracking, reconnect handling, backpressure, heartbeats, connection draining, realtime metrics/tracing.
- **DA**: Redis/NATS/Kafka streaming.
- **F**: live reload (dev only), live updates, live dashboards (Cut admin gated), collaborative events.

**Destaque**: Realtime é o **maior buraco horizontal** — 0 cobertas. Decisão de design: adiar até pilot.

### 21. Observabilidade (871–925)

- **L0**: request ID (correlation), audit logs (events), `agent_run` built-in trace event (A.8 shipado). Structured logging declarado em intent (não em fixture executando) — é **DF** mais do que L.
- **DL**: log levels declarados (em `app.lzi`), trace propagation (built-in `agent_run` é o primeiro), span attributes nomeados.
- **DF (gap massivo)**: `log/slog`, `slog.NewMultiHandler`, JSON/text logs, log sampling, log redaction (via `@pii`), request/query/job logs, metrics, Prometheus exporter, OpenTelemetry traces/metrics/logs, runtime metrics, `runtime/metrics`, health/readiness/liveness/startup/dependency checks, `/debug/pprof`, `runtime/pprof`, `runtime/trace`, `runtime/trace.FlightRecorder`, goroutine leak profile, memory/CPU/mutex/block/alloc profiles, GC/scheduler metrics, panic reporting, error/DB/HTTP/queue spans, log correlation, build info/version endpoint.
- **DA**: Sentry, Honeycomb, Datadog, New Relic, Grafana Tempo, Jaeger, Zipkin.
- **N**: flame graphs como feature do framework (tooling externo).

### 22. Testes (926–975)

- **L**: evals shipados (Cut A + A.10 golden), policy testing (rule cases), snapshot tests (golden), contract tests (eval intent).
- **DL**: `case` em mais kinds (em command, em job, em workflow — gap claro), factories declaráveis (kind `factory`), fixture helpers (já tem seeds), test artifacts annotations.
- **DF**: unit/integration/system/feature/request/controller/model/job/mailer/policy/view/component/golden/API/browser/E2E/load/benchmark/fuzz tests scaffolding, `testing/synctest`, virtualized time, `testing.ArtifactDir`, test containers, database test transactions, test database reset, parallel test isolation, fakes (mailer/queue/cache/clock/events/HTTP), HTTP recorder, snapshot serializers, mock/stub/spy generation, assertions, coverage, race detector, leak detection, allocation assertions, CI test matrix, test sharding.
- **F**: deterministic concurrency tests (sync test maturity).
- **N**: Ginkgo/Gomega/Testify (Lazuli gera testes em Go nativo + synctest); Mockery/GoMock paralelos.

### 23. Configuração (976–1005)

- **L**: `.env`, environment variables, secret files, config schema (em `app.lzi`/`registry.lzi`/`profiles.lzi`), config validation, config defaults, config overrides (profile), environment-specific config, feature flags, typed feature flags, kill switches, maintenance flags, redacted config output, build-time config.
- **DL**: secret manager adapter declarado, percentage rollout, user/tenant targeting (decoradores em flags), config inspection CLI.
- **DF**: runtime config reload.
- **DA**: AWS Secrets Manager, GCP Secret Manager, Azure Key Vault, Vault, SOPS.
- **N**: YAML/TOML/JSON/Go config paralelos a `.lzi`/`registry.lzi` (Rule Zero); remote feature flags como adapter múltiplo.

### 24. Internacionalização (1006–1025)

- **L0**: `default_locale "pt-BR"` em `app.lzi:7`. Mínimo, mas não-zero.
- **DL**: kind `locale`, kind `translation`, locale negotiation (decorador em api/app), locale middleware, missing translation reporting (doctor rule), translation fallback, translation extraction (CLI).
- **DF**: ICU message format, pluralization, gender rules, date/time/number/currency localization, timezone support.
- **DA**: Lokalise, Crowdin, Phrase.
- **F**: i18n é Cut futuro (gated).

### 25. Admin (1026–1050)

- **L0**: surface admin existe parcialmente — `full-capsule.admin.web.lzx` declara `audience admin` em ≥3 surfaces; `full-capsule.lzx` tem rotas `/admin/customers` + `/admin/customers/:id` com `audience admin`; `escape_route "/admin/..."` está nos fixtures de feature. Falta o **generator** + chrome de admin (forms/tables/filters/bulk/charts) — isso é **F** (Cut admin gated).
- **DL**: kind `admin_resource`, kind `admin_dashboard`, kind `admin_action`, admin permissions (via policy), admin audit (via event), admin impersonation.
- **DF**: admin generator, CRUD admin, resource admin, admin dashboards UI, admin forms/tables/filters/search/sorting/pagination, bulk actions, export/import actions, admin notifications, admin charts, admin custom pages, admin theme, menu builder, breadcrumbs.
- **F**: **toda a seção admin é Cut admin gated** — gera muita gramática nova, requer pilot.

### 26. SaaS / multi-tenant (1051–1075)

- **L**: tenant model (resource Org), organization model, team model, membership model, invitations (kind), tenant routing, tenant isolation (tenant_from), tenant config (profiles), tenant feature flags, tenant audit logs, tenant roles, tenant permissions, tenant API keys, tenant deletion workflow (workflow).
- **DL**: tenant subdomains/custom domains (decorador), tenant SSO, tenant quotas/limits, tenant webhooks (kind), tenant storage namespace, tenant cache namespace, tenant migrations.
- **DF**: tenant analytics dashboards.
- **F**: tenant billing (Cut billing gated).

**Destaque**: SaaS/multi-tenant é a **terceira coluna mais madura** (~18/25 cobertas).

### 27. Pagamentos / billing (1076–1100)

- **L**: nenhum.
- **DL**: kind `subscription`, kind `plan`, kind `invoice`, kind `entitlement`, kind `quota`, kind `revenue_event`.
- **DF**: trials/coupons/receipts/taxes/usage billing/metered billing/payment methods/dunning/billing portal/webhook verification/plan changes/cancellation/grace periods/refunds.
- **DA**: Stripe (primário), Paddle, Mercado Pago, PayPal.
- **F**: **todo o billing é Cut billing gated**.

### 28. Arquivos / storage (1101–1125)

- **L**: `@cap.File`, file validation, file MIME validation.
- **DL**: kind `storage`, kind `bucket`, signed URLs (decorador), temporary URLs, public/private files (decorador), storage quotas (decorador no tenant).
- **DF**: local storage, direct uploads, multipart uploads, resumable uploads, file deduplication, file versioning, file lifecycle policies.
- **DA**: S3, GCS, Azure Blob, MinIO.
- **F**: image processing, thumbnail generation, video/audio processing, metadata extraction, virus scanning, CDN integration, backup integration (cuts media gated).

### 29. Busca (1126–1145)

- **L**: nenhum.
- **DL**: kind `index`, search filters/ranking/facets declaráveis, tenant-scoped search (via tenant_from), permission-scoped search (via policy).
- **DF**: SQL full-text search, PostgreSQL tsvector, async indexing, reindex CLI, search analytics.
- **DA**: Meilisearch (primário), Typesense, Elasticsearch, OpenSearch, Algolia.
- **F**: multilingual search, synonyms, highlighting (cut search gated).

### 30. Relatórios / exportação (1146–1165)

- **L**: nenhum.
- **DL**: kind `report`, kind `export`, kind `import_wizard`, scheduled reports (já é `job`), report emails (já é `notification`).
- **DF**: CSV/Excel/JSON/XML export mechanics, PDF generation, report builder UI, dashboard widgets, chart helpers, CSV/Excel/JSON import, validation reports, import rollback.
- **DA**: BI connectors, data warehouse export (Snowflake, BigQuery).
- **F**: report builder visual (cut admin gated), ETL jobs (cut data gated).

### 31. Deploy / operação (1166–1200)

- **L0**: `app.lzi:91-95` declara `deploy / migrations before_deploy / migration_lock required / rollback on_failed_healthcheck`; `app.lzi:79-80` tem `healthcheck "/healthz"` + `readiness "/readyz"`; `profiles.lzi` tem `deploy + migrations + rollback` por ambiente. Surface declarativa de deploy/ops já existe. Falta o **runtime que materializa** (Dockerfile, k8s manifests, blue-green) — **DF**.
- **DF**: Dockerfile/Docker Compose/Kubernetes manifests/Helm chart/systemd unit generators, Procfile, multi-stage/distroless/static binary builds, CGO/no-CGO modes, cross compilation, release/rollback command, blue-green/canary deployment, migrations on deploy, pre/post-deploy hooks, health gates, smoke tests, runtime config injection, secrets injection, autoscaling metrics.
- **DA**: Fly.io, Render, Railway, Cloud Run, ECS, Terraform module, GitHub Actions/GitLab CI templates, buildpacks.
- **F**: ops é **Cut deploy gated**.
- **N**: Heroku adapter (mercado em declínio); Nomad (preferir k8s ou single-binary).

### 32. Performance (1201–1230)

- **L**: nenhum (linguagem não fala de hot paths).
- **DF**: zero-allocation hot paths, pooling opt-in, buffer pooling, `sync.Pool`, OnceValue/OnceFunc, `WaitGroup.Go`, `unique` package, fast router, fast params, streaming parsers, backpressure, connection reuse, query batching, N+1 detection (doctor rule já é language), cache hints, GC-aware defaults, Green Tea GC awareness, container-aware GOMAXPROCS, runtime metrics tuning, memory budget config.
- **F**: load testing command, bench command, profile command, trace command, flame graph command, performance regression checks (Cut perf).
- **N**: fast JSON mode (default a v2 quando estável); JSON v2 experiment flag separado.

### 33. Compat Go recente / nativa (1231–1280)

- **DF (toda)**: Go 1.26 support, toolchain management, `go.mod`/`go.work` awareness, `go doc -http`, `go vet`, vet analyzers (waitgroup, hostport), `testing/synctest`, `testing.ArtifactDir/T.Attr/T.Output`, `testing/cryptotest`, `runtime/trace.FlightRecorder`, goroutine leak profile, runtime/metrics scheduler, `runtime.SetDefaultGOMAXPROCS`, container-aware GOMAXPROCS, Green Tea GC, `encoding/json/v2`, `jsontext`, `errors.AsType`, `reflect.Type.Fields/Methods`, `reflect.Value.Fields/Methods`, `reflect.TypeAssert`, `log/slog.NewMultiHandler`, `crypto/hpke`, `crypto/mlkem`, `crypto/tls` PQ, `crypto/fips140`, `runtime/secret`, `net/http.CrossOriginProtection`, `net/http.HTTP2Config`, `Transport.NewClientConn`, `ReverseProxy.Rewrite`, `io.ReadAll` improvements, `bytes.Buffer.Peek`, `os.Process.WithHandle`, `signal.NotifyContext` cancel cause, `io/fs.ReadLinkFS`, `MapFS` symlink, `tar.Writer.AddFS`, `os.Root` helpers, `hash.Cloner`, `go/ast.ParseDirective`, `go/token.File.End`, `new(value)` initializer.
- **F**: experimental APIs (json/v2, runtime/secret) gated por estabilização.
- **N**: surface explícita em `.lzi` para essas features (`runtime` é Drusa-only).

**Destaque**: toda essa seção é trabalho do **runtime team**, não da linguagem.

### 34. Integrações libs (1281–1370)

- **DA (quase toda)**: 90 features, 80 são adapters plugáveis registrados em `registry.lzi`.
- **DF (poucos)**: chi (router primário), pgx (DB primário), slog nativo, validator/v10 (adapter primário).
- **N**: `gin`/`echo`/`fiber`/`httprouter` (chi é o pick); `gorilla/websocket` vs `nhooyr/websocket` (pick um); `gorm`/`ent`/`sqlc`/`bun`/`sqlboiler`/`xo` paralelos; Ginkgo/Gomega/Testify/GoMock/Mockery; Wire/Fx/Dig; Koanf/godotenv/envconfig; Cobra/Viper/Kong; Bubble Tea/Survey.

**Destaque**: dos 90 listados, **só ~20 são primários**. Os outros 70 são variações que violariam Rule Zero.

### 35. Qualidade / manutenção (1371–1400)

- **L**: opinionated defaults, override-friendly internals, minimal magic mode, convention-over-configuration, explicit mode, stable public API, error catalog (typed errors).
- **DL**: error codes namespace, deprecation warnings (doctor rule), compatibility layer kind.
- **DF**: semantic versioning, upgrade guides, LTS policy, plugin API, extension/generator/middleware/driver/provider registry, internal diagnostics, documentation generator, API reference generator.
- **F**: example apps, starter kits (SaaS/API/admin/microservice/CLI/monorepo).
- **N**: monorepo starter genérico (Lazuli é monorepo-friendly por design).

---

## Veredicto

### Onde Lazuli **já é o estado da arte**

1. **Models/domínio** (~44/50) — `@semantic.*`, `@pii.*`, typed fields, audit, retention.
2. **Autorização** (~24/30) — RBAC + ABAC + policy reachability + field/row/tenant policies.
3. **SaaS/multi-tenant** (~18/25) — tenancy first-class.
4. **AI-first** (Cuts A.7–A.11) — `agent`, `tools`, `evals`, `safety`, `approval`, `expose http`, `agent_run` trace, golden evals, CORS.

### Onde Lazuli deve crescer na linguagem (~95 DL)

Top 10 gaps:
1. `auth` kinds (password, passkeys, oauth, mfa).
2. `notification` expandidas (digest, throttle, receipts).
3. `storage` kind + signed URLs.
4. `cache` kind (tags, namespace, stampede, sliding).
5. `outbox`/`inbox`/`event_store` (Cut B).
6. `webhook_event` registry + replay/DLQ.
7. `report`/`export`/`import_wizard`.
8. `index`/`facet` (busca).
9. `i18n locale`/`translation`.
10. `flow` (Cut B agent orchestration).

### Onde runtime Drusa precisa entregar (~485 DF — 35% do total)

Categorias críticas:
- **Observabilidade** (~30) — `slog`, OTEL, runtime/metrics, pprof, health checks.
- **HTTP avançado** (~38) — HTTP/2, streaming, SSE, WebSocket, compressão, ETag.
- **Database operacional** (~32) — pool, replicas, slow log, savepoints, bulk ops.
- **Testes** (~28) — sync test, factories, parallel isolation, fakes, recorders.
- **Migrations** (~22) — execution, online, locking, rollback.
- **Deploy** (~20) — Dockerfile, k8s, systemd, blue-green, smoke tests.
- **CLI** (~38) — `make`, `db`, `routes`, `console`, `doctor`, `upgrade`.
- **Go 1.26 nativos** (~38) — json/v2, slog multi-handler, runtime/secret, etc.

### Adapters (~290) — registry-driven, pick-one-primary

- **Primários (~20)**: PostgreSQL, pgx, chi, slog, OTEL, River, Redis, S3, Stripe, Sendgrid, OpenAI, Anthropic, Sentry, Meilisearch, validator/v10, atlas, golang-migrate, esbuild, React, Expo.
- **Secundários (~50)**: providers alternativos (GCS/Azure; Typesense/Algolia; SES/Postmark/Resend/Mailgun; Paddle/Mercado Pago; TOTP libs; CDN).
- **Out-of-scope (~220)**: rotated providers, routers paralelos, ORMs paralelos, test frameworks paralelos, CLI builders paralelos, DI libs.

### Pilot-gated / futuro (~115)

- Cut B inteiro (~25): flow, budget tokens, knowledge, quota cost.
- Cuts D–H (~10): multi-slot context, agent in jobs, contract record reuse, prompt manifest.
- Cut admin (~25), Cut billing (~20), Cut realtime (~15), Cut media (~10), Cut search (~5), Cut reports (~5).

### Out-of-scope deliberado (~260)

- Adapters paralelos (gin/echo/fiber, gorm/ent/sqlc, ginkgo/gomega, wire/fx/dig).
- Layout Rails-style por tipo (`app/controllers`, `app/models`, ...).
- Cascade/herança em config (anti Rule Zero).
- ORM ativo em runtime (Lazuli gera; não orquestra agregados).
- "Modos" como CLI/cron/worker como features primárias.
- Múltiplas serializações no core.
- Frameworks de templates múltiplos paralelos a `.lzx`.
- Visual editor / reverse-engineering de schema.
- Legacy (long polling, XML APIs, Heroku, FIPS antigo).

---

## Conclusão executiva

- Lazuli **expressa ~11% (155) em fixture/IR** hoje (L0). L1/L2 estrito é subset menor.
- **Core desejável sem pilot-gated**: ~73% (1.025 = L+DL+DF+DA).
- **Terreno legítimo total** (incluindo F): ~82% (1.145).
- **Out-of-scope (N)**: ~19%.
- **Linguagem é compacta** (~95 primitivas adicionais a maturar — não 1400). Maturação chega a ~250 kinds totais.
- **Peso real está no runtime/codegen Drusa** (~485 DF) e nos **adapters** (~290 DA).
- **Fronteira clara**: muda prova/shape/policy/tenancy/eval = linguagem; mecânica reusável = framework; provider-specific = adapter.
- **Pilot-gated (~115)** é backlog disciplinado. Sem evidence, não promove.
- **Rubric ≥8.5 + Rule Zero** explicam os ~260 N's (estritos + DA baixo-prioridade).

**Recomendação tática (revisada após crítica)**:

1. **Não cobrir as 1.400** — ~260 são anti-tese ou DA baixo-prioridade, ~115 são pilot-gated, ~290 crescem com demanda real.
2. **Curto-prazo — provar ciclo L0→L2 em 4 buckets críticos** em vez de espalhar DL solto:
   - **Auth/session** — flow completo: kind → parser → IR → codegen Go → Drusa roda → eval/test.
   - **Storage/file upload** — `@cap.File` end-to-end com S3 + local + signed URLs.
   - **Jobs/queue** — `job` kind end-to-end com River-backed dispatch + retries + DLQ.
   - **Observability/health/logging** — slog + OTEL + health/ready endpoints + `agent_run` consumindo.
   Em cada bucket: expressível em `.lzi`, parseado, IR, codegen Go, roda em Drusa, evals/testes, doctor/inspect cobrem, fixture canônico atualizado.
3. **Médio-prazo** (depois do ciclo provado): segunda onda — cache, notifications expandidas, webhooks, migrations, OpenAPI gen, admin básico.
4. **Longo-prazo**: cuts B/admin/billing/realtime/media aguardam pilot evidence.

A diferença entre essa recomendação e a anterior é grande: **não adianta crescer DL solto se Drusa não executa**. Provar L0→L2 num bucket pequeno valida o pipeline inteiro; depois disso, expansão horizontal é segura.

Para checklist-vivo derivado deste audit (sem os N's), ver [`docs/roadmap.md`](../roadmap.md).
