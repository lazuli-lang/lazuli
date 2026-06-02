//! API-HANDLER-UNWIRED-001 — an `api` contract whose runtime `Handler`
//! cannot be wired, so the endpoint is silently never mounted (404).
//!
//! ## History (the bug this rule was born for, now fixed)
//!
//! An `api <name>` block declares `method` / `path` / `output` / `policy`
//! and (optionally) a `handler @fn.<name>` reference. The Go codegen
//! (`crates/lazuli_codegen_go/src/emitter/api/mod.rs`) emits the typed
//! contract value
//!
//! ```text
//! var getAttachmentURLApi = lazuli.Api[Args, Out]{
//!     Name:    "attachments.get_attachment_url",
//!     Method:  lazuli.MethodGet,
//!     Path:    "/attachments/{id}/url",
//!     Policy:  ...,
//! }
//! func init() { lazuli.RegisterApi(&getAttachmentURLApi) }
//! // Wire getAttachmentURLApi.Handler in your application code, then call
//! // `lazuli.ValidateApiHandlers()` at startup to fail fast on omissions.
//! ```
//!
//! Note what is missing: the `Handler` field is **never set**. The
//! `handler @fn.<name>` reference from the DSL is dropped on the floor —
//! codegen does *not* bridge the declared `@fn` to the api's `Handler`
//! func. The contract self-registers (so `RegisterApi` runs) but its
//! `Handler` stays `nil`.
//!
//! At runtime the mount loop in `runtime/go/lazuli/http.go` skips any
//! registration whose handler is unwired:
//!
//! ```text
//! for _, api := range GlobalRegistry.Apis() {
//!     if api.Dispatch == nil || api.Method == "" { continue }
//!     if api.HandlerChecker != nil && !api.HandlerChecker() { continue } // <- nil Handler → skipped
//!     mux.HandleFunc(string(api.Method)+" "+api.Path, ...)
//! }
//! ```
//!
//! `HandlerChecker()` is `func() bool { return api.Handler != nil }`
//! (`runtime/go/lazuli/registry_typed.go`). A `nil` Handler → the route
//! is never added to the mux → the endpoint returns **404** even though
//! the DSL declared it. The author only discovers the dead endpoint in
//! production (or never, if they trust the green `go build`).
//!
//! ## Fixed by the codegen bridge (wave 3)
//!
//! `crates/lazuli_codegen_go/src/emitter/api/mod.rs` now bridges the
//! declared handler into the emitted `Handler` field via
//! `lazuli.HandlerFromRegistry[Args, Out]("<feature>.<name>")` — the
//! api-surface analogue of the command surface's `ReturnsFromRegistry`.
//! The handler resolves at dispatch time from the global fn registry, so
//! the `Handler` is a non-nil closure, `HandlerChecker()` is true, the
//! route mounts, and the endpoint serves. Generated `<f>gen` code never
//! imports the user handler package; the author only registers the fn
//! (`lazuli.RegisterFn("<feature>.<name>", Fn)`), exactly as for commands.
//!
//! ## The rule now
//!
//! Because every well-formed api carries a registry key derived from its
//! handler reference (`@fn.<name>` -> `<feature>.<name>`; convention /
//! file -> `<feature>.<api name>`), the bridge can always wire it. So the
//! rule is QUIET on well-formed apis. It fires when an `api` declares a
//! genuinely un-bridgeable handler: one whose `handler` reference is
//! empty/malformed (an example: `handler @fn.` with no name) so
//! the emitter cannot derive a stable registry key (an `@fn.` with no
//! name, or a blank path). The normally-lowered IR never trips this (a
//! missing `handler` defaults to a convention `./api/<name>.go` path); the
//! rule is the last-line guard for a structurally broken handler ref.
//!
//! ## Severity
//!
//! A declared endpoint that 404s is a concrete wiring bug, not style
//! drift — so `error` at `production` / `iron-hand`, downgraded to
//! `warning` at `prototype` / `strict`. This module returns plain
//! [`Finding`] values; the profile mapping lives in the doctor
//! correctness aggregator (mirrors `HANDLER-SIGNATURE-MISMATCH-001`).

use std::path::{Path, PathBuf};

use lazuli_ir::{Api, Feature, PathSource};

