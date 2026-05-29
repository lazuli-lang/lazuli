//! Inspect expansion-axis selector.
//!
//! `--expand=<csv>` is the inspect command's projection-axis selector.
//! Each axis maps to a boolean flag on `ExpandSet`; the projector
//! checks the flag and either includes the typed sub-block in the
//! report or omits it. The `all` sentinel turns every flag on; the
//! `none` sentinel (and the empty string) leave them off.
//!
//! Keeping `ExpandSet` + `parse_expand_set` in their own file matches
//! the Rails-style refactor cadence:
//!
//! - Each `pub(crate)` field is a typed inspect-side observable.
//! - `all()` / `any()` / `labels()` stay `pub(super)`; only the
//!   parent `inspect/` module composes the report.
//! - `parse_expand_set` is the only public surface; the CLI / MCP
//!   server route every `--expand` string through it so the
//!   "unknown expansion" error message is centralized.
//!
//! Cross-refs:
//! - `crate::commands::inspect::inspect_command` dispatches on the
//!   parsed set.
//! - `tests.rs` calls `parse_expand_set` directly to mirror the CLI
//!   path without re-running the binary.

use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExpandSet {
    pub(crate) refs: bool,
    pub(crate) summary: bool,
    pub(crate) locators: bool,
    pub(crate) dependencies: bool,
    pub(crate) security: bool,
    pub(crate) events: bool,
    pub(crate) targets: bool,
    pub(crate) policies: bool,
    pub(crate) tests: bool,
    pub(crate) defaults: bool,
    /// Cut A — `--expand=tools` produces the per-agent dispatch graph
    /// keyed by tool reference. Per the proposal's "Pass model" note
    /// (plan §7.2), this is the first expansion that explicitly opts
    /// into cross-feature resolution.
    pub(crate) tools: bool,
    /// Cut A.7 — `--expand=expose` produces a unified HTTP route table
    /// across every `api` block and every agent declaring
    /// `expose http`. Cross-feature path collisions surface via doctor;
    /// this projection is the inspect-side observable.
    pub(crate) expose: bool,
    /// Phase L — `--expand=auth` projects the per-feature `auth` block
    /// (identity / password / sessions / mfa / oauth) from the canonical
    /// IR. Without the flag the projection is omitted entirely; this
    /// keeps the default inspect output stable.
    pub(crate) auth: bool,
    /// Phase L Tier 2 — `--expand=storage` projects every typed
    /// `@cap.File(...)` site (resource fields + api outputs) with the
    /// parsed `max_size`/`accept`/`visibility`/`signed_ttl`. Cross-
    /// feature symmetry checks live in doctor (storage bucket cycle);
    /// this projection is the per-feature observable.
    pub(crate) storage: bool,
    /// Observability bucket cycle row 36 — `--expand=tracing` /
    /// `--expand=logging`. The `AppManifest.logging` /
    /// `AppManifest.tracing` blocks always serialize when populated;
    /// these labels mark the report as having intentionally surfaced
    /// the observability axis so consumers (LLM, CLI users, docs)
    /// know the projection is current.
    pub(crate) tracing: bool,
    pub(crate) logging: bool,
    /// Roadmap §1.2 — `--expand=http` surfaces a unified `http`
    /// projection covering the three app-level HTTP hygiene blocks
    /// (`cookie` / `proxy` / `limits`). Each present block is
    /// included with `origin` metadata. Without the flag the typed
    /// blocks still serialize on `app` (because `AppManifest` carries
    /// them), but the unified projection at the report root is
    /// omitted.
    pub(crate) http: bool,
    /// Phase L Tier 3 — `--expand=jobs` projects every lifted
    /// `ir::Job` (handler-backed + declarative) on the feature.
    /// Without the flag the projection is omitted; with the flag the
    /// feature carries a `jobs` array mirroring `InspectAgent`.
    pub(crate) jobs: bool,
    /// Phase L Tier 3 — `--expand=webhooks` projects every lifted
    /// `ir::Webhook` on the feature.
    pub(crate) webhooks: bool,
    /// Phase L Tier 3 — `--expand=event_groups` projects every
    /// `ir::EventGroup` on the feature (pattern + inheritance).
    pub(crate) event_groups: bool,
    /// Migrations bucket cycle Route C — `--expand=migrations` projects
    /// every lifted `ir::TenantMigration` per feature and the
    /// app-level `deploy.checkpoint` + expansion fields.
    pub(crate) migrations: bool,
    /// Notifications expanded bucket cycle — `--expand=notifications`
    /// opts into the typed `digest` / `throttle` projection from the
    /// lifted `ir::Notification` slice. The scalar notification
    /// fields surface in default inspect regardless; this flag adds
    /// the structured sub-blocks so they appear without `--expand=all`.
    pub(crate) notifications: bool,
    /// Cache bucket cycle (CL.C.3) — `--expand=caches` projects every
    /// feature-level `cache <name>` profile (lifted `ir::CacheProfile`)
    /// per feature. Sibling of `--expand=jobs`/`webhooks`/`notifications`.
    /// Inline (per-query) cache blocks remain visible on each query's
    /// `cache` slot regardless of this flag; this flag controls the
    /// dedicated profile array.
    pub(crate) caches: bool,
    /// Roadmap §1.11 — `--expand=webhook_events` projects the package
    /// registry's canonical outbound webhook event schemas.
    pub(crate) webhook_events: bool,
    /// CL.C.4 — `--expand=aggregates` projects every lifted
    /// `ir::Aggregate` declaration on the feature, including the
    /// `root` resource, the `contains` cluster, and any invariants
    /// (with predicate text + message). Roadmap §1.7.
    pub(crate) aggregates: bool,
    /// Phase L Tier 4b — `--expand=commands` projects every lifted
    /// `ir::Command` on the feature (route + input + policy + audit +
    /// approval + invalidates + external_calls + rate_limit +
    /// timeout/retry/idempotency). Retires the legacy text-pattern
    /// command surface; mirrors `jobs`/`webhooks` shape.
    pub(crate) commands: bool,
    /// Phase L Tier 4b — `--expand=apis` projects every lifted
    /// `ir::Api` on the feature (method + path + output + policy +
    /// handler + locale_negotiate). Accepts `api` or `apis` token.
    pub(crate) apis: bool,
    /// Phase L Tier 4c — `--expand=resources` projects every lifted
    /// `ir::Resource` on the feature (fields + retention + has_many
    /// + constraints + validate + previous_names). Mirrors `commands`.
    pub(crate) resources: bool,
    /// Phase L Tier 4d — `--expand=queries` projects every lifted
    /// `ir::Query` on the feature (`List`/`Lookup`/`Sql` variants).
    pub(crate) queries: bool,
    /// Phase L Tier 4d — `--expand=records` projects every lifted
    /// `ir::Record` on the feature (fields + discriminator_field).
    pub(crate) records: bool,
    /// IR Error-Vocab (Cell PARSE-1) — `--expand=errors` projects the
    /// lifted `ir::FeatureErrors` block on the feature (exposure
    /// defaults + 4xx/5xx field allowlists + per-code message
    /// overrides). Mirrors `commands`/`apis` shape. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.6.
    pub(crate) errors: bool,
    /// `knowledge <sector>` (iron-hand context) — `--expand=knowledge`
    /// projects the feature's intent triad: `purpose` + `non_goals` +
    /// the `knowledge` sector slug, all read from the lowered IR. The
    /// scalar context fields surface here as a single grouped axis (the
    /// "memory is the compiler" projection); reading the on-disk
    /// `knowledge/<sector>/` vault is a later/runtime concern.
    /// See `docs/proposals/knowledge-sector-field.md`.
    pub(crate) knowledge: bool,
}

