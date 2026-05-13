# Runtime Vendor Audit — 2026-05-13

Audit pós-Gate 3 do `runtime/go/lazuli/`. Levantamento de violações da política de namespace (`@runtime/` vs `@plugin/`) introduzidas no batch GPT 375-commit.

**Contexto:** core repo passou de ~150 arquivos pré-batch para 849 .go files (128.798 LOC não-teste). Muitos deles são bindings SaaS proprietários que deveriam viver em repos `@plugin/<nome>` separados (privados).

**Política (de `project_plugin_namespace_policy.md`):**
- `@runtime/<name>` — adapters commodity (postgres, redis, s3, smtp). OSS amplamente adotado, sem lock-in vendor. Vive no core repo.
- `@plugin/<name>` — proprietário ou opinionado (Stripe, SendGrid, Mercadopago, Datadog, etc.). Vive em repo separado privado.

---

## Resumo executivo

| Categoria | Arquivos | LOC aprox. | Ação |
|---|---|---|---|
| **EXTRACT** → repos `@plugin/<nome>` | ~60 .go (+testes) | ~25k | Mover pra plugins privados; deletar do core |
| **MOVE-TO-LAZURITE** → distro | ~16 .go (deploy/) | ~10k | Deploy não é Lazuli core; vai pra Lazurite |
| **REVIEW** — caso a caso | ~10 .go | ~3k | Decisão pendente (Prometheus, OpenFeature, etc.) |
| **CORE** — fica | ~360 .go | ~90k | Abstrações + commodities OSS |

---

## EXTRACT — vendor SaaS / proprietário

Cada bloco vira um repo `github.com/lazurite/lazuli-plugin-<nome>` separado (privado por padrão).

### email — 5 vendors

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `email/sendgrid.go` + `_test.go` | 49 | `@plugin/sendgrid` |
| `email/mailgun.go` + `_test.go` | ? | `@plugin/mailgun` |
| `email/postmark.go` + `_test.go` | ? | `@plugin/postmark` |
| `email/resend.go` + `_test.go` | ? | `@plugin/resend` |
| `email/ses.go` + `_test.go` | ? | `@plugin/ses` (AWS) |

**Fica em core (`@runtime/smtp`):** `smtp.go`, `smtp_dev_server.go`, `smtp_server.go` — SMTP é protocolo aberto.
**Fica em core (abstrações):** `bounce.go`, `bulk.go`, `delivery.go`, `message.go`, `preview.go`, `retry.go`, `template.go`, `unsubscribe.go`.

### payments — 5 vendors

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `payments/stripe/*` (2 files) | ? | `@plugin/stripe` |
| `payments/mercadopago/*` (4 files) | 841+ | `@plugin/mercadopago` *(memória 2026-05-11)* |
| `payments/pagarme.go` + `_test.go` | ? | `@plugin/pagarme` |
| `payments/paypal.go` + `_test.go` | ? | `@plugin/paypal` |
| `payments/pix.go` + `_test.go` | ? | `@plugin/pix` (rail BR, opinionado) |

**Fica em core:** `contract.go`, `idempotency.go`, `lifecycle.go`, `webhook_signature.go`, `webhook.go`.

### maps — 5 vendors

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `maps/google.go` + `_test.go` | 171 | `@plugin/google_maps` *(decisão hostpoint 2026-05-11)* |
| `maps/here.go` + `_test.go` | ? | `@plugin/here_maps` |
| `maps/mapbox.go` + `_test.go` | ? | `@plugin/mapbox` |
| `maps/maptiler.go` + `_test.go` | ? | `@plugin/maptiler` |
| `maps/nominatim.go` + `_test.go` | ? | `@plugin/nominatim` (OSM service) |

**Fica em core:** `contract.go`, `fake.go` (test double).

### observability — 2 SaaS

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `observability/datadog.go` + `_test.go` | 299 | `@plugin/datadog` |
| `observability/sentry.go` + `_test.go` | 358 | `@plugin/sentry` (tem OSS edition, mas 95%+ usa SaaS) |

**REVIEW:** `observability/prometheus.go` (305 LOC) — Prometheus é CNCF OSS, formato `/metrics` é padrão. Ou fica como `@runtime/prometheus_exporter`, ou vira plugin. Recomendação: **fica em core** (formato é commodity standard).

