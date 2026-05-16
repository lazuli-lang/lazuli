//! Closed-catalog `TypeRef` → Go type mapping. Centralised so per-kind
//! walkers in `module.rs` / `resource.rs` / etc. never inline the
//! mapping. Each entry returns `(go_type, optional_import_path)` so the
//! caller can register the import on the file's `ImportSet`.
//!
//! `@semantic.GeoPoint` maps to `postgis.Point` from
//! `github.com/cridenour/go-postgis` (GeoPoint follow-up + proposal
//! §10.1, resolved 2026-05-11). The IR variant lives at
//! `lazuli_ir::BuiltinType::SemanticGeoPoint` (commit `97b193d`).
//!
//! ## Cross-feature resolution (Phase Prep §1.1 mini-cell pré-E3)
//!
//! `TypeRef::UserDefined` and `TypeRef::EnumRef` lift to the same
//! emission path: when the analyzer-side resolve pass lands the
//! variant carries `qname.feature = Some(<owner>)`. Until then the
//! emitter consults [`crate::emitter::cross_feature::CrossFeatureIndex`]
//! via [`TypeCtx`]. Three cases:
//!
//! - `owner == current_feature`: bare PascalCase ref, no import.
//! - `owner == other`: qualified `<feature>.<Name>` ref + import on
//!   `<module_name>/<feature>` (classified by `ImportSet` as
//!   third-party today; classification is the only knob, the Go
//!   compiler doesn't care).
//! - ambiguous or unknown: fall back to `sanitise_go_ident` so the
//!   file at least parses. The §6.2.1 error catalog (cell I4)
//!   upgrades this to a hard diagnostic; for now we emit the bare
//!   form and a warning comment is the resource emitter's
//!   responsibility (codegen layer only — doctor surfaces the
//!   structural smell separately).

use lazuli_ir::{BuiltinType, CapabilityRef, TypeRef};

use super::cross_feature::CrossFeatureIndex;

/// Context the per-feature emitter passes to [`go_type_for`] so the
/// closed-catalog mapping can resolve cross-feature references.
/// Borrowed for the lifetime of one feature emission — callers
/// construct it once at the entry point and reuse it across every
/// field walk.
pub struct TypeCtx<'a> {
    /// Name of the feature currently being emitted. Used to detect
    /// self-references (same-package, no import) vs cross-feature.
    pub current_feature: &'a str,
    /// Module path emitted at the top of the root `go.mod`
    /// (e.g. `lazuli/acme-crm`). Cross-feature imports concatenate
    /// `<module_name>/<feature>`.
    pub module_name: &'a str,
    /// Cross-feature index — built once per `generate_v1` run, shared
    /// by reference.
    pub cross_index: &'a CrossFeatureIndex<'a>,
}

/// Resolve a `TypeRef` to its concrete Go-source form plus the import
/// path that must be present on the consuming file (if any).
///
/// Returns `(go_type, Option<String>)` — `String` (not `&'static str`)
/// because cross-feature imports are dynamic in the module path. The
/// rest of the closed catalog still produces a single owned `String`
/// for the type and either `None` or a known runtime path; the heap
/// cost is one short allocation per field, paid only at codegen time.
pub fn go_type_for(ty: &TypeRef, ctx: &TypeCtx) -> (String, Option<String>) {
    match ty {
        TypeRef::Builtin(builtin) => {
            let (go, import) = go_type_for_builtin(*builtin);
            (go, import.map(str::to_owned))
        }
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => resolve_named(qname, ctx),
        TypeRef::Many(inner) => {
            let (inner_go, import) = go_type_for(inner, ctx);
            (format!("[]{}", inner_go), import)
        }
        TypeRef::Unresolved(name) => {
            // Surface as a string fallback; the §6.2.1 error catalog
            // (cell I4) will upgrade this to a hard failure under
            // `--check`. For now the unresolved name is preserved
            // verbatim as a Go-typed placeholder so the output is
            // still syntactically a Go identifier.
            (sanitise_go_ident(name), None)
        }
        TypeRef::Capability(cap) => {
            let (go, import) = go_type_for_capability(cap);
            (go, import.map(str::to_owned))
        }
    }
}

