/// Phase L Tier 4c — lower a canonical-indent `resource` block into
/// `ir::Resource`. `tenancy` (resource-local override), `soft_delete`,
/// `timestamps`, `retention`, `validates`, and `derived_from` all
/// project through additive IR fields landed alongside this lowering.
pub(crate) fn lower_resource_decl(r: &syntax::ResourceDecl) -> Result<ir::Resource, AnalyzeError> {
    let tenancy = r.tenancy.as_ref().map(|t| match t {
        syntax::DefaultsTenancy::Org => ir::Tenancy::Org,
        syntax::DefaultsTenancy::Team => ir::Tenancy::Team,
        syntax::DefaultsTenancy::None => ir::Tenancy::None,
        syntax::DefaultsTenancy::Custom(axis) => ir::Tenancy::Custom(axis.clone()),
    });
    let fields = r
        .fields
        .iter()
        .map(lower_resource_field)
        .collect::<Result<Vec<_>, _>>()?;
    let retention = r.retention.as_ref().map(|ret| ir::RetentionSpec {
        duration: ret.duration.clone(),
        action: match ret.action {
            syntax::ResourceRetentionAction::Anonymize => ir::RetentionAction::Anonymize,
            syntax::ResourceRetentionAction::Delete => ir::RetentionAction::Delete,
            syntax::ResourceRetentionAction::Archive => ir::RetentionAction::Archive,
        },
    });
    // `validates @validator.tier_check` collapses onto `Resource.validate`
    // for a single-entry case (the fixture pattern). Multi-entry would
    // need a `Vec`; defer until pilot evidence demands it.
    let validate = r.validates.first().map(ir::PathRef::authored);
    // CL.C.4 — lower resource-scoped `invariant <name>` blocks.
    let invariants = r
        .invariants
        .iter()
        .map(lower_invariant_decl)
        .collect::<Vec<_>>();
    // Roadmap §1.5 (CL.C.2) — lower `lock` decorator into typed IR.
    let lock = r.lock.as_ref().map(|spec| match spec {
        syntax::ResourceLock::Optimistic { version_field } => ir::LockSpec::Optimistic {
            version_field: version_field.clone(),
        },
        syntax::ResourceLock::Pessimistic => ir::LockSpec::Pessimistic,
        syntax::ResourceLock::RowLevel => ir::LockSpec::RowLevel,
    });
    // Roadmap §1.5 (CL.C.2) — lower `composite_key` block into typed IR.
    let composite_key = r.composite_key.as_ref().map(|ck| ir::CompositeKey {
        fields: ck.fields.clone(),
        primary: ck.primary,
    });
    let conventions = r
        .conventions
        .iter()
        .map(|c| match c {
            syntax::ResourceConventionAst::Crud => ir::ConventionRef::Crud,
            syntax::ResourceConventionAst::Me => ir::ConventionRef::Me,
        })
        .collect();
    let constraints = r
        .constraints
        .iter()
        .map(lower_resource_constraint)
        .collect();
    // GAP-13 — lower `polymorphic_ref` declarations into typed IR.
    let polymorphic_refs = r
        .polymorphic_refs
        .iter()
        .map(|pr| ir::PolymorphicRef {
            type_field: pr.type_field.clone(),
            id_field: pr.id_field.clone(),
            targets: pr.targets.clone(),
        })
        .collect();
    // GAP-07 — lower `many_through <Junction> to <Partner>` declarations.
    // Each payload field reuses the resource field lowering so it carries
    // the full typed-field surface; the synthesized junction resource is
    // assembled in `feature.rs` (it needs the declaring resource's name).
    let many_through = r
        .many_through
        .iter()
        .map(|mt| {
            let payload = mt
                .payload
                .iter()
                .map(lower_resource_field)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ir::ManyThrough {
                junction: mt.name.clone(),
                partner: mt.partner.clone(),
                payload,
            })
        })
        .collect::<Result<Vec<_>, AnalyzeError>>()?;
    let lifecycle_routes = r.lifecycle_routes.as_ref().map(|lr| ir::LifecycleRoutes {
        arms: lr
            .arms
            .iter()
            .map(|arm| ir::LifecycleRouteArm {
                state: arm.state.clone(),
                url: arm.url.clone(),
            })
            .collect(),
        span_ref: Some(span_of(lr.span)),
    });
    Ok(ir::Resource {
        name: r.name.clone(),
        public_contract: lower_public_contract(&r.public_contract),
        tenancy,
        soft_delete: r.soft_delete,
        timestamps: if r.timestamps { Some(true) } else { None },
        fields,
        constraints,
        validate,
        validates: Vec::new(),
        retention,
        previous_names: r
            .previously
            .iter()
            .map(|p| strip_previously_mode(p))
            .collect(),
        span_ref: Some(span_of(r.span)),
        lifecycle: None,
        invariants,
        lock,
        composite_key,
        conventions,
        lifecycle_routes,
        polymorphic_refs,
        // GAP-AUDIT-02 — insert-only marker. Doctor RESOURCE-APPEND-ONLY-001
        // rejects commands that update/delete this resource.
        append_only: r.append_only,
        // GAP-07 — M:N-with-metadata relationships. The junction resources
        // they desugar into are synthesized in `feature.rs`.
        many_through,
        // Spec 0014 — referential guards. `tenant_scoped` / `soft_delete`
        // start `false` here; they are DERIVED in a second pass
        // (`resolve_restrict_on_delete_scopes` in `feature.rs`) once every
        // resource in the feature is lowered, by inspecting the referencing
        // relation's schema. The author never supplies them.
        restrict_on_delete: r
            .restrict_on_delete
            .iter()
            .map(|g| ir::RestrictOnDelete {
                relation: g.relation.clone(),
                fk: g.fk.clone(),
                extra_where: g.extra_where.clone(),
                tenant_scoped: false,
                soft_delete: false,
            })
            .collect(),
    })
}