/// One API-HANDLER-UNWIRED-001 finding — a declared `api` whose runtime
/// `Handler` will be nil, so the endpoint is never mounted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the `api` was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// `api <name>` — the declared endpoint name.
    pub api_name: String,
    /// HTTP method + path, surfaced verbatim so the author recognizes
    /// the dead endpoint (`GET /attachments/{id}/url`).
    pub method: String,
    pub route: String,
    /// The Go var name the codegen emits + that the author must wire
    /// (`getAttachmentURLApi`). Surfaced so the fix is copy-pasteable.
    pub var_name: String,
    /// The declared `handler @fn.<name>` reference, when the author wrote
    /// one (the **bridge gap** case). `None` when no handler was declared
    /// (codegen defaulted the path).
    pub declared_handler: Option<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "API-HANDLER-UNWIRED-001";

    /// Render the "api endpoint will not be mounted" message naming the
    /// dead route, the wiring site, and the `ValidateApiHandlers` call.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::api_handler_unwired_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("attachments.lzi"),
    ///     feature: "attachments".into(),
    ///     api_name: "get_attachment_url".into(),
    ///     method: "GET".into(),
    ///     route: "/attachments/{id}/url".into(),
    ///     var_name: "getAttachmentURLApi".into(),
    ///     declared_handler: Some("@fn.generate_signed_url".into()),
    /// };
    /// assert!(f.message().contains("404"));
    /// assert!(f.message().contains("getAttachmentURLApi.Handler"));
    /// ```
    pub fn message(&self) -> String {
        let cause = match &self.declared_handler {
            Some(h) => format!(
                "the declared `handler {h}` is not bridged to the generated `Handler` field by codegen",
            ),
            None => "no `handler` is declared and codegen emits no `Handler` field".to_owned(),
        };
        format!(
            "api `{}` ({} {}) will never be mounted: {}, so `{}.Handler` stays nil at runtime \
             and the route returns 404. Wire `{}.Handler = <yourHandler>` in your application code \
             (e.g. main.go) and call `lazuli.ValidateApiHandlers()` at startup to fail fast on omissions.",
            self.api_name, self.method, self.route, cause, self.var_name, self.var_name,
        )
    }
}

/// Walk `feature` and emit one finding per declared `api` — every api is
/// dead-on-arrival because the current codegen wires no api `Handler`.
/// `lzi_path` anchors the resulting findings.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::api_handler_unwired_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with an api");
/// let _ = check(&feature, Path::new("attachments.lzi"));
/// ```
pub fn check(feature: &Feature, lzi_path: &Path) -> Vec<Finding> {
    feature
        .apis
        .iter()
        // Codegen now bridges every well-formed api's declared handler
        // into the runtime `Handler` field via `HandlerFromRegistry`
        // (wave-3 fix). The only api that stays unwireable is one whose
        // `handler` reference is structurally broken so the emitter
        // cannot derive a stable registry key — i.e. an empty handler
        // path. Such apis 404 exactly as before; well-formed apis are
        // now mounted, so the rule must stay quiet on them.
        .filter(|api| !api_is_bridgeable(api))
        .map(|api| Finding {
            path: lzi_path.to_path_buf(),
            feature: feature.name.clone(),
            api_name: api.name.clone(),
            method: http_method_token(api).to_owned(),
            route: api.path.clone(),
            var_name: api_var_name(&api.name),
            declared_handler: declared_handler(api),
        })
        .collect()
}

/// Whether codegen can bridge `api`'s declared handler into a non-nil
/// runtime `Handler`. True for every normally-lowered api: the emitter
/// derives a registry key `<feature>.<short>` where `<short>` is the
/// `@fn.<name>` name (authored `@fn` form) or the api name (convention
/// `./api/<name>.go` / file path). The bridge fails ONLY when the handler
/// reference is empty — an `@fn.` with no name, or a blank path — leaving
/// no stable key to register under. This mirrors
/// `crates/lazuli_codegen_go/src/emitter/api/mod.rs::api_handler_short`.
fn api_is_bridgeable(api: &Api) -> bool {
    let path = api.handler.path.trim();
    if path.is_empty() {
        // No handler reference at all → no registry key.
        return false;
    }
    if let Some(name) = path.strip_prefix("@fn.") {
        // `@fn.` with an empty name → unbridgeable; otherwise fine.
        return !name.trim().is_empty();
    }
    // A file / convention path (`./api/<name>.go`) bridges under the api
    // name; any non-empty reference is bridgeable.
    true
}

/// Uppercase HTTP method token for the message (`GET`, `POST`, ...).
fn http_method_token(api: &Api) -> &'static str {
    use lazuli_ir::HttpMethod::*;
    match api.method {
        Get => "GET",
        Post => "POST",
        Put => "PUT",
        Patch => "PATCH",
        Delete => "DELETE",
    }
}

