//! Roadmap §1.2 — `--expand=http` unified projection.
//!
//! Surfaces the three app-level HTTP hygiene blocks (`cookie`,
//! `proxy`, `limits`) under a single `http` field on the inspect
//! report. Each present block is included with `origin` metadata
//! mirroring `expand_auth`'s shape.
//!
//! Without `--expand=http` the typed blocks still serialize via
//! `AppManifest`, but the unified `http` projection at the report
//! root is omitted entirely. With the flag, the projection is
//! `null` when no block is populated (skipped via `Option`).

use lazuli_ir::{AppCookie, AppLimits, AppManifest, AppProxy, SpanRef};
use serde_json::{Map, Value, json};

/// Build the `http` projection from the lifted `AppManifest`. Returns
/// `None` when none of cookie / proxy / limits is present — the
/// caller's `Option<Value>` slot keeps the report stable.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::inspect::expand_http::expand_http;
///
/// // let http = expand_http(module.app.as_ref());
/// ```
pub fn expand_http(app: Option<&AppManifest>) -> Option<Value> {
    let app = app?;
    if app.cookie.is_none() && app.proxy.is_none() && app.limits.is_none() {
        return None;
    }

    let mut block = Map::new();
    block.insert("origin".to_owned(), origin(app.name.as_str(), app.span_ref));

    if let Some(cookie) = app.cookie.as_ref() {
        block.insert(
            "cookie".to_owned(),
            with_origin(
                serde_json::to_value(cookie).expect("AppCookie serializes"),
                app.name.as_str(),
                cookie.span_ref,
            ),
        );
    }

    if let Some(proxy) = app.proxy.as_ref() {
        block.insert(
            "proxy".to_owned(),
            with_origin(
                serde_json::to_value(proxy).expect("AppProxy serializes"),
                app.name.as_str(),
                proxy.span_ref,
            ),
        );
    }

    if let Some(limits) = app.limits.as_ref() {
        block.insert(
            "limits".to_owned(),
            with_origin(
                serde_json::to_value(limits).expect("AppLimits serializes"),
                app.name.as_str(),
                limits.span_ref,
            ),
        );
    }

    Some(Value::Object(block))
}

fn with_origin(mut value: Value, app_name: &str, span_ref: Option<SpanRef>) -> Value {
    if let Value::Object(object) = &mut value {
        object.insert("origin".to_owned(), origin(app_name, span_ref));
    }
    value
}

fn origin(app_name: &str, span_ref: Option<SpanRef>) -> Value {
    let mut origin = Map::new();
    origin.insert("app".to_owned(), Value::String(app_name.to_owned()));
    if let Some(span_ref) = span_ref {
        origin.insert("line".to_owned(), json!(span_ref.start));
    }
    Value::Object(origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::CookieProfile;

    fn empty_manifest() -> AppManifest {
        AppManifest {
            name: "Acme".to_owned(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: vec![],
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            uses: vec![],
            packs: vec![],
            bindings: vec![],
            architecture: None,
            services: vec![],
            communication: None,
            environments: vec![],
            urls: vec![],
            cors: None,
            env: vec![],
            integrations: vec![],
            capabilities: vec![],
            runtime: vec![],
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: vec![],
            cookie: None,
            proxy: None,
            limits: None,
            error_pages: vec![],
            headers: None,
            route_guard: None,
            actor_query: None,
            span_ref: None,
        }
    }

    #[test]
    fn expand_http_returns_none_when_no_block_is_populated() {
        let manifest = empty_manifest();
        assert!(expand_http(Some(&manifest)).is_none());
    }

    #[test]
    fn expand_http_returns_none_when_app_is_absent() {
        assert!(expand_http(None).is_none());
    }

    #[test]
    fn expand_http_emits_full_projection_when_all_three_blocks_are_populated() {
        let mut manifest = empty_manifest();
        manifest.cookie = Some(AppCookie {
            profiles: vec![CookieProfile {
                name: "default".to_owned(),
                signed: Some(true),
                secure: Some(true),
                http_only: Some(true),
                same_site: Some("strict".to_owned()),
                max_age: Some("7d".to_owned()),
                span_ref: None,
            }],
            span_ref: Some(SpanRef { start: 10, end: 20 }),
        });
        manifest.proxy = Some(AppProxy {
            trusted: vec!["10.0.0.0/8".to_owned(), "172.16.0.0/12".to_owned()],
            real_ip_header: Some("X-Forwarded-For".to_owned()),
            forwarded_proto_header: Some("X-Forwarded-Proto".to_owned()),
            forwarded_host_header: None,
            span_ref: Some(SpanRef { start: 25, end: 30 }),
        });
        manifest.limits = Some(AppLimits {
            body_size: Some("10mb".to_owned()),
            header_size: Some("16kb".to_owned()),
            upload_size: Some("100mb".to_owned()),
            timeout: Some("30s".to_owned()),
            span_ref: Some(SpanRef { start: 35, end: 40 }),
        });

        let projection = expand_http(Some(&manifest)).expect("projection populated");

        assert_eq!(projection["origin"]["app"], "Acme");
        assert_eq!(projection["cookie"]["origin"]["line"], 10);
        assert_eq!(projection["cookie"]["profiles"][0]["name"], "default");
        assert_eq!(projection["cookie"]["profiles"][0]["same_site"], "strict");
        assert_eq!(projection["cookie"]["profiles"][0]["max_age"], "7d");
        assert_eq!(projection["proxy"]["origin"]["line"], 25);
        assert_eq!(projection["proxy"]["trusted"][0], "10.0.0.0/8");
        assert_eq!(projection["proxy"]["real_ip_header"], "X-Forwarded-For");
        assert_eq!(projection["limits"]["origin"]["line"], 35);
        assert_eq!(projection["limits"]["body_size"], "10mb");
        assert_eq!(projection["limits"]["timeout"], "30s");
    }

    #[test]
    fn expand_http_skips_absent_blocks() {
        let mut manifest = empty_manifest();
        manifest.cookie = Some(AppCookie {
            profiles: vec![CookieProfile {
                name: "session".to_owned(),
                signed: None,
                secure: None,
                http_only: None,
                same_site: Some("lax".to_owned()),
                max_age: None,
                span_ref: None,
            }],
            span_ref: None,
        });

        let projection = expand_http(Some(&manifest)).expect("projection populated");

        assert!(projection.get("cookie").is_some());
        assert!(projection.get("proxy").is_none());
        assert!(projection.get("limits").is_none());
    }
}
