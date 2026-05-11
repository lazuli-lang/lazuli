# Lazuli Roadmap

**Última revisão**: 2026-05-10
**Source audit**: [`docs/audit/framework-coverage-1400.md`](audit/framework-coverage-1400.md)
**Near-term execution**: [`docs/next-checklist.md`](next-checklist.md)
**Language backlog**: [`docs/language-backlog.md`](language-backlog.md)

## Como ler

Este é o roadmap-vivo de **tudo que vamos implementar** no Lazuli, derivado do audit de 1.400 features de framework Go completo. Os ~260 itens classificados como **N (não-Lazuli)** foram descartados — sua justificativa fica no audit, não aqui. Mesmo que demore, o conteúdo abaixo é compromisso.

Estrutura:

- **§1 Linguagem** (`.lzi`) — ~95 primitivas DL. Muda prova/shape/policy/tenancy/eval.
- **§2 Runtime/Framework** (Drusa) — ~485 capabilities DF. Mecânica que não muda prova.
- **§3 Adapters** — ~70 primários + secundários DA. Provider-specific, registry-driven.
- **§4 Pilot-gated** — ~115 features F. Aguardam pilot evidence.

Cada checkbox cobre 1 a N features da lista original; o agrupamento foi feito quando faz sentido implementar junto. Próximos cuts vivem em `next-checklist.md`.

---

## §0. Estratégia de execução — provar ciclo L0→L2 em 4 buckets

> **Revisão 2026-05-10**: a recomendação anterior era "completar DL de auth+storage+cache+notification". Substituída por: **provar o pipeline inteiro** (declarado → parseado → IR → codegen Go → Drusa executa → eval/test → doctor/inspect → fixture canônico) em 4 buckets críticos antes de espalhar DL horizontalmente. Razão: hoje muitos kinds existem como **L0** (surface declarativa aceita) mas não como **L2** (runtime executa). Crescer DL solto sem fechar o ciclo gera dívida de execução acumulada.

> **Revisão 2026-05-10 (atualização pós-design)**: os 4 buckets-piloto foram **desenhados em paralelo** via `/lazuli-bucket-cycle` (pipeline em `.orion/pipelines/lazuli-bucket-cycle/`). 3 dos 4 (auth, storage, jobs) **independentemente descobriram o mesmo blocker estrutural**: surface autorada não chega ao IR porque `parse_feature_skeleton` (`crates/lazuli_syntax/src/parser.rs:1168-1173`) só faz lowering de `agent`. Conclusão: **Phase L** (row 24 do `next-checklist.md`, antes `backlog`/tech-debt) é **pré-requisito real** dos 3 buckets, não trabalho isolado. Foi repriorizada para `prerequisite`. A exceção (observability) é instrutiva: ficou linear porque `event.trace` já é L1 e Cut A.8 estabeleceu padrão (built-in trace event) replicável mecanicamente. Proposals canônicos:
> - `docs/proposals/auth-lowering-scope.md` + `docs/proposals/bucket-auth-cycle.md`
> - `docs/proposals/bucket-storage-scope.md` + `docs/proposals/bucket-storage-cycle.md`
> - `docs/proposals/bucket-jobs-scope.md` + `docs/proposals/bucket-jobs-cycle.md`
> - `docs/proposals/bucket-observability-cycle.md` (sem scope.md — caminho limpo)
>
> **Ordem de implementação consensual** (extraída dos 4 reviews): **Phase L → Auth → Storage ∥ Jobs (paralelos) → Observability §3.5 trace events (após auth, paralelo aos outros) → resto de observability**. Rows 24, 26-37 do `next-checklist.md`.

**Buckets-piloto** (executar em sequência, cada um end-to-end antes do próximo):

1. **Auth / session** — kind `auth` (password+session) → parser → IR → codegen Go (handlers, middleware, hash) → Drusa executa login real → eval/test → doctor/inspect cobrem → fixture atualizado.
2. **Storage / file upload** — `@cap.File` end-to-end com adapter S3 + local + signed URLs declarados, parseados, gerados, executando.
3. **Jobs / queue** — `job` kind end-to-end via River + retries + DLQ; trigger por event funcional.
4. **Observability / health / logging** — slog + OTEL traces + `/healthz` + `/readyz` reais; `agent_run` consumido por exporter.