**Fica em core (abstrações OTel):** `audit.go`, `buildinfo.go`, `go_metrics.go`, `health.go`, `log_sampling.go`, `logging.go`, `metrics.go`, `oplabels.go`, `panic*.go`, `pprof.go`, `profile*.go`, `ring.go`, `sink.go`, `trace*.go`, `tracing.go`.

### featureflags — 3 (4?) vendors

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `featureflags/launchdarkly.go` + `_test.go` | 352 | `@plugin/launchdarkly` (SaaS clássico) |
| `featureflags/growthbook.go` + `_test.go` | ? | `@plugin/growthbook` (OSS mas ferramenta específica) |
| `featureflags/unleash.go` + `_test.go` | ? | `@plugin/unleash` (OSS mas ferramenta específica) |

**REVIEW:** `featureflags/openfeature.go` (CNCF spec) — pode virar a **abstração** core (contrato) e os 3 acima implementam ela como plugins. Recomendação: **fica em core como contract layer**.

### notifications — 5 SaaS

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `notifications/discord.go` + `_test.go` | ? | `@plugin/discord` |
| `notifications/slack.go` + `_test.go` | 287 | `@plugin/slack` |
| `notifications/twilio.go` + `_test.go` | 503 | `@plugin/twilio` (SMS/voice) |
| `notifications/pagerduty.go` + `_test.go` | ? | `@plugin/pagerduty` |
| `notifications/expo.go` + `_test.go` | 378 | `@plugin/expo_push` *(decisão hostpoint 2026-05-11)* |

**REVIEW:** `notifications/fcm.go` (728 LOC) — FCM (Firebase Cloud Messaging) é plataforma Google, mas é o caminho default pra Android push. Pode ser commodity. Recomendação: **vira `@plugin/fcm`** porque exige credenciais Google e é proprietário.

**Fica em core:** `webpush.go` (W3C standard), `bulk_plan.go`, `contract.go`, `delivery_receipt.go`, `digest_*.go`, `dispatch.go`, `idempotency.go`, `inapp.go`, `read_receipt.go`, `receipts.go`, `send_options.go`, `template.go`, `throttle_store.go`.

### search — 4 vendors

| Arquivo | LOC | Plugin alvo |
|---|---|---|
| `search/algolia.go` + `_test.go` | ? | `@plugin/algolia` |
| `search/meilisearch.go` + `_test.go` | ? | `@plugin/meilisearch` |
| `search/typesense.go` + `_test.go` | ? | `@plugin/typesense` |
| `search/opensearch.go` + `_test.go` | ? | `@plugin/opensearch` |

**Fica em core (PG-based search):** `tsvector.go`, `like.go`, `index_plan.go`, `facet.go`, `highlight.go`, `reindexer.go`, `analytics.go`.

### cache — 1 ambíguo

| Arquivo | LOC | Decisão |
|---|---|---|
| `cache/memcached.go` | ? | **CORE** (OSS commodity, protocolo aberto) |
| `cache/redis.go` | 229 | **CORE** (OSS commodity) |
| `cache/valkey.go` | 457 | **CORE** (Linux Foundation fork OSS de Redis) |

Cache fica todo em core. Nenhum extract.

### queues — 1 ambíguo

| Arquivo | LOC | Decisão |
|---|---|---|
| `queues/kafka.go` | 463 | **CORE** (Apache Kafka, OSS commodity) |
| `queues/nats.go` | ? | **CORE** (CNCF OSS) |
| `queues/rabbitmq.go` | ? | **CORE** (OSS commodity) |
| `queues/pubsub.go` | ? | **CORE** (abstração) |
| `queues/sqs.go` | 368 | **REVIEW** — AWS-specific. Talvez `@plugin/sqs` |

---

## MOVE-TO-LAZURITE — deploy/

Tese: **Lazuli não orquestra deploy.** Lazuli emite binário Go runnable. Como o usuário deploya é decisão dele (ou da distro Lazurite).

`runtime/go/lazuli/deploy/` (16 arquivos não-teste, ~10k LOC):