/// Spec 0014 — second-pass derivation of the `tenant_scoped` /
/// `soft_delete` flags on every `restrict on_delete` clause in a feature.
///
/// Runs AFTER all resources are lowered (so the referencing relation can
/// be looked up by name). For each guard, the referencing relation is the
/// `ir::Resource` whose snake-cased name matches `guard.relation`; the
/// flags are read off THAT relation's schema:
///   - `tenant_scoped` iff the relation carries a tenant axis
///     (`tenancy` is `Some(Org|Team|Custom)`, i.e. not `None`),
///   - `soft_delete` iff the relation has `soft_delete`.
///
/// These derivations are why the emitted `EXISTS` can never forget the
/// `tenant_id = …` / `deleted_at IS NULL` predicates — they are read off
/// the schema, not the author's line. A guard whose relation does not
/// resolve keeps both flags `false` (doctor / typecheck reports the
/// unknown relation separately).
pub(crate) fn resolve_restrict_on_delete_scopes(resources: &mut [ir::Resource]) {
    // Snapshot (name → (tenant_scoped, soft_delete)) so we can mutate the
    // protected resources while reading the referencing relations' shapes.
    let scopes: std::collections::HashMap<String, (bool, bool)> = resources
        .iter()
        .map(|res| {
            let tenant_scoped = matches!(
                res.tenancy,
                Some(ir::Tenancy::Org | ir::Tenancy::Team | ir::Tenancy::Custom(_))
            );
            (
                crate::helpers::pascal_to_snake(&res.name),
                (tenant_scoped, res.soft_delete),
            )
        })
        .collect();
    for res in resources.iter_mut() {
        for guard in res.restrict_on_delete.iter_mut() {
            if let Some(&(tenant_scoped, soft_delete)) = scopes.get(&guard.relation) {
                guard.tenant_scoped = tenant_scoped;
                guard.soft_delete = soft_delete;
            }
        }
    }
}

