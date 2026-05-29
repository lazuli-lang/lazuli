//! Indent-6 and indent-8 line handlers extracted from `manifest.rs`.
//!
//! `parse_app_manifest` walks `app.lzi` source line by line and
//! dispatches on `leading_spaces(line)`. The indent-2 and indent-4
//! arms live inline in `manifest.rs`; the indent-6 and indent-8 arms
//! land here so the state machine fits under the per-file LOC
//! ceiling without losing co-located state across the dispatch.
//!
//! State that needs to flow across handlers (open service / runtime
//! unit / encryption binding / cookie profile indices, plus the
//! sub-child tokens that route indent-8 lines) lives on
//! `ManifestParseState`. The handlers borrow `&mut AppManifest` and
//! `&mut ManifestParseState` together so each can mutate either side
//! freely without cloning indices around.

use lazuli_ir::{
    AppHsts, AppIntegrationCredentialBinding, AppIntegrationCredentials, AppManifest,
    AppServiceExposure, EncryptionAlgorithm, EncryptionRotation, EncryptionSource,
    EncryptionTemplate, LocaleNegotiate,
};

use super::parsers::{
    adapter_source_provenance, parse_app_env_var, parse_bool, parse_credential_binding,
    parse_hsts_inline, split_items, unquote,
};

/// Shared cursor state for `parse_app_manifest`. Each field tracks
/// the currently open sub-block at one of the four indent depths;
/// indent-2 transitions reset most of them. Lifetimes track the
/// `'static` interned child-token strings (`"integrations"`,
/// `"locale_negotiate"`, etc.) the dispatcher emits.
pub(super) struct ManifestParseState {
    pub current_child: Option<&'static str>,
    pub current_runtime_unit: Option<usize>,
    pub current_service: Option<usize>,
    pub current_service_child: Option<&'static str>,
    /// i18n bucket cycle — tracks which child of the current `runtime
    /// unit` is open (e.g. `locale_negotiate`). Indent-8 lines branch
    /// off this token rather than the top-level `current_child`.
    pub current_runtime_child: Option<&'static str>,
    pub current_env_group: Option<String>,
    pub current_integration: Option<usize>,
    pub current_integration_child: Option<&'static str>,
    pub current_error_page: Option<usize>,
    /// Encryption bucket cycle — tracks the open
    /// `encryption.key @key.<scope>` binding. Indent-6 lines
    /// (`source`, `algorithm`, `rotation`, `rotation_profile`)
    /// populate this index. `None` when no binding is currently open.
    pub current_encryption_binding: Option<usize>,
    /// Roadmap §1.10 — tracks the open `headers.hsts` sub-block.
    /// Indent-6 lines (`max_age`, `include_subdomains`, `preload`)
    /// populate the HSTS struct. `None` when no HSTS body is open.
    pub in_headers_hsts: bool,
    /// Roadmap §1.2 — tracks the open cookie profile (`default`,
    /// `session`, `csrf`, ...). Indent-4 lines headed by a bare ident
    /// open a profile; indent-6 lines (`signed`, `secure`, etc.)
    /// populate it.
    pub current_cookie_profile: Option<usize>,
}

impl ManifestParseState {
    pub(super) fn new() -> Self {
        Self {
            current_child: None,
            current_runtime_unit: None,
            current_service: None,
            current_service_child: None,
            current_runtime_child: None,
            current_env_group: None,
            current_integration: None,
            current_integration_child: None,
            current_error_page: None,
            current_encryption_binding: None,
            in_headers_hsts: false,
            current_cookie_profile: None,
        }
    }

    /// Indent-2 transition — clears every sub-block cursor (the new
    /// indent-2 line opens a fresh section). `current_child` is NOT
    /// reset here because indent-2 dispatch picks the next child
    /// itself; the caller writes the new value.
    pub(super) fn reset_for_indent2(&mut self) {
        self.current_runtime_unit = None;
        self.current_service = None;
        self.current_service_child = None;
        self.current_env_group = None;
        self.current_integration = None;
        self.current_integration_child = None;
        self.current_error_page = None;
        self.current_encryption_binding = None;
        self.in_headers_hsts = false;
        self.current_cookie_profile = None;
    }
}