| Arquivo | LOC | Destino |
|---|---|---|
| `deploy/fly.go` + `_test.go` | 752 | `lazurite/deploy/fly` ou `@plugin/fly` |
| `deploy/helm.go` + `_test.go` | ? | `lazurite/deploy/helm` |
| `deploy/cloudrun.go` + `_test.go` | ? | `lazurite/deploy/cloudrun` |
| `deploy/github_actions.go` + `_test.go` | ? | `lazurite/ci/github_actions` |
| `deploy/kubernetes.go` + `_test.go` | ? | `lazurite/deploy/k8s` |
| `deploy/terraform.go` + `_test.go` | ? | `lazurite/deploy/terraform` |
| `deploy/dockerfile.go` + `_test.go` | ? | `lazurite/deploy/docker` |
| `deploy/compose.go` + `_test.go` | ? | `lazurite/deploy/compose` |
| `deploy/autoscaling.go`, `bluegreen.go`, `cross_compile.go`, `health_gate.go`, `migration_gate.go`, `process.go`, `release.go`, `secrets.go` | ? | `lazurite/deploy/runtime` (abstrações) |

**Sugestão pragmática:** primeiro **deletar deploy/ inteiro** do core; reintroduzir em lazurite quando for projetada com contratos sãos. (Versionado em git history se precisar reaver.)

---

## REVIEW — decisão pendente

| Arquivo | LOC | Pergunta |
|---|---|---|
| `observability/prometheus.go` | 305 | Commodity standard ou plugin? |
| `featureflags/openfeature.go` | ? | Contract layer (core) ou plugin? |
| `notifications/fcm.go` | 728 | Platform commodity ou plugin? |
| `queues/sqs.go` | 368 | AWS commodity ou plugin? |
| `migrations/atlas.go` | ? | Atlas é uma ferramenta específica — mover? |
| `storage/minio.go` | ? | MinIO é OSS S3-compatible — fica? (sim provavelmente) |

---

## Plano de execução (waves sequenciais)

Cada wave: 1 commit de remoção do core + git tag com snapshot do código removido (caso queira reusar no plugin repo depois).

### Wave A — vendor SaaS clássicos (baixo risco)
1. Delete `email/{sendgrid,mailgun,postmark,resend,ses}.go` + tests
2. Delete `payments/{stripe/,mercadopago/,pagarme.go,paypal.go,pix.go}` + tests
3. Delete `maps/{google,here,mapbox,maptiler,nominatim}.go` + tests
4. Delete `observability/{datadog,sentry}.go` + tests
5. Delete `featureflags/{launchdarkly,growthbook,unleash}.go` + tests
6. Delete `notifications/{discord,slack,twilio,pagerduty,expo}.go` + tests
7. Delete `search/{algolia,meilisearch,typesense,opensearch}.go` + tests

Verificação: `go build ./lazuli/...` + `cargo check --all-targets` após cada delete.

### Wave B — deploy/ inteiro
1. Delete `runtime/go/lazuli/deploy/` inteiro
2. Atualizar `docs/architecture.md` + `docs/runtime-handoff.md` removendo menções

### Wave C — REVIEW items (uma decisão por vez)
1. fcm.go → plugin (recomendado)
2. sqs.go → plugin (recomendado)
3. atlas.go → review
4. prometheus.go → fica (recomendado)
5. openfeature.go → fica como contract (recomendado)

### Wave D — docs cleanup
Atualizar `docs/project-structure.md`, `docs/canonical-semantics.md`, `docs/next-checklist.md`, `docs/runtime-handoff.md`, `docs/quickref.md` removendo referências vendor.

### Wave E — IR/descriptor cleanup
Verificar se `crates/lazuli_codegen_go/` emite descritores pra vendors removidos; remover esses emitters.

---

## Snapshot pré-cleanup

Antes de qualquer delete:
- Branch tag: `git tag -a runtime-pre-vendor-audit-2026-05-13 -m "snapshot before vendor extraction"`
- Pushed to origin pra recuperação se necessário.

LOC total provável removida: **~35-40k LOC** (vendor + deploy).
LOC restante (core): **~90k LOC** ainda inflado mas legítimo.

---

## Não fazer (yet)

- **Não criar os repos `@plugin/<nome>`** ainda. Só deletar do core e capturar em git tag. Criar os repos quando o usuário decidir reativar cada plugin.
- **Não tocar em `examples/full-capsule/`** ou outras fixtures (separado do bug).
- **Não tocar em `crates/lazuli_ir/`** — IR não tem vendor binding direto.