impl ExpandSet {
    pub(in crate::commands::inspect) fn all() -> Self {
        Self {
            refs: true,
            summary: true,
            locators: true,
            dependencies: true,
            security: true,
            events: true,
            targets: true,
            policies: true,
            tests: true,
            defaults: true,
            tools: true,
            expose: true,
            auth: true,
            storage: true,
            tracing: true,
            logging: true,
            http: true,
            jobs: true,
            webhooks: true,
            event_groups: true,
            migrations: true,
            notifications: true,
            caches: true,
            webhook_events: true,
            aggregates: true,
            commands: true,
            apis: true,
            resources: true,
            queries: true,
            records: true,
            errors: true,
            knowledge: true,
        }
    }

    pub(super) fn any(self) -> bool {
        self.refs
            || self.summary
            || self.locators
            || self.dependencies
            || self.security
            || self.events
            || self.targets
            || self.policies
            || self.tests
            || self.defaults
            || self.tools
            || self.expose
            || self.auth
            || self.storage
            || self.tracing
            || self.logging
            || self.http
            || self.jobs
            || self.webhooks
            || self.event_groups
            || self.migrations
            || self.webhook_events
            || self.notifications
            || self.caches
            || self.aggregates
            || self.commands
            || self.apis
            || self.resources
            || self.queries
            || self.records
            || self.errors
            || self.knowledge
    }

