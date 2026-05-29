//! HTTP hygiene aggregator (roadmap §1.2).
//!
//! Three small closed-catalog contracts that sit on `app.lzi`:
//!
//! - `app.cookie.<profile>` — `same_site` against `{lax, strict, none}`
//!   (RFC 6265bis CSRF policy) and `max_age` against the duration
//!   parser (`"7d"`, `"12h"`, …).
//! - `app.proxy` — `trusted` against the CIDR parser
//!   (`10.0.0.0/8`, `2001:db8::/32`, …) plus non-empty header names on
//!   `real_ip_header` / `forwarded_proto_header` / `forwarded_host_header`.
//! - `app.limits` — size knobs (`body_size` / `header_size` / `upload_size`)
//!   against the size parser (`"512b"`, `"16kb"`, `"10mb"`, …) and
//!   `timeout` against the duration parser.
//!
//! Runtime owns real validation (Go `net/url`, `time.ParseDuration`,
//! `netip.Prefix`). Doctor catches the obvious typos so an LLM cold-
//! reading the manifest sees the bar at compile time.

use crate::doctor::parsers::{
    catalog_list, is_parseable_cidr, is_parseable_duration, is_parseable_size,
};
use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity};

/// Closed catalog for `same_site`. CSRF policy per RFC 6265bis.
const COOKIE_SAME_SITE_CATALOG: &[&str] = &["lax", "strict", "none"];

pub(crate) fn cookie_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(cookie) = app_manifest.manifest.cookie.as_ref() else {
        return diagnostics;
    };

    for profile in &cookie.profiles {
        if let Some(token) = profile.same_site.as_deref() {
            if !COOKIE_SAME_SITE_CATALOG.contains(&token) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_cookie_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.cookie.{name}.same_site {token}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(COOKIE_SAME_SITE_CATALOG),
                        name = profile.name,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
        if let Some(raw) = profile.max_age.as_deref() {
            if !is_parseable_duration(raw) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_cookie_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.cookie.{name}.max_age \"{raw}\"` is not a parseable duration. Use forms like `\"7d\"`, `\"12h\"`, `\"30m\"`, `\"45s\"`.",
                        name = profile.name,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn proxy_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(proxy) = app_manifest.manifest.proxy.as_ref() else {
        return diagnostics;
    };

    for cidr in &proxy.trusted {
        if !is_parseable_cidr(cidr) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_proxy_contract_diagnostics".to_owned(),
                message: format!(
                    "`app.proxy.trusted \"{cidr}\"` is not a parseable CIDR. Use forms like `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `2001:db8::/32`.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // A missing header name on any of the three slots is a contract
    // error: the runtime needs a token to look up. Empty strings reach
    // here when the author wrote `real_ip_header ""` or
    // `real_ip_header` (with no value).
    let header_slots: [(&str, Option<&String>); 3] = [
        ("real_ip_header", proxy.real_ip_header.as_ref()),
        (
            "forwarded_proto_header",
            proxy.forwarded_proto_header.as_ref(),
        ),
        (
            "forwarded_host_header",
            proxy.forwarded_host_header.as_ref(),
        ),
    ];
    for (slot, value) in header_slots {
        if let Some(name) = value {
            if name.trim().is_empty() {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_proxy_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.proxy.{slot}` requires a non-empty header name (e.g. `X-Forwarded-For`). Remove the line to let the runtime fall back to its default.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn limits_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(limits) = app_manifest.manifest.limits.as_ref() else {
        return diagnostics;
    };

    let size_slots: [(&str, Option<&String>); 3] = [
        ("body_size", limits.body_size.as_ref()),
        ("header_size", limits.header_size.as_ref()),
        ("upload_size", limits.upload_size.as_ref()),
    ];
    for (slot, value) in size_slots {
        if let Some(raw) = value {
            if !is_parseable_size(raw) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_limits_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.limits.{slot} \"{raw}\"` is not a parseable size. Use forms like `\"512b\"`, `\"16kb\"`, `\"10mb\"`, `\"2gb\"`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    if let Some(raw) = limits.timeout.as_ref() {
        if !is_parseable_duration(raw) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_limits_contract_diagnostics".to_owned(),
                message: format!(
                    "`app.limits.timeout \"{raw}\"` is not a parseable duration. Use forms like `\"30s\"`, `\"5m\"`, `\"2h\"`.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}
