//! Manifest / runtime / deploy / auth vocabulary one-liners.
//!
//! Covers the first ~100 keywords by file order: `workspace`, `app`,
//! `registry`, `profile`, the workspace graph (`apps`,
//! `shared_registry`, `boundaries`, `gateway`), external contracts
//! (`contract`, `compatibility`, `import`, `operation`), environments
//! / URLs / bindings / packs / capabilities / integrations / services
//! / runtime units / deploy contracts, the `agent` + `notification` +
//! `channel` families, the `auth` block (identity, OAuth, MFA,
//! sessions, rotation TTLs), and feature-domain `errors` / `api` /
//! `event_group` / `event.trace`.

pub(crate) fn keyword_description(keyword: &str) -> Option<&'static str> {
    match keyword {
        "workspace" => Some(
            "Declares an optional distributed-system contract across local apps and external services.",
        ),
        "app" => Some("Declares the `.lzi` application entrypoint and operational contract."),
        "error_page" => Some(
            "Declares one custom app-level error response page. Header is `error_page <status>`; children are `template \"./...\"` and optional `audience <name>`.",
        ),
        "registry" => Some(
            "Declares the package-level catalog for env, capabilities, integrations, and packs.",
        ),
        "profile" => Some(
            "Declares environment-specific app overrides such as public URLs, sandbox integrations, binding overrides, and deploy topology.",
        ),
        "apps" => Some("Groups apps that participate in a workspace graph."),
        "shared_registry" => {
            Some("Declares the package registry shared by apps in a workspace contract.")
        }
        "boundaries" => Some("Groups workspace event publication and consumption edges."),
        "gateway" => {
            Some("Declares provider-neutral public ingress routes for a distributed workspace.")
        }
        "contract" => {
            Some("References a versioned external service contract, not an implementation.")
        }
        "compatibility" => Some("Declares the external contract compatibility policy."),
        "import" => Some(
            "Imports an external contract schema such as OpenAPI, AsyncAPI, Proto, JSON Schema, or Avro.",
        ),
        "operation" => Some("Declares one provider-neutral external service operation."),
        "environments" => {
            Some("Declares deployment/runtime environments such as local, staging, and production.")
        }
        "urls" => {
            Some("Declares public app URLs used by clients, CORS, emails, callbacks, and webhooks.")
        }
        "bindings" => Some(
            "Two shapes by context. (1) Under `app` / `profile`: binds abstract feature requirements to concrete integration entries (`<feature>.<slot> = integrations.<name>`). (2) Under `registry`: sugar alias for `integrations`, with the simplified `endpoint env.<NAME>` / `auth keys env.<ID> env.<SECRET>` child surface for adapter credential wiring (B1 — see W3-blockers + the canonical pilot-complete-roadmap §3.5).",
        ),
        "packs" => Some(
            "Declares Lazuli pack catalog entries in `registry.lzi` or enabled pack references in `app.lzi`.",
        ),
        "provides" => {
            Some("Declares what a registry pack provides, such as `provides feature payments`.")
        }
        "from" => Some(
            "Declares a source relationship, such as pack enablement or create-from-input sugar.",
        ),
        "capabilities" => Some(
            "Declares required runtime capabilities without choosing concrete infrastructure providers.",
        ),
        "integrations" => Some(
            "Declares external integration registry entries without provider-specific operation details.",
        ),
        "credentials" => {
            Some("Declares credential scope and bindings for an external integration.")
        }
        "data_classification" => Some("Declares the broad PII class returned by an integration."),
        "architecture" => Some(
            "Declares provider-neutral architecture mode and service-boundary enforcement intent.",
        ),
        "services" => Some("Declares logical service ownership boundaries for the app."),
        "service" => Some("Declares one logical app service boundary under `services`."),
        "owns" => Some("Declares which Lazuli features a logical service owns."),
        "exposes" => Some("Groups commands, queries, APIs, or workflows exposed by a service."),
        "publishes" => Some("Declares event patterns a logical service publishes."),
        "consumes" => Some("Declares external or cross-service events a logical service consumes."),
        "communication" => {
            Some("Declares sync/async intent and context propagation across service boundaries.")
        }
        "internal" => Some("Declares the internal sync communication contract."),
        "external" => Some("Declares the external communication contract."),
        "async" => Some("Declares the asynchronous communication contract."),
        "propagate" => Some("Declares context values propagated across service boundaries."),
        "timeout" => Some("Declares a default service-boundary timeout."),
        "runtime" => {
            Some("Declares generated runtime units such as api, web, worker, and scheduler.")
        }
        "unit" => Some("Declares one app runtime unit under the app manifest `runtime` block."),
        "mode" => Some("Declares the app architecture mode."),
        "service_ready" => Some(
            "Marks whether the app keeps service boundaries visible for future split deployments.",
        ),
        "enforce_service_boundaries" => {
            Some("Marks whether cross-service ownership boundaries should be enforced by tooling.")
        }
        "serves" => Some("Declares which contracts a runtime unit serves."),
        "runs" => Some("Declares which jobs or schedules a runtime unit runs."),
        "healthcheck" => Some("Declares a runtime healthcheck path for deploy safety."),
        "readiness" => Some("Declares a runtime readiness path for deploy safety."),
        "deploy" => {
            Some("Declares provider-neutral deploy gates such as migrations and rollback behavior.")
        }
        "migrations" => Some("Declares when deploy applies database migrations."),
        "migration_lock" => Some("Declares whether deploy must hold a migration lock."),
        "destructive_migrations" => Some("Declares how deploy handles destructive schema changes."),
        "rollback" => Some("Declares rollback behavior for failed deploy health checks."),
        "topology" => Some("Declares an environment deploy topology override in a profile."),
        "environment" => Some("Selects a provider environment such as sandbox or production."),
        "env" => Some("Declares typed environment variables and client/server exposure."),
        "aggregate" | "entity" => Some(
            "Declares a domain resource with fields and behavior. Inspect with `lazuli inspect <file> --expand=resources` to project the typed slice.",
        ),
        "record" => Some(
            "Declares a non-persisted typed result/DTO shape. Inspect with `lazuli inspect <file> --expand=records` to project the typed slice.",
        ),
        "agent" => Some(
            "Declares an LLM-powered capability with typed input, output, prompt template, model reference, policy, and rate limits. the runtime wires the LLM transport; Lazuli owns the contract.",
        ),
        "notification" => Some(
            "Declares a multi-channel outbound notification with `channel`, `recipient`, `trigger`, `template`, and `policy`. the runtime generates dispatch wiring; adapters (Sendgrid/SES/Twilio/APNs/FCM) handle transport.",
        ),
        "channel" => Some(
            "Two distinct uses, disambiguated by indent level:\n\n\
             • Feature-level kind: `channel <name>` declares a typed, tenant-scoped, \
               policy-gated push stream (realtime bucket cycle MVP). Required children: \
               `tenant_from <axis>`, `policy @policy.<name>`, `payload <RecordType>`. \
               Transport (WebSocket / SSE) is adapter-resolved at runtime; the language \
               declares the contract. Doctor: `CHANNEL-PAYLOAD-001`.\n\n\
             • On a `notification`, declares one or more delivery channels: \
               `email`, `push`, `sms`, `in_app`.",
        ),
        "recipient" => Some(
            "On a `notification`, declares the recipient expression (e.g., `target.email`, `payload.user_id`).",
        ),
        "template" => Some(
            "Points to a relative template file. Used by `notification` delivery templates and app-level `error_page` responses.",
        ),
        "digest" => Some(
            "On a `notification`, declares window-based aggregation. Children: `every \"<duration>\"` (required), `group_by <payload-path>`, `max_size <N>` (1..=10000), `template_strategy merge|append`. Distinct from scalar `rate_limit`. Doctor: `NOTIF-DIGEST-001/002/003`.",
        ),
        "throttle" => Some(
            "On a `notification`, declares structured per-recipient / per-channel rate-limit with optional burst. Children: `max_per \"<duration>\"` (required), `per_recipient`, `per_channel`, `burst <N>`. Distinct from scalar `rate_limit` (which is per-call). Doctor: `NOTIF-THROTTLE-001/002/003`.",
        ),
        "every" => Some(
            "On `notification.digest`, sets the aggregation window. Closed shape: `<N> (seconds|minutes|hours|days)`. Example: `every \"15 minutes\"`.",
        ),
        "group_by" => {
            Some("On `notification.digest`, keys the aggregation bucket on a payload path.")
        }
        "max_size" => Some(
            "On `notification.digest`, caps items per digest window. Range: 1..=10000. Above the ceiling buffers unbounded payloads.",
        ),
        "template_strategy" => Some(
            "On `notification.digest`, declares how the adapter combines per-trigger payloads when rendering the digest template. Closed catalog: `merge` (last-write-wins per key), `append` (emits a list).",
        ),
        "max_per" => Some(
            "On `notification.throttle`, sets the refill window for the rate-limit bucket. Closed shape: `<N> (seconds|minutes|hours|days)`.",
        ),
        "per_recipient" => Some(
            "On `notification.throttle`, keys the throttle bucket on the notification's `recipient <path>`. At least one of `per_recipient` or `per_channel` is required.",
        ),
        "per_channel" => Some(
            "On `notification.throttle`, gives each channel of a multi-channel notification its own bucket (e.g., email and `in_app` throttled independently).",
        ),
        "burst" => Some(
            "On `notification.throttle`, number of immediate dispatches the bucket allows before throttling starts. Useful for OTP / login flows.",
        ),
        "model" => {
            Some("On an `agent`, references the LLM model under the `@llm.<name>` namespace.")
        }
        "prompt" => Some("On an `agent`, points to the prompt template file at `./path`."),
        "tools" => Some(
            "On an `agent`, declares the closed list of `@tool.<name>` references the agent may invoke.",
        ),
        "safety" => Some(
            "On an `agent`, declares safety classifiers or policy checks applied to inputs/outputs.",
        ),
        "stream" => Some(
            "On an `agent` `output`, marks the response as a streamed value: `output stream <Type>`.",
        ),
        "command" => Some("Declares a write operation for an aggregate."),
        "query.list" => Some("Declares a generated collection query."),
        "query.lookup" => Some("Declares a generated single-record lookup query."),
        "query.sql" => Some("Declares a query backed by an external SQL file."),
        "query.view" => Some("Declares a typed SQL-backed screen-read query."),
        "defaults" => Some(
            "Declares repeated feature defaults such as tenancy and timestamps. Inspect with `lazuli inspect <file> --expand=defaults` to project the IR-driven defaults block.",
        ),
        "domain" => Some("Groups resources, records, queries, rules, and events."),
        "policies" => Some("Declares feature-local policy categories and field policies."),
        "auth" => Some(
            "Authentication block: groups identity, password, sessions, MFA, and OAuth subcontracts for a feature.",
        ),
        "identity" => Some(
            "`identity <Resource>.<field>` — names the resource field used as the canonical login identifier.",
        ),
        "oauth" => Some(
            "OAuth subcontract: `oauth <provider>` with `adapter @adapter.<x>`. v0 providers: `google`, `github`, `microsoft`, `apple`.",
        ),
        "mfa" => {
            Some("MFA subcontract: `mfa <method>` with `enroll` + `verify`. v0 method: `totp`.")
        }
        "sessions" => Some(
            "Sessions subcontract: backing resource, legacy `ttl`, optional short-lived `access_ttl`, and optional nested `rotation` refresh-token discipline.",
        ),
        "access_ttl" => Some(
            "Short-lived access token TTL. Used on every API request. Framework default under `rotation` is `15 minutes`. Shorter reduces stolen-access-token blast radius; longer reduces refresh round trips. Doctor warns above 1 hour (AUTH-REFRESH-005).",
        ),
        "refresh_ttl" => Some(
            "Long-lived refresh token TTL inside `auth.sessions.rotation`. Used only by the refresh endpoint. Framework default is `30 days`; it must exceed `access_ttl` so access tokens stay short-lived (AUTH-REFRESH-001).",
        ),
        "grace" => Some(
            "Refresh rotation grace window. A recently revoked refresh can still succeed briefly to absorb legitimate two-tab races. Framework default is `30 seconds`; longer windows weaken theft detection and doctor warns above 5 minutes (AUTH-REFRESH-004).",
        ),
        "theft_detection_action" => Some(
            "Closed auth-rotation catalog for revoked-refresh reuse past grace. `revoke_session_family` revokes this device chain; `revoke_user` revokes every session for the user. Framework default is `revoke_session_family`.",
        ),
        "refresh" => Some("Whether the session adapter issues refresh tokens. Default `false`."),
        "enroll" => {
            Some("Enrolment function reference (`@fn.*`) returning method-specific enrolment data.")
        }
        "verify" => {
            Some("Verification reference (`@fn.*` or `@validator.*`) returning success/failure.")
        }
        "errors" => Some("Declares public/private client error exposure defaults or cases."),
        "api" => {
            Some("Declares a custom typed HTTP endpoint outside command/query/webhook semantics.")
        }
        "event_group" => Some("Declares a shared same-feature event payload template."),
        "event.trace" => {
            Some("Declares an observability-only event that is outside the feature reaction graph.")
        }
        // CL.C.4 — domain-model vocabulary (roadmap §1.7).
        _ => None,
    }
}
