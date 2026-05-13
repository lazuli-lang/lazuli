# Lazuli Production Runbook

Single-page operations reference for a Lazuli Go runtime process. Keep this
file next to the service deployment and update service-specific endpoints,
queries, and SLO links before promotion.

## Environment

Required production configuration is explicit. Do not rely on local defaults
outside dev or smoke environments.

```bash
export LAZULI_PORT="8080"
export LAZULI_DB="postgres://user:pass@postgres:5432/app?sslmode=require"
export LAZULI_LOG_LEVEL="info"          # debug, info, warn, error
export LAZULI_PPROF="0"                 # 1 only during incident response
export LAZULI_OTEL_EXPORTER="otlp-http" # noop, otlp-http
export LAZULI_GRACE_PERIOD="30s"
```

`LAZULI_PORT` is the service listen port. Some generated roots and runtime
helpers may still read `PORT` or `LAZULI_ADDR`; deployment manifests should map
the single production value to the entrypoint actually used by this service.

```bash
# Compatibility wrapper for older generated main.go files.
export PORT="${LAZULI_PORT}"
export LAZULI_ADDR=":${LAZULI_PORT}"
```

`LAZULI_DB` is the Postgres connection string used at boot. Include TLS,
timeouts, and pool parameters in the DSN when the entrypoint uses pgx pool
parsing directly.

```bash
export LAZULI_DB="postgres://user:pass@postgres:5432/app?sslmode=require&connect_timeout=5&pool_max_conns=20&pool_min_conns=2"
```

## Healthchecks

Use dependency-aware readiness for traffic and shallow liveness for restarts.
Never use a dependency-heavy check as a Kubernetes liveness probe.

`/readyz` returns 200 only when the process should receive traffic. In
Kubernetes, use it for `readinessProbe` and rollout gates. It should fail before
graceful shutdown begins.

`/livez` is the preferred liveness endpoint when mounted by the service. It
must return 200 when the process can answer HTTP, regardless of DB or queue
health. In Kubernetes, use it for `livenessProbe`.

`/healthz` is the baseline shallow health endpoint mounted by the runtime. Use
it for load balancer smoke checks and as liveness only when `/livez` is not
mounted. Do not use shallow `/healthz` as readiness unless this service has no
external dependencies.

```bash
BASE="http://127.0.0.1:${LAZULI_PORT:-8080}"

curl -fsS "$BASE/healthz"
curl -fsS "$BASE/readyz"
curl -fsS "$BASE/livez"
```

Kubernetes probe defaults:

```yaml
readinessProbe:
  httpGet: { path: /readyz, port: http }
  periodSeconds: 5
  timeoutSeconds: 2
  failureThreshold: 3
livenessProbe:
  httpGet: { path: /livez, port: http }
  periodSeconds: 10
  timeoutSeconds: 2
  failureThreshold: 3
startupProbe:
  httpGet: { path: /readyz, port: http }
  periodSeconds: 5
  failureThreshold: 24
```

## Logging

Production logs are single-line structured logs to stdout. Use JSON for
shipping and `info` as the normal production floor; raise to `debug` only for a
bounded incident window.

Expected request fields are `method`, `path`, `status`, and `duration_ms`.
Service wiring should also include `request_id`, `trace_id`, `tenant`, `actor`,
`feature`, `op`, `code`, and `error` when those values exist.

Default redaction replaces sensitive fields with `[REDACTED]`. The built-in key
set is `password`, `secret`, `token`, `api_key`, `authorization`, and `cookie`;
PII-tagged generated fields should not be logged raw.

```bash
export LAZULI_LOG_LEVEL="warn"
kubectl logs deploy/lazuli-api --since=10m \
  | jq -c 'select(.status >= 500 or .level == "ERROR")'
```

## Tracing

Set `LAZULI_OTEL_EXPORTER=noop` to disable export. Set
`LAZULI_OTEL_EXPORTER=otlp-http` for OpenTelemetry OTLP/HTTP export and use the
standard OTel environment variables for collector details.

```bash
export LAZULI_OTEL_EXPORTER="otlp-http"
export OTEL_SERVICE_NAME="lazuli-api"
export OTEL_EXPORTER_OTLP_ENDPOINT="https://otel-collector.example.com"
export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer ${OTEL_TOKEN}"
export OTEL_TRACES_SAMPLER="parentbased_traceidratio"
export OTEL_TRACES_SAMPLER_ARG="0.05"
```

For incident correlation, pass incoming `traceparent` and `X-Request-ID`
through gateways. Logs should carry the resolved `trace_id` so a failed request
can be joined to spans.

## Metrics

Prometheus scraping is available only when the service or adapter mounts it.
The default scrape path is `/metrics`; a 404 means metrics are not mounted in
this binary.

```bash
BASE="http://127.0.0.1:${LAZULI_PORT:-8080}"
curl -fsS "$BASE/metrics" | head
```

Use a 30s scrape interval and a timeout below the interval. Keep labels bounded:
never label metrics with raw user input, emails, IDs with high cardinality, or
full URLs.