/// Cross-feature resolver for `UserDefined` and `EnumRef` qnames.
/// Returns the rendered Go type plus the import path to register on
/// the file's `ImportSet`. See module-level doc for the three cases.
fn resolve_named(
    qname: &lazuli_ir::QualifiedName,
    ctx: &TypeCtx,
) -> (String, Option<String>) {
    // Decorator-prefixed names (e.g. `@semantic.<unknown>` that
    // fell through the builtin match) are not real type refs —
    // sanitise and bail. The analyzer should catch these as
    // diagnostics under cell I4; the emitter only needs to produce
    // a Go-valid identifier so the file parses.
    if qname.name.starts_with('@') {
        return (sanitise_go_ident(&qname.name), None);
    }

    // Project DSL identifiers (PascalCase resources/records/enums)
    // through the shared `casing::pascal_case` so the reference
    // matches whatever the resource/enum emitter wrote at the
    // declaration site: `ApiKey` → `APIKey`, `MfaContract` →
    // `MfaContract`. Acronyms in the closed table (`api`, `id`,
    // `url`, `json`, ...) uppercase consistently across files.
    let go_name = super::casing::pascal_case(&qname.name);

    // Step 1 — honour the analyzer's resolution if it landed. Today
    // `qname.feature` is always `None` (analyzer lifts every unknown
    // ident with `feature: None`), but the branch is here so future
    // analyzer cells can short-circuit the index lookup.
    if let Some(owner) = qname.feature.as_deref() {
        if owner == ctx.current_feature {
            return (go_name, None);
        }
        let import = format!("{}/{}", ctx.module_name, owner);
        return (format!("{}.{}", owner, go_name), Some(import));
    }

    // Step 2 — consult the cross-feature index. Same-package lookup
    // is bare; cross-feature emits a qualified ref plus the import.
    match ctx.cross_index.owner(&qname.name) {
        Some(owner) if owner == ctx.current_feature => (go_name, None),
        Some(owner) => {
            let import = format!("{}/{}", ctx.module_name, owner);
            (format!("{}.{}", owner, go_name), Some(import))
        }
        None => {
            // Either ambiguous or not declared anywhere. Both
            // currently fall back to a bare sanitised ref so the
            // file still parses; cell I4 will upgrade this to a hard
            // diagnostic. We don't emit a warning comment here
            // because the emitter sees one field at a time and a
            // mid-struct warning would corrupt the column-aligned
            // tag rows; resource emitter records a separate warning
            // line outside the struct body when it detects the
            // condition via `cross_index.is_ambiguous`.
            (go_name, None)
        }
    }
}