    pub(super) fn labels(self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.refs {
            labels.push("refs");
        }
        if self.summary {
            labels.push("summary");
        }
        if self.locators {
            labels.push("locators");
        }
        if self.dependencies {
            labels.push("dependencies");
        }
        if self.security {
            labels.push("security");
        }
        if self.events {
            labels.push("events");
        }
        if self.targets {
            labels.push("targets");
        }
        if self.policies {
            labels.push("policies");
        }
        if self.tests {
            labels.push("tests");
        }
        if self.defaults {
            labels.push("defaults");
        }
        if self.tools {
            labels.push("tools");
        }
        if self.expose {
            labels.push("expose");
        }
        if self.auth {
            labels.push("auth");
        }
        if self.storage {
            labels.push("storage");
        }
        if self.tracing {
            labels.push("tracing");
        }
        if self.logging {
            labels.push("logging");
        }
        if self.http {
            labels.push("http");
        }
        if self.jobs {
            labels.push("jobs");
        }
        if self.webhooks {
            labels.push("webhooks");
        }
        if self.event_groups {
            labels.push("event_groups");
        }
        if self.migrations {
            labels.push("migrations");
        }
        if self.notifications {
            labels.push("notifications");
        }
        if self.caches {
            labels.push("caches");
        }
        if self.webhook_events {
            labels.push("webhook_events");
        }
        if self.aggregates {
            labels.push("aggregates");
        }
        if self.commands {
            labels.push("commands");
        }
        if self.apis {
            labels.push("apis");
        }
        if self.resources {
            labels.push("resources");
        }
        if self.queries {
            labels.push("queries");
        }
        if self.records {
            labels.push("records");
        }
        if self.errors {
            labels.push("errors");
        }
        if self.knowledge {
            labels.push("knowledge");
        }
        labels
    }
}

pub(crate) fn parse_expand_set(value: &str) -> Result<ExpandSet> {
    let mut set = ExpandSet::default();

    for raw_item in value.split(',') {
        let item = raw_item.trim();
        if item.is_empty() || item == "none" {
            continue;
        }

        if item == "all" {
            return Ok(ExpandSet::all());
        }

        match item {
            "refs" => set.refs = true,
            "summary" => set.summary = true,
            "locators" => set.locators = true,
            "dependencies" => set.dependencies = true,
            "security" => set.security = true,
            "events" => set.events = true,
            "targets" => set.targets = true,
            "policies" => set.policies = true,
            "tests" => set.tests = true,
            "defaults" => set.defaults = true,
            "tools" => set.tools = true,
            "expose" => set.expose = true,
            "auth" => set.auth = true,
            "storage" => set.storage = true,
            "tracing" => set.tracing = true,
            "logging" => set.logging = true,
            "http" => set.http = true,
            "jobs" => set.jobs = true,
            "webhooks" => set.webhooks = true,
            "event_groups" => set.event_groups = true,
            "webhook_events" => set.webhook_events = true,
            // Migrations bucket cycle Route C — projects every lifted
            // `ir::TenantMigration` on the feature + the app deploy
            // block's checkpoint/strategy/lock_timeout/hook fields.
            "migrations" | "tenant_migrations" => set.migrations = true,
            // Notifications expanded bucket cycle — projects every
            // lifted `ir::Notification` with typed `digest` /
            // `throttle` sub-blocks. The scalar fields surface in
            // default inspect; this flag adds the structured shapes.
            "notifications" => set.notifications = true,
            // Cache bucket cycle (CL.C.3) — projects every lifted
            // `ir::CacheProfile` on the feature. Inline (per-query)
            // cache slots remain visible regardless of this flag;
            // this flag controls the dedicated profile array.
            "caches" => set.caches = true,
            // CL.C.4 — projects every lifted `ir::Aggregate` on the
            // feature (root + contains + invariants). Roadmap §1.7.
            "aggregates" => set.aggregates = true,
            // Phase L Tier 4b — projects every lifted `ir::Command` on
            // the feature (route + input + policy + audit + approval
            // + invalidates + external_calls + rate_limit +
            // timeout/retry/idempotency). Mirrors `jobs`/`webhooks`.
            "commands" => set.commands = true,
            // Phase L Tier 4b — projects every lifted `ir::Api` on the
            // feature (method + path + output + policy + handler +
            // locale_negotiate). Accepts both singular and plural to
            // mirror `migrations`/`tenant_migrations`.
            "api" | "apis" => set.apis = true,
            // Phase L Tier 4c — projects every lifted `ir::Resource`
            // on the feature (fields + retention + has_many +
            // constraints + validate + previous_names).
            "resources" => set.resources = true,
            // Phase L Tier 4d — projects every lifted `ir::Query`
            // on the feature (List / Lookup / Sql variants).
            "queries" => set.queries = true,
            // Phase L Tier 4d — projects every lifted `ir::Record`
            // on the feature (fields + discriminator_field).
            "records" => set.records = true,
            // IR Error-Vocab (Cell PARSE-1) — projects the lifted
            // `ir::FeatureErrors` block (exposure defaults + 4xx/5xx
            // field allowlists + per-code message overrides).
            "errors" => set.errors = true,
            // `knowledge <sector>` (iron-hand context) — projects the
            // feature intent triad (purpose + non_goals + knowledge
            // sector) from the lowered IR. Reading the on-disk
            // `knowledge/<sector>/` vault is a later concern.
            "knowledge" => set.knowledge = true,
            _ => bail!(
                "unknown inspect expansion `{item}`; use none, all, refs, summary, locators, dependencies, security, events, targets, policies, tests, defaults, tools, expose, auth, storage, tracing, logging, jobs, webhooks, event_groups, webhook_events, migrations, tenant_migrations, notifications, caches, aggregates, commands, api, apis, resources, queries, records, errors, or knowledge"
            ),
        }
    }

    Ok(set)
}
