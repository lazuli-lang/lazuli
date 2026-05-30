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

use lazuli_ir::{QualifiedName, TypeRef};

use super::cross_feature::{CrossFeatureIndex, DeclKind};

mod builtins;
#[cfg(test)]
mod tests_support;

use builtins::{go_type_for_builtin, go_type_for_capability};

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
///
/// ## Examples
///
/// ```ignore
/// let (go, import) = go_type_for(&type_ref, &ctx);
/// // FK columns resolve to "lazuli.ID"; builtins to their Go counterpart.
/// ```
pub fn go_type_for(ty: &TypeRef, ctx: &TypeCtx) -> (String, Option<String>) {
    match ty {
        TypeRef::Builtin(builtin) => {
            let (go, import) = go_type_for_builtin(builtin);
            (go, import.map(str::to_owned))
        }
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) if is_implicit_empty(qname, ctx) => {
            ("struct{}".to_owned(), None)
        }
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => resolve_named(qname, ctx),
        TypeRef::Many(inner) => {
            let (inner_go, import) = go_type_for(inner, ctx);
            (format!("[]{}", inner_go), import)
        }
        TypeRef::Unresolved(name) if is_empty_name(name) => ("struct{}".to_owned(), None),
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

/// Variant of [`go_type_for`] that resolves resource references to their
/// full struct type (e.g. `User`, `<owner>gen.User`) instead of the FK
/// collapse to `lazuli.ID`.
///
/// `go_type_for` is correct for resource fields — `org: Org required`
/// is a BIGINT FK column and `pgx.RowToStructByName` needs `lazuli.ID`
/// at scan time. But command return positions (`command me returns User`
/// → `Command[Input, User]` + `ReturnsFromRegistry[Input, User]`) want
/// the full row shape; the handler returns the struct, not the id.
///
/// Mirrors `query.rs`'s `resource_type = pascal_case(&r.name)` path for
/// `Query[A, R]` — both axes carry the typed row.
///
/// ## Examples
///
/// ```ignore
/// let (go, import) = go_return_type_for(&return_ty, &ctx);
/// // Resource return positions resolve to the full struct shape, not lazuli.ID.
/// ```
pub fn go_return_type_for(ty: &TypeRef, ctx: &TypeCtx) -> (String, Option<String>) {
    match ty {
        TypeRef::Builtin(builtin) => {
            let (go, import) = go_type_for_builtin(builtin);
            (go, import.map(str::to_owned))
        }
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) if is_implicit_empty(qname, ctx) => {
            ("struct{}".to_owned(), None)
        }
        TypeRef::UserDefined(qname) | TypeRef::EnumRef(qname) => resolve_named_full(qname, ctx),
        TypeRef::Many(inner) => {
            let (inner_go, import) = go_return_type_for(inner, ctx);
            (format!("[]{}", inner_go), import)
        }
        TypeRef::Unresolved(name) if is_empty_name(name) => ("struct{}".to_owned(), None),
        TypeRef::Unresolved(name) => (sanitise_go_ident(name), None),
        TypeRef::Capability(cap) => {
            let (go, import) = go_type_for_capability(cap);
            (go, import.map(str::to_owned))
        }
    }
}

fn is_implicit_empty(qname: &QualifiedName, ctx: &TypeCtx<'_>) -> bool {
    qname.feature.is_none()
        && is_empty_name(&qname.name)
        && ctx.cross_index.owner("Empty").is_none()
        && !ctx.cross_index.is_ambiguous("Empty")
}

fn is_empty_name(name: &str) -> bool {
    name.trim() == "Empty"
}

/// Like [`resolve_named`] but skips the FK collapse. Resources resolve
/// to their full struct name (same-feature) or `<owner>gen.<Name>`
/// (cross-feature) so the consumer (command return type) gets the typed
/// row instead of the BIGINT id alias.
fn resolve_named_full(qname: &lazuli_ir::QualifiedName, ctx: &TypeCtx) -> (String, Option<String>) {
    if qname.name.starts_with('@') {
        return (sanitise_go_ident(&qname.name), None);
    }

    let go_name = super::casing::pascal_case(&qname.name);

    // Honour any analyzer-supplied owner directly — skips the kind
    // lookup so future analyzer resolve-pass output keeps working
    // without a re-index.
    if let Some(owner) = qname.feature.as_deref() {
        if owner == ctx.current_feature {
            return (go_name, None);
        }
        let import = format!("{}/{}", ctx.module_name, owner);
        let gen_pkg = super::casing::gen_package_name(owner);
        return (format!("{}.{}", gen_pkg, go_name), Some(import));
    }

    match ctx.cross_index.owner(&qname.name) {
        Some(owner) if owner == ctx.current_feature => (go_name, None),
        Some(owner) => {
            let import = format!("{}/{}", ctx.module_name, owner);
            let gen_pkg = super::casing::gen_package_name(owner);
            (format!("{}.{}", gen_pkg, go_name), Some(import))
        }
        None => (go_name, None),
    }
}

