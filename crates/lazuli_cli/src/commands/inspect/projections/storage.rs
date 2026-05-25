//! `--expand=storage` projection plus the Phase L auth projection.
//!
//! Walks every feature's source lines for `@cap.File(...)` capability
//! sites and projects each through the typed analyzer pass. Two site
//! shapes are recognised:
//!
//! - `<field>: @cap.File(...)` inside a `resource <Name>` block — surfaces
//!   in `InspectStorage.fields`.
//! - `output @cap.File(...)` inside an `api <name>` block — surfaces in
//!   `InspectStorage.api_outputs`.
//!
//! Unparseable shapes are skipped silently so the LSP's existing
//! file-local diagnostics remain the canonical source of shape errors;
//! `lazuli inspect` is a read-only projection.
//!
//! The companion `project_auth` projects a lowered `ir::Auth` block
//! into the inspect-shaped `InspectAuth` carrier. Mirrors the IR
//! structure 1:1; the only translation is joining `FieldRef` back into
//! a `<Resource>.<field>` string so the JSON projection reads exactly
//! like the source surface. Each sub-block (identity, password,
//! sessions, mfa, oauth) carries the per-feature origin computed from
//! the auth block's `span_ref`.

use super::super::{
    InspectAuth, InspectAuthIdentity, InspectAuthMfa, InspectAuthOAuthProvider,
    InspectAuthPassword, InspectAuthSessions, InspectFileCapability, InspectFileSize,
    InspectMimeType, InspectOrigin, InspectStorage, InspectStorageApiOutput, InspectStorageField,
};
use super::super::expand::leading_spaces;
use super::super::formatters::{format_file_size_literal, format_file_visibility};

pub(in crate::commands::inspect) fn inspect_storage_projection(lines: &[String]) -> InspectStorage {
    let mut fields: Vec<InspectStorageField> = Vec::new();
    let mut api_outputs: Vec<InspectStorageApiOutput> = Vec::new();

    let mut current_resource: Option<String> = None;
    let mut current_api: Option<String> = None;
    let mut resource_indent: usize = 0;
    let mut api_indent: usize = 0;

    for raw in lines.iter() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(raw);

        // Detect entering / exiting resource and api blocks. The
        // existing canonical fixture uses 4-space resource headers
        // (inside `domain`) and 2-space api headers; we close the
        // block as soon as the indent retreats to the header level
        // or shallower.
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            current_resource = Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            current_api = None;
            resource_indent = indent;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("api ") {
            current_api = Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            current_resource = None;
            api_indent = indent;
            continue;
        }
        if current_resource.is_some() && indent <= resource_indent && !trimmed.is_empty() {
            current_resource = None;
        }
        if current_api.is_some() && indent <= api_indent && !trimmed.is_empty() {
            current_api = None;
        }

        // Try a resource-field shape: `<field>: @cap.File(...)`.
        if let Some(resource) = current_resource.as_deref() {
            if let Some((field_name, cap_text)) = extract_cap_file_field(trimmed) {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file)) =
                    lazuli_analyzer::type_ref_from_syntax_public(&cap_text)
                {
                    fields.push(InspectStorageField {
                        resource: resource.to_owned(),
                        field: field_name,
                        file_capability: project_file_capability(&file),
                    });
                }
            }
        }

        // Try an api-output shape: `output @cap.File(...)`.
        if let Some(api) = current_api.as_deref() {
            if let Some(cap_text) = trimmed
                .strip_prefix("output ")
                .map(str::trim)
                .filter(|rest| rest.starts_with("@cap.File("))
            {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file)) =
                    lazuli_analyzer::type_ref_from_syntax_public(cap_text)
                {
                    api_outputs.push(InspectStorageApiOutput {
                        api: api.to_owned(),
                        file_capability: project_file_capability(&file),
                    });
                }
            }
        }
    }

    InspectStorage {
        fields,
        api_outputs,
    }
}

/// Extract `(field_name, "@cap.File(...)")` from a `<field>: @cap.File(...) [required]`
/// resource line. Returns `None` if the line is not a `@cap.File` field.
fn extract_cap_file_field(trimmed: &str) -> Option<(String, String)> {
    let (name_part, type_part) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    // Drop trailing `required` / `optional` / annotation keywords so the
    // analyzer parses the bare type expression.
    let type_tokens = type_part.trim();
    let cap_start = type_tokens.find("@cap.File(")?;
    let from_cap = &type_tokens[cap_start..];
    let close = from_cap.find(')')?;
    let cap_text = &from_cap[..=close];
    Some((name.to_owned(), cap_text.to_owned()))
}

fn project_file_capability(file: &lazuli_ir::FileCapability) -> InspectFileCapability {
    InspectFileCapability {
        max_size: InspectFileSize {
            bytes: file.max_size.bytes,
            literal: format_file_size_literal(file.max_size.literal),
        },
        accept: file
            .accept
            .iter()
            .map(|m| InspectMimeType {
                family: m.family.clone(),
                subtype: m.subtype.clone(),
            })
            .collect(),
        visibility: file
            .visibility
            .map(|v| format_file_visibility(v).to_owned()),
        signed_ttl: file.signed_ttl.clone(),
    }
}

/// Phase L — project a lowered `ir::Auth` into the inspect-shaped
/// `InspectAuth`. Mirrors the IR structure 1:1; the only translation is
/// joining `FieldRef` back into a `<Resource>.<field>` string so the
/// json projection reads exactly like the source surface.
pub(in crate::commands::inspect) fn project_auth(feature_name: &str, auth: &lazuli_ir::Auth) -> InspectAuth {
    let origin = inspect_origin(feature_name, auth.span_ref);
    InspectAuth {
        origin: origin.clone(),
        identity: InspectAuthIdentity {
            field: format!(
                "{}.{}",
                auth.identity.field.resource.name, auth.identity.field.field
            ),
            resource: auth.identity.field.resource.name.clone(),
            origin: origin.clone(),
        },
        password: auth.password.as_ref().map(|p| InspectAuthPassword {
            algorithm: p.algorithm.clone(),
            hash: p.hash.clone(),
            verify: p.verify.clone(),
            // `ir-rate-limit-env-aware` cell 1 — inspect shim: surface
            // the default literal. Cell 3 extends the projection with
            // the env-qualified shape.
            rate_limit: p.rate_limit.as_ref().map(|spec| spec.default.clone()),
            origin: origin.clone(),
        }),
        sessions: auth.sessions.as_ref().map(|s| InspectAuthSessions {
            resource: s.resource.name.clone(),
            ttl: s.ttl.clone(),
            refresh: s.refresh,
            access_ttl: s.access_ttl.clone(),
            rotation: s.rotation.clone(),
            origin: origin.clone(),
        }),
        mfa: auth.mfa.as_ref().map(|m| InspectAuthMfa {
            method: m.method.clone(),
            enroll: m.enroll.clone(),
            verify: m.verify.clone(),
            adapter: m.adapter.clone(),
            origin: origin.clone(),
        }),
        oauth: auth
            .oauth
            .iter()
            .map(|o| InspectAuthOAuthProvider {
                provider: o.provider.clone(),
                adapter: o.adapter.clone(),
                origin: origin.clone(),
            })
            .collect(),
    }
}

fn inspect_origin(feature_name: &str, span_ref: Option<lazuli_ir::SpanRef>) -> InspectOrigin {
    InspectOrigin {
        feature: feature_name.to_owned(),
        line: span_ref.map(|span| span.start),
    }
}