**Critério de "ciclo fechado" por bucket**:

- [ ] Aparece em [`examples/full-capsule/`](../examples/full-capsule/) como fixture canônico.
- [ ] `lazuli check` aceita a sintaxe.
- [ ] `lazuli inspect` mostra IR completo.
- [ ] `lazuli doctor` tem ≥1 lint relevante.
- [ ] `lazuli generate` produz Go válido que compila.
- [ ] Drusa executa um cenário ponta-a-ponta (request → handler → DB/disp/log → response).
- [ ] Existe `eval`/`case` ou Go test cobrindo o caminho.
- [ ] LSP enxerga e dá hover/completion.

Depois dos 4 buckets fechados, **expansão horizontal** (§1 e §2 abaixo) deixa de ser arriscada — o pipeline está provado.

**Segunda onda** (depois do ciclo provado): cache, notifications expandidas, webhooks, migrations, OpenAPI gen, admin básico.

---

## §1. Linguagem `.lzi` — ~95 primitivas DL

A linguagem já é compacta e estável. O crescimento aqui é **horizontal** (mais kinds para mais domínios) e **decorativo** (mais decoradores em kinds existentes). Nenhum desses muda o estilo de autoria — todos seguem o padrão indent + PascalCase + closed namespaces.

### 1.1 Core / arquitetura

> **Cuidado com mecanismo demais**: composição em Lazuli já é `uses` + `registry.lzi` + `requires integration` + `bindings`. Um `bind CRMProvider to FooCRMProvider scoped singleton` seria Go/DI vazando para DSL. Os itens abaixo devem ser **declarativos descritivos**, não mecanismos imperativos.

- [ ] `boot_hook` / `shutdown_hook` kinds (lifecycle declarável — não confundir com hook genérico)
- [ ] `pack` extensão madura (provenance já existe; refinar surface autoral)
- [ ] `engine` kind (modular sub-app) — **só se** pilot mostrar que `uses` + workspace boundaries não bastam; caso contrário **N**.
- [ ] **Anti-padrões a recusar**: `bind` kind explícito estilo DI Laravel/Nest, `service container` imperativo, "plugins" como mecanismo geral em vez de extensão tipada.

### 1.2 HTTP / cookies / headers / errors

- [ ] `cookie` decorator (`signed`, `encrypted`, `secure`, `same_site`)
- [ ] `proxy` block em `app.lzi` (trusted proxies, real IP, forwarded headers)
- [ ] `limits` block em `app.lzi` (body, header, upload globais)
- [ ] `session` kind explícito (declarar store, TTL, rotation)
- [ ] `error_page` kind (custom 404/405/500/maintenance)
- [ ] `problem` shape padrão (Problem Details automático)
- [ ] `maintenance` kind

### 1.3 Controllers / handlers

- [ ] `binding` decorators (cookie, xml — raros)
- [ ] `lifecycle` em commands (before/after/around explícitos)
- [ ] `concern` reutilizável via `uses` cross-feature

### 1.4 Views / .lzx

- [ ] Helpers nomeados (form_builder, asset, url, csrf, flash, validation_error, i18n) catalogados em closed namespace
- [ ] `slot` + `component` (estender para component capsule initiative cross web+mobile)

### 1.5 Database / persistence

- [ ] `lock` decorator (optimistic/pessimistic/row-level)
- [ ] `view` kind (materialized + database views)
- [ ] `outbox` / `inbox` kinds
- [ ] `event_store` kind
- [ ] `read_model` kind
- [ ] `full_text` decorator em field
- [ ] `composite_key` block em resource
- [ ] `polymorphic` decorator em relation
- [ ] `inheritance` (STI) decorator em resource
- [ ] `shard_by` decorator
- [ ] `replica` / `primary` routing decorator
- [ ] `tenant_database` / `tenant_schema` decorator

### 1.6 Migrations / schema

- [ ] `tenant_migration` kind
- [ ] `index` / `foreign_key` / `constraint` / `enum_column` / `extension` / `trigger` / `generated_column` / `partition` decoradores em resource
- [ ] Doctor rule: schema drift detection

### 1.7 Models / domínio

- [ ] `aggregate` kind
- [ ] `spec` kind (specification pattern)
- [ ] `invariant` (rule + doctor reach)
- [ ] `slug` decorator
- [ ] `async_validate` decorator
- [ ] `dirty_tracking` decorator