/// Cross-feature resolver for `UserDefined` and `EnumRef` qnames.
/// Returns the rendered Go type plus the import path to register on
/// the file's `ImportSet`. See module-level doc for the three cases.
fn resolve_named(qname: &lazuli_ir::QualifiedName, ctx: &TypeCtx) -> (String, Option<String>) {
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

    // A `TypeRef::UserDefined(<Resource>)` field is an FK. The DB
    // column holds a BIGINT id, NOT the full row, so the Go field
    // must be `lazuli.ID` for `pgx.RowToStructByName` to scan
    // correctly. Emitting `*orggen.User` here would fault every
    // `creates`/`updates` `RETURNING *` and every `query.list`.
    // Records and enums keep their struct/enum identity — they DON'T
    // scan from a BIGINT column. `kind()` returns `None` for
    // unknown/ambiguous names; those fall through the existing
    // resolver and emit a sanitised bare ref.
    if matches!(ctx.cross_index.kind(&qname.name), Some(DeclKind::Resource)) {
        return ("lazuli.ID".to_owned(), None);
    }

    // Step 1 — honour the analyzer's resolution if it landed. Today
    // `qname.feature` is always `None` (analyzer lifts every unknown
    // ident with `feature: None`), but the branch is here so future
    // analyzer cells can short-circuit the index lookup.
    //
    // Cross-feature refs use `<owner>gen.<Name>` because every
    // generated package now lives at `dist/go/<owner>/` declaring
    // `package <owner>gen` (per the handler-home pivot — see
    // project-structure.md). The import path is unchanged
    // (`<module>/<owner>`); Go auto-aliases the package by its
    // declared name.
    if let Some(owner) = qname.feature.as_deref() {
        if owner == ctx.current_feature {
            return (go_name, None);
        }
        let import = format!("{}/{}", ctx.module_name, owner);
        let gen_pkg = super::casing::gen_package_name(owner);
        return (format!("{}.{}", gen_pkg, go_name), Some(import));
    }

    // Step 2 — consult the cross-feature index. Same-package lookup
    // is bare; cross-feature emits a qualified ref plus the import.
    match ctx.cross_index.owner(&qname.name) {
        Some(owner) if owner == ctx.current_feature => (go_name, None),
        Some(owner) => {
            let import = format!("{}/{}", ctx.module_name, owner);
            let gen_pkg = super::casing::gen_package_name(owner);
            (format!("{}.{}", gen_pkg, go_name), Some(import))
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

/// Cheap sanitiser for unresolved identifiers so we never emit raw
/// `@lazuli/plugin-foo` text into Go source. The §6.2.1 error catalog (cell
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
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::tests_support::{
        cross_ref_module, empty_feature, make_record, make_resource, module_with_features, type_ctx,
    };
    use super::*;
    use lazuli_ir::{QualifiedName, Record};

    // Suppress unused-import warning when only some test branches
    // construct a `Record`.
    #[allow(dead_code)]
    fn _record_compiles(_: Record) {}

    #[test]
    fn user_defined_resource_ref_emits_lazuli_id_no_import() {
        // Resource refs are FKs (BIGINT in DB). The emitted Go field
        // must be `lazuli.ID` so `pgx.RowToStructByName` can scan the
        // column; the prior `Customer`/`orggen.Customer` shape would
        // fault every `RETURNING *`. Same-package and cross-feature
        // both collapse to `lazuli.ID` for this reason — the FK id is
        // unqualified.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "Customer".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "lazuli.ID");
        assert_eq!(import, None);
    }

    #[test]
    fn user_defined_cross_feature_resource_ref_collapses_to_lazuli_id() {
        // Resource refs (here `User` declared in `org`) emit
        // `lazuli.ID` regardless of which feature consumes them —
        // the underlying DB column is BIGINT in every case.
        let module = cross_ref_module();
        let index = CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "User".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "lazuli.ID");
        assert_eq!(import, None);
    }

    #[test]
    fn user_defined_record_ref_emits_struct_with_qualified_import() {
        // Records ARE struct-shaped (no FK collapse) so their refs
        // keep the qualified-name + import treatment when consumed
        // from another feature. This is the path that authoring
        // typically hits from `Function[Input, Output]` extension
        // shapes referencing a feature-local Record.
        let mut customer = empty_feature("customer");
        let mut org = empty_feature("org");
        org.records.push(make_record("UserSnapshot"));
        let module = module_with_features(vec![customer.clone(), org]);
        let index = CrossFeatureIndex::build(&module);
        let _ = &mut customer;
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "UserSnapshot".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "orggen.UserSnapshot");
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
        let ctx = type_ctx("a", "lazuli/test", &index);
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
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "Ghost".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "Ghost");
        assert_eq!(import, None);
    }

    #[test]
    fn declared_empty_record_stays_named() {
        let mut customer = empty_feature("customer");
        customer.records.push(make_record("Empty"));
        let module = module_with_features(vec![customer]);
        let index = CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "Empty".to_owned(),
        };
        let (go, import) = go_return_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "Empty");
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
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: Some("org".to_owned()),
            name: "User".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "orggen.User");
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
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "@semantic.Money".to_owned(),
        };
        let (go, _) = go_type_for(&TypeRef::UserDefined(qname), &ctx);
        assert_eq!(go, "_semantic_Money");
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
                label_key: None,
                hint_key: None,
                icon_key: None,
                previous_names: Vec::new(),
            }],
            previous_names: Vec::new(),
            span_ref: None,
        });
        let module = module_with_features(vec![customer, billing]);
        let index = CrossFeatureIndex::build(&module);
        let ctx = type_ctx("customer", "lazuli/test", &index);
        let qname = QualifiedName {
            feature: None,
            name: "PlanTier".to_owned(),
        };
        let (go, import) = go_type_for(&TypeRef::EnumRef(qname), &ctx);
        assert_eq!(go, "billinggen.PlanTier");
        assert_eq!(import.as_deref(), Some("lazuli/test/billing"));
    }
}