/// GAP-07 — synthesize the junction `ir::Resource` for one `many_through`
/// declaration. The junction carries:
///   - `<declaring>_id: ID` — a same-feature FK to the declaring resource,
///   - `<partner>_id: ID` — an FK to the partner resource,
///   - one column per payload field (lowered already, on `ManyThrough`), and
///   - a composite `UNIQUE (<declaring>_id, <partner>_id)` so the endpoint
///     pair is a set, not a bag.
///
/// FK column naming: `pascal_to_snake(<Resource>) + "_id"`. The FK fields
/// are `TypeRef::UserDefined` so the migration emitter raises a real DB
/// `FOREIGN KEY` when the endpoint table is in the same module (and a plain
/// `BIGINT` column otherwise — e.g. a partner owned by another feature's
/// migration set, mirroring the cross-feature FK posture).
pub(crate) fn synthesize_junction_resource(
    declaring_resource: &str,
    mt: &ir::ManyThrough,
) -> ir::Resource {
    let declaring_fk = format!("{}_id", crate::helpers::pascal_to_snake(declaring_resource));
    let partner_fk = format!("{}_id", crate::helpers::pascal_to_snake(&mt.partner));

    let mut fields = vec![
        junction_fk_field(&declaring_fk, declaring_resource),
        junction_fk_field(&partner_fk, &mt.partner),
    ];
    fields.extend(mt.payload.iter().cloned());

    ir::Resource {
        name: mt.junction.clone(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: None,
        fields,
        constraints: vec![ir::Constraint::Unique(ir::UniqueConstraint {
            fields: vec![declaring_fk, partner_fk],
            per: None,
            when: None,
        })],
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        append_only: false,
        // A synthesized junction declares no `many_through` of its own.
        many_through: Vec::new(),
        restrict_on_delete: Vec::new(),
    }
}

/// GAP-07 — build one endpoint FK field on a synthesized junction. The
/// field type is a same-feature `UserDefined` reference to the endpoint
/// resource (`feature: None`); the migration emitter resolves it to a
/// `BIGINT` FK column. `required` so neither endpoint can dangle.
fn junction_fk_field(name: &str, target_resource: &str) -> ir::Field {
    ir::Field {
        name: name.to_owned(),
        type_ref: ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: target_resource.to_owned(),
        }),
        required: true,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

pub(crate) fn lower_resource_constraint(
    constraint: &syntax::ResourceConstraintAst,
) -> ir::Constraint {
    match constraint {
        syntax::ResourceConstraintAst::Unique(unique) => {
            ir::Constraint::Unique(ir::UniqueConstraint {
                fields: unique.fields.clone(),
                per: None,
                // GAP-NEW-001 — run the verbatim `when` text through the
                // shared closed-predicate parser (same path as
                // `invariant when`); unrecognized shapes land in
                // `EvalPredicate::Unparsed` so doctor can echo the source.
                when: unique
                    .when
                    .as_deref()
                    .map(crate::agent::parse_closed_predicate),
            })
        }
        syntax::ResourceConstraintAst::Index(index) => ir::Constraint::Index(ir::IndexConstraint {
            fields: index.fields.clone(),
            method: index.method.map(|method| match method {
                syntax::ResourceIndexMethodAst::Btree => ir::IndexMethod::Btree,
                syntax::ResourceIndexMethodAst::Gin => ir::IndexMethod::Gin,
                syntax::ResourceIndexMethodAst::Gist => ir::IndexMethod::Gist,
            }),
            full_text: index.full_text,
        }),
    }
}

/// Migrations bucket cycle Route C — strip the `migrated`/`alias` mode
/// prefix from a parsed `previously` line. `previously migrated Foo`
/// keeps `Foo` in IR; `previously alias Foo` ditto. Doctor compares
/// against current symbol names, so the mode keyword is noise here.
fn strip_previously_mode(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("migrated ") {
        return rest.trim().to_owned();
    }
    if let Some(rest) = trimmed.strip_prefix("alias ") {
        return rest.trim().to_owned();
    }
    trimmed.to_owned()
}

pub(crate) fn lower_resource_field(
    f: &syntax::ResourceFieldDecl,
) -> Result<ir::Field, AnalyzeError> {
    let (type_text_with_recovered_modifiers, type_pii) =
        extract_field_level_pii_decorator(&f.type_text);
    let recovered = peel_trailing_field_modifiers(&type_text_with_recovered_modifiers);
    let (default_text, default_pii) = match f.default.as_deref() {
        Some(raw) => {
            let (cleaned, pii) = extract_field_level_pii_decorator(raw);
            (Some(cleaned), pii)
        }
        None => (None, None),
    };
    let pii = type_pii.or(default_pii);
    let default = default_text.as_deref().map(|raw| parse_default(raw.trim()));
    let constraints = lift_field_constraints(&f.name, &f.constraints)?;
    // L0 #3 §10.2 + §10.3 — combination rules + default compatibility.
    validate_constraint_combinations(&f.name, &f.constraints)?;
    // Wave-B-CL4 — three follow-up diagnostics for the inline-validator
    // surface: range invariants (`min>max`, `between A>B`), per-type
    // applicability (§10.1), and structural regex sanity. Combination
    // conflicts run first so `length+min` etc. take precedence over
    // the per-constraint type / range checks.
    validate_constraint_range_invariant(&f.name, &f.constraints)?;
    validate_constraint_type_compatibility(&f.name, &recovered.type_text, &f.constraints)?;
    validate_constraint_pattern_compile(&f.name, &f.constraints)?;
    if let Some(default_text) = default_text.as_deref() {
        validate_default_against_constraints(&f.name, default_text.trim(), &f.constraints)?;
    }
    let type_ref = type_ref_from_syntax(&recovered.type_text);
    // `ir-resource-conventions-owner-scope` §11.1 — `owner_axis_on_non_fk`.
    // The annotation is only meaningful on FK fields (`UserDefined`
    // resources). Primitives, builtins, and capability-typed fields
    // can't carry an ownership chain; reject at lowering so the synth
    // pass (O2) doesn't have to guard against malformed IR.
    let owner_axis = match f.owner_axis.as_ref() {
        Some(axis) => {
            if !matches!(type_ref, ir::TypeRef::UserDefined(_)) {
                return Err(AnalyzeError::OwnerAxisOnNonFk {
                    field: f.name.clone(),
                    type_text: recovered.type_text.clone(),
                });
            }
            Some(ir::OwnerAxis {
                through_column: axis.through_column.clone(),
            })
        }
        None => None,
    };
    // GAP-12 — project the `target @feature.<feature>.<Resource>`
    // cross-feature FK annotation onto the IR. The reference is logical;
    // doctor (`REF-CROSS-FEATURE-UNKNOWN-001`) validates the feature is in
    // the declaring feature's `uses` and the resource exists there.
    let cross_feature_target = f
        .cross_feature_target
        .as_ref()
        .map(|t| ir::CrossFeatureTarget {
            feature: t.feature.clone(),
            resource: t.resource.clone(),
        });
    Ok(ir::Field {
        name: f.name.clone(),
        // Phase L Tier 4 follow-up — use `type_ref_from_syntax` so
        // `@cap.Hashed(algorithm:…)`, `@cap.Encrypted(key:…)`,
        // `@cap.Token(…)`, and `@semantic.*` lift into typed variants.
        // The legacy `type_ref_from_text` path is preserved for
        // call sites that pass cleaned-up identifiers only.
        type_ref,
        required: f.required || recovered.required,
        unique: f.unique || recovered.unique,
        // CL.C.4 — lift `@slug` decorator presence into the typed IR.
        slug: f.slug,
        default,
        derived_from: f.derived_from.clone(),
        // W3 GAP-03 — project the typed `computed_date from <base> offset
        // <offset>` field kind onto the IR. The references are validated by
        // doctor (`COMPUTED-DATE-EXPR-001`): `base` must be a declared
        // `Date` field and `offset` an `Integer` field or integer literal.
        computed_date: f.computed_date.as_ref().map(lower_computed_date),
        constraints,
        // Roadmap §1.5 (CL.C.2) — `@full_text` decorator captured by
        // the parser as a flag on the field declaration; threaded
        // through to the IR so DDL emission can attach a GIN tsvector
        // index per marked field.
        full_text: f.full_text,
        previous_names: f
            .previously
            .iter()
            .map(|p| strip_previously_mode(p))
            .collect(),
        pii,
        owner_axis,
        cross_feature_target,
        span_ref: Some(span_of(f.span)),
    })
}

/// W3 GAP-03 — lower the AST `computed_date from <base> offset <offset>`
/// payload onto the typed IR node. Pure projection: reference validity
/// (base is a `Date` field, offset is an `Integer` field or literal) is
/// enforced by doctor `COMPUTED-DATE-EXPR-001`, not here, mirroring how
/// `derived from` keeps its expression verbatim and defers field-resolution
/// to doctor.
fn lower_computed_date(ast: &syntax::ComputedDateAst) -> ir::ComputedDate {
    let offset = match &ast.offset {
        syntax::ComputedDateOffsetAst::Field(name) => ir::ComputedDateOffset::Field(name.clone()),
        syntax::ComputedDateOffsetAst::Literal(n) => ir::ComputedDateOffset::Literal(*n),
    };
    // W4 GAP-08 — project the base selector: a bare `Date` field (W3) or a
    // rule-enum + `@fn` (W4 `schedule_rule`). Reference validity (the `@fn`
    // is a declared binding fn, the rule arg references a field) is enforced
    // by doctor `SCHEDULE-RULE-001`, not here.
    let base = match &ast.base {
        syntax::ComputedDateBaseAst::Field(name) => ir::ComputedDateBase::Field(name.clone()),
        syntax::ComputedDateBaseAst::Rule { rule, fn_ref } => ir::ComputedDateBase::Rule {
            rule: rule.clone(),
            fn_ref: fn_ref.clone(),
        },
    };
    ir::ComputedDate { base, offset }
}