### 1.8 Autenticação

- [ ] `auth` kind explícito (password, passkeys, magic_link, oauth, saml, ldap)
- [ ] `mfa` decorator (totp, recovery_codes)
- [ ] `device_session` kind
- [ ] `service_account` kind
- [ ] `api_token` kind (`@cap.Token` ampliado: scopes, rotation, revocation, refresh)
- [ ] `impersonation` kind

### 1.9 Autorização

- [ ] `role_inherits` kind
- [ ] `audit_log` kind (formalizar evento existente)
- [ ] `impersonation_policy` kind

### 1.10 Segurança

- [ ] Decoradores em `app.lzi`: `csp`, `hsts`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, `permissions_policy`
- [ ] `secret_rotation` policy kind
- [ ] `ssrf_guard` / `open_redirect_guard` / `host_authorization` / `path_traversal_guard` decoradores
- [ ] `virus_scan_hook` decorator em storage
- [ ] `abuse_detection_hook` kind

### 1.11 APIs

- [ ] `openapi` generation a partir do IR (built-in `lazuli generate --openapi`)
- [ ] `deprecated` decorator em api/command (deprecation headers)
- [ ] `webhook_event` registry kind
- [ ] `webhook_replay` / `webhook_dlq` decoradores
- [ ] API changelog gerado de IR diff

### 1.12 JSON / serialização

- [ ] `null_policy` decorator (omitempty/include/null)
- [ ] `date_format` decorator (já parcial via `@semantic`)

### 1.13 Jobs / filas

- [ ] `chain` kind (job chaining)
- [ ] `batch` decorator em job
- [ ] `lock` kind (distributed)
- [ ] `leader` kind (election)

### 1.14 Eventos

- [ ] `subscriber` kind
- [ ] Event versioning (semver no kind name)
- [ ] `upcaster` kind

### 1.15 Cache

- [ ] `cache` kind explícito (tags, namespace, coalesce, stale-while-revalidate, sliding TTL, locks)

### 1.16 Sessões

- [ ] `flash` kind
- [ ] `rotate_on_login` decorator
- [ ] `per_device` decorator
- [ ] `wizard` kind (multi-step state)

### 1.17 Notificações

- [ ] `digest` decorator em notification
- [ ] `throttle` decorator em notification
- [ ] `delivery_receipt` / `read_receipt` decoradores

### 1.18 Realtime

- [ ] `channel` kind
- [ ] `presence` kind
- [ ] `broadcast` kind
- [ ] `subscription` kind

### 1.19 Observabilidade

- [ ] `log_level` declarado em `app.lzi`
- [ ] `span` decorator nomeado
- [ ] Trace propagation manifest (estender `agent_run` para outros built-ins)

### 1.20 Testes

- [ ] `case` em command/job/workflow (hoje só em rule/agent)
- [ ] `factory` kind
- [ ] `artifact` decorator em test
- [ ] `test` block formalizado em rule (já existe ad-hoc)

### 1.21 Configuração

- [ ] `secret_manager` kind (declarar provider em `registry.lzi`)
- [ ] `rollout` decorator em feature_flag (percentage, user/tenant target)
- [ ] `config_inspect` CLI

### 1.22 Internacionalização

- [ ] `locale` kind
- [ ] `translation` kind
- [ ] `locale_negotiate` middleware decorator
- [ ] Doctor rule: missing translation reporting
- [ ] CLI: `lazuli translate extract`

### 1.23 Admin

- [ ] `admin_resource` kind
- [ ] `admin_dashboard` kind
- [ ] `admin_action` kind

### 1.24 SaaS / multi-tenant

- [ ] `subdomain` / `custom_domain` decoradores em tenant
- [ ] `tenant_sso` decorator
- [ ] `quota` / `limit` kind no tenant
- [ ] `tenant_webhook` kind
- [ ] `tenant_storage_namespace` / `tenant_cache_namespace` decoradores
- [ ] `tenant_migration` kind (já listado em §1.6)

### 1.25 Pagamentos / billing

- [ ] `subscription` kind
- [ ] `plan` kind
- [ ] `invoice` kind
- [ ] `entitlement` kind
- [ ] `quota` kind (compartilhado com SaaS)
- [ ] `revenue_event` event_group

### 1.26 Storage