/// The declared `handler @fn.<name>` reference, when the author wrote
/// one. The analyzer stores an authored handler as a [`PathSource::Authored`]
/// [`lazuli_ir::PathRef`] (`@fn.me`, `./api/foo.go`); a missing handler
/// defaults to a [`PathSource::Convention`] `./api/<name>.go`. We surface
/// only the authored form so the message can call out the **bridge gap**.
fn declared_handler(api: &Api) -> Option<String> {
    match api.handler.source {
        PathSource::Authored => Some(api.handler.path.clone()),
        PathSource::Convention => None,
    }
}

/// Reproduce the Go var name the api emitter assigns
/// (`<lowerCamel(name)>Api`, see
/// `crates/lazuli_codegen_go/src/emitter/api/mod.rs:emit_api`). Acronym
/// casing in the real emitter (`getAttachmentURL`) is best-effort here;
/// the var name is advisory copy-paste, not a load-bearing match.
fn api_var_name(name: &str) -> String {
    format!("{}Api", lower_camel(name))
}

/// Minimal `snake_case` → `lowerCamel` for the advisory var name. Mirrors
/// the common case; the emitter's acronym-aware caser is not duplicated
/// here (the value is only surfaced as a copy-paste hint).
fn lower_camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for (i, ch) in name.chars().enumerate() {
        if ch == '_' || ch == '-' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else if i == 0 {
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, HttpMethod, PathRef, Policies, PolicyRef, TypeRef,
    };
    use std::collections::BTreeMap;

    fn mk_api(name: &str, handler: PathRef) -> Api {
        Api {
            name: name.to_owned(),
            method: HttpMethod::Get,
            path: format!("/{name}"),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            rate_limit: None,
            output: TypeRef::Builtin(BuiltinType::Boolean),
            handler,
            locale_negotiate: None,
            deprecated: None,
            span_ref: None,
        }
    }

    fn mk_feature(apis: Vec<Api>) -> Feature {
        Feature {
            name: "attachments".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis,
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn negative_declared_fn_handler_is_bridged_now() {
        // `handler @fn.generate_signed_url` declared → codegen now bridges
        // it into `Handler` via HandlerFromRegistry → the route mounts →
        // the rule is quiet. The real pauta case, now fixed.
        let api = mk_api("get_attachment_url", PathRef::authored("@fn.generate_signed_url"));
        let feature = mk_feature(vec![api]);
        let findings = check(&feature, Path::new("features/attachments/attachments.lzi"));
        assert!(
            findings.is_empty(),
            "a declared @fn handler is bridged now and must not fire: {findings:#?}"
        );
        assert_eq!(Finding::CODE, "API-HANDLER-UNWIRED-001");
    }

    #[test]
    fn negative_convention_handler_is_bridged_now() {
        // Convention path (no `handler` line → `./api/me.go`) bridges under
        // the api name → the route mounts → quiet.
        let api = mk_api("me", PathRef::convention("./api/me.go"));
        let feature = mk_feature(vec![api]);
        let findings = check(&feature, Path::new("features/account/account.lzi"));
        assert!(
            findings.is_empty(),
            "a convention handler is bridged now and must not fire: {findings:#?}"
        );
    }

    #[test]
    fn positive_empty_fn_handler_is_unbridgeable() {
        // `@fn.` with no name → the emitter cannot derive a registry key →
        // the handler stays effectively unwireable → the rule fires (the
        // last-line guard for a structurally broken handler ref).
        let api = mk_api("broken", PathRef::authored("@fn."));
        let feature = mk_feature(vec![api]);
        let findings = check(&feature, Path::new("features/x/x.lzi"));
        assert_eq!(findings.len(), 1, "{findings:#?}");
        let msg = findings[0].message();
        assert!(msg.contains("404"), "{msg}");
    }

    #[test]
    fn positive_blank_handler_path_is_unbridgeable() {
        let api = mk_api("blank", PathRef::authored(""));
        let feature = mk_feature(vec![api]);
        let findings = check(&feature, Path::new("features/x/x.lzi"));
        assert_eq!(findings.len(), 1, "{findings:#?}");
    }

    #[test]
    fn negative_feature_without_apis_does_not_fire() {
        let feature = mk_feature(vec![]);
        let findings = check(&feature, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn well_formed_apis_stay_quiet() {
        // Two normally-lowered apis (one @fn, one convention) → both
        // bridged → no findings.
        let feature = mk_feature(vec![
            mk_api("a", PathRef::authored("@fn.a")),
            mk_api("b", PathRef::convention("./api/b.go")),
        ]);
        let findings = check(&feature, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty(), "{findings:#?}");
    }
}