fn go_type_for_builtin(builtin: BuiltinType) -> (String, Option<&'static str>) {
    match builtin {
        BuiltinType::Id => ("lazuli.ID".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::Text => ("string".to_owned(), None),
        BuiltinType::Boolean => ("bool".to_owned(), None),
        BuiltinType::Integer => ("int64".to_owned(), None),
        BuiltinType::Decimal => ("float64".to_owned(), None),
        BuiltinType::Date => ("lazuli.Date".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::DateTime => ("lazuli.Time".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::Json => ("lazuli.JSON".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticEmail => (
            "lazuli.Email".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        // Per proposal `semantic-types-money-brazilian.md` v0.3:
        // `Money` is the currency-aware semantic type. The Go field
        // type is the rich struct `lazuli.MoneyValue` (decimal +
        // currency), not the legacy `int64` alias `lazuli.Money`
        // which is preserved for backward compatibility.
        BuiltinType::SemanticMoney => (
            "lazuli.MoneyValue".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticPhone => (
            "lazuli.Phone".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticUrl => ("lazuli.URL".to_owned(), Some("lazuli.dev/runtime/lazuli")),
        BuiltinType::SemanticUuid => (
            "lazuli.UUID".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::SemanticCurrency => (
            "lazuli.Currency".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        // GeoPoint follow-up — `@semantic.GeoPoint` resolves to
        // `postgis.Point` via the lightweight `cridenour/go-postgis`
        // binding (chosen per `codegen-lazuli-go.md` §10.1; revisit if
        // a future bucket needs `twpayne/go-geom`'s broader WKT/WKB
        // roundtrip). Resource fields carrying this type also emit a
        // `db:"…,type:geography(point,4326)"` tag (proposal §3.1) and
        // a `GIST` index in the DDL migration (proposal §9.2).
        BuiltinType::SemanticGeoPoint => (
            "postgis.Point".to_owned(),
            Some("github.com/cridenour/go-postgis"),
        ),
        BuiltinType::CapSecret => (
            "lazuli.Secret".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        BuiltinType::CapFile => {
            // Legacy flat `@cap.File` (no args); modern typed form is
            // `TypeRef::Capability(CapabilityRef::File)`. Both resolve
            // to the same `storage.FileRef` Go type.
            (
                "storage.FileRef".to_owned(),
                Some("lazuli.dev/runtime/lazuli/storage"),
            )
        }
    }
}

fn go_type_for_capability(cap: &CapabilityRef) -> (String, Option<&'static str>) {
    match cap {
        CapabilityRef::File(_) => (
            "storage.FileRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli/storage"),
        ),
        CapabilityRef::Hashed(_) => (
            "lazuli.HashedRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        CapabilityRef::Encrypted(_) => (
            "lazuli.EncryptedRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        // `@cap.E2ee` shares the byte envelope shape with
        // `@cap.Encrypted` — both store opaque ciphertext. The
        // semantic distinction (server cannot decrypt) is enforced
        // at codegen call-site time, not by the column type.
        CapabilityRef::E2ee(_) => (
            "lazuli.EncryptedRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
        CapabilityRef::Token(_) => (
            "lazuli.TokenRef".to_owned(),
            Some("lazuli.dev/runtime/lazuli"),
        ),
    }
}

/// Cheap sanitiser for unresolved identifiers so we never emit raw
/// `@plugin/foo` text into Go source. The §6.2.1 error catalog (cell
/// I4) replaces this with a hard error.
fn sanitise_go_ident(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "lazuliUnresolved".to_owned();
    }
    if out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, Defaults, Feature, Module, Policies, QualifiedName, Record, Resource,
    };

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(AppManifest {
                name: "test".to_owned(),
                title: None,
                version: None,
        lazuli_version: None,
                targets: Vec::new(),
                default_locale: None,
                default_timezone: None,
                auth_failed_redirect: None,
                not_found: None,
                error_pages: Vec::new(),
                uses: Vec::new(),
                packs: Vec::new(),
                bindings: Vec::new(),
                architecture: None,
                services: Vec::new(),
                communication: None,
                environments: Vec::new(),
                urls: Vec::new(),
                cors: None,
                headers: None,
                cookie: None,
                proxy: None,
                limits: None,
                env: Vec::new(),
                integrations: Vec::new(),
                capabilities: Vec::new(),
                runtime: Vec::new(),
                deploy: None,
                logging: None,
                tracing: None,
                observability: None,
                locale: None,
                encryption_bindings: Vec::new(),
                span_ref: None,
            }),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn make_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
        }
    }

    fn cross_ref_module() -> Module {
        let mut customer = empty_feature("customer");
        customer.resources.push(make_resource("Customer"));
        let mut org = empty_feature("org");
        org.resources.push(make_resource("User"));
        module_with_features(vec![customer, org])
    }

    #[test]
    fn id_maps_to_lazuli_id_with_lazuli_import() {
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::Id), &ctx);
        assert_eq!(go, "lazuli.ID");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn text_maps_to_string_without_import() {
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::Text), &ctx);
        assert_eq!(go, "string");
        assert_eq!(import, None);
    }

    #[test]
    fn semantic_email_maps_to_lazuli_email() {
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticEmail), &ctx);
        assert_eq!(go, "lazuli.Email");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn semantic_currency_maps_to_lazuli_currency() {
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticCurrency), &ctx);
        assert_eq!(go, "lazuli.Currency");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn many_wraps_inner_with_slice_prefix() {
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let inner = TypeRef::Builtin(BuiltinType::Id);
        let (go, import) = go_type_for(&TypeRef::Many(Box::new(inner)), &ctx);
        assert_eq!(go, "[]lazuli.ID");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli"));
    }

    #[test]
    fn user_defined_same_package_renders_bare_name_without_import() {
        // `Customer` resolved in the `customer` feature where it is
        // declared — bare ref, no import.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: None,
            name: "Customer".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "Customer");
        assert_eq!(import, None);
    }

    #[test]
    fn user_defined_cross_feature_emits_qualified_ref_and_import() {
        // `User` declared in `org`, referenced from `customer`. Emits
        // `org.User` and `lazuli/test/org` import.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: None,
            name: "User".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "org.User");
        assert_eq!(import.as_deref(), Some("lazuli/test/org"));
    }

    #[test]
    fn user_defined_ambiguous_falls_back_to_bare_ref() {
        // `Status` declared in two features → ambiguous; emitter
        // falls back to a bare ref and no import (the file at least
        // parses; resource emitter is responsible for the warning).
        let mut a = empty_feature("a");
        a.resources.push(make_resource("Status"));
        let mut b = empty_feature("b");
        b.resources.push(make_resource("Status"));
        let module = module_with_features(vec![a, b]);
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "a",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: None,
            name: "Status".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "Status");
        assert_eq!(import, None);
    }

    #[test]
    fn user_defined_unknown_name_falls_back_to_sanitised() {
        // `Ghost` is referenced but never declared — the analyzer
        // already left it as `UserDefined`; sanitise so we emit a
        // Go-valid identifier even if the build later fails on
        // `undefined: Ghost`.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: None,
            name: "Ghost".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "Ghost");
        assert_eq!(import, None);
    }

    #[test]
    fn user_defined_with_pre_resolved_owner_uses_qname_feature() {
        // When the analyzer eventually grows a resolve pass and
        // sets `qname.feature` directly, the emitter should honour
        // it without consulting the index. The branch is exercised
        // by setting `feature = Some("org")` explicitly.
        let mut customer = empty_feature("customer");
        customer.resources.push(make_resource("Customer"));
        let module = module_with_features(vec![customer]);
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: Some("org".to_owned()),
            name: "User".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "org.User");
        assert_eq!(import.as_deref(), Some("lazuli/test/org"));
    }

    #[test]
    fn user_defined_decorator_prefixed_names_sanitise() {
        // Analyzer gap: `@semantic.Money` is not in the closed catalog
        // and falls through to `UserDefined`. The emitter must still
        // produce Go-valid identifiers; cell I4 will upgrade this to a
        // hard diagnostic instead of an emission fix. Don't consult
        // the cross-feature index for these (they're a decorator
        // namespace, not a type name).
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: None,
            name: "@semantic.Money".to_owned(),
        };
        let (go, _) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "_semantic_Money");
    }

    #[test]
    fn capability_file_maps_to_storage_file_ref() {
        // Legacy flat variant carries no args and resolves to the same
        // typed alias as the modern `CapabilityRef::File` form.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::CapFile), &ctx);
        assert_eq!(go, "storage.FileRef");
        assert_eq!(import.as_deref(), Some("lazuli.dev/runtime/lazuli/storage"));
    }

    #[test]
    fn semantic_geopoint_maps_to_postgis_point() {
        // GeoPoint follow-up — geo codegen materialises via cridenour/go-postgis.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let (go, import) = go_type_for(&TypeRef::Builtin(BuiltinType::SemanticGeoPoint), &ctx);
        assert_eq!(go, "postgis.Point");
        assert_eq!(import.as_deref(), Some("github.com/cridenour/go-postgis"));
    }

    #[test]
    fn enum_ref_resolves_through_cross_feature_index() {
        // `EnumRef` qnames share the same resolution path as
        // `UserDefined` — the analyzer differentiates them but the
        // emitter doesn't care.
        let mut customer = empty_feature("customer");
        customer.resources.push(make_resource("Customer"));
        let mut billing = empty_feature("billing");
        billing.enums.push(lazuli_ir::EnumDecl {
            name: "PlanTier".to_owned(),
            public_contract: None,
            variants: vec![lazuli_ir::EnumVariant {
                name: "free".to_owned(),
                storage_value: None,
                previous_names: Vec::new(),
            }],
            previous_names: Vec::new(),
            span_ref: None,
        });
        let module = module_with_features(vec![customer, billing]);
        let index = CrossFeatureIndex::build(&module);
        let ctx = TypeCtx {
            current_feature: "customer",
            module_name: "lazuli/test",
            cross_index: &index,
        };
        let qname = QualifiedName {
            feature: None,
            name: "PlanTier".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::EnumRef(qname), &ctx);
        assert_eq!(go, "billing.PlanTier");
        assert_eq!(import.as_deref(), Some("lazuli/test/billing"));
    }

    // Suppress unused-import warning when only some test branches
    // construct a `Record`.
    #[allow(dead_code)]
    fn _record_compiles(_: Record) {}
}