- [ ] `storage` kind
- [ ] `bucket` kind
- [ ] `signed_url` decorator
- [ ] `public` / `private` decoradores
- [ ] `storage_quota` decorator no tenant

### 1.27 Busca

- [ ] `index` kind
- [ ] `facet` decorator
- [ ] `ranking` decorator

### 1.28 Relatórios / exportação

- [ ] `report` kind
- [ ] `export` kind
- [ ] `import_wizard` kind

### 1.29 Qualidade

- [ ] `error_code` namespace
- [ ] `compatibility_layer` kind
- [ ] Doctor rule: deprecation warning

---

## §2. Runtime / Framework (Drusa) — ~485 capabilities DF

A maior parte do trabalho. Hoje Drusa está em **~5%** (spike CRUD + queries). Tudo abaixo precisa ser construído. Ordenado por prioridade de bloqueio de produção.

### 2.1 HTTP / servidor (P1 — bloqueia produção)

- [ ] HTTP/1.1 + HTTP/2 + keep-alive + timeouts
- [ ] mTLS, ACME/Let's Encrypt, HSTS native
- [ ] Streaming / chunked / SSE
- [ ] WebSocket server (sem rooms/presence — F)
- [ ] Multipart parser robusto
- [ ] Static files com fingerprint + manifest
- [ ] Reverse proxy + forwarded headers + real IP
- [ ] Compressão gzip native
- [ ] ETag + conditional requests + Cache-Control + Range
- [ ] CSRF via `net/http.CrossOriginProtection` (Go 1.26)
- [ ] Middleware: circuit breaker, retry, body parser, validation, observability, slow request, timeout
- [ ] Panic recovery
- [ ] Custom error pages (renderiza decorador da linguagem)
- [ ] Maintenance mode

### 2.2 Observabilidade (P1 — bloqueia produção)

- [ ] `log/slog` + `slog.NewMultiHandler` (Go 1.26)
- [ ] Structured JSON + text logs
- [ ] Log sampling
- [ ] Log redaction (consome `@pii` annotations)
- [ ] Request / query / job logs auto-instrumentados
- [ ] OpenTelemetry traces + metrics + logs
- [ ] Trace propagation + span attributes
- [ ] Spans built-in: error, DB, HTTP, queue, agent (`agent_run`)
- [ ] Runtime metrics (`runtime/metrics`)
- [ ] Health / readiness / liveness / startup / dependency checks
- [ ] `/debug/pprof` + `runtime/pprof`
- [ ] `runtime/trace` + `runtime/trace.FlightRecorder` (Go 1.26)
- [ ] Profiles: memory, CPU, mutex, block, alloc, goroutine leak
- [ ] GC / scheduler metrics
- [ ] Panic reporting
- [ ] Build info + version endpoint
- [ ] Log correlation com request ID

### 2.3 Database operacional (P1)

- [ ] Connection pool tuning
- [ ] Read replicas + primary/replica routing (consome decoradores da linguagem)
- [ ] Health checks
- [ ] Query logging + slow query log + query comments + query tracing (spans)
- [ ] Prepared statements automáticos
- [ ] Nested transactions + savepoints
- [ ] Unit of work pattern
- [ ] Lazy/preloading mechanics
- [ ] Cursor + offset pagination
- [ ] Bulk insert / update / upsert
- [ ] Batch queries
- [ ] Pessimistic locking (consome decorador)
- [ ] ULID / Snowflake ID generation
- [ ] Advisory locks

### 2.4 Migrations (P1)

- [ ] SQL migrations execution (atlas-backed)
- [ ] Transactional + non-transactional modes
- [ ] Online migrations (zero-downtime helpers)
- [ ] Migration locking + status + rollback + redo + squashing
- [ ] Database create / drop / reset / truncate commands
- [ ] Seed loader

### 2.5 CLI (P1)

