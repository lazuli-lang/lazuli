//! `conventions [me]` synthesis helpers — Cell M2.
//!
//! Spec: `docs/proposals/ir-resource-conventions-me.md` §§5.3, 5.5, 5.6.
//!
//! `classify_me_mode` is the entire decision surface — a 4-row truth
//! table over resource shape, evaluated once at synth time. Once a mode
//! is picked, `build_lookup_my_query` emits ONE fixed `Query::Lookup`
//! shape with a mode-specific `keys` vector (the WHERE-clause builder).
//! **The emitted IR contains zero branches** — RULE-VOCAB-03 (me §7).
//!
//! ## Layout
//!
//! * `MeMode` — the 4-row taxonomy.
//! * `classify_me_mode` — resource → mode classification.
//! * `build_lookup_my_query` — mode → `Query::Lookup` IR.
//! * `check_me_lookup_signature_mismatch` — author-override sanity check.

use lazuli_ir as ir;

/// me §5.3 — the four key-resolution modes. Classification is static;
/// each variant carries no runtime state. The selected variant uniquely
/// determines the `KeyClause` vector emitted into the synthesized
/// `Query::Lookup` (me §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeMode {
    /// Resource has `user: User required` (with or without `unique`)
    /// AND an `org`-bearing field. `WHERE org_id = ctx.User.OrgID AND
    /// "user" = ctx.User.ID`.
    UserKeyed,
    /// Resource has `user: User required` AND no `org` field.
    /// `WHERE "user" = ctx.User.ID`.
    UserKeyedNoOrg,
    /// Resource has `org: Org required` AND no `user: User required`.
    /// `WHERE org_id = ctx.User.OrgID`.
    OrgKeyed,
    /// Resource IS the User table (name == "User"). `WHERE id = ctx.User.ID`.
    SelfKeyed,
}

/// me §5.3 — classify the resource's actor axis. Pure inspection of the
/// resource's field list + name. Returns `None` only when the resource
/// has neither `user` nor `org` and is not named `User`, triggering
/// `me_synth_no_actor_resolution`.
///
/// **RULE-VOCAB-03 affirmation**: this function is the entire
/// authoring-time decision surface for the `me` bundle. Its `if`/`match`
/// statements pick which IR shape the *synth pass* emits; the emitted
/// IR contains exactly one fixed `Query::Lookup` per call site, with
/// no branches in the runtime lowering path.
pub(crate) fn classify_me_mode(resource: &ir::Resource) -> Option<MeMode> {
    // me §5.3 row 4 — `self_keyed`: the resource IS the User table.
    // Checked first because a resource literally named `User` could in
    // principle declare its own `user` self-reference field; the
    // self-keyed shape (`WHERE id = ctx.User.ID`) is the correct one.
    if resource.name == "User" {
        return Some(MeMode::SelfKeyed);
    }

    let has_user_required = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });
    let has_org_field = resource.fields.iter().any(|f| {
        f.name == "org" && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
    });

    // me §5.3 rows 1 and 2 — `user_keyed` variants.
    if has_user_required {
        if has_org_field {
            return Some(MeMode::UserKeyed);
        }
        return Some(MeMode::UserKeyedNoOrg);
    }

    // me §5.3 row 3 — `org_keyed` (org-singleton resource).
    let has_org_required = resource.fields.iter().any(|f| {
        f.name == "org"
            && f.required
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
    });
    if has_org_required {
        return Some(MeMode::OrgKeyed);
    }

    // me §5.3 row 5 — no key. Diagnostic.
    None
}