/// Indent-6 dispatch — populates the currently open sub-block. The
/// outer `match` arm only routes by `current_child`; every leaf
/// reads its specific cursor (e.g. `current_integration` for the
/// `integrations` arm) and writes back via `&mut app`.
pub(super) fn handle_indent6(trimmed: &str, app: &mut AppManifest, state: &mut ManifestParseState) {
    if state.current_child == Some("env") {
        if let Some(group) = state.current_env_group.as_deref() {
            if let Some(env_var) = parse_app_env_var(trimmed, Some(group)) {
                app.env.push(env_var);
            }
        }
    } else if state.current_child == Some("integrations") {
        let Some(integration_index) = state.current_integration else {
            return;
        };
        let integration = &mut app.integrations[integration_index];
        if let Some(rest) = trimmed.strip_prefix("adapter ") {
            let adapter = rest.trim();
            integration.adapter = Some(adapter.to_owned());
            integration.adapter_provenance = adapter_source_provenance(adapter).map(str::to_owned);
            state.current_integration_child = None;
        } else if let Some(rest) = trimmed.strip_prefix("environments ") {
            integration.environments.extend(split_items(rest));
            state.current_integration_child = None;
        } else if let Some(rest) = trimmed.strip_prefix("credentials ") {
            integration.credentials = Some(AppIntegrationCredentials {
                scope: rest.trim().to_owned(),
                bindings: Vec::new(),
            });
            state.current_integration_child = Some("credentials");
        } else if let Some(rest) = trimmed.strip_prefix("data_classification ") {
            integration.data_classification = Some(rest.trim().to_owned());
            state.current_integration_child = None;
        }
    } else if state.current_child == Some("runtime") {
        let Some(unit_index) = state.current_runtime_unit else {
            return;
        };
        let unit = &mut app.runtime[unit_index];
        if let Some(rest) = trimmed.strip_prefix("serves ") {
            unit.serves.extend(split_items(rest));
        } else if let Some(rest) = trimmed.strip_prefix("runs ") {
            unit.runs.extend(split_items(rest));
        } else if let Some(rest) = trimmed.strip_prefix("healthcheck ") {
            unit.healthcheck = Some(unquote(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("readiness ") {
            unit.readiness = Some(unquote(rest.trim()).to_owned());
        } else if trimmed == "locale_negotiate" {
            // i18n bucket cycle — open the block; its
            // children at indent 8 land below via the
            // `current_runtime_child` token.
            unit.locale_negotiate
                .get_or_insert_with(LocaleNegotiate::default);
            state.current_runtime_child = Some("locale_negotiate");
        }
    } else if state.current_child == Some("services") {
        let Some(service_index) = state.current_service else {
            return;
        };
        let service = &mut app.services[service_index];
        if let Some(rest) = trimmed.strip_prefix("owns ") {
            service.owns.extend(split_items(rest));
            state.current_service_child = None;
        } else if trimmed == "exposes" {
            state.current_service_child = Some("exposes");
        } else if let Some(rest) = trimmed.strip_prefix("publishes ") {
            service.publishes.extend(split_items(rest));
            state.current_service_child = None;
        } else if let Some(rest) = trimmed.strip_prefix("consumes ") {
            service.consumes.extend(split_items(rest));
            state.current_service_child = None;
        }
    } else if state.current_child == Some("encryption") {
        // Encryption bucket cycle — `source`, `algorithm`,
        // `rotation`, `rotation_profile` children at indent
        // 6 populate the currently open binding. Unknown
        // algorithm/rotation tokens are silently kept at
        // the default; doctor diagnostics surface shape
        // errors.
        let Some(binding_index) = state.current_encryption_binding else {
            return;
        };
        let binding = &mut app.encryption_bindings[binding_index];
        if let Some(rest) = trimmed.strip_prefix("source ") {
            let raw = rest.trim();
            let template = if let Some(literal) = raw.strip_prefix("env.") {
                Some(EncryptionSource::Env(EncryptionTemplate::parse(literal)))
            } else if let Some(literal) = raw.strip_prefix("secrets.") {
                Some(EncryptionSource::Secrets(EncryptionTemplate::parse(
                    literal,
                )))
            } else {
                None
            };
            if let Some(source) = template {
                binding.source = source;
            }
        } else if let Some(rest) = trimmed.strip_prefix("algorithm ") {
            if let Some(alg) = EncryptionAlgorithm::parse(rest.trim()) {
                binding.algorithm = alg;
            }
        } else if let Some(rest) = trimmed.strip_prefix("rotation_profile ") {
            // Roadmap §1.10 — bind this `@key.<scope>` to
            // a `registry.secret_rotation <name>` profile.
            // The cross-check that the profile exists
            // lives in doctor's
            // `secret-rotation-binding-unknown`.
            let name = rest.trim().to_owned();
            if !name.is_empty() {
                binding.rotation_profile = Some(name);
            }
        } else if let Some(rest) = trimmed.strip_prefix("rotation ") {
            if let Some(rot) = EncryptionRotation::parse(rest.trim()) {
                binding.rotation = rot;
            }
        }
    } else if state.current_child == Some("headers") && state.in_headers_hsts {
        // Roadmap §1.10 — six-space HSTS body. Children
        // are `max_age <n>`, `include_subdomains`,
        // `preload`. The inline form `hsts max_age N
        // include_subdomains preload` covers the same
        // ground.
        let Some(headers) = app.headers.as_mut() else {
            return;
        };
        let hsts = headers.hsts.get_or_insert_with(AppHsts::default);
        parse_hsts_inline(trimmed, hsts);
    } else if state.current_child == Some("cookie") {
        // Roadmap §1.2 — populate the currently open profile.
        // Boolean slots accept `true | false`; unparseable
        // values are silently kept at `None` and surface
        // through doctor's app-cookie-contract diagnostic.
        let Some(profile_index) = state.current_cookie_profile else {
            return;
        };
        let Some(cookie) = app.cookie.as_mut() else {
            return;
        };
        let profile = &mut cookie.profiles[profile_index];
        if let Some(rest) = trimmed.strip_prefix("signed ") {
            profile.signed = parse_bool(rest.trim());
        } else if let Some(rest) = trimmed.strip_prefix("secure ") {
            profile.secure = parse_bool(rest.trim());
        } else if let Some(rest) = trimmed.strip_prefix("http_only ") {
            profile.http_only = parse_bool(rest.trim());
        } else if let Some(rest) = trimmed.strip_prefix("same_site ") {
            profile.same_site = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("max_age ") {
            profile.max_age = Some(unquote(rest.trim()).to_owned());
        }
    }
}

/// Indent-8 dispatch — leaf children that need both a top-level
/// child token (`current_child`) and a sub-child token
/// (`current_integration_child`, `current_service_child`,
/// `current_runtime_child`). Three concrete shapes today: credential
/// bindings under `integrations.<name>.credentials`,
/// `services.<name>.exposes` entries, and `runtime.unit.<name>.
/// locale_negotiate` fields.
pub(super) fn handle_indent8(trimmed: &str, app: &mut AppManifest, state: &ManifestParseState) {
    if state.current_child == Some("integrations")
        && state.current_integration_child == Some("credentials")
    {
        let Some(integration_index) = state.current_integration else {
            return;
        };
        let Some(credentials) = &mut app.integrations[integration_index].credentials else {
            return;
        };
        if let Some((name, source)) = parse_credential_binding(trimmed) {
            credentials
                .bindings
                .push(AppIntegrationCredentialBinding { name, source });
        }
    } else if state.current_child == Some("services")
        && state.current_service_child == Some("exposes")
    {
        let Some(service_index) = state.current_service else {
            return;
        };
        let parts: Vec<_> = trimmed.split_whitespace().collect();
        if parts.len() == 2 {
            app.services[service_index]
                .exposes
                .push(AppServiceExposure {
                    kind: parts[0].to_owned(),
                    target: parts[1].to_owned(),
                });
        }
    } else if state.current_child == Some("runtime")
        && state.current_runtime_child == Some("locale_negotiate")
    {
        let Some(unit_index) = state.current_runtime_unit else {
            return;
        };
        let Some(ln) = app.runtime[unit_index].locale_negotiate.as_mut() else {
            return;
        };
        if let Some(rest) = trimmed.strip_prefix("source ") {
            ln.source = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("strategy ") {
            ln.strategy = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("fallback ") {
            ln.fallback = Some(unquote(rest.trim()).to_owned());
        }
    }
}