- [ ] `lazuli new`
- [ ] `lazuli serve` (dev + prod)
- [ ] `lazuli console` (REPL com bindings)
- [ ] `lazuli db {create,drop,migrate,rollback,seed,reset,status}`
- [ ] `lazuli make {model,migration,job,mailer,command,policy,middleware,request,resource,serializer,test,scaffold,crud}` (admin gated)
- [ ] `lazuli destroy scaffold`
- [ ] `lazuli doctor` (estender — já existe parte)
- [ ] `lazuli upgrade`
- [ ] `lazuli fmt`
- [ ] `lazuli lint`
- [ ] `lazuli test` (cobre evals + Go tests + sync test)
- [ ] `lazuli bench`
- [ ] `lazuli profile`
- [ ] `lazuli trace`
- [ ] `lazuli env`
- [ ] `lazuli secrets`
- [ ] `lazuli deploy`
- [ ] `lazuli routes` (a partir de IR)
- [ ] `lazuli middleware`
- [ ] `lazuli graph` (dependency)
- [ ] `lazuli config`
- [ ] `lazuli health`
- [ ] `lazuli build-info`
- [ ] Shell completions

### 2.6 Testes (P1)

- [ ] Test scaffolding por kind (unit/integration/system/feature/request/controller/model/job/mailer/policy/view/component/golden/API/E2E/benchmark/fuzz)
- [ ] `testing/synctest` integração (Go 1.26)
- [ ] Virtualized time helpers
- [ ] `testing.ArtifactDir` + `T.Attr` + `T.Output` (Go 1.26)
- [ ] Testcontainers integration
- [ ] Database test transactions + reset
- [ ] Parallel test isolation
- [ ] Fakes: mailer, queue, cache, clock, events, HTTP client
- [ ] HTTP recorder helpers
- [ ] Snapshot serializers
- [ ] Mock / stub / spy codegen
- [ ] Coverage reports
- [ ] Race detector wiring
- [ ] Leak detection
- [ ] Allocation assertions
- [ ] CI test matrix templates + test sharding

### 2.7 Segurança (P2)

- [ ] Native crypto helpers (`crypto/hpke`, `crypto/mlkem`, `crypto/tls` PQ, `crypto/fips140`)
- [ ] `runtime/secret` integration (Go 1.26)
- [ ] Password pepper + key derivation
- [ ] Envelope encryption
- [ ] HTML escaping automático
- [ ] SQL injection safeguards (codegen)
- [ ] Safe file serving
- [ ] Dependency vulnerability scanning hook
- [ ] SBOM generation
- [ ] SLSA / provenance hooks
- [ ] Request body redaction
- [ ] At-rest encryption helpers

### 2.8 Jobs / filas (P2 — River-based)

- [ ] Exponential backoff
- [ ] Dead-letter queue
- [ ] Concurrency limits
- [ ] Job cancellation
- [ ] Job progress reporting
- [ ] Job metrics
- [ ] Job dashboard UI
- [ ] Graceful worker shutdown

### 2.9 Eventos (P2)

- [ ] Event replay mechanics
- [ ] Event store backend (PostgreSQL default)

### 2.10 Cache (P2)

- [ ] HTTP cache
- [ ] Fragment / query / model / view / template cache
- [ ] Cache warming
- [ ] Cache metrics
- [ ] Two-level cache (local + remote)
- [ ] ETag integration

### 2.11 Sessões (P2)

- [ ] Redis / database / memory session stores
- [ ] Session invalidation + audit + cleanup
- [ ] Session TTL / renewal

### 2.12 Email / notificações (P2)

- [ ] SMTP server
- [ ] Transactional + bulk mail
- [ ] Attachments + inline attachments
- [ ] Mail previews (dev) + sandbox
- [ ] Mail retries
- [ ] Bounce handling
- [ ] Unsubscribe links

### 2.13 Realtime (P3 — Cut realtime gated)

- [ ] WebSocket server completo
- [ ] SSE server
- [ ] Pub/sub mechanics
- [ ] Presence tracking + heartbeats
- [ ] Reconnect handling + backpressure + connection draining
- [ ] Realtime metrics / tracing

### 2.14 APIs (P2)

- [ ] OpenAPI validation + UI
- [ ] Server stubs
- [ ] gRPC adapter
- [ ] JSON-RPC
- [ ] ConnectRPC
- [ ] Contract tests runner
- [ ] HATEOAS helpers
- [ ] API analytics

### 2.15 Views / templates (P2)

- [ ] Template inheritance / layouts / partials (atrás de `.lzx`)
- [ ] Markdown rendering
- [ ] Syntax highlighting
- [ ] Asset fingerprinting + manifest
- [ ] Source maps

### 2.16 JSON v2 (P2)