/// me §5.2 — build the `lookup_my_<resource>` `Query::Lookup` IR. The
/// `keys` vector is the WHERE-clause builder; its shape is fixed by
/// `mode` at synth time (me §7 / RULE-VOCAB-03). The emitted IR carries
/// no `params` (route-less per me §5.2) and no `filters`.
///
/// Path-on-the-right-hand-side uses `ctx.User.*` paths, mirroring the
/// already-proven IR shape used by hand-authored
/// `query.lookup ... filters` blocks (e.g.,
/// `traveler.lzi:79-83` references `ctx.actor.user_id`; the IR-level
/// `KeyClause.equals` carries an `Expr::Path` per `Path::from_segments`).
pub(crate) fn build_lookup_my_query(name: &str, resource: &str, mode: MeMode) -> ir::Query {
    let _ = resource; // reserved for future signature-mismatch detail
    // Runtime `readCtx` (runtime/go/lazuli/handle.go:893) accepts only
    // canonical snake-case ctx paths: `actor.user_id` / `actor.org_id`.
    // PascalCase variants (`ctx.User.OrgID`) fall through to default
    // and return 500 "unknown ctx path". Emit the canonical segments
    // matching the existing commands' FromCtx convention.
    let keys: Vec<ir::KeyClause> = match mode {
        // §5.3 user_keyed: WHERE org_id = ctx.actor.org_id AND "user" = ctx.actor.user_id
        MeMode::UserKeyed => vec![
            ir::KeyClause {
                path: ir::Path::from_segments(["org".to_owned()]),
                equals: ir::Expr::Path(ir::Path::from_segments([
                    "ctx".to_owned(),
                    "actor".to_owned(),
                    "org_id".to_owned(),
                ])),
            },
            ir::KeyClause {
                path: ir::Path::from_segments(["user".to_owned()]),
                equals: ir::Expr::Path(ir::Path::from_segments([
                    "ctx".to_owned(),
                    "actor".to_owned(),
                    "user_id".to_owned(),
                ])),
            },
        ],
        // §5.3 user_keyed_no_org: WHERE "user" = ctx.actor.user_id
        MeMode::UserKeyedNoOrg => vec![ir::KeyClause {
            path: ir::Path::from_segments(["user".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "user_id".to_owned(),
            ])),
        }],
        // §5.3 org_keyed: WHERE org_id = ctx.actor.org_id
        MeMode::OrgKeyed => vec![ir::KeyClause {
            path: ir::Path::from_segments(["org".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "org_id".to_owned(),
            ])),
        }],
        // §5.3 self_keyed: WHERE id = ctx.actor.user_id
        MeMode::SelfKeyed => vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "user_id".to_owned(),
            ])),
        }],
    };

    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        // me §5.2 — NO route, NO params. The actor IS the input.
        params: Vec::new(),
        keys,
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        // me §5.4 — default policy is `authenticated`.
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// me §11.1 — `me_synth_signature_mismatch` trigger. Compares an
/// author-written `lookup_my_<resource>` query to the canonical shape.
/// Returns `None` when the signatures match.
///
/// The canonical `me` synth produces a `Query::Lookup` (route-less,
/// returning the resource row). Mismatches the author can introduce:
/// - Wrong query kind (`Query::List` or `Query::Sql` under the same
///   name). The `me` bundle owns the `lookup_my_*` name prefix.
/// - Author-supplied `params` (the canonical shape is parameter-less).
pub(crate) fn check_me_lookup_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    resource: &str,
) -> Option<String> {
    let _ = resource; // reserved for future richer diff messages.
    let query = feature.queries.iter().find(|q| q.name() == name)?;

    match query {
        ir::Query::Lookup(lq) => {
            // me §5.2 — canonical shape is parameter-less. An author
            // who introduces `params` diverges from the canonical
            // route-less actor-keyed shape.
            if !lq.params.is_empty() {
                return Some(format!(
                    "author-written `{}` declares params; canonical `me` shape is route-less + parameter-less",
                    name
                ));
            }
            None
        }
        // §11.1 mismatch — `lookup_my_<r>` should be a Lookup query.
        _ => Some(format!(
            "author-written `{}` is not a `query.lookup`; canonical `me` shape is route-less Lookup",
            name
        )),
    }
}