Runtime diagnostic JSON may be mounted separately, commonly at
`/metrics/runtime`, and includes Go heap, GC, and goroutine counters.

```bash
curl -fsS "$BASE/metrics/runtime" | jq .
```

## Database Pool

Start with a small per-pod pool and scale from the database connection budget,
not CPU count. Reserve connections for migrations, admin access, and background
maintenance.

```bash
DB_MAX=300
RESERVED=50
REPLICAS=10
POOL_MAX=$(((DB_MAX - RESERVED) / REPLICAS))
echo "pool_max_conns=${POOL_MAX}"
```

Runtime defaults used by `lazuli.NewPool` are `MaxConns=25`, `MinConns=2`,
`MaxConnLifetime=30m`, `MaxConnIdleTime=5m`, and `HealthCheckPeriod=1m`.
Generated entrypoints that call `Boot(ctx, LAZULI_DB)` may inherit pgx DSN pool
parameters instead; check the service main before changing production limits.

During saturation, reduce per-pod pool size before increasing replicas. Watch
Postgres connection count, wait events, query latency, and readiness failures.

## Shutdown

SIGINT and SIGTERM should cancel the process context. The server marks
readiness unready before shutdown, stops accepting new connections, and waits
for active requests until the grace period expires.

```bash
export LAZULI_GRACE_PERIOD="30s"
kubectl rollout restart deploy/lazuli-api
kubectl wait --for=condition=available deploy/lazuli-api --timeout=120s
```

Set Kubernetes `terminationGracePeriodSeconds` greater than or equal to
`LAZULI_GRACE_PERIOD` plus a small buffer. Keep `preStop` hooks short and
idempotent.

```yaml
terminationGracePeriodSeconds: 35
lifecycle:
  preStop:
    exec:
      command: ["sh", "-c", "sleep 3"]
```

## Error Codes

HTTP errors are RFC 9457 problem details. `lazuli.Error` preserves the stable
code as the top-level `code` extension; non-Lazuli errors map to
`code=internal` and HTTP 500.

Common runtime mappings:

| Code | Typical HTTP | Action |
| --- | ---: | --- |
| `bad_request` | 400 | Fix malformed JSON, route params, or request shape. |
| `validation_failed` | 422 | Return field errors to caller; do not retry unchanged. |
| `policy_denied` | 403 | Check actor, role, tenant, and policy bindings. |
| `tenant_mismatch` | 403 | Check tenant resolution and cross-tenant access. |
| `not_found` | 404 | Verify identifier, tenant scope, and soft-delete state. |
| `method_not_allowed` | 405 | Check generated API method and ingress rewrite rules. |
| `rate_limited` | 429 | Back off; check rate-limit key and caller burst. |
| `integration_error` | 502 | Inspect downstream dependency and adapter logs. |
| `internal` | 500 | Treat as service bug or unhandled dependency failure. |

```bash
curl -fsS -X POST "$BASE/api/v1/q/customer.query.by_id" \
  -H 'content-type: application/json' \
  -d '{"id":999}' | jq '{status, code, detail}'
```

## pprof

Enable pprof only for a bounded incident window and only on a private network
path. `LAZULI_PPROF=1` should mount the standard handlers under
`/debug/pprof`.

```bash
kubectl set env deploy/lazuli-api LAZULI_PPROF=1
kubectl rollout status deploy/lazuli-api

POD="$(kubectl get pod -l app=lazuli-api -o jsonpath='{.items[0].metadata.name}')"
kubectl port-forward "$POD" 18080:"${LAZULI_PORT:-8080}"
```

Capture CPU, heap, goroutines, and trace artifacts, then disable pprof.

```bash
go tool pprof -seconds=30 "http://127.0.0.1:18080/debug/pprof/profile"
go tool pprof "http://127.0.0.1:18080/debug/pprof/heap"
curl -fsS "http://127.0.0.1:18080/debug/pprof/goroutine?debug=2" > goroutines.txt
curl -fsS "http://127.0.0.1:18080/debug/pprof/trace?seconds=10" > trace.out

kubectl set env deploy/lazuli-api LAZULI_PPROF=0
kubectl rollout status deploy/lazuli-api
```

## Smoke

Run these after deploy, rollback, secret rotation, and database failover.

```bash
BASE="https://api.example.com"

curl -fsS "$BASE/healthz" | jq .
curl -fsS "$BASE/readyz" | jq .
curl -fsS "$BASE/metrics" >/dev/null || echo "metrics not mounted"
```

Check one write path and one read path. Replace the command/query names with
this service's generated names and use a disposable tenant or fixture account.

```bash
curl -fsS -X POST "$BASE/api/v1/c/customer.create" \
  -H 'content-type: application/json' \
  -H "x-request-id: smoke-$(date +%s)" \
  -d '{"name":"Smoke Co","email":"smoke@example.invalid"}' | jq .

curl -fsS -X POST "$BASE/api/v1/q/customer.query.list" \
  -H 'content-type: application/json' \
  -d '{}' | jq '.[0] // empty'
```

When smoke fails, capture the request ID, response problem details, pod logs for
the same request ID, readiness output, and recent deployment revision before
restarting anything.