- [ ] `encoding/json/v2` + `jsontext` (Go 1.26)
- [ ] NDJSON streaming
- [ ] Streaming JSON parsers

### 2.17 Configuração (P2)

- [ ] Runtime config reload (sem restart)

### 2.18 Internacionalização (P3 — Cut i18n)

- [ ] ICU message format
- [ ] Pluralization + gender rules
- [ ] Date / time / number / currency localization
- [ ] Timezone propagation (user + tenant)

### 2.19 Admin (P3 — Cut admin)

- [ ] Admin generator + CRUD + dashboards UI
- [ ] Admin forms / tables / filters / search / sorting / pagination
- [ ] Bulk actions + export / import
- [ ] Admin notifications + charts + custom pages + theme + menu builder + breadcrumbs

### 2.20 SaaS / multi-tenant (P2)

- [ ] Tenant analytics dashboards

### 2.21 Pagamentos / billing (P3 — Cut billing)

- [ ] Trials, coupons, receipts, taxes mechanics
- [ ] Usage / metered billing
- [ ] Payment methods, dunning, billing portal
- [ ] Webhook verification, plan changes, cancellation, grace periods, refunds

### 2.22 Storage (P2)

- [ ] Local storage adapter
- [ ] Direct uploads + multipart + resumable
- [ ] File deduplication + versioning + lifecycle policies

### 2.23 Busca (P3)

- [ ] SQL full-text + PostgreSQL tsvector
- [ ] Async indexing + reindex CLI
- [ ] Search analytics

### 2.24 Relatórios (P3)

- [ ] CSV / Excel / JSON / XML export
- [ ] PDF generation
- [ ] Report builder UI (admin gated)
- [ ] Dashboard widgets + chart helpers
- [ ] CSV / Excel / JSON import + validation reports + rollback

### 2.25 Deploy / ops (P2 — Cut deploy)

- [ ] Dockerfile + Docker Compose + Kubernetes manifests + Helm chart + systemd + Procfile generators
- [ ] Multi-stage + distroless + static binary builds
- [ ] CGO / no-CGO modes + cross compilation
- [ ] Release + rollback commands
- [ ] Blue-green + canary deployment
- [ ] Migrations on deploy + pre/post hooks
- [ ] Health gates + smoke tests
- [ ] Runtime config + secrets injection
- [ ] Autoscaling metrics

### 2.26 Performance (P3)

- [ ] Zero-allocation hot paths
- [ ] Pooling opt-in (`sync.Pool`)
- [ ] Buffer pooling
- [ ] `sync.OnceValue` / `OnceFunc` / `WaitGroup.Go` (Go 1.26)
- [ ] `unique` package integration
- [ ] Fast router + params
- [ ] Streaming parsers + backpressure
- [ ] Connection reuse + query batching
- [ ] N+1 detection (doctor rule)
- [ ] GC-aware defaults + Green Tea GC awareness + container-aware GOMAXPROCS
- [ ] Memory budget config

### 2.27 Go 1.26 nativos (P2 — toolchain wiring)

- [ ] Toolchain management + `go.mod`/`go.work` awareness
- [ ] `go vet` integration (waitgroup, hostport analyzers)
- [ ] `errors.AsType` adoption
- [ ] `reflect.Type.Fields/Methods` + `reflect.TypeAssert`
- [ ] `net/http.HTTP2Config` + `Transport.NewClientConn` + `ReverseProxy.Rewrite`
- [ ] `io.ReadAll` improvements + `bytes.Buffer.Peek`
- [ ] `os.Process.WithHandle` + `signal.NotifyContext` cancel cause
- [ ] `io/fs.ReadLinkFS` + `MapFS` symlink + `tar.Writer.AddFS`
- [ ] `os.Root` helpers + `hash.Cloner`

### 2.28 Qualidade (P2)

- [ ] Plugin API + extension/generator/middleware/driver/provider registries
- [ ] Semantic versioning + upgrade guides + LTS policy
- [ ] Internal diagnostics
- [ ] Documentation generator + API reference generator

---

## §3. Adapters — ~70 primários + secundários DA

Registry-driven, plugados via `registry.lzi`. **Pick-one-primary** por categoria: Rule Zero.

### 3.1 Primários (alvo must-have, ~20 — ainda nenhum wired em Drusa)

> Todos abaixo são **alvo declarado** em `docs/architecture.md` / `docs/target-stack.md`, **não implementação shipada**. O primeiro adapter realmente wired vai surgir junto com §0 buckets (auth → Redis sessions; storage → S3; jobs → River; observability → slog/OTEL).

- [ ] **DB**: PostgreSQL via `pgx/v5`
- [ ] **Router**: `chi`
- [ ] **Migrations**: `atlas` + `golang-migrate`
- [ ] **Logs**: `slog` (stdlib) + OpenTelemetry
- [ ] **Validation**: `validator/v10`
- [ ] **Jobs**: `river` (Postgres-backed)
- [ ] **Cache / sessions**: Redis (`redis/go-redis`)
- [ ] **Storage**: S3 (AWS SDK)
- [ ] **Email**: Sendgrid (primário)
- [ ] **LLM**: OpenAI + Anthropic (alvo; depende de runtime de agent dispatch)
- [ ] **Observability backend**: Sentry (errors) + OTEL collector
- [ ] **Search**: Meilisearch
- [ ] **Billing**: Stripe
- [ ] **Bundler**: esbuild
- [ ] **Frontend**: React (web) + Expo (mobile)

### 3.2 Secundários (provider alternatives, ~50)

- [ ] **DB extras**: MySQL/MariaDB, SQLite (`modernc.org/sqlite`), CockroachDB, ClickHouse, MongoDB
- [ ] **Cloud storage**: GCS, Azure Blob, MinIO
- [ ] **Email extras**: Mailgun, SES, Postmark, Resend
- [ ] **Notification**: Twilio (SMS), Firebase (push), Slack webhook, Discord webhook
- [ ] **Payment extras**: Paddle, Mercado Pago, PayPal
- [ ] **Auth providers**: GitHub, Google, Microsoft, Apple
- [ ] **Search extras**: Typesense, Elasticsearch, OpenSearch, Algolia
- [ ] **Queues alternativos**: SQS, Pub/Sub, NATS, Kafka, RabbitMQ
- [ ] **Cache extras**: Memcached, Valkey
- [ ] **APM**: Datadog, Honeycomb, New Relic, Grafana Tempo, Jaeger, Zipkin
- [ ] **KMS**: AWS / GCP / Azure / Vault, SOPS
- [ ] **Feature flags**: GrowthBook (ou similar)
- [ ] **Authz engines**: Casbin, OPA/Rego, Spicedb (Zanzibar)
- [ ] **Deploy**: Fly.io, Render, Railway, Cloud Run, ECS, Terraform module
- [ ] **CI templates**: GitHub Actions, GitLab CI
- [ ] **Translation management**: Lokalise, Crowdin, Phrase
- [ ] **Image / video**: libvips, ffmpeg
- [ ] **PDF**: gotenberg ou similar
- [ ] **Compression**: brotli, zstd
- [ ] **Data warehouse**: Snowflake, BigQuery (BI export)

---

## §4. Pilot-gated — ~115 features F

Em design ou aprovado mas não shipado até pilot evidence. Gates documentados nas propostas linkadas.

### 4.1 AI primitives — Cut A.5, A.6 (já em design)

- [ ] **A.5** [`safety` list (multi-class PII coverage)](proposals/ai-primitives-cut-a-5.md) — gate: primeiro pilot com multi-class PII fan-in
- [ ] **A.6** [tool result schema](proposals/ai-primitives-cut-a-6.md) — gate: primeiro pilot referenciando `tools.<x>.<field>` em prompt/eval

### 4.2 AI primitives — Cut B (deferred, em design)

- [ ] **B.1** `flow` (multi-step agent orchestration) — gate: pilot com ≥2 agent steps com handoff
- [ ] **B.2** `budget tokens` (cost enforce) — gate: pilot com SLO de custo
- [ ] **B.3** `knowledge` (RAG) — gate: pilot com retrieval estruturado
- [ ] **B.4** `quota cost` (per-tenant) — gate: pilot SaaS com tiers

### 4.3 AI primitives — Cuts D–H (audit completo, pilot-gated)

- [ ] **D** multi-slot `context` block
- [ ] **E** `calls agent.<name>(args)` em jobs ([proposal](proposals/ai-primitives-cut-e.md))
- [ ] **F** `input from contract.X.Y` (record reuse de contract.lzi)
- [ ] **G** `calls contract.X.Y` em agent body
- [ ] **H** typed prompt manifest inline ([pressure-2](proposals/pressure-2-typed-prompt-manifest.md))

### 4.4 Component capsules cross-platform

- [ ] Estender `.lzi` para primitivas reusáveis compartilhadas web (React) + mobile (Expo)

### 4.5 Cut admin (gated por pilot SaaS)

- [ ] Admin kinds em linguagem (já listados em §1.23)
- [ ] Admin generator em runtime (§2.19)
- [ ] Admin theme + custom pages + impersonation UI

### 4.6 Cut billing (gated por pilot SaaS com pagamento)

- [ ] Subscription/Plan/Invoice/Entitlement/Quota/RevenueEvent kinds (§1.25)
- [ ] Stripe adapter primário (§3.1)
- [ ] Billing mechanics (§2.21)

### 4.7 Cut realtime (gated por pilot com colaboração/presença real)

- [ ] Channel/Presence/Broadcast/Subscription kinds (§1.18)
- [ ] WebSocket / SSE runtime (§2.13)

### 4.8 Cut media (gated por pilot com upload+processing)

- [ ] Image processing
- [ ] Thumbnail generation
- [ ] Video / audio processing hooks
- [ ] Metadata extraction
- [ ] File virus scanning
- [ ] CDN integration
- [ ] Backup integration

### 4.9 Cut search avançado (gated por pilot multilingue)

- [ ] Multilingual search
- [ ] Synonyms
- [ ] Highlighting

### 4.10 Cut reports avançado (gated por pilot com report builder visual)

- [ ] Report builder visual (admin-integrated)
- [ ] ETL jobs

### 4.11 Cut performance (gated por evidência de carga real)

- [ ] Load testing command
- [ ] Bench / profile / trace / flame graph commands
- [ ] Performance regression checks em CI

### 4.12 Starter kits (gated por v0 estabilizar)

- [ ] SaaS starter
- [ ] API starter
- [ ] Admin starter
- [ ] Microservice starter
- [ ] CLI starter

---

## Notas de execução

**Ordem sugerida** (revisada — alinhada com §0):

1. **Ciclo L0→L2 nos 4 buckets-piloto** (§0): auth/session, storage/file upload, jobs/queue, observability/health/logging. Cada bucket fecha o pipeline inteiro (fixture → parser → IR → codegen → Drusa executa → eval → doctor → LSP) **antes** de espalhar DL solto.
2. **Segunda onda** depois do ciclo provado: cache, notifications expandidas, webhooks, migrations runtime, OpenAPI gen, admin básico — cada um seguindo o mesmo ciclo L0→L2.
3. **DF P1 restante** (§2.1 HTTP avançado, §2.2 observabilidade full, §2.3 DB operacional, §2.4 migrations, §2.5 CLI, §2.6 testes) — preenche os gaps de runtime que os buckets-piloto não cobriram.
4. **DA primários** (§3.1) — surgem encadeados aos buckets (Redis/S3/River com os buckets-piloto; outros conforme demanda).
5. **DL médios** (§1.6/§1.7/§1.9/§1.10/§1.11/§1.12) — depois que P1 está estável e o ciclo está rodando.
6. **DF P2 / P3** em paralelo conforme adapters surgem.
7. **F gated** — só quando pilot valida (Cuts B, admin, billing, realtime, media, search avançado, reports visual).

**Não-objetivos preservados** (~260 N):

- Adapters paralelos a chi/PostgreSQL/React/Expo (Rule Zero).
- ORMs paralelos (gorm/ent/sqlc/bun/sqlboiler) — Lazuli gera SQL próprio.
- Test frameworks paralelos (ginkgo/gomega/testify) — usa `testing` + synctest nativos.
- DI libs (wire/fx/dig) — DI é implícita.
- Multi-config formats (YAML/TOML/JSON paralelos a `.lzi`).
- Layout Rails-style por tipo (`app/controllers`, etc).
- Multi-template frameworks (HTMX/Alpine/Vue/Svelte paralelos a React).
- Visual editor / reverse-engineering de schema.
- Legacy: long polling, XML APIs, Heroku adapter, FIPS antigo.

A justificativa completa de cada N vive em [`docs/audit/framework-coverage-1400.md`](audit/framework-coverage-1400.md).
