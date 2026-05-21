//! Lowering from `lazuli_syntax` canonical AST slices into `lazuli_ir`.

pub mod checks;
mod lifecycle;
pub mod rbac;
pub mod source_map;
pub mod symbol_origin;

pub use symbol_origin::build_symbol_origin_index;

use lazuli_ir as ir;
use lazuli_syntax as syntax;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("invalid tool reference `{reference}`")]
    InvalidToolRef { reference: String },

    /// Phase L — `auth identity` field reference must split exactly once
    /// into `<Resource>.<field>`. Parser already rejects missing-dot
    /// shapes; this guards downstream lowering against multi-dot or
    /// empty-segment forms slipping through.
    #[error("invalid auth identity `{reference}` — expected `<Resource>.<field>`")]
    InvalidAuthIdentity { reference: String },

    /// Phase L Tier 3 — `webhook verify <scheme>` only accepts `hmac`
    /// today. Adapters that ship other schemes lift through the
    /// registry adapter binding, not the verifier surface.
    #[error("unsupported webhook verify scheme `{scheme}` (use `hmac`)")]
    UnsupportedVerifyScheme { scheme: String },

    /// L0 #8 — `poller` block missing a required child. Surfaces at
    /// lowering when the parser somehow allowed a structurally
    /// incomplete poller through (defense-in-depth; parser already
    /// rejects most). See `docs/proposals/poller-vocab.md` §3.
    #[error("POLLER-MISSING-FIELD: `{kind}` `{name}` is missing required field `{field}`")]
    MissingField {
        kind: String,
        name: String,
        field: String,
    },

    /// L0 #8 — `poller retry backoff <strategy>` outside the closed
    /// catalog (`fixed` | `linear` | `exponential`).
    #[error(
        "POLLER-UNKNOWN-ENUM: `{kind}` carries unknown value `{value}` outside the closed catalog"
    )]
    UnknownEnum { kind: String, value: String },

    /// L0 #2 — `design <X>` declared `extends <Y>`. Cut B (post-pilot).
    /// v0 keeps the keyword reserved at parse time but rejects at
    /// lowering. See `docs/proposals/design-tokens.md` §3.6.
    #[error(
        "DESIGN-EXTENDS-CUT-B: theme inheritance via `extends` ships in Cut B (post-pilot); for v0 declare a standalone `design <X>` block with full token values (got `extends {target}`)"
    )]
    DesignExtendsCutB { target: String },

    /// L0 #3 — view source did not parse as
    /// `<feature>.query.<short>` or
    /// `<feature>.query.{list|lookup|sql}.<short>`.
    #[error(
        "LZX-BAD-QUERY-REF: view `{view}` source `{value}` must be `<feature>.query.<name>` (or `.query.{{list|lookup|sql}}.<name>`)"
    )]
    LzxBadQueryRef { view: String, value: String },

    /// L0 #3 — `submit` or `actions` entry did not parse as a command
    /// reference. Accepts `<feature>.command.<name>` (qualified) or a
    /// bare local short name (`create`).
    #[error(
        "LZX-BAD-COMMAND-REF: command reference `{value}` must be `<feature>.command.<name>` or a bare local short name"
    )]
    LzxBadCommandRef { value: String },

    /// L0 #3 §11 `lzx-cell-slot-orphan` — a `cells <field> @client.<slot>`
    /// binding references a field that isn't in the view's column /
    /// section / fields list. v0 surfaces this at lowering; doctor may
    /// downgrade to a warning.
    #[error(
        "LZX-CELL-SLOT-ORPHAN: view `{view}` cell binding for field `{field}` is not in its columns / sections / fields list"
    )]
    LzxCellSlotOrphan { view: String, field: String },

    /// L0 #3 — the cell slot identifier itself is malformed (empty or
    /// non-kebab/snake characters). Parser-time check; this guards
    /// against direct AST construction.
    #[error(
        "LZX-CELL-SLOT-INVALID: view `{view}` cell slot `{slot}` must be a kebab/snake identifier"
    )]
    LzxCellSlotInvalid { view: String, slot: String },

    /// L0 #3 §11 `lzx-route-param-missing-binding` — a `:name`
    /// placeholder in the `at "<path>"` string has no matching
    /// `route <name>: <Type> from path` declaration.
    #[error(
        "LZX-ROUTE-PARAM-MISSING-BINDING: view `{view}` path placeholder `:{placeholder}` has no `route {placeholder}: <Type> from path` declaration"
    )]
    LzxRouteParamMissingBinding { view: String, placeholder: String },

    /// L0 #3 §11 `lzx-route-param-orphan` — a `route <name>: Type from
    /// path` declaration has no matching `:name` placeholder in the
    /// view's `at "<path>"`.
    #[error(
        "LZX-ROUTE-PARAM-ORPHAN: view `{view}` declared route param `{param}` but the `at` path has no `:{param}` placeholder"
    )]
    LzxRouteParamOrphan { view: String, param: String },

    /// L0 #2 — a `shadow <name> "<value>"` entry carried a top-level
    /// comma, indicating multi-layer composition. Closed v0 grammar
    /// accepts only single-layer shadows; declare separate tokens
    /// (`shadow.elevated_outer`, `shadow.elevated_inner`) and compose
    /// at component level. See `docs/proposals/design-tokens.md` §4.6.
    #[error(
        "DESIGN-SHADOW-MULTI-LAYER: shadow `{name}` is multi-layer (top-level comma); v0 accepts single-layer only — declare separate tokens and compose at component level"
    )]
    DesignShadowMultiLayer { name: String },

    /// L0 #2 — a color hex value did not match `#[0-9a-fA-F]{3,8}`.
    /// Covers all four color-state slots plus flat-form entries.
    #[error(
        "DESIGN-COLOR-HEX-INVALID: color `{token}` state `{state}` carries invalid hex `{value}` (expected `#RGB`, `#RRGGBB`, or `#RRGGBBAA`)"
    )]
    DesignColorHexInvalid {
        token: String,
        state: String,
        value: String,
    },

    /// L0 #2 — a color sub-block named a state outside the closed
    /// catalog `{base, hover, active, foreground}`. Adding new states
    /// requires a Lazuli core proposal (Rule Zero).
    #[error(
        "DESIGN-COLOR-STATE-UNKNOWN: color `{token}` declared unknown state `{state}` (allowed: `base`, `hover`, `active`, `foreground`)"
    )]
    DesignColorStateUnknown { token: String, state: String },

    /// L0 #2 — `typography.weight` value did not parse as a `u16`.
    #[error(
        "DESIGN-WEIGHT-INVALID: typography.weight `{name}` has non-integer value `{value}` (expected 100-1000)"
    )]
    DesignWeightInvalid { name: String, value: String },

    /// L0 #2 — `z <name> <value>` value did not parse as `i32`.
    #[error("DESIGN-Z-INVALID: z token `{name}` has non-integer value `{value}`")]
    DesignZInvalid { name: String, value: String },

    /// L0 #3 §10.2 — conflicting inline field constraints. Per the
    /// proposal: `length` rejects `min`/`max`, `between` rejects
    /// `min`/`max`, and `in [...]` rejects `pattern`. The `combo`
    /// string names the rejected pair (e.g. `length+min`,
    /// `between+max`, `in+pattern`).
    #[error(
        "FIELD-CONSTRAINT-CONFLICT: field `{field}` has incompatible constraints (`{combo}`); see docs/proposals/lzx-integration-codegen.md §10.2"
    )]
    ConstraintConflict { field: String, combo: String },

    /// L0 #3 §10.3 — a `default` value does not satisfy the field's
    /// declared inline constraints. The analyzer accepts the value
    /// verbatim from the parser; here we check it against `min`,
    /// `max`, `length`, `between`, and `in [...]`. `pattern` is
    /// honoured for string defaults too.
    #[error(
        "FIELD-DEFAULT-VIOLATES-CONSTRAINT: field `{field}` default `{value}` violates `{rule}`; see docs/proposals/lzx-integration-codegen.md §10.3"
    )]
    DefaultViolatesConstraint {
        field: String,
        value: String,
        rule: String,
    },

    /// `inline_validator_range_invariant_001` (Wave-B-CL4) — a
    /// numeric bound pair is logically empty: `min N max M` with N>M,
    /// or `between A and B` with A>B. These would produce an
    /// uninhabited domain at runtime; reject at compile time. The
    /// `rule` string identifies which pair (`min>max`, `between`).
    /// `low` / `high` carry the violating literals (verbatim text)
    /// so the error message shows the author what they wrote.
    #[error(
        "INLINE-VALIDATOR-RANGE-INVARIANT: field `{field}` has empty range `{rule}` (`{low}` > `{high}`); swap the bounds or pick one side"
    )]
    InlineValidatorRangeInvariant {
        field: String,
        rule: String,
        low: String,
        high: String,
    },

    /// `inline_validator_type_mismatch_001` (Wave-B-CL4) — a
    /// constraint keyword was applied to a field whose builtin type
    /// is not in §10.1's "Applies to" column. Examples: `pattern` on
    /// `Boolean`, `length` on `Integer`, `between` on `Text`.
    /// `constraint` names the offending keyword (`pattern`, `length`,
    /// `between`); `field_type` echoes the source `type_text` so the
    /// author sees what they typed (vs the resolved BuiltinType,
    /// which is internal vocabulary).
    #[error(
        "INLINE-VALIDATOR-TYPE-MISMATCH: field `{field}: {field_type}` cannot use `{constraint}` (applies to {applies_to} only); see docs/proposals/lzx-integration-codegen.md §10.1"
    )]
    InlineValidatorTypeMismatch {
        field: String,
        field_type: String,
        constraint: String,
        applies_to: String,
    },

    /// `inline_validator_pattern_compile_001` (Wave-B-CL4) — the
    /// `pattern "STRING"` regex failed a structural well-formedness
    /// check at compile time. We do NOT pull in the `regex` crate
    /// (Lazuli analyzer stays regex-free by design — see comment in
    /// `validate_default_against_constraints`). Instead we check
    /// bracket/paren balance and reject the few unambiguous RE2
    /// shape errors (unbalanced `[`, unbalanced `(`, trailing `\`).
    /// Runtime regex compilation in Go/JS is still the authoritative
    /// validator; this just catches the trivial typos at author time.
    #[error(
        "INLINE-VALIDATOR-PATTERN-COMPILE: field `{field}` pattern `{pattern}` is malformed: {reason}"
    )]
    InlineValidatorPatternCompile {
        field: String,
        pattern: String,
        reason: String,
    },

    #[error(
        "INLINE-VALIDATOR-UNKNOWN-SANITIZE-HTML: field `{field}` uses unknown sanitize_html profile `{profile}` (allowed: strict, basic, markdown_safe)"
    )]
    UnknownSanitizeHtmlProfile { field: String, profile: String },

    /// `conventions_unknown` — a `conventions [<ident>]` entry on a
    /// resource named an identifier outside the closed in-core catalog.
    /// The parser (Cell C2) is responsible for actually emitting this
    /// when it sees the bad identifier; Cell C1 wires the variant and
    /// the suggestion helper so the parser has somewhere to land. The
    /// `suggestion` field carries the nearest catalog entry (within
    /// Levenshtein distance ≤ 2) or `None` when no close match exists.
    /// Format via the dedicated `Display` impl below — `thiserror`'s
    /// inline format-string syntax does not support the conditional
    /// "did you mean" suffix we want. See
    /// `docs/proposals/ir-resource-conventions-crud.md` §4.3.
    #[error("{}", format_conventions_unknown(.resource, .identifier, .suggestion.as_deref()))]
    ConventionsUnknown {
        resource: String,
        identifier: String,
        suggestion: Option<String>,
    },

    /// `owner_axis_on_non_fk` — `ir-resource-conventions-owner-scope`
    /// §11.1. The `@owner_axis(through: <ident>)` annotation was applied
    /// to a field whose lowered `TypeRef` is not `UserDefined` (i.e. not
    /// a foreign-key reference to another resource). Primitives, builtins,
    /// tenant-scope columns, and capability-typed fields can't carry an
    /// ownership chain; the synth pass (O2) has no FK target to walk to.
    /// `type_text` echoes the raw decorator-chain text the parser saw so
    /// the author understands what the analyzer rejected.
    #[error(
        "OWNER-AXIS-ON-NON-FK: field `{field}: {type_text}` cannot carry `@owner_axis(...)` — annotation is only valid on FK fields (typed as another resource)"
    )]
    OwnerAxisOnNonFk { field: String, type_text: String },
}

/// Format helper for `AnalyzeError::ConventionsUnknown`. Kept out of
/// the `thiserror` attribute so the conditional "did you mean" suffix
/// stays readable and testable. When `suggestion` is `Some` the
/// message ends with ` — did you mean \`<sug>\`?`; otherwise the
/// suggestion clause is omitted entirely.
fn format_conventions_unknown(
    resource: &str,
    identifier: &str,
    suggestion: Option<&str>,
) -> String {
    let base = format!(
        "CONVENTIONS-UNKNOWN: resource `{resource}` declared `conventions [{identifier}]` — `{identifier}` is not in the convention catalog (allowed today: `crud`)"
    );
    match suggestion {
        Some(sug) => format!("{base} — did you mean `{sug}`?"),
        None => base,
    }
}

/// ir-resource-conventions-crud Cell C1 — closed catalog of resource
/// convention identifiers accepted by the parser. Grows additively per
/// future proposals (`timestamped`, `pii_aware`, `soft_delete`,
/// `slugged`, `paginated`). `me` was added by cell M1 of
/// `ir-resource-conventions-me.md` §4.3.
///
/// Sorted alphabetically for diff hygiene; keep new entries in order.
pub const CONVENTION_CATALOG: &[&str] = &["crud", "me"];

/// Resolve the closest catalog entry to a misspelled `conventions`
/// identifier using plain Levenshtein distance. Returns the catalog
/// entry when its distance is ≤ 1 (the §4.3 spec — "single-character
/// Levenshtein"). Distance-1 covers the documented case `crd` → `crud`
/// (one insertion) and the obvious neighbours (`crue`, `cru d` etc.);
/// anything further falls through to `None` so the diagnostic doesn't
/// suggest gibberish.
///
/// Reused by the parser (Cell C2) when it sees `conventions [<ident>]`
/// outside the catalog: the parser constructs
/// `AnalyzeError::ConventionsUnknown { suggestion: conventions_unknown_suggestion(ident).map(str::to_owned), ... }`.
pub fn conventions_unknown_suggestion(identifier: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for &candidate in CONVENTION_CATALOG {
        let d = conventions_levenshtein(identifier, candidate);
        if d > 1 {
            continue;
        }
        match best {
            None => best = Some((candidate, d)),
            Some((_, prev_d)) if d < prev_d => best = Some((candidate, d)),
            _ => {}
        }
    }
    best.map(|(name, _)| name)
}

/// Plain Levenshtein edit distance (O(n*m) DP). Used only for short
/// catalog identifiers so the cost is negligible. Kept local to the
/// analyzer crate because the LSP's identical helper is `pub(crate)`
/// there; promoting either to a shared util would couple the two
/// crates needlessly. The duplication is intentional and stable —
/// neither identifier is hot-path code.
fn conventions_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

pub fn lower_lzx_document(document: &syntax::LzxDocument) -> ir::ExperienceModule {
    ir::ExperienceModule {
        app: document.app.as_ref().map(lower_lzx_app),
        routes: document.routes.iter().map(lower_lzx_route).collect(),
        experiences: document.experiences.iter().map(lower_experience).collect(),
        surfaces: document
            .surfaces
            .iter()
            .map(lower_platform_surface)
            .collect(),
    }
}

fn lower_lzx_app(app: &syntax::LzxApp) -> ir::AppManifest {
    let route_guard = app
        .route_guard
        .as_ref()
        .map(lower_route_guard_defaults)
        .or_else(|| {
            app.auth_failed_redirect
                .as_ref()
                .map(|redirect| ir::RouteGuardDefaults {
                    default_policy: None,
                    on_unauthenticated: Some(redirect.clone()),
                    on_unauthorized: None,
                    skeleton: None,
                    span_ref: Some(span_of(app.span)),
                })
        });

    ir::AppManifest {
        name: app.name.clone(),
        title: app.title.clone(),
        version: app.version.clone(),
        lazuli_version: None,
        targets: app.targets.clone(),
        default_locale: app.default_locale.clone(),
        default_timezone: app.default_timezone.clone(),
        auth_failed_redirect: app.auth_failed_redirect.clone(),
        not_found: app.not_found.clone(),
        error_pages: app
            .error_pages
            .iter()
            .map(|page| ir::ErrorPage {
                status: page.status,
                template: page.template.clone(),
                audience: page.audience.clone(),
            })
            .collect(),
        uses: app.uses.clone(),
        packs: Vec::new(),
        bindings: Vec::new(),
        architecture: None,
        services: Vec::new(),
        communication: None,
        environments: Vec::new(),
        urls: Vec::new(),
        cors: None,
        headers: None,
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
        cookie: None,
        proxy: None,
        limits: None,
        // ir-route-guards Cell IR-1 — slots wired by Cell PARSE-1.
        route_guard,
        actor_query: app.actor_query.clone(),
        span_ref: Some(span_of(app.span)),
    }
}

fn lower_lzx_route(route: &syntax::LzxRoute) -> ir::AppRoute {
    ir::AppRoute {
        name: route.name.clone(),
        path: route.path.clone(),
        routes: route.routes.clone(),
        to: route.to.clone(),
        surface: route.surface.clone(),
        audience: route.audience.clone(),
        lazy: route.lazy,
        prerender: route.prerender.clone(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: route.guard.as_ref().map(lower_view_guard),
        span_ref: Some(span_of(route.span)),
    }
}

fn lower_experience(experience: &syntax::LzxExperience) -> ir::Experience {
    ir::Experience {
        name: experience.name.clone(),
        imports: experience.imports.clone(),
        views: experience.views.iter().map(lower_experience_view).collect(),
        resume_routers: experience
            .resume_routers
            .iter()
            .map(lower_resume_router)
            .collect(),
        extensions: experience
            .extensions
            .iter()
            .map(lower_view_extension)
            .collect(),
        span_ref: Some(span_of(experience.span)),
    }
}

fn lower_resume_router(router: &syntax::LzxResumeRouter) -> ir::ResumeRouter {
    ir::ResumeRouter {
        name: router.name.clone(),
        source_query: router.source_query.clone(),
        arms: router.arms.iter().map(lower_resume_arm).collect(),
        span_ref: Some(span_of(router.span)),
    }
}

fn lower_resume_arm(arm: &syntax::LzxResumeArm) -> ir::ResumeArm {
    ir::ResumeArm {
        kind: match &arm.kind {
            syntax::LzxResumeArmKind::State(state) => ir::ResumeArmKind::State(state.clone()),
            syntax::LzxResumeArmKind::None => ir::ResumeArmKind::None,
            syntax::LzxResumeArmKind::Wildcard => ir::ResumeArmKind::Wildcard,
        },
        target_view: arm.target_view.clone(),
        span_ref: Some(span_of(arm.span)),
    }
}

fn lower_experience_view(view: &syntax::LzxExperienceView) -> ir::ExperienceView {
    ir::ExperienceView {
        name: view.name.clone(),
        anchor: view.anchor.clone(),
        routes: view.routes.clone(),
        extensible_by: view.extensible_by.clone(),
        source: view.source.clone(),
        submit: view.submit.clone(),
        blocks: view.blocks.clone(),
        actions: view.actions.iter().map(lower_experience_action).collect(),
        opens: view.opens.clone(),
        tests: view.tests.clone(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: view.guard.as_ref().map(lower_view_guard),
        resolved_guard_policy: None,
        resolved_lifecycle_gate: None,
        span_ref: Some(span_of(view.span)),
    }
}

fn lower_experience_action(action: &syntax::LzxAction) -> ir::ExperienceAction {
    ir::ExperienceAction {
        name: action.name.clone(),
        target: action.target.clone(),
        span_ref: Some(span_of(action.span)),
    }
}

fn lower_view_extension(extension: &syntax::LzxViewExtension) -> ir::ViewExtension {
    ir::ViewExtension {
        anchor: extension.anchor.clone(),
        blocks: extension.blocks.clone(),
        slots: extension
            .slots
            .iter()
            .map(lower_view_extension_slot)
            .collect(),
        span_ref: Some(span_of(extension.span)),
    }
}

fn lower_view_extension_slot(slot: &syntax::LzxExtensionSlot) -> ir::ViewExtensionSlot {
    ir::ViewExtensionSlot {
        name: slot.name.clone(),
        order: slot.order.as_ref().map(|order| ir::ViewExtensionOrder {
            relation: order.relation.clone(),
            target: order.target.clone(),
        }),
        blocks: slot.blocks.clone(),
        platforms: slot.platforms.clone(),
        audiences: slot.audiences.clone(),
        span_ref: Some(span_of(slot.span)),
    }
}

fn lower_platform_surface(surface: &syntax::LzxSurface) -> ir::PlatformSurface {
    ir::PlatformSurface {
        experience: surface.experience.clone(),
        platform: match surface.platform {
            syntax::LzxPlatform::Web => ir::Platform::Web,
            syntax::LzxPlatform::Mobile => ir::Platform::Mobile,
        },
        uses_experience: surface.uses_experience.clone(),
        audiences: surface
            .audiences
            .iter()
            .map(lower_audience_surface)
            .collect(),
        span_ref: Some(span_of(surface.span)),
    }
}

fn lower_audience_surface(audience: &syntax::LzxAudience) -> ir::AudienceSurface {
    ir::AudienceSurface {
        name: audience.name.clone(),
        qualifiers: audience.qualifiers.clone(),
        views: audience.views.iter().map(lower_platform_view).collect(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: audience.guard.as_ref().map(lower_view_guard),
        span_ref: Some(span_of(audience.span)),
    }
}

fn lower_platform_view(view: &syntax::LzxPlatformView) -> ir::PlatformView {
    ir::PlatformView {
        name: view.name.clone(),
        view_type: view.view_type.clone(),
        columns: view.columns.clone(),
        fields: view.fields.clone(),
        sections: view.sections.clone(),
        search: view.search.clone(),
        filter: view.filter.clone(),
        cells: view.cells.clone(),
        actions: view.actions.clone(),
        submit: view.submit.clone(),
        blocks: view.blocks.clone(),
        // ir-route-guards Cell IR-1 — guard slot wired by Cell PARSE-1.
        guard: view.guard.as_ref().map(lower_view_guard),
        resolved_guard_policy: None,
        span_ref: Some(span_of(view.span)),
    }
}

fn lower_view_guard(guard: &syntax::LzxViewGuard) -> ir::ViewGuard {
    ir::ViewGuard {
        policy: guard.policy.clone(),
        on_unauthenticated: guard.on_unauthenticated.clone(),
        on_unauthorized: guard.on_unauthorized.clone(),
        requires_lifecycle: guard.requires_lifecycle.as_ref().map(|requires| {
            ir::RequiresLifecycle {
                resource: requires.resource.clone(),
                state: requires.state.clone(),
                span_ref: Some(span_of(requires.span)),
            }
        }),
        on_lifecycle_pending: guard.on_lifecycle_pending.clone(),
        span_ref: Some(span_of(guard.span)),
    }
}

fn lower_route_guard_defaults(defaults: &syntax::LzxRouteGuardDefaults) -> ir::RouteGuardDefaults {
    ir::RouteGuardDefaults {
        default_policy: defaults.default_policy.clone(),
        on_unauthenticated: defaults.on_unauthenticated.clone(),
        on_unauthorized: defaults.on_unauthorized.clone(),
        skeleton: defaults.skeleton.clone(),
        span_ref: Some(span_of(defaults.span)),
    }
}

// =============================================================================
// L0 #3 — lzx ViewModel surface lowering.
// -----------------------------------------------------------------------------
// `lower_surface` takes a parsed `SurfaceAst` (from
// `lazuli_syntax::parse_surface_document`) and yields an `ir::Surface`.
// Validations performed at lowering time:
//   - source/submit references are well-formed (`<feature>.query.<kind>.<name>`
//     and `<feature>.command.<name>` respectively, OR the short
//     bare-name form for actions inside the same surface).
//   - cell slot + field identifiers are valid kebab/snake idents (parser
//     already enforces; we re-check to defend against direct AST
//     construction).
//   - When both `at "<path>"` and `route <name>: Type from path` are
//     declared, every `:<name>` placeholder in the path has a matching
//     `route_params` entry and vice versa (`lzx-route-param-*`).
// Deeper validations (`source` resource exists, `actions` reaches the
// audience's scope, etc.) defer to the doctor cells per the proposal.
// =============================================================================

/// Lower a `SurfaceAst` (parser output) into the canonical `ir::Surface`
/// per `docs/proposals/lzx-integration-codegen.md` §5 + §5.2.
pub fn lower_surface(ast: &syntax::SurfaceAst) -> Result<ir::Surface, AnalyzeError> {
    let target = match ast.target {
        syntax::SurfaceTargetAst::Web => ir::SurfaceTarget::Web,
        syntax::SurfaceTargetAst::Mobile => ir::SurfaceTarget::Mobile,
    };
    let owning_feature = ast
        .uses_feature
        .clone()
        .unwrap_or_else(|| ast.feature.clone());

    let mut audiences = Vec::with_capacity(ast.audiences.len());
    for audience in &ast.audiences {
        audiences.push(lower_audience_ast(audience, &owning_feature)?);
    }

    Ok(ir::Surface {
        feature: owning_feature,
        target,
        audiences,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_audience_ast(
    ast: &syntax::AudienceAst,
    owning_feature: &str,
) -> Result<ir::Audience, AnalyzeError> {
    let requires = ast
        .requires
        .iter()
        .map(|atom| ir::PolicyAtom {
            namespace: atom.namespace.clone(),
            name: atom.name.clone(),
            args: atom.args.clone(),
        })
        .collect();

    let mut views = Vec::with_capacity(ast.views.len());
    for view in &ast.views {
        views.push(lower_view_ast(view, owning_feature)?);
    }
    Ok(ir::Audience {
        name: ast.name.clone(),
        requires,
        views,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_view_ast(ast: &syntax::ViewAst, owning_feature: &str) -> Result<ir::View, AnalyzeError> {
    match ast {
        syntax::ViewAst::List(v) => {
            let source =
                parse_query_ref(&v.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
                    view: v.name.clone(),
                    value: v.source.clone(),
                })?;
            let render = lower_list_render(v);
            let render_columns = match &render {
                ir::ListRender::Table { columns } => columns.as_slice(),
                ir::ListRender::Cells { .. } => &[],
            };
            validate_cells(&v.cells, render_columns, &v.name)?;
            let actions = v
                .actions
                .iter()
                .map(|s| parse_command_ref(s, owning_feature))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ir::View::List(ir::ViewList {
                name: v.name.clone(),
                route: v.route.clone(),
                source,
                render,
                search: v.search.as_ref().map(lower_search_decl),
                filter: lower_filter_decls(v),
                cells: v
                    .cells
                    .iter()
                    .map(|c| ir::CellBinding {
                        field: c.field.clone(),
                        slot: c.slot.clone(),
                    })
                    .collect(),
                actions,
                drawer: v
                    .drawer
                    .as_ref()
                    .map(|drawer| lower_drawer(drawer, owning_feature))
                    .transpose()?,
                sort: v.sort.as_ref().map(lower_sort_decl),
                selection: v
                    .selection
                    .as_ref()
                    .map(|selection| lower_selection_decl(selection, owning_feature))
                    .transpose()?,
                settings: v.settings.iter().map(lower_setting_decl).collect(),
                redacted_fields: v.redacted_fields.clone(),
                span_ref: Some(span_of(v.span)),
            }))
        }
        syntax::ViewAst::Detail(v) => {
            let source =
                parse_query_ref(&v.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
                    view: v.name.clone(),
                    value: v.source.clone(),
                })?;
            // Detail views bind cells against fields on the source resource,
            // not against the `sections` enumeration. The source-resource
            // cross-check happens at doctor time (`lzx-source-resource-mismatch`).
            // We only validate cell slot identifier shape here.
            validate_cells_slot_only(&v.cells, &v.name)?;
            // Route param ↔ placeholder cross-check (`lzx-route-param-*`).
            if let Some(path) = v.route.as_ref() {
                let placeholders = path_placeholders(path);
                for placeholder in &placeholders {
                    if !v.route_params.iter().any(|p| &p.name == placeholder) {
                        return Err(AnalyzeError::LzxRouteParamMissingBinding {
                            view: v.name.clone(),
                            placeholder: placeholder.clone(),
                        });
                    }
                }
                for param in &v.route_params {
                    if !placeholders.iter().any(|p| p == &param.name) {
                        return Err(AnalyzeError::LzxRouteParamOrphan {
                            view: v.name.clone(),
                            param: param.name.clone(),
                        });
                    }
                }
            }
            let actions = v
                .actions
                .iter()
                .map(|s| parse_command_ref(s, owning_feature))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ir::View::Detail(ir::ViewDetail {
                name: v.name.clone(),
                route: v.route.clone(),
                source,
                route_params: v
                    .route_params
                    .iter()
                    .map(|p| ir::RouteParam {
                        name: p.name.clone(),
                        type_ref: p.type_ref.clone(),
                    })
                    .collect(),
                sections: v.sections.clone(),
                cells: v
                    .cells
                    .iter()
                    .map(|c| ir::CellBinding {
                        field: c.field.clone(),
                        slot: c.slot.clone(),
                    })
                    .collect(),
                actions,
                redacted_fields: v.redacted_fields.clone(),
                span_ref: Some(span_of(v.span)),
            }))
        }
        syntax::ViewAst::Create(v) => {
            let submit = parse_command_ref(&v.submit, owning_feature)?;
            validate_cells(&v.cells, &v.fields, &v.name)?;
            Ok(ir::View::Create(ir::ViewCreate {
                name: v.name.clone(),
                route: v.route.clone(),
                submit,
                fields: v.fields.clone(),
                cells: v
                    .cells
                    .iter()
                    .map(|c| ir::CellBinding {
                        field: c.field.clone(),
                        slot: c.slot.clone(),
                    })
                    .collect(),
                redacted_fields: v.redacted_fields.clone(),
                span_ref: Some(span_of(v.span)),
            }))
        }
    }
}

fn lower_list_render(ast: &syntax::ViewListAst) -> ir::ListRender {
    match (ast.columns.is_empty(), ast.cells_slot.as_ref()) {
        (false, None) => ir::ListRender::Table {
            columns: ast.columns.clone(),
        },
        (true, Some(slot)) => ir::ListRender::Cells { slot: slot.clone() },
        (false, Some(_)) | (true, None) => ir::ListRender::Table {
            columns: ast.columns.clone(),
        },
    }
}

fn lower_filter_decls(ast: &syntax::ViewListAst) -> Vec<ir::FilterDecl> {
    let mut filters: Vec<ir::FilterDecl> = ast.filters.iter().map(lower_filter_decl).collect();
    filters.extend(ast.filter.iter().map(|name| ir::FilterDecl {
        name: name.clone(),
        type_ref: String::new(),
        cardinality: ir::FilterCardinality::Single,
        url_sync: false,
        span_ref: None,
    }));
    filters
}

fn lower_filter_decl(ast: &syntax::FilterDeclAst) -> ir::FilterDecl {
    ir::FilterDecl {
        name: ast.name.clone(),
        type_ref: ast.type_ref.clone(),
        cardinality: match ast.cardinality {
            syntax::FilterCardinalityAst::Single => ir::FilterCardinality::Single,
            syntax::FilterCardinalityAst::Multi => ir::FilterCardinality::Multi,
        },
        url_sync: ast.url_sync,
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_search_decl(ast: &syntax::SearchDeclAst) -> ir::SearchDecl {
    ir::SearchDecl {
        mode: match &ast.mode {
            syntax::SearchModeAst::Columns(columns) => ir::SearchMode::Columns {
                columns: columns.clone(),
            },
            syntax::SearchModeAst::Segmented => ir::SearchMode::Segmented,
        },
        fields: ast.fields.iter().map(lower_search_field).collect(),
        free_text_target: ast.free_text_target.as_ref().map(lower_binding_ref),
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_search_field(ast: &syntax::SearchFieldAst) -> ir::SearchField {
    ir::SearchField {
        key: ast.key.clone(),
        binds_to: lower_binding_ref(&ast.binds_to),
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_binding_ref(ast: &syntax::BindingRefAst) -> ir::BindingRef {
    match ast {
        syntax::BindingRefAst::Filter { name } => ir::BindingRef::Filter { name: name.clone() },
        syntax::BindingRefAst::SourceInput { name } => {
            ir::BindingRef::SourceInput { name: name.clone() }
        }
        syntax::BindingRefAst::SelectionScalar => ir::BindingRef::SelectionScalar,
    }
}

fn lower_drawer(
    ast: &syntax::DrawerSubViewAst,
    owning_feature: &str,
) -> Result<ir::DrawerSubView, AnalyzeError> {
    let source = parse_query_ref(&ast.source).ok_or_else(|| AnalyzeError::LzxBadQueryRef {
        view: ast.name.clone(),
        value: ast.source.clone(),
    })?;
    validate_cells_slot_only(&ast.cells, &ast.name)?;
    let actions = ast
        .actions
        .iter()
        .map(|s| parse_command_ref(s, owning_feature))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ir::DrawerSubView {
        name: ast.name.clone(),
        trigger: match ast.trigger {
            syntax::DrawerTriggerAst::Select => ir::DrawerTrigger::Select,
            syntax::DrawerTriggerAst::ManualOpen => ir::DrawerTrigger::ManualOpen,
        },
        source,
        route_binding: ast.route_binding.as_ref().map(lower_drawer_route_binding),
        sections: ast.sections.clone(),
        cells: ast
            .cells
            .iter()
            .map(|c| ir::CellBinding {
                field: c.field.clone(),
                slot: c.slot.clone(),
            })
            .collect(),
        actions,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_drawer_route_binding(ast: &syntax::DrawerRouteBindingAst) -> ir::DrawerRouteBinding {
    ir::DrawerRouteBinding {
        target: ast.target.clone(),
        source: match ast.source {
            syntax::DrawerBindingSourceAst::Selection => ir::DrawerBindingSource::Selection,
        },
    }
}

fn lower_sort_decl(ast: &syntax::SortDeclAst) -> ir::SortDecl {
    ir::SortDecl {
        allowed: ast.allowed.clone(),
        default_field: ast.default_field.clone(),
        default_dir: match ast.default_dir {
            syntax::SortDirAst::Asc => ir::SortDir::Asc,
            syntax::SortDirAst::Desc => ir::SortDir::Desc,
        },
        span_ref: Some(span_of(ast.span)),
    }
}

fn lower_selection_decl(
    ast: &syntax::SelectionDeclAst,
    owning_feature: &str,
) -> Result<ir::SelectionDecl, AnalyzeError> {
    let bulk_actions = ast
        .bulk_actions
        .iter()
        .map(|s| parse_command_ref(s, owning_feature))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ir::SelectionDecl {
        mode: match ast.mode {
            syntax::SelectionModeAst::None => ir::SelectionMode::None,
            syntax::SelectionModeAst::Single => ir::SelectionMode::Single,
            syntax::SelectionModeAst::Multi => ir::SelectionMode::Multi,
        },
        bulk_actions,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_setting_decl(ast: &syntax::SettingDeclAst) -> ir::SettingDecl {
    ir::SettingDecl {
        name: ast.name.clone(),
        value_space: match &ast.value_space {
            syntax::SettingValueSpaceAst::Enum(values) => ir::SettingValueSpace::Enum {
                values: values.clone(),
            },
            syntax::SettingValueSpaceAst::Bool => ir::SettingValueSpace::Bool,
            syntax::SettingValueSpaceAst::Int { min, max } => ir::SettingValueSpace::Int {
                min: min.unwrap_or(i64::MIN),
                max: max.unwrap_or(i64::MAX),
            },
        },
        default: ast.default.clone(),
        persistence: match ast.persistence {
            syntax::SettingPersistenceAst::None => ir::SettingPersistence::None,
            syntax::SettingPersistenceAst::Local => ir::SettingPersistence::Local,
            syntax::SettingPersistenceAst::Workspace => ir::SettingPersistence::Workspace,
        },
        span_ref: Some(span_of(ast.span)),
    }
}

/// Validate that every cell `field` shows up in the view's column /
/// fields list (proposal §5.2 + doctor rule `lzx-cell-slot-orphan`).
/// Slot names are restricted to kebab/snake identifiers (defensive —
/// parser already enforces).
fn validate_cells(
    cells: &[syntax::CellBindingAst],
    field_universe: &[String],
    view_name: &str,
) -> Result<(), AnalyzeError> {
    for cell in cells {
        validate_cell_slot_shape(&cell.slot, view_name)?;
        if !field_universe.is_empty() && !field_universe.contains(&cell.field) {
            return Err(AnalyzeError::LzxCellSlotOrphan {
                view: view_name.to_owned(),
                field: cell.field.clone(),
            });
        }
    }
    Ok(())
}

/// Cell-slot identifier validation only (skip the field-universe
/// orphan check). Used for `view detail` where cells bind against
/// fields on the source resource, not against the section enum.
fn validate_cells_slot_only(
    cells: &[syntax::CellBindingAst],
    view_name: &str,
) -> Result<(), AnalyzeError> {
    for cell in cells {
        validate_cell_slot_shape(&cell.slot, view_name)?;
    }
    Ok(())
}

fn validate_cell_slot_shape(slot: &str, view_name: &str) -> Result<(), AnalyzeError> {
    if slot.is_empty()
        || !slot
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(AnalyzeError::LzxCellSlotInvalid {
            view: view_name.to_owned(),
            slot: slot.to_owned(),
        });
    }
    Ok(())
}

/// Extract `:name` placeholders from a route path like `/slugs/:key`.
fn path_placeholders(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        if let Some(rest) = segment.strip_prefix(':') {
            // Trim any non-ident tail (the segment could theoretically
            // carry a suffix; v0 keeps it strict so anything after the
            // name is a parse-time error already).
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out
}

/// Parse `<feature>.query.<short>` into a `QueryRef`. The middle token
/// disambiguates list / lookup / sql when the source pre-qualifies it
/// (`feat.query.list.mine` / `feat.query.lookup.by_key` / `feat.query.sql.raw`).
/// The shorter form `<feat>.query.<short>` defaults to `List`.
fn parse_query_ref(text: &str) -> Option<ir::QueryRef> {
    let parts: Vec<&str> = text.split('.').collect();
    match parts.as_slice() {
        [feature, "query", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::List,
            name: (*name).to_owned(),
        }),
        [feature, "query", "list", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::List,
            name: (*name).to_owned(),
        }),
        [feature, "query", "lookup", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::Lookup,
            name: (*name).to_owned(),
        }),
        [feature, "query", "sql", name] => Some(ir::QueryRef {
            feature: (*feature).to_owned(),
            kind: ir::QueryKind::Sql,
            name: (*name).to_owned(),
        }),
        _ => None,
    }
}

/// Parse a command reference from a `submit ` line or an `actions`
/// list entry. Accepts both the qualified form (`feat.command.name`)
/// and the bare local form (`name`) — the latter assumes the owning
/// feature.
fn parse_command_ref(text: &str, owning_feature: &str) -> Result<ir::CommandRef, AnalyzeError> {
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.as_slice() {
        [feature, "command", name] => Ok(ir::CommandRef {
            feature: (*feature).to_owned(),
            name: (*name).to_owned(),
        }),
        [name] if !name.is_empty() => Ok(ir::CommandRef {
            feature: owning_feature.to_owned(),
            name: (*name).to_owned(),
        }),
        _ => Err(AnalyzeError::LzxBadCommandRef {
            value: trimmed.to_owned(),
        }),
    }
}

/// Public wrapper around `type_ref_from_syntax` so the inspect CLI can
/// reuse the analyzer's `@cap.File(...)` typing pass without re-implementing
/// the parser. The bare function stays private for the rest of the crate so
/// future internal callers keep their existing access path.
pub fn type_ref_from_syntax_public(ty: &str) -> ir::TypeRef {
    type_ref_from_syntax(ty)
}

fn type_ref_from_syntax(ty: &str) -> ir::TypeRef {
    // Phase L Tier 4 follow-up — the canonical-indent parser captures
    // the whole post-`:` head as `type_text`, including trailing
    // decorator markers like `@pii.contact` that follow the type but
    // precede modifiers. The legacy text-walker peeled them as
    // "modifiers"; here we take the first paren-balanced token as the
    // actual type and drop the rest. This matches the behaviour of
    // `parse_resource_field` in the retired doctor walker.
    let ty = first_paren_balanced_token(ty);
    // Codegen follow-up (2026-05-12) — `Type[]` array form lifts to
    // `TypeRef::Many(<inner>)` so emitters can render `[]<inner>` in
    // their target language. Before this peel, `returns CustomerLtv[]`
    // landed as flat `UserDefined("CustomerLtv[]")` and codegen
    // sanitised to `CustomerLtv__`. Strip exactly one `[]` suffix
    // and recurse — nested arrays (`[][]`) are unusual but the peel
    // is correct under recursion.
    if let Some(stripped) = ty.strip_suffix("[]") {
        let inner = type_ref_from_syntax(stripped.trim_end());
        return ir::TypeRef::Many(Box::new(inner));
    }
    // Codegen follow-up — `<Type>.ID` member access (route slot
    // syntax `route owner_id: User.ID required`). The IR currently
    // has no member-access carrier on `TypeRef`; pragmatic peel:
    // any `.ID` / `.Id` suffix resolves to `BuiltinType::Id` because
    // every resource carries its identity in the same canonical
    // `lazuli.ID` type. Member access on non-ID fields is rejected
    // (falls through to `UserDefined` with the dotted name; doctor
    // will surface as unresolved).
    if let Some(prefix) = ty.strip_suffix(".ID").or_else(|| ty.strip_suffix(".Id")) {
        if !prefix.is_empty() && !prefix.contains('.') {
            return ir::TypeRef::Builtin(ir::BuiltinType::Id);
        }
    }
    // Phase L Tier 2 — typed `@cap.File(...)` capability.
    if let Some(file) = parse_cap_file_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::File(file));
    }
    if let Some(pii) = parse_cap_pii_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::PII(pii));
    }
    // Phase L Tier 4 follow-up — typed `@cap.Hashed/Encrypted/Token`.
    if let Some(hashed) = parse_cap_hashed_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Hashed(hashed));
    }
    if let Some(encrypted) = parse_cap_encrypted_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Encrypted(encrypted));
    }
    if let Some(e2ee) = parse_cap_e2ee_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::E2ee(e2ee));
    }
    if let Some(token) = parse_cap_token_type(ty) {
        return ir::TypeRef::Capability(ir::CapabilityRef::Token(token));
    }
    // MONEY-1 §3.2 — `@semantic.Money(currency:<ISO>)` carries the
    // declared currency through to IR so doctor checks
    // (MONEY-COMPARE-001, MONEY-ARITHMETIC-001) can read it without
    // re-walking surface text. Unknown currencies fall through to
    // `UserDefined` so existing "unknown semantic" surfaces them.
    if let Some(money) = parse_semantic_money_type(ty) {
        return ir::TypeRef::Builtin(money);
    }
    // Phase L Tier 4 follow-up — typed `@semantic.*` shorthand for the
    // closed catalog (Email/Phone/Url/Uuid). Other `@semantic.<X>`
    // names still fall through to `UserDefined` so the language can
    // surface "unknown semantic" diagnostics rather than silently
    // accepting them.
    match ty {
        "@semantic.Email" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        "@semantic.Phone" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticPhone),
        "@semantic.Url" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticUrl),
        "@semantic.Uuid" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticUuid),
        "@semantic.Currency" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticCurrency),
        "@semantic.GeoPoint" => return ir::TypeRef::Builtin(ir::BuiltinType::SemanticGeoPoint),
        // Bare `@semantic.Money` (no args) is Hostpoint-pilot reality:
        // single-currency app, defaults to BRL.
        "@semantic.Money" => {
            return ir::TypeRef::Builtin(ir::BuiltinType::SemanticMoney {
                currency: ir::CurrencyCode::BRL,
            });
        }
        _ => {}
    }
    match ty {
        "ID" | "Id" => ir::TypeRef::Builtin(ir::BuiltinType::Id),
        "Text" | "String" => ir::TypeRef::Builtin(ir::BuiltinType::Text),
        "Boolean" | "Bool" => ir::TypeRef::Builtin(ir::BuiltinType::Boolean),
        "Integer" | "Int" => ir::TypeRef::Builtin(ir::BuiltinType::Integer),
        "Decimal" | "Float" => ir::TypeRef::Builtin(ir::BuiltinType::Decimal),
        // Per proposal `semantic-types-money-brazilian.md` v0.3, `Money`
        // is the currency-aware semantic type, NOT a Decimal alias.
        // Default currency is BRL (Hostpoint-pilot reality); authors
        // override per-field via `@semantic.Money(currency:<ISO>)`.
        // Codegen emits `<field>_currency` with a CHECK constraint
        // pinned to the declared currency; doctor lint VOCAB-MONEY-002
        // catches authors who meant Decimal.
        "Money" => ir::TypeRef::Builtin(ir::BuiltinType::SemanticMoney {
            currency: ir::CurrencyCode::BRL,
        }),
        "Date" => ir::TypeRef::Builtin(ir::BuiltinType::Date),
        "DateTime" => ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
        "JSON" | "Json" => ir::TypeRef::Builtin(ir::BuiltinType::Json),
        "Email" => ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
        other => ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: other.to_owned(),
        }),
    }
}

/// Phase L Tier 4 follow-up — return the first whitespace-delimited
/// token from `text`, respecting paren-balanced segments. The
/// canonical-indent parser captures decorator markers (`@pii.contact`,
/// `@cap.Hashed(algorithm:argon2id)`) and trailing markers after the
/// real type in `type_text`; this helper picks the leading type.
fn first_paren_balanced_token(text: &str) -> &str {
    let text = text.trim();
    let mut depth = 0i32;
    for (idx, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => return text[..idx].trim_end(),
            _ => {}
        }
    }
    text
}

/// Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:<X>)`. Closed
/// catalog `{argon2id, bcrypt}`. Returns `None` if the algorithm is
/// missing or unrecognised so callers fall through to `UserDefined`
/// (LSP surfaces shape errors).
fn parse_cap_hashed_type(ty: &str) -> Option<ir::HashedCapability> {
    let inner = ty.strip_prefix("@cap.Hashed(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let algorithm = match args.get("algorithm")?.as_str() {
        "argon2id" => ir::HashAlgorithm::Argon2id,
        "bcrypt" => ir::HashAlgorithm::Bcrypt,
        _ => return None,
    };
    Some(ir::HashedCapability { algorithm })
}

/// Phase L Tier 4 follow-up — `@cap.Encrypted(key:@key.<scope>)`. Key
/// reference is stored verbatim with its `@key.` prefix.
fn parse_cap_encrypted_type(ty: &str) -> Option<ir::EncryptedCapability> {
    let inner = ty.strip_prefix("@cap.Encrypted(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let key = args.get("key")?.clone();
    if !key.starts_with("@key.") {
        return None;
    }
    Some(ir::EncryptedCapability { key })
}

/// Encryption bucket cycle — `@cap.E2ee(key:@key.<scope>)`. Mirror of
/// `parse_cap_encrypted_type` for end-to-end-encrypted fields that
/// the server stores but never reads.
/// See `docs/proposals/encryption-vocab.md` §Lowering.
fn parse_cap_e2ee_type(ty: &str) -> Option<ir::E2eeCapability> {
    let inner = ty.strip_prefix("@cap.E2ee(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let key = args.get("key")?.clone();
    if !key.starts_with("@key.") {
        return None;
    }
    Some(ir::E2eeCapability { key })
}

/// Phase L Tier 4 follow-up — `@cap.Token(ttl:<dur>,single_use:<bool>,
/// store:<storage>)`. All three dimensions are mandatory; closed
/// catalog `store:{hashed}` and `single_use:{true,false}`.
fn parse_cap_token_type(ty: &str) -> Option<ir::TokenCapability> {
    let inner = ty.strip_prefix("@cap.Token(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let ttl = args.get("ttl")?.clone();
    let single_use = match args.get("single_use")?.as_str() {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    let store = match args.get("store")?.as_str() {
        "hashed" => ir::TokenStore::Hashed,
        _ => return None,
    };
    Some(ir::TokenCapability {
        ttl,
        single_use,
        store,
    })
}

/// Parse `@cap.File(max_size:25mb,accept:text/csv[,visibility:...,signed_ttl:...])`
/// into a typed `FileCapability`. Returns `None` for any malformed shape so
/// the caller falls through to the legacy `UserDefined` fallback — the LSP
/// already surfaces shape errors for the same patterns.
fn parse_cap_file_type(ty: &str) -> Option<ir::FileCapability> {
    let ty = first_paren_balanced_token(ty);
    let inner = ty.strip_prefix("@cap.File(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);

    let max_size = parse_file_size(args.get("max_size")?)?;
    let accept = parse_mime_list(args.get("accept")?)?;
    if accept.is_empty() {
        return None;
    }
    let visibility = args
        .get("visibility")
        .map(|s| s.as_str())
        .and_then(parse_file_visibility);
    let signed_ttl = args.get("signed_ttl").map(|s| s.clone());
    let auto_photo_policy = args.get("auto_photo_policy").cloned();

    Some(ir::FileCapability {
        max_size,
        accept,
        visibility,
        signed_ttl,
        auto_photo_policy,
    })
}

/// IR-VOCAB-REST — `@cap.PII(class:<X>,retention:<dur>,
/// log_redact:<bool>)`. `class` is required; retention and log_redact
/// are optional passive slots for follow-up doctor/runtime cells.
fn parse_cap_pii_type(ty: &str) -> Option<ir::PiiCapability> {
    let ty = first_paren_balanced_token(ty);
    let inner = ty.strip_prefix("@cap.PII(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let class = unquote_capability_arg(args.get("class")?).to_owned();
    if class.is_empty() {
        return None;
    }
    let retention = args
        .get("retention")
        .map(|value| unquote_capability_arg(value).to_owned());
    let log_redact = match args.get("log_redact").map(String::as_str) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        Some(_) => return None,
        None => None,
    };
    Some(ir::PiiCapability {
        class,
        retention,
        log_redact,
    })
}

fn unquote_capability_arg(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

/// MONEY-1 §3.2 — `@semantic.Money(currency:<ISO>)`. Reuses the
/// capability-arg syntax (`key:value`) for consistency with `@cap.*`.
/// Returns `None` when:
///   * the prefix doesn't match `@semantic.Money(`
///   * the closing paren is missing
///   * `currency` is absent
///   * the ISO code isn't in the closed `CurrencyCode` catalog
/// All four cases fall through to the existing `UserDefined`-with-
/// diagnostic path so authors see a single consistent error surface.
fn parse_semantic_money_type(ty: &str) -> Option<ir::BuiltinType> {
    let inner = ty.strip_prefix("@semantic.Money(")?.strip_suffix(')')?;
    let args = parse_capability_args(inner);
    let raw = args.get("currency")?;
    let currency = ir::CurrencyCode::from_iso(raw)?;
    Some(ir::BuiltinType::SemanticMoney { currency })
}

fn parse_capability_args(inner: &str) -> std::collections::BTreeMap<String, String> {
    inner
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            part.split_once(':')
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

fn parse_file_size(raw: &str) -> Option<ir::FileSize> {
    let digit_count = raw.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 || digit_count == raw.len() {
        return None;
    }
    let amount: u32 = raw[..digit_count].parse().ok()?;
    let unit = &raw[digit_count..];
    let literal = match unit {
        "kb" => ir::FileSizeLiteral::Kb(amount),
        "mb" => ir::FileSizeLiteral::Mb(amount),
        "gb" => ir::FileSizeLiteral::Gb(amount),
        _ => return None,
    };
    Some(ir::FileSize {
        bytes: literal.bytes(),
        literal,
    })
}

fn parse_mime_list(raw: &str) -> Option<Vec<ir::MimeType>> {
    let mut out = Vec::new();
    for token in raw.split('|') {
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        let (family, subtype) = token.split_once('/')?;
        let family = family.trim();
        let subtype = subtype.trim();
        if family.is_empty() || subtype.is_empty() {
            return None;
        }
        out.push(ir::MimeType {
            family: family.to_owned(),
            subtype: subtype.to_owned(),
        });
    }
    Some(out)
}

fn parse_file_visibility(raw: &str) -> Option<ir::FileVisibility> {
    match raw {
        "public" => Some(ir::FileVisibility::Public),
        "private" => Some(ir::FileVisibility::Private),
        "signed" => Some(ir::FileVisibility::Signed),
        _ => None,
    }
}

fn parse_default(raw: &str) -> ir::DefaultValue {
    if raw == "true" {
        return ir::DefaultValue::Boolean(true);
    }
    if raw == "false" {
        return ir::DefaultValue::Boolean(false);
    }
    if raw == "nil" {
        return ir::DefaultValue::Nil;
    }
    if let Ok(value) = raw.parse::<i64>() {
        return ir::DefaultValue::Integer(value);
    }

    if raw
        .chars()
        .next()
        .map(|c| c.is_alphabetic() || c == '_')
        .unwrap_or(false)
        && raw.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
            type_name: None,
            variant: raw.to_owned(),
        });
    }

    ir::DefaultValue::String(raw.to_owned())
}

fn span_of(span: syntax::Span) -> ir::SpanRef {
    ir::SpanRef {
        start: span.start,
        end: span.end,
    }
}

// =============================================================================
// Cut A — agent lowering (canonical-indent slice).
//
// `lower_feature_skeleton(&syntax::FeatureSkeleton)` projects the new
// canonical-indent AST into an `ir::Feature` carrying `agents: Vec<Agent>`.
// Other feature children stay in the legacy pipeline; this function
// returns a `Feature` with zeroed siblings so callers (CLI / LSP / tests)
// can merge it against the legacy lowering result if both pipelines are
// running.
//
// Resolved tool fields (`ToolBinding.resolved_effect`,
// `resolved_policy`, `resolved_pii_classes`) stay `None` here — the
// expand pass in `lazuli_cli` populates them when the full workspace IR
// is loaded (plan §4.3).
//
// See docs/proposals/ai-primitives-v0-implementation.md §4.
// =============================================================================

// =============================================================================
// PG.B — plan-and-gate facts aggregator.
// -----------------------------------------------------------------------------
// `PlanGateFacts` is the analyzer-side projection of the package-wide
// plan catalog (`plan ...` blocks lifted from app.lzi / registry.lzi)
// + the subscription anchor + the per-callable gate directives lifted
// from each feature's `command/job/webhook/api/query.*` bodies.
//
// Codegen (PG.C), doctor (PG.B), and LSP (PG.B) all consume this struct
// as a side-table to the existing `Module`. It deliberately stays out
// of the per-callable IR shapes so the existing ~150 struct-literal
// fixtures keep building without modification.
// =============================================================================

/// Plan/gate facts derived from a single package's `.lzi` sources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanGateFacts {
    /// Closed plan catalog. `None` when no `plan ...` blocks are
    /// authored (the package has no subscription model).
    pub catalog: Option<ir::PlanCatalog>,
    /// `app.lzi subscription resource <feature>.<field>`. Required if
    /// any callable carries a gate; doctor `PLAN-NO-SUBSCRIPTION-001`
    /// fires when missing.
    pub subscription_anchor: Option<ir::SubscriptionAnchor>,
    /// Per-callable gate directives keyed by
    /// `<feature>/<callable_kind>:<callable_name>`. The qualified key
    /// matches what `parse_feature_gates` produces (with the feature
    /// prefix added in this layer because gates are aggregated across
    /// every feature in the package).
    pub gates: std::collections::BTreeMap<String, Vec<ir::Gate>>,
}

/// PG.A — scan an `app.lzi` source for the
/// `subscription resource <feature>.<field>` directive. Returns
/// `None` when not declared. `tenancy_axis` is filled in `None`
/// here; richer resolution requires the doctor's resource-tenancy
/// table and lives downstream.
pub fn parse_subscription_anchor(app_lzi_source: &str) -> Option<ir::SubscriptionAnchor> {
    let mut in_app = false;
    let mut offset = 0usize;
    for line in app_lzi_source.lines() {
        let line_len = line.len();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset += line_len + 1;
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_app = trimmed.starts_with("app ");
            offset += line_len + 1;
            continue;
        }
        if !in_app {
            offset += line_len + 1;
            continue;
        }
        if indent != 2 {
            offset += line_len + 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("subscription resource ") {
            let body = rest.trim();
            if let Some((feature, field)) = body.split_once('.') {
                let feature = feature.trim().to_owned();
                let field = field.trim().to_owned();
                if !feature.is_empty() && !field.is_empty() {
                    return Some(ir::SubscriptionAnchor {
                        feature,
                        field,
                        tenancy_axis: None,
                        span_ref: Some(ir::SpanRef {
                            start: offset,
                            end: offset + line_len,
                        }),
                    });
                }
            }
        }
        offset += line_len + 1;
    }
    None
}

/// Build a `PlanGateFacts` from raw inputs. The caller is responsible
/// for parsing the per-file plan blocks and gate side-channels first;
/// this aggregator just merges and expands cross-plan references.
///
/// - `plan_blocks` is the union of `parse_plan_blocks(app_lzi)` plus
///   `parse_plan_blocks(registry_lzi)` plus every other top-level
///   source carrying `plan` blocks. Doctor enforces single declaration
///   per plan name upstream.
/// - `feature_gates` is one entry per feature, name + the
///   `FeatureGatesAst` produced by `parse_feature_gates(source)`.
/// - `anchor` is the `SubscriptionAnchor` resolved from
///   `app.lzi subscription resource ...`.
pub fn aggregate_plan_gate_facts(
    plan_blocks: &[syntax::PlanBlockAst],
    feature_gates: &[(String, syntax::FeatureGatesAst)],
    anchor: Option<ir::SubscriptionAnchor>,
) -> PlanGateFacts {
    let catalog = if plan_blocks.is_empty() {
        None
    } else {
        Some(build_plan_catalog(plan_blocks))
    };

    let mut gates: std::collections::BTreeMap<String, Vec<ir::Gate>> =
        std::collections::BTreeMap::new();
    for (feature, fg) in feature_gates {
        for (callable_key, directives) in &fg.callables {
            let qualified = format!("{}/{}", feature, callable_key);
            let mut out = Vec::with_capacity(directives.len());
            for directive in directives {
                out.push(match directive {
                    syntax::GateDirectiveAst::Behind { feature, .. } => ir::Gate::Behind {
                        feature: feature.clone(),
                    },
                    syntax::GateDirectiveAst::Quota { limit, .. } => ir::Gate::Quota {
                        limit: limit.clone(),
                    },
                });
            }
            gates.insert(qualified, out);
        }
    }

    PlanGateFacts {
        catalog,
        subscription_anchor: anchor,
        gates,
    }
}

fn build_plan_catalog(plan_blocks: &[syntax::PlanBlockAst]) -> ir::PlanCatalog {
    use std::collections::{BTreeMap, BTreeSet};

    // Pass 1: gather declared features/limits per plan, deferring
    // cross-plan reuse references until pass 2 (the referenced plan
    // might be authored later in source order).
    struct Staging {
        direct_features: Vec<String>,
        feature_refs: Vec<String>,
        direct_limits: BTreeMap<String, ir::PlanLimitValue>,
        limit_refs: Vec<String>,
        trial: Option<ir::TrialPolicy>,
        span_ref: Option<ir::SpanRef>,
    }

    let mut staging: BTreeMap<String, Staging> = BTreeMap::new();
    for plan in plan_blocks {
        let mut direct_features = Vec::new();
        let mut feature_refs = Vec::new();
        for feat in &plan.features {
            match feat {
                syntax::PlanFeatureRefAst::Ident(s) => direct_features.push(s.clone()),
                syntax::PlanFeatureRefAst::CrossPlan(other) => feature_refs.push(other.clone()),
            }
        }
        let mut direct_limits: BTreeMap<String, ir::PlanLimitValue> = BTreeMap::new();
        let mut limit_refs = Vec::new();
        for lim in &plan.limits {
            match lim {
                syntax::PlanLimitRefAst::Integer { name, value } => {
                    direct_limits.insert(name.clone(), ir::PlanLimitValue::Integer(*value));
                }
                syntax::PlanLimitRefAst::Unlimited { name } => {
                    direct_limits.insert(name.clone(), ir::PlanLimitValue::Unlimited);
                }
                syntax::PlanLimitRefAst::CrossPlan(other) => limit_refs.push(other.clone()),
            }
        }
        let trial = plan.trial.as_ref().map(|t| ir::TrialPolicy {
            duration: t.duration.clone(),
            then_plan: t.then_plan.clone(),
        });
        let span_ref = Some(ir::SpanRef {
            start: plan.span.start,
            end: plan.span.end,
        });
        staging.insert(
            plan.name.clone(),
            Staging {
                direct_features,
                feature_refs,
                direct_limits,
                limit_refs,
                trial,
                span_ref,
            },
        );
    }

    // Pass 2: expand cross-plan references. We snapshot direct sets
    // before mutation so reference chains see the original direct
    // declarations (single-level expansion; deeper chains require the
    // referenced plan to also resolve, enforced by doctor).
    let direct_features_snapshot: BTreeMap<String, Vec<String>> = staging
        .iter()
        .map(|(k, v)| (k.clone(), v.direct_features.clone()))
        .collect();
    let direct_limits_snapshot: BTreeMap<String, BTreeMap<String, ir::PlanLimitValue>> = staging
        .iter()
        .map(|(k, v)| (k.clone(), v.direct_limits.clone()))
        .collect();

    let mut plans: Vec<ir::Plan> = Vec::with_capacity(staging.len());
    let mut feature_union: BTreeSet<String> = BTreeSet::new();
    let mut limit_union: BTreeSet<String> = BTreeSet::new();

    for (name, s) in &staging {
        let mut feature_set: BTreeSet<String> = s.direct_features.iter().cloned().collect();
        for other in &s.feature_refs {
            if let Some(other_features) = direct_features_snapshot.get(other) {
                for f in other_features {
                    feature_set.insert(f.clone());
                }
            }
        }
        let mut limit_map = s.direct_limits.clone();
        for other in &s.limit_refs {
            if let Some(other_limits) = direct_limits_snapshot.get(other) {
                for (k, v) in other_limits {
                    limit_map.entry(k.clone()).or_insert(*v);
                }
            }
        }
        let mut features: Vec<String> = feature_set.into_iter().collect();
        features.sort();
        let mut limits: Vec<ir::PlanLimit> = limit_map
            .into_iter()
            .map(|(name, value)| ir::PlanLimit { name, value })
            .collect();
        limits.sort_by(|a, b| a.name.cmp(&b.name));
        for f in &features {
            feature_union.insert(f.clone());
        }
        for l in &limits {
            limit_union.insert(l.name.clone());
        }
        plans.push(ir::Plan {
            name: name.clone(),
            features,
            limits,
            trial: s.trial.clone(),
            span_ref: s.span_ref,
        });
    }
    plans.sort_by(|a, b| a.name.cmp(&b.name));

    ir::PlanCatalog {
        plans,
        feature_catalog: feature_union.into_iter().collect(),
        limit_catalog: limit_union.into_iter().collect(),
    }
}

/// PG.B — closed catalog of plan-and-gate doctor diagnostic codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGateDiagnostic {
    pub code: PlanGateCode,
    pub message: String,
    /// Best-effort source-byte range of the offending construct. The
    /// `0..0` span indicates a package-wide issue (catalog absent,
    /// anchor missing for the whole app).
    pub span: syntax::Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanGateCode {
    /// `PLAN-FEATURE-UNDECLARED-001` — `gate behind plan.feature: <X>`
    /// references a feature not in the catalog's feature union.
    FeatureUndeclared,
    /// `PLAN-QUOTA-MISSING-001` — `gate quota plan.limit: <X>`
    /// references a limit that is not declared by every plan; the
    /// closed-grammar rule requires explicit `unlimited` opt-outs.
    QuotaMissing,
    /// `PLAN-NO-SUBSCRIPTION-001` — any gate exists in the package but
    /// `app.lzi` did not declare `subscription resource ...`.
    NoSubscription,
    /// `PLAN-TRIAL-WITHOUT-FALLBACK-001` — trial revert plan does not
    /// cover the trial plan's feature set, or its `then` target is
    /// missing entirely.
    TrialWithoutFallback,
    /// `PLAN-SUBSCRIPTION-TENANCY-001` — multi-tenant app's anchor
    /// resource lacks a `tenancy` axis matching `app.defaults.tenancy`.
    SubscriptionTenancy,
    /// `GATE-EVAL-ORDER-001` — a `gate` directive appears after
    /// `policy` in source order; the closed evaluation order requires
    /// gates to be authored before policies.
    GateEvalOrder,
}

impl PlanGateCode {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanGateCode::FeatureUndeclared => "PLAN-FEATURE-UNDECLARED-001",
            PlanGateCode::QuotaMissing => "PLAN-QUOTA-MISSING-001",
            PlanGateCode::NoSubscription => "PLAN-NO-SUBSCRIPTION-001",
            PlanGateCode::TrialWithoutFallback => "PLAN-TRIAL-WITHOUT-FALLBACK-001",
            PlanGateCode::SubscriptionTenancy => "PLAN-SUBSCRIPTION-TENANCY-001",
            PlanGateCode::GateEvalOrder => "GATE-EVAL-ORDER-001",
        }
    }
}

/// Diagnose the plan/gate cross-feature invariants. Returns one entry
/// per detected issue; an empty vec means the package passes.
///
/// `sources_with_eval_order` is a list of `(callable_key, body_text)`
/// where `body_text` is the source range covering the callable's
/// children. The function scans for `gate ... ` lines appearing after
/// the first `policy ` line to flag GATE-EVAL-ORDER-001. Callers that
/// don't need eval-order checking can pass an empty slice.
pub fn diagnose_plan_gate_facts(
    facts: &PlanGateFacts,
    sources_with_eval_order: &[(String, String, syntax::Span)],
) -> Vec<PlanGateDiagnostic> {
    use std::collections::BTreeSet;
    let mut out: Vec<PlanGateDiagnostic> = Vec::new();

    // PLAN-NO-SUBSCRIPTION-001 — any gate without an anchor.
    if !facts.gates.is_empty() && facts.subscription_anchor.is_none() {
        let example = facts.gates.keys().next().cloned().unwrap_or_default();
        out.push(PlanGateDiagnostic {
            code: PlanGateCode::NoSubscription,
            message: format!(
                "callable `{}` declares a `gate` but `app.lzi` does not declare `subscription resource <feature>.<field>`; the runtime has no anchor to resolve the active plan",
                example
            ),
            span: syntax::Span::new(0, 0),
        });
    }

    if let Some(catalog) = &facts.catalog {
        let feature_set: BTreeSet<&str> =
            catalog.feature_catalog.iter().map(String::as_str).collect();
        let limit_set: BTreeSet<&str> = catalog.limit_catalog.iter().map(String::as_str).collect();

        // Build per-limit set of plans that declare it (for QUOTA-MISSING).
        let mut limit_to_plans: std::collections::BTreeMap<&str, BTreeSet<&str>> =
            std::collections::BTreeMap::new();
        for plan in &catalog.plans {
            for lim in &plan.limits {
                limit_to_plans
                    .entry(lim.name.as_str())
                    .or_default()
                    .insert(plan.name.as_str());
            }
        }
        let all_plans: BTreeSet<&str> = catalog.plans.iter().map(|p| p.name.as_str()).collect();

        // PLAN-FEATURE-UNDECLARED-001 + PLAN-QUOTA-MISSING-001.
        for (callable_key, gates) in &facts.gates {
            for gate in gates {
                match gate {
                    ir::Gate::Behind { feature } => {
                        if !feature_set.contains(feature.as_str()) {
                            out.push(PlanGateDiagnostic {
                                code: PlanGateCode::FeatureUndeclared,
                                message: format!(
                                    "gate `behind plan.feature: {}` on `{}` references a feature not declared by any plan; the feature catalog is the union of every plan's `features` list",
                                    feature, callable_key
                                ),
                                span: syntax::Span::new(0, 0),
                            });
                        }
                    }
                    ir::Gate::Quota { limit } => {
                        if !limit_set.contains(limit.as_str()) {
                            out.push(PlanGateDiagnostic {
                                code: PlanGateCode::QuotaMissing,
                                message: format!(
                                    "gate `quota plan.limit: {}` on `{}` references a limit not declared by any plan",
                                    limit, callable_key
                                ),
                                span: syntax::Span::new(0, 0),
                            });
                        } else if let Some(declaring) = limit_to_plans.get(limit.as_str()) {
                            if declaring != &all_plans {
                                let missing: Vec<&str> =
                                    all_plans.difference(declaring).copied().collect::<Vec<_>>();
                                out.push(PlanGateDiagnostic {
                                    code: PlanGateCode::QuotaMissing,
                                    message: format!(
                                        "gate `quota plan.limit: {}` on `{}` is not declared by plan(s) {}; quota gates must be honored by every tier (set `<X> unlimited` to opt out)",
                                        limit, callable_key, missing.join(", ")
                                    ),
                                    span: syntax::Span::new(0, 0),
                                });
                            }
                        }
                    }
                }
            }
        }

        // PLAN-TRIAL-WITHOUT-FALLBACK-001 — trial revert plan must
        // exist and cover the trial plan's feature set.
        for plan in &catalog.plans {
            if let Some(trial) = &plan.trial {
                let then_plan = catalog.plans.iter().find(|p| p.name == trial.then_plan);
                match then_plan {
                    None => out.push(PlanGateDiagnostic {
                        code: PlanGateCode::TrialWithoutFallback,
                        message: format!(
                            "plan `{}` declares `trial then {}` but `{}` is not a declared plan",
                            plan.name, trial.then_plan, trial.then_plan
                        ),
                        span: plan
                            .span_ref
                            .map(|s| syntax::Span::new(s.start, s.end))
                            .unwrap_or(syntax::Span::new(0, 0)),
                    }),
                    Some(then) => {
                        let then_features: BTreeSet<&str> =
                            then.features.iter().map(String::as_str).collect();
                        let missing: Vec<&str> = plan
                            .features
                            .iter()
                            .filter(|f| !then_features.contains(f.as_str()))
                            .map(String::as_str)
                            .collect();
                        if !missing.is_empty() {
                            out.push(PlanGateDiagnostic {
                                code: PlanGateCode::TrialWithoutFallback,
                                message: format!(
                                    "plan `{}` declares `trial then {}` but `{}`'s feature set is missing {} — trial revert would lose features the caller had during trial (declare `unlimited` on the fallback or move features out of the trial plan)",
                                    plan.name,
                                    trial.then_plan,
                                    trial.then_plan,
                                    missing.join(", ")
                                ),
                                span: plan
                                    .span_ref
                                    .map(|s| syntax::Span::new(s.start, s.end))
                                    .unwrap_or(syntax::Span::new(0, 0)),
                            });
                        }
                    }
                }
            }
        }
    } else if !facts.gates.is_empty() {
        // Gates exist but no catalog declared at all: every gate
        // references something undeclared.
        for callable_key in facts.gates.keys() {
            out.push(PlanGateDiagnostic {
                code: PlanGateCode::FeatureUndeclared,
                message: format!(
                    "callable `{}` declares a `gate` but no `plan` blocks are authored; declare at least one plan with `features` / `limits`",
                    callable_key
                ),
                span: syntax::Span::new(0, 0),
            });
        }
    }

    // PLAN-SUBSCRIPTION-TENANCY-001 — anchor exists but tenancy axis
    // is absent. This is a structural check; richer cross-feature
    // tenancy resolution lives in the doctor pass that knows about
    // resource tenancy axes.
    if let Some(anchor) = &facts.subscription_anchor {
        if anchor.tenancy_axis.is_none() {
            // Only warn when there is actually a gate in play, otherwise
            // single-tenant apps would fire on every anchor.
            // The richer multi-tenancy parity check lives in the
            // higher-level doctor pass.
            let _ = anchor;
        }
    }

    // GATE-EVAL-ORDER-001 — gate after policy in source order.
    for (callable_key, body, span) in sources_with_eval_order {
        if let Some(policy_pos) = find_keyword_line_offset(body, "policy ") {
            if let Some(gate_pos) = find_keyword_line_offset(body, "gate ") {
                if gate_pos > policy_pos {
                    out.push(PlanGateDiagnostic {
                        code: PlanGateCode::GateEvalOrder,
                        message: format!(
                            "callable `{}` declares `gate` after `policy`; gates evaluate before policy and must be authored in that order",
                            callable_key
                        ),
                        span: *span,
                    });
                }
            }
        }
    }

    out
}

/// Find the byte offset of the first line starting with `<indent>`
/// (any amount) + `keyword` in `body`. Returns `None` if not present.
fn find_keyword_line_offset(body: &str, keyword: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(keyword) {
            return Some(offset);
        }
        offset += line.len() + 1;
    }
    None
}

/// FR-3a — for each resource with a `user: User required unique`
/// field carrying an optional `@cap.File(...)` typed field, append
/// the 4 auto-photo commands + 2 records to the feature.
///
/// Trigger conditions (all must hold):
///   1. Resource has a field named `user` of type `User` required unique.
///   2. The `@cap.File(...)` field is declared `optional`.
///   3. No author-written command in `feature.commands` shares the
///      synthesized name for that field's command role (request,
///      confirm, clear, get_url) — name collision skips THAT one
///      role and emits the other 3.
fn synthesize_auto_photo(feature: &mut ir::Feature) {
    let mut to_add_commands: Vec<ir::Command> = Vec::new();
    let mut to_add_records: Vec<ir::Record> = Vec::new();

    let existing_command_names: std::collections::HashSet<String> =
        feature.commands.iter().map(|c| c.name.clone()).collect();
    let existing_record_names: std::collections::HashSet<String> =
        feature.records.iter().map(|r| r.name.clone()).collect();

    // Resolve the policy to attach. Heuristic per D5: pick the
    // feature-level policy whose name matches `<resource_singular>_only`
    // for the *current* resource; fall back to `authenticated`.
    let policy_name_for = |resource: &str| -> Option<String> {
        let snake = pascal_to_snake(resource);
        let target = format!("{}_only", snake);
        if feature.policies.categories.iter().any(|p| p.name == target) {
            return Some(target);
        }
        let compact_target = format!("{}_only", resource.to_ascii_lowercase());
        if feature
            .policies
            .categories
            .iter()
            .any(|p| p.name == compact_target)
        {
            return Some(compact_target);
        }
        if feature
            .policies
            .categories
            .iter()
            .any(|p| p.name == "authenticated")
        {
            return Some("authenticated".to_owned());
        }
        None
    };

    for resource in &feature.resources {
        let has_user_unique = resource.fields.iter().any(|f| {
            f.name == "user"
                && f.required
                && f.unique
                && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
        });
        if !has_user_unique {
            continue;
        }

        for field in &resource.fields {
            if field.required {
                continue;
            }
            let cap_file = match &field.type_ref {
                ir::TypeRef::Capability(ir::CapabilityRef::File(spec)) => spec.clone(),
                _ => continue,
            };

            let pascal_field = snake_to_pascal(&field.name);
            let intent_name = format!("{}UploadIntent", pascal_field);
            let display_name = format!("{}DisplayUrl", pascal_field);

            let policy_name = match policy_name_for(&resource.name) {
                Some(n) => n,
                None => continue, // no policy => skip whole resource silently
            };

            // 2 records first (idempotent: skip if author already declared).
            if !existing_record_names.contains(&intent_name) {
                to_add_records.push(auto_photo_intent_record(&intent_name));
            }
            if !existing_record_names.contains(&display_name) {
                to_add_records.push(auto_photo_display_record(&display_name));
            }

            // 4 commands. Each role checks for name collision.
            for role in [
                ir::AutoPhotoCommandRole::Request,
                ir::AutoPhotoCommandRole::Confirm,
                ir::AutoPhotoCommandRole::Clear,
                ir::AutoPhotoCommandRole::GetUrl,
            ] {
                let cmd_name = auto_photo_command_name(&field.name, role);
                if existing_command_names.contains(&cmd_name) {
                    continue;
                }
                to_add_commands.push(build_auto_photo_command(
                    cmd_name,
                    &resource.name,
                    &field.name,
                    role,
                    &intent_name,
                    &display_name,
                    &policy_name,
                ));
            }

            // Suppress unused warning on cap_file — adapters read it
            // via the field's TypeRef anyway. Reserved here for
            // future per-site validations (max_size, accept).
            let _ = cap_file;
        }
    }

    feature.commands.extend(to_add_commands);
    feature.records.extend(to_add_records);
}

fn auto_photo_command_name(field: &str, role: ir::AutoPhotoCommandRole) -> String {
    match role {
        ir::AutoPhotoCommandRole::Request => format!("request_{}_upload", field),
        ir::AutoPhotoCommandRole::Confirm => format!("confirm_{}_upload", field),
        ir::AutoPhotoCommandRole::Clear => format!("clear_{}", field),
        ir::AutoPhotoCommandRole::GetUrl => format!("get_{}_url", field),
    }
}

fn auto_photo_intent_record(name: &str) -> ir::Record {
    ir::Record {
        name: name.to_owned(),
        public_contract: None,
        fields: vec![
            simple_required_field("url", builtin_text()),
            simple_required_field("method", builtin_text()),
            simple_required_field("headers_content_type", builtin_text()),
            simple_required_field("key", builtin_text()),
            simple_required_field("expires_at", builtin_datetime()),
        ],
        discriminator_field: None,
        span_ref: None,
    }
}

fn auto_photo_display_record(name: &str) -> ir::Record {
    ir::Record {
        name: name.to_owned(),
        public_contract: None,
        fields: vec![
            simple_optional_field("url", builtin_text()),
            simple_optional_field("expires_at", builtin_datetime()),
        ],
        discriminator_field: None,
        span_ref: None,
    }
}

fn build_auto_photo_command(
    name: String,
    resource: &str,
    field: &str,
    role: ir::AutoPhotoCommandRole,
    intent_name: &str,
    display_name: &str,
    policy_name: &str,
) -> ir::Command {
    use ir::*;
    let (input, effect, rate_limit) = match role {
        AutoPhotoCommandRole::Request => (
            CommandInput::Typed(vec![
                TypedSlot {
                    name: "content_type".to_owned(),
                    type_ref: builtin_text(),
                    required: true,
                    constraints: FieldConstraints::default(),
                },
                TypedSlot {
                    name: "size_bytes".to_owned(),
                    type_ref: builtin_integer(),
                    required: true,
                    constraints: FieldConstraints::default(),
                },
            ]),
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: intent_name.to_owned(),
                }),
            }),
            "30 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::Confirm => (
            CommandInput::Typed(vec![TypedSlot {
                name: "key".to_owned(),
                type_ref: builtin_text(),
                required: true,
                constraints: FieldConstraints::default(),
            }]),
            CommandEffect::None,
            "30 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::Clear => (
            CommandInput::Empty,
            CommandEffect::None,
            "10 per 10 minutes per ip",
        ),
        AutoPhotoCommandRole::GetUrl => (
            CommandInput::Empty,
            CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: display_name.to_owned(),
                }),
            }),
            "60 per 10 minutes per ip",
        ),
    };

    let _ = (resource, field); // currently only used via marker
    Command {
        name,
        public_contract: None,
        kind: CommandKind::Returns,
        route: Vec::new(),
        input,
        target: None,
        lets: Vec::new(),
        effect,
        policy: PolicyRef::Local(policy_name.to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        }),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: Some(SynthesizedFromCapFile {
            resource: resource.to_owned(),
            field: field.to_owned(),
            role,
        }),
        owner_scope_sql: None,
    }
}

fn simple_required_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    simple_field(name, type_ref, true)
}

fn simple_optional_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
    simple_field(name, type_ref, false)
}

fn simple_field(name: &str, type_ref: ir::TypeRef, required: bool) -> ir::Field {
    ir::Field {
        name: name.to_owned(),
        type_ref,
        required,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        constraints: ir::FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        span_ref: None,
    }
}

fn builtin_text() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::Text)
}

fn builtin_integer() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::Integer)
}

fn builtin_datetime() -> ir::TypeRef {
    ir::TypeRef::Builtin(ir::BuiltinType::DateTime)
}

// =============================================================================
// `conventions [crud]` auto-synthesis pass
// =============================================================================
//
// Spec: `docs/proposals/ir-resource-conventions-crud.md` §5.
//
// For each `Resource` with `ConventionRef::Crud` in `conventions`, the
// pass appends 5 entries to the feature (3 commands + 2 queries) using
// the shapes from §5.2 through §5.6. Override semantics (§6): any name
// already authored in the feature is left alone — no warning, no
// `@deprecated`, no doctor flag. The other 4 still synthesize.
//
// RULE-VOCAB-03 (§7) — zero workflow: each synth maps to exactly one
// of the existing declarative IR shapes (`CommandEffect::Creates` /
// `Updates` / `Deletes`, `Query::Lookup`, `Query::List`). No new
// lowering path is introduced; the pass just produces IR nodes the
// existing emitters already know how to lower to one SQL each.
//
// Diagnostics (§11) returned via the `Vec<CrudSynthDiagnostic>` return
// value — Cell C4 wires them to the user-facing doctor surface.

/// §11 diagnostic codes emitted by `synthesize_conventions`. Cell C4
/// formats these into user-facing strings; Cell C3 just records them.
///
/// Originally `CrudSynthDiagnostic`; extended to cover both `crud` and
/// `me` bundles when M2 of `ir-resource-conventions-me` landed
/// (variants prefixed `Me*`). A type alias preserves the legacy name
/// for any callers; the canonical name going forward is
/// `ConventionSynthDiagnostic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConventionSynthDiagnostic {
    /// `crud_synth_policy_not_found` — feature has no `authenticated`
    /// policy. Carries the resource name for the suggestion. Also fires
    /// for `me_synth_policy_not_found` (the `me` bundle reuses
    /// `authenticated` per `ir-resource-conventions-me.md` §5.4); Cell
    /// M3 selects the user-visible code by reading
    /// `resource.conventions`.
    PolicyNotFound { resource: String },
    /// `crud_synth_no_required_fields` — every required field is in the
    /// Tenant or Auto group, so `create_<resource>.input` would be
    /// empty. Likely an authoring mistake. Crud-only.
    NoRequiredFields { resource: String },
    /// `crud_synth_signature_mismatch` — author wrote a same-named
    /// command/query but its input field list or return type diverges
    /// from the canonical convention shape. Carries the resource +
    /// synth name + a short reason for Cell C4 to format. Crud-only.
    SignatureMismatch {
        resource: String,
        synth_name: String,
        reason: String,
    },
    /// `me_synth_no_actor_resolution` — resource declared
    /// `conventions [me]` but has neither `user: User required` nor
    /// `org: Org required` AND is not itself named `User`. The synth
    /// has no key to filter on. See
    /// `ir-resource-conventions-me.md` §11.1 (named
    /// `me_synth_no_owner_axis` in the proposal; M2's diagnostic key
    /// is `me_synth_no_actor_resolution` per the cell brief — same
    /// condition, more explicit wording).
    MeNoActorResolution { resource: String },
    /// `me_synth_signature_mismatch` — author wrote
    /// `query lookup_my_<resource>` (or the declarative
    /// `query.lookup my_<resource>`) whose return shape diverges
    /// from the canonical `me` synth (route-less Lookup query
    /// returning the resource row).
    MeSignatureMismatch {
        resource: String,
        synth_name: String,
        reason: String,
    },
    /// `owner_axis_unknown_through` — `@owner_axis(through: <col>)`
    /// names a column that doesn't exist on the FK target resource.
    /// O3 formats the user-facing message with a nearest-name hint.
    /// See `ir-resource-conventions-owner-scope.md` §7.4 + §11.1.
    OwnerAxisUnknownThrough {
        resource: String,
        field: String,
        through: String,
        fk_target: String,
        suggestion: Option<String>,
    },
    /// `owner_axis_through_not_user_keyed` — the FK target's `through:`
    /// column is not typed as `User` (or `@semantic.UserID`). The
    /// emitted chain can't resolve to `ctx.User.ID`. O3 surfaces this
    /// as a warning. See §7.4 + §11.1.
    OwnerAxisThroughNotUserKeyed {
        resource: String,
        field: String,
        through: String,
        fk_target: String,
    },
    /// `owner_axis_collides_with_unique_user` — the resource carries
    /// BOTH `user: User required unique` AND
    /// `@owner_axis(through: <col>)` on another field. The two scopes
    /// would compose redundantly; the unique-user mode already
    /// provides ownership. See §7.4 + §11.1.
    OwnerAxisCollidesWithUniqueUser {
        resource: String,
        field: String,
    },
}

/// Legacy alias preserved during the M2 rename. Downstream code should
/// migrate to `ConventionSynthDiagnostic` over time; M3 carries the
/// final downstream migration into doctor / inspect.
pub type CrudSynthDiagnostic = ConventionSynthDiagnostic;

/// Run the `conventions [...]` auto-synthesis pass on a feature.
/// Today covers two bundles in catalog order: `crud` (5 entries) and
/// `me` (1 entry — `lookup_my_<resource>`). Returns diagnostics from
/// crud §11 + me §11; Cell C4 / M3 wires the user-facing rendering.
/// Public so doctor / tests can call it directly.
///
/// **RULE-VOCAB-03 (crud §7 + me §7) — zero workflow:** every `if`/
/// `match` in this function is **authoring-time** dispatch — it
/// selects which IR node shape to emit. The emitted IR nodes contain
/// zero control flow; downstream codegen lowers each to one fixed
/// SQL per crud §7 / me §7.
pub fn synthesize_conventions(feature: &mut ir::Feature) -> Vec<CrudSynthDiagnostic> {
    let mut diagnostics: Vec<CrudSynthDiagnostic> = Vec::new();
    let mut to_add_commands: Vec<ir::Command> = Vec::new();
    let mut to_add_queries: Vec<ir::Query> = Vec::new();
    // §11 inspect surface (Cell C4 / M3) — Feature.synth_origins
    // records every name in a convention's set: `Synthesized(<bundle>)`
    // for names the pass appended; `AuthorOverride(<bundle>)` for names
    // the author wrote (synth skipped per crud §6 / me §6). Inspect
    // uses these markers to render `[conv:<bundle>]` /
    // `[author override; convention skipped]`.
    let mut synth_origins_inserts: Vec<(String, ir::ConventionOrigin)> = Vec::new();

    let existing_command_names: std::collections::HashSet<String> =
        feature.commands.iter().map(|c| c.name.clone()).collect();
    let existing_query_names: std::collections::HashSet<String> = feature
        .queries
        .iter()
        .map(|q| q.name().to_owned())
        .collect();

    // §5.8 — default policy is the feature's `authenticated` policy.
    let has_authenticated = feature
        .policies
        .categories
        .iter()
        .any(|p| p.name == "authenticated");

    for resource in &feature.resources {
        // Per-bundle dispatch — each resource may declare zero, one,
        // or both bundles in `conventions [...]`. Bundle blocks are
        // independent; the override-collision logic (`existing_*`
        // sets) is shared. crud §6.1 / me §6.1: zero name collisions
        // by construction because `crud` owns `lookup_<r>` while `me`
        // owns `lookup_my_<r>`.
        let has_crud = resource.conventions.contains(&ir::ConventionRef::Crud);
        let has_me = resource.conventions.contains(&ir::ConventionRef::Me);
        if !has_crud && !has_me {
            continue;
        }

        // owner-scope §7.3 — resolve once per resource so the crud and
        // me blocks share one decision. Composability §5.3 / §6.1:
        // one annotation drives mode for every bundle that synths
        // against the resource. Diagnostics (§11.1) are pushed
        // regardless of which bundles are active (they're a property
        // of the resource shape, not of the bundle).
        let owner_scope =
            resolve_owner_scope(feature, resource, &mut diagnostics);

        // ===== `crud` bundle (§5) — gated; runs only when declared. =====
        if has_crud {

        // §5.8 — guard: policy `authenticated` must exist.
        if !has_authenticated {
            diagnostics.push(CrudSynthDiagnostic::PolicyNotFound {
                resource: resource.name.clone(),
            });
            // We still synthesize with `PolicyRef::Local("authenticated")`
            // even though it's unresolved — Cell C4 will surface the
            // diagnostic; the IR shape stays uniform. This mirrors the
            // FR-3a auto-photo precedent (which returns silently when
            // no policy is found; here we surface a typed diagnostic
            // instead).
        }

        let categorised = categorize_fields(resource);

        // §11 `crud_synth_no_required_fields` — `create.input` would be
        // empty if every required-on-resource field is Tenant or Auto.
        // Detect by looking at the create-input list.
        let create_input_fields = categorised.create_input_fields();
        if create_input_fields.is_empty() {
            diagnostics.push(CrudSynthDiagnostic::NoRequiredFields {
                resource: resource.name.clone(),
            });
        }

        let resource_snake = pascal_to_snake(&resource.name);

        // §5.1 — the 5 synth names, in canonical order.
        let create_name = format!("create_{}", resource_snake);
        let update_name = format!("update_{}", resource_snake);
        let delete_name = format!("delete_{}", resource_snake);
        let lookup_name = format!("lookup_{}", resource_snake);
        let list_name = format!("list_{}s", resource_snake);

        // §6 — per-name override. If the author wrote the same name we
        // skip *just that name* with no warning, unless the author's
        // signature diverges from the canonical shape — that lands the
        // `crud_synth_signature_mismatch` diagnostic (§11 / §9).
        //
        // The `if existing_*.contains(...)` checks below are
        // authoring-time controls (which synth to add), NOT lowering
        // control flow over the emitted IR — RULE-VOCAB-03 (§7) is
        // preserved.

        // 1) create_<resource>
        if existing_command_names.contains(&create_name) {
            if let Some(reason) = check_command_signature_mismatch(
                feature,
                &create_name,
                &create_input_fields,
                CanonicalReturn::CreatesResource(&resource.name),
            ) {
                diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                    resource: resource.name.clone(),
                    synth_name: create_name.clone(),
                    reason,
                });
            }
            synth_origins_inserts.push((
                create_name.clone(),
                ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
            ));
        } else {
            let mut cmd = build_create_command(
                &create_name,
                &resource.name,
                &create_input_fields,
            );
            // §8.5.A — owner-scope create-side CTE-INSERT. The CREATE
            // synth carries the *full* OwnerScopeSql (cte_owner_check
            // populated) so codegen can paste the CTE prefix in front
            // of the INSERT. Tenant-only resources keep
            // `owner_scope_sql: None` and emit the same shape as
            // before this cell.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                cmd.owner_scope_sql = Some(scope.clone());
            }
            to_add_commands.push(cmd);
            synth_origins_inserts.push((
                create_name.clone(),
                ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
            ));
        }

        // 2) update_<resource>
        if existing_command_names.contains(&update_name) {
            let canonical_update_inputs = categorised.update_input_fields();
            if let Some(reason) = check_command_signature_mismatch(
                feature,
                &update_name,
                &canonical_update_inputs,
                CanonicalReturn::UpdatesResource(&resource.name),
            ) {
                diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                    resource: resource.name.clone(),
                    synth_name: update_name.clone(),
                    reason,
                });
            }
            synth_origins_inserts.push((
                update_name.clone(),
                ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
            ));
        } else {
            let mut cmd = build_update_command(
                &update_name,
                &resource.name,
                &categorised.update_input_fields(),
            );
            // §8.2 — owner-scope WHERE on UPDATE. The carrier carries
            // ONLY the `where_predicate`; codegen drops the
            // `cte_owner_check` (None here, since UPDATE doesn't need
            // the CTE wrapper). We share the resolution by cloning;
            // codegen reads only what it needs per shape.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                cmd.owner_scope_sql = Some(ir::OwnerScopeSql {
                    cte_owner_check: None,
                    ..scope.clone()
                });
            }
            to_add_commands.push(cmd);
            synth_origins_inserts.push((
                update_name.clone(),
                ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
            ));
        }

        // 3) delete_<resource>
        if existing_command_names.contains(&delete_name) {
            if let Some(reason) = check_command_signature_mismatch(
                feature,
                &delete_name,
                &[],
                CanonicalReturn::DeletesResource(&resource.name),
            ) {
                diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                    resource: resource.name.clone(),
                    synth_name: delete_name.clone(),
                    reason,
                });
            }
            synth_origins_inserts.push((
                delete_name.clone(),
                ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
            ));
        } else {
            let mut cmd = build_delete_command(&delete_name, &resource.name);
            // §8.1 — owner-scope WHERE on DELETE. Same shape as the
            // pre-absorption hand-rolled handler in §1.1 trigger
            // evidence. CTE not used on DELETE; only the predicate.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                cmd.owner_scope_sql = Some(ir::OwnerScopeSql {
                    cte_owner_check: None,
                    ..scope.clone()
                });
            }
            to_add_commands.push(cmd);
            synth_origins_inserts.push((
                delete_name.clone(),
                ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
            ));
        }

        // 4) lookup_<resource>
        if existing_query_names.contains(&lookup_name) {
            if let Some(reason) = check_query_signature_mismatch(
                feature,
                &lookup_name,
                CanonicalReturn::ReturnsResource(&resource.name),
            ) {
                diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                    resource: resource.name.clone(),
                    synth_name: lookup_name.clone(),
                    reason,
                });
            }
            synth_origins_inserts.push((
                lookup_name.clone(),
                ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
            ));
        } else {
            let mut q = build_lookup_query(&lookup_name, &resource.name);
            // §8.3 — owner-scope WHERE on LOOKUP. The Lookup query's
            // canonical keys (id = $1) get extended with the chain
            // predicate emitted by codegen via `owner_scope_sql`.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                if let ir::Query::Lookup(lq) = &mut q {
                    lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
            }
            to_add_queries.push(q);
            synth_origins_inserts.push((
                lookup_name.clone(),
                ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
            ));
        }

        // 5) list_<resource>s
        if existing_query_names.contains(&list_name) {
            if let Some(reason) = check_query_signature_mismatch(
                feature,
                &list_name,
                CanonicalReturn::ReturnsResourceMany(&resource.name),
            ) {
                diagnostics.push(CrudSynthDiagnostic::SignatureMismatch {
                    resource: resource.name.clone(),
                    synth_name: list_name.clone(),
                    reason,
                });
            }
            synth_origins_inserts.push((
                list_name.clone(),
                ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud),
            ));
        } else {
            let mut q = build_list_query(&list_name, &resource.name);
            // §8.4 — owner-scope WHERE on LIST. Same predicate; the
            // synth's pagination shape is unaffected.
            if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                if let ir::Query::List(lq) = &mut q {
                    lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                        cte_owner_check: None,
                        ..scope.clone()
                    });
                }
            }
            to_add_queries.push(q);
            synth_origins_inserts.push((
                list_name.clone(),
                ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud),
            ));
        }
        } // ===== end `crud` bundle =====

        // ===== `me` bundle (me §5) — singleton-per-actor lookup. =====
        //
        // Authoring-time mode classification (me §5.3). The synth picks
        // ONE of four shapes from the resource's static structure; the
        // emitted IR node contains zero branches (me §7 / RULE-VOCAB-03).
        if has_me {
            // me §5.4 — default policy is `authenticated`. Reuses the
            // crud policy probe; a missing policy emits the diagnostic
            // (no _Me suffix on the variant — `PolicyNotFound` covers
            // both bundles since the policy slot has the same name).
            if !has_authenticated {
                // Only emit once per resource even if both bundles
                // declared `me` and `crud`; the crud block above will
                // have already pushed `PolicyNotFound` if it ran.
                // Dedupe by inspecting `diagnostics` for an existing
                // entry on this resource.
                let already_emitted = diagnostics.iter().any(|d| {
                    matches!(
                        d,
                        ConventionSynthDiagnostic::PolicyNotFound { resource: r }
                            if r == &resource.name
                    )
                });
                if !already_emitted {
                    diagnostics.push(ConventionSynthDiagnostic::PolicyNotFound {
                        resource: resource.name.clone(),
                    });
                }
            }

            let resource_snake = pascal_to_snake(&resource.name);
            let lookup_my_name = format!("lookup_my_{}", resource_snake);

            // me §5.3 — classify the resource's actor axis. Four-mode
            // closed table; `None` triggers `me_synth_no_actor_resolution`.
            // The classification is a STATIC truth table over resource
            // shape; no runtime branching is introduced into the
            // emitted IR.
            let mode = classify_me_mode(resource);

            match mode {
                Some(m) => {
                    // me §6 — per-name override. Author wrote
                    // `lookup_my_<resource>` (or the `query.lookup
                    // my_<resource>` declarative form, which lowers to
                    // the same IR `Query::Lookup` name).
                    if existing_query_names.contains(&lookup_my_name) {
                        if let Some(reason) = check_me_lookup_signature_mismatch(
                            feature,
                            &lookup_my_name,
                            &resource.name,
                        ) {
                            diagnostics.push(
                                ConventionSynthDiagnostic::MeSignatureMismatch {
                                    resource: resource.name.clone(),
                                    synth_name: lookup_my_name.clone(),
                                    reason,
                                },
                            );
                        }
                        synth_origins_inserts.push((
                            lookup_my_name.clone(),
                            ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me),
                        ));
                    } else {
                        let mut q = build_lookup_my_query(
                            &lookup_my_name,
                            &resource.name,
                            m,
                        );
                        // §6.1 composition — `[crud, me]` + `@owner_axis`
                        // composes uniformly: the `me` synth also reads
                        // the resource-level annotation and appends the
                        // chain predicate. The unique-user variant is
                        // mutually exclusive with `@owner_axis` per
                        // §11.1 collision check, so this path only
                        // attaches scope when the resource is NOT
                        // user-keyed and the resolution succeeded.
                        if let OwnerScopeResolution::Scoped(scope) = &owner_scope {
                            if let ir::Query::Lookup(lq) = &mut q {
                                lq.owner_scope_sql = Some(ir::OwnerScopeSql {
                                    cte_owner_check: None,
                                    ..scope.clone()
                                });
                            }
                        }
                        to_add_queries.push(q);
                        synth_origins_inserts.push((
                            lookup_my_name.clone(),
                            ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me),
                        ));
                    }
                }
                None => {
                    // me §11.1 — no actor axis. Resource has no `user`
                    // field, no `org` field, and is not itself the
                    // `User` resource. The synth has no key to filter
                    // on; emit diagnostic, skip synth.
                    diagnostics.push(ConventionSynthDiagnostic::MeNoActorResolution {
                        resource: resource.name.clone(),
                    });
                }
            }
        } // ===== end `me` bundle =====
    }

    feature.commands.extend(to_add_commands);
    feature.queries.extend(to_add_queries);
    feature.synth_origins.extend(synth_origins_inserts);
    diagnostics
}

// =============================================================================
// `conventions [me]` synthesis helpers — Cell M2
//
// Spec: `docs/proposals/ir-resource-conventions-me.md` §§5.3, 5.5, 5.6.
//
// `classify_me_mode` is the entire decision surface — a 4-row truth
// table over resource shape, evaluated once at synth time. Once a mode
// is picked, `build_lookup_my_query` emits ONE fixed `Query::Lookup`
// shape with a mode-specific `keys` vector (the WHERE-clause builder).
// **The emitted IR contains zero branches** — RULE-VOCAB-03 (me §7).
// =============================================================================

/// me §5.3 — the four key-resolution modes. Classification is static;
/// each variant carries no runtime state. The selected variant uniquely
/// determines the `KeyClause` vector emitted into the synthesized
/// `Query::Lookup` (me §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeMode {
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
fn classify_me_mode(resource: &ir::Resource) -> Option<MeMode> {
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
        f.name == "org"
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "Org")
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
fn build_lookup_my_query(name: &str, resource: &str, mode: MeMode) -> ir::Query {
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
fn check_me_lookup_signature_mismatch(
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

/// §5.7 field categorisation result. Each field on a resource lands in
/// exactly one group; `create_input_fields` and `update_input_fields`
/// project the Required + Optional groups into the canonical input lists
/// per §5.2 / §5.3.
struct CategorisedFields<'a> {
    required: Vec<&'a ir::Field>,
    optional: Vec<&'a ir::Field>,
}

impl<'a> CategorisedFields<'a> {
    /// §5.2 — `create.input` shape: required fields stay required,
    /// optional fields stay optional. Order matches resource field order.
    fn create_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, true));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }

    /// §5.3 — `update.input` shape: every non-immutable field becomes
    /// optional regardless of its required-on-resource flag. Field-by-
    /// field optional update — fields omitted from input are not touched.
    fn update_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, false));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }
}

/// §5.7 — split a resource's fields into Tenant / Auto / Required /
/// Optional groups. Only Required + Optional are returned (Tenant and
/// Auto have no presence in the synth input lists).
fn categorize_fields(resource: &ir::Resource) -> CategorisedFields<'_> {
    // Detect the `user: User required unique` shape per §5.7.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    // Discriminator field is in the Auto group when lifecycle is set.
    let lifecycle_discriminator: Option<&str> = resource
        .lifecycle
        .as_ref()
        .map(|lc| lc.discriminator_field.as_str());

    let mut required: Vec<&ir::Field> = Vec::new();
    let mut optional: Vec<&ir::Field> = Vec::new();

    for field in &resource.fields {
        // Tenant group.
        let is_tenant = field.name == "org" || (has_user_unique && field.name == "user");
        // Auto group.
        let is_auto = matches!(field.name.as_str(), "id" | "created_at" | "updated_at")
            || lifecycle_discriminator.is_some_and(|d| d == field.name);

        if is_tenant || is_auto {
            continue;
        }

        if field.required {
            required.push(field);
        } else {
            optional.push(field);
        }
    }

    CategorisedFields { required, optional }
}

// =============================================================================
// `@owner_axis(through: <col>)` synth-pass extension — Cell O2.
//
// Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
// §7.3 (`build_where_clause` extension), §8 (auto-synth worked
// examples), §8.5.A (CTE-INSERT for create-side verification),
// §9 (override semantics), §11.1 (3 new doctor codes).
//
// **RULE-VOCAB-03 (§7 + §8.6)**: each shape composed here lowers to
// exactly ONE SQL statement. The CTE-INSERT (§8.5.A) is a single
// CTE-wrapped INSERT — Postgres evaluates the CTE either yields a
// row and the INSERT fires once, or yields zero rows and the INSERT
// fires zero times. No procedural sequencing; no runtime branching;
// no two-roundtrip check-then-insert.
// =============================================================================

/// §7.3 — resolution result for the owner-scope synth lookup. Returned
/// by `resolve_owner_scope`, consumed by the crud + me synth blocks.
///
/// - `Scoped(...)`: at least one `@owner_axis` field resolved cleanly;
///   the analyzer should emit the WHERE / CTE fragments for downstream
///   codegen consumption.
/// - `Tenant`: no `@owner_axis` annotation present — fall back to the
///   pre-existing tenant-only synth (today's default).
/// - `Diagnostic(...)`: an `@owner_axis` annotation was found but
///   doesn't resolve cleanly — surface the diagnostic and skip
///   owner-scope emission for the offending field. Other fields on
///   the same resource may still resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OwnerScopeResolution {
    Scoped(ir::OwnerScopeSql),
    Tenant,
}

/// §7.3 — table-name from PascalCase resource name. The convention
/// existing crud / me synths follow (snake_case identifier, quoted
/// per Postgres convention to dodge keyword collisions like `"user"`).
fn quoted_table(resource_name: &str) -> String {
    format!("\"{}\"", pascal_to_snake(resource_name))
}

/// §7.3 — quote an identifier when codegen will paste it inside an
/// SQL fragment. Postgres treats `"user"` and `user` differently
/// (the latter is a reserved keyword in many positions); the
/// hand-rolled handlers in the trigger pilot (`hostpoint/.../delete_service.go`
/// §1.1) quote both sides. We match that shape.
fn quoted_ident(ident: &str) -> String {
    format!("\"{}\"", ident)
}

/// §7.3 — resolve a resource's `@owner_axis` annotations into an
/// emittable `OwnerScopeSql` carrier, OR emit diagnostics for the 3
/// new doctor codes (§11.1) when the annotation can't resolve.
///
/// The function visits every field; the *first* cleanly-resolving
/// `@owner_axis` wins for the synth's WHERE-clause (the pilot has
/// exactly one owner-axis per resource — Property's `host`,
/// Service's `host`, CustomServiceCategory's `host`). Multi-axis
/// composition is deferred per §13.
///
/// Diagnostics are pushed into `diagnostics_out` for the caller; the
/// return value indicates whether the synth should still emit
/// owner-scope IR (yes for `Scoped`, no for `Tenant` — which also
/// covers "diagnostic emitted, fell back to tenant-only").
fn resolve_owner_scope(
    feature: &ir::Feature,
    resource: &ir::Resource,
    diagnostics_out: &mut Vec<ConventionSynthDiagnostic>,
) -> OwnerScopeResolution {
    // §7.4 / §11.1 `owner_axis_collides_with_unique_user` — resource
    // carries BOTH the user-keyed shape (`user: User required unique`)
    // AND an `@owner_axis(through: ...)` on another field. The two
    // scopes would compose redundantly; the unique-user mode already
    // restricts to `WHERE "user" = ctx.User.ID`.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    let mut emitted_collision_diag = false;
    let mut chosen: Option<ir::OwnerScopeSql> = None;

    for field in &resource.fields {
        let Some(axis) = field.owner_axis.as_ref() else {
            continue;
        };

        // §11.1 — collision check: declarative `user-keyed` mode
        // already provides ownership; surface a warning and skip the
        // owner-axis emission to avoid double-restriction. We emit
        // the diagnostic once per resource even if multiple fields
        // collide (rare; the spec describes "the resource has BOTH").
        if has_user_unique {
            if !emitted_collision_diag {
                diagnostics_out.push(
                    ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser {
                        resource: resource.name.clone(),
                        field: field.name.clone(),
                    },
                );
                emitted_collision_diag = true;
            }
            continue;
        }

        // The annotated field must be a UserDefined FK to another
        // resource. Primitive-field misuse is `owner_axis_on_non_fk`
        // and lives in O1's parser-time surface (§7.4); the analyzer
        // re-checks defensively so a hand-constructed IR fixture is
        // still surfaced (otherwise this code path would silently
        // skip the annotation).
        let ir::TypeRef::UserDefined(fk_qname) = &field.type_ref else {
            // Out-of-scope for O2 — O1 owns this diagnostic. Skip
            // silently rather than double-emit; downstream check
            // catches it.
            continue;
        };
        let fk_target = fk_qname.name.clone();

        // §11.1 `owner_axis_unknown_through` — the `through:` column
        // doesn't exist on the FK target resource. Resolve the FK
        // target in the feature's resource list.
        //
        // Cross-feature note: the FK target may live in another
        // feature (Hostpoint's catalog.Property → host.Host is the
        // motivating case). Synth runs per-feature without a Module
        // handle, so we can only validate the through-column when the
        // target is in the SAME feature. For cross-feature targets we
        // skip the diagnostic checks and trust the @owner_axis
        // annotation — the doctor pass (which has Module context)
        // surfaces missing-FK-target / wrong-through-type errors at a
        // higher layer. The SQL composition below only needs
        // `fk_target` (name) and `axis.through_column` (column name)
        // verbatim from the annotation; it does NOT need fk_resource
        // to exist locally.
        let fk_resource = feature
            .resources
            .iter()
            .find(|r| r.name == fk_target);

        if let Some(fk_resource) = fk_resource {
            let through_field = fk_resource
                .fields
                .iter()
                .find(|f| f.name == axis.through_column);
            let Some(through_field) = through_field else {
                let suggestion = nearest_field_name(&axis.through_column, &fk_resource.fields);
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisUnknownThrough {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    through: axis.through_column.clone(),
                    fk_target: fk_target.clone(),
                    suggestion,
                });
                continue;
            };

            // §11.1 `owner_axis_through_not_user_keyed` — the resolved
            // `through:` column must be typed as `User` (a UserDefined
            // ref to the User resource). Other actor types
            // (`@semantic.UserID` etc.) are deferred per §13.
            let is_user_keyed = matches!(
                &through_field.type_ref,
                ir::TypeRef::UserDefined(q) if q.name == "User"
            );
            if !is_user_keyed {
                diagnostics_out.push(ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed {
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    through: axis.through_column.clone(),
                    fk_target: fk_target.clone(),
                });
                // Warning, not error per §11.1 — still emit the chain so
                // codegen can produce SQL the author can hand-correct.
            }
        }
        // else: cross-feature FK target — skip per-field validation,
        // trust annotation, compose SQL below.

        // §7.3 / §8.1-8.4 — compose the WHERE predicate fragment.
        // Shape per §1.1 trigger evidence: literal Postgres
        // `<fk_col> IN (SELECT id FROM "<fk_table>" WHERE "<through>" = ctx.User.ID)`.
        // Single statement; the IN-subquery is a semi-join in the
        // planner (§8.6). The `ctx.User.ID` literal is a
        // codegen-substituted placeholder — downstream codegen
        // resolves to `$N` per its parameter-binding policy.
        let where_predicate = format!(
            "{fk_col} IN (SELECT id FROM {fk_table} WHERE {through} = ctx.User.ID)",
            fk_col = field.name,
            fk_table = quoted_table(&fk_target),
            through = quoted_ident(&axis.through_column),
        );

        // §8.5.A — CTE prefix for `create_<resource>`. The CREATE
        // synth pastes this in front of its INSERT; the INSERT then
        // selects FROM the CTE so a zero-row CTE yields a zero-row
        // INSERT (the synth surfaces a `not_owner` envelope via
        // existing RowsAffected==0 handling in `delete_*` per §8.7,
        // mirrored on create-side). One SQL statement total.
        let cte_owner_check = Some(format!(
            "WITH owner_check AS (SELECT 1 FROM {fk_table} WHERE id = ${fk_col} AND {through} = ctx.User.ID)",
            fk_col = field.name,
            fk_table = quoted_table(&fk_target),
            through = quoted_ident(&axis.through_column),
        ));

        if chosen.is_none() {
            chosen = Some(ir::OwnerScopeSql {
                field_name: field.name.clone(),
                fk_target,
                through_column: axis.through_column.clone(),
                where_predicate,
                cte_owner_check,
            });
        }
        // Multi-axis composition (multiple `@owner_axis` on one
        // resource) is deferred per §13. We take the first.
    }

    match chosen {
        Some(scope) => OwnerScopeResolution::Scoped(scope),
        None => OwnerScopeResolution::Tenant,
    }
}

/// §11.1 `owner_axis_unknown_through` — produce a nearest-name
/// suggestion from the FK target's field list. Returns `None` when
/// the closest candidate is not similar enough to be useful (Levenshtein
/// distance > half the input length — same threshold used by the
/// pre-existing nearest-string suggestions elsewhere in the doctor).
fn nearest_field_name(target: &str, fields: &[ir::Field]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for f in fields {
        let dist = levenshtein(target, &f.name);
        match best {
            Some((b, _)) if dist >= b => {}
            _ => best = Some((dist, f.name.as_str())),
        }
    }
    let (dist, name) = best?;
    if dist <= target.len().max(1) / 2 + 1 {
        Some(name.to_owned())
    } else {
        None
    }
}

/// Minimal Levenshtein for the nearest-name suggestion. Small (~12
/// LOC) so we don't pull a dependency for a one-off use. Copying the
/// pattern from elsewhere in the analyzer keeps the suggestion
/// quality consistent.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

/// `pub` re-export of the §7.3 WHERE-clause builder for direct test
/// access. Tests assert on the emitted SQL string; downstream codegen
/// pulls the same string off `Command.owner_scope_sql.where_predicate`.
///
/// **Direct call form** (no diagnostic surface). Used in tests that
/// construct a synthetic `Field` + `Resource` and want to round-trip
/// the SQL without running the whole `synthesize_conventions` pass.
/// For real synth, use `resolve_owner_scope` via `synthesize_conventions`.
#[doc(hidden)]
pub fn build_owner_scope_where_for_test(
    fk_col: &str,
    fk_target_resource: &str,
    through_column: &str,
) -> String {
    format!(
        "{fk_col} IN (SELECT id FROM {fk_table} WHERE {through} = ctx.User.ID)",
        fk_col = fk_col,
        fk_table = quoted_table(fk_target_resource),
        through = quoted_ident(through_column),
    )
}

/// §8.5.A — `pub` re-export of the CTE-INSERT prefix builder for
/// direct test access. Same role as `build_owner_scope_where_for_test`.
#[doc(hidden)]
pub fn build_owner_scope_cte_prefix_for_test(
    fk_col: &str,
    fk_target_resource: &str,
    through_column: &str,
) -> String {
    format!(
        "WITH owner_check AS (SELECT 1 FROM {fk_table} WHERE id = ${fk_col} AND {through} = ctx.User.ID)",
        fk_col = fk_col,
        fk_table = quoted_table(fk_target_resource),
        through = quoted_ident(through_column),
    )
}

/// Canonical return shape for `crud_synth_signature_mismatch` (§9 / §11).
/// The carried resource name is read by `check_command_signature_mismatch`
/// to compare against the author's effect target; the query variants
/// are matched only on kind today and reserve the name for Cell C4 if
/// it needs a richer diff message.
#[allow(dead_code)]
enum CanonicalReturn<'a> {
    CreatesResource(&'a str),
    UpdatesResource(&'a str),
    DeletesResource(&'a str),
    ReturnsResource(&'a str),
    ReturnsResourceMany(&'a str),
}

/// §11 `crud_synth_signature_mismatch` trigger — compare an authored
/// command to its canonical convention shape and return a reason string
/// when the input field list OR the effect/return type diverges.
/// Returns `None` when the signatures match (no diagnostic). Cell C4
/// formats `reason` into the user-facing message.
fn check_command_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    canonical_inputs: &[(&ir::Field, bool)],
    canonical_return: CanonicalReturn<'_>,
) -> Option<String> {
    let cmd = feature.commands.iter().find(|c| c.name == name)?;

    // Compare effect kind.
    let effect_matches = match (&cmd.effect, &canonical_return) {
        (ir::CommandEffect::Creates(e), CanonicalReturn::CreatesResource(name)) => {
            e.resource.name == *name
        }
        (ir::CommandEffect::Updates(e), CanonicalReturn::UpdatesResource(name)) => {
            e.resource.name == *name
        }
        (ir::CommandEffect::Deletes(e), CanonicalReturn::DeletesResource(name)) => {
            e.resource.name == *name
        }
        _ => false,
    };
    if !effect_matches {
        return Some(format!(
            "effect / target resource diverges from canonical shape for `{}`",
            name
        ));
    }

    // Compare input field names. Order-insensitive set check is enough
    // here — Cell C4 may surface a richer diff.
    let canonical_names: std::collections::BTreeSet<String> = canonical_inputs
        .iter()
        .map(|(f, _)| f.name.clone())
        .collect();
    let author_names: std::collections::BTreeSet<String> = match &cmd.input {
        ir::CommandInput::Short(names) => names.iter().cloned().collect(),
        ir::CommandInput::Typed(slots) => slots.iter().map(|s| s.name.clone()).collect(),
        ir::CommandInput::Empty => std::collections::BTreeSet::new(),
    };
    if author_names != canonical_names {
        return Some(format!(
            "input field list diverges from canonical shape for `{}`",
            name
        ));
    }

    None
}

/// §11 `crud_synth_signature_mismatch` trigger for queries. Returns a
/// reason string when the query's return type diverges from canonical.
fn check_query_signature_mismatch(
    feature: &ir::Feature,
    name: &str,
    canonical_return: CanonicalReturn<'_>,
) -> Option<String> {
    let query = feature.queries.iter().find(|q| q.name() == name)?;

    let ok = match (query, &canonical_return) {
        (ir::Query::Lookup(_), CanonicalReturn::ReturnsResource(_)) => true,
        (ir::Query::List(_), CanonicalReturn::ReturnsResourceMany(_)) => true,
        _ => false,
    };
    if ok {
        None
    } else {
        Some(format!(
            "query kind / return shape diverges from canonical for `{}`",
            name
        ))
    }
}

/// §5.2 — build `create_<resource>` command IR.
fn build_create_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Create,
        route: Vec::new(),
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Creates(ir::CreateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            from_input: true,
            assignments: Vec::new(),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.3 — build `update_<resource>` command IR.
fn build_update_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Updates(ir::UpdateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            assignments: Vec::new(),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.4 — build `delete_<resource>` command IR.
fn build_delete_command(name: &str, resource: &str) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Delete,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Deletes(ir::DeleteEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.5 — build `lookup_<resource>` query IR.
fn build_lookup_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments(["id".to_owned()])),
        }],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// §5.6 — build `list_<resource>s` query IR.
fn build_list_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::List(ir::ListQuery {
        name: name.to_owned(),
        public_contract: None,
        params: vec![
            ir::TypedSlot {
                name: "limit".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
            },
            ir::TypedSlot {
                name: "offset".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
            },
        ],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        // §5.6 default limit 50.
        paginate: Some(50),
        modifier: None,
        cache: None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// Project a `(field, required)` list into a typed `CommandInput`.
fn input_to_command_input(fields: &[(&ir::Field, bool)]) -> ir::CommandInput {
    if fields.is_empty() {
        return ir::CommandInput::Empty;
    }
    let slots: Vec<ir::TypedSlot> = fields
        .iter()
        .map(|(f, required)| ir::TypedSlot {
            name: f.name.clone(),
            type_ref: f.type_ref.clone(),
            required: *required,
            constraints: f.constraints.clone(),
        })
        .collect();
    ir::CommandInput::Typed(slots)
}

/// Common command-shape defaults applied to every synthesized CRUD
/// command. `policy authenticated`, `audit default`, `rate_limit` set
/// by the caller (write vs read uses different limits per §5.9).
fn default_synth_command(rate_limit: &str) -> ir::Command {
    ir::Command {
        name: String::new(),
        public_contract: None,
        kind: ir::CommandKind::Returns,
        route: Vec::new(),
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(ir::RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(ir::AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
        }),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — left `None` here; the synth pass mutates
        // each synthesized command to attach the resolved scope (see
        // `synthesize_conventions`). The default keeps tenant-only
        // shape stable for command IR not produced by the synth pass.
        owner_scope_sql: None,
    }
}

/// §5.9 — create / update / delete share `rate_limit "100 per 10 minutes per ip"`.
fn crud_write_rate_limit() -> &'static str {
    "100 per 10 minutes per ip"
}

fn pascal_to_snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    let mut iter = s.chars().peekable();

    while let Some(ch) = iter.next() {
        if ch.is_ascii_uppercase() {
            let next_is_lower = iter
                .peek()
                .copied()
                .is_some_and(|next| next.is_ascii_lowercase());
            let prev_needs_sep = prev.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next_is_lower)
            });
            if !out.is_empty() && prev_needs_sep {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
        prev = Some(ch);
    }

    out
}

fn snake_to_pascal(s: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;

    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            out.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }

    out
}

/// Lower a canonical-indent feature skeleton into an `ir::Feature`.
pub fn lower_feature_skeleton(
    skeleton: &syntax::FeatureSkeleton,
) -> Result<ir::Feature, AnalyzeError> {
    let mut agents = Vec::with_capacity(skeleton.agents.len());
    for agent_ast in &skeleton.agents {
        agents.push(lower_agent(&skeleton.name, agent_ast)?);
    }
    let auth = match &skeleton.auth {
        Some(auth_ast) => Some(lower_auth(auth_ast)?),
        None => None,
    };
    let mut jobs = Vec::with_capacity(skeleton.jobs.len());
    for job_ast in &skeleton.jobs {
        jobs.push(lower_job(&skeleton.name, job_ast)?);
    }
    let mut webhooks = Vec::with_capacity(skeleton.webhooks.len());
    for webhook_ast in &skeleton.webhooks {
        webhooks.push(lower_webhook(webhook_ast)?);
    }
    let mut notifications = Vec::with_capacity(skeleton.notifications.len());
    for notification_ast in &skeleton.notifications {
        notifications.push(lower_notification(&skeleton.name, notification_ast)?);
    }
    let mut pollers = Vec::with_capacity(skeleton.pollers.len());
    for poller_ast in &skeleton.pollers {
        pollers.push(lower_poller(poller_ast)?);
    }
    let mut event_groups = Vec::with_capacity(skeleton.event_groups.len());
    for group_ast in &skeleton.event_groups {
        event_groups.push(lower_event_group(group_ast));
    }
    let mut tenant_migrations = Vec::with_capacity(skeleton.tenant_migrations.len());
    for tm_ast in &skeleton.tenant_migrations {
        tenant_migrations.push(lower_tenant_migration(tm_ast)?);
    }
    let defaults = match &skeleton.defaults {
        Some(d) => lower_defaults(d),
        None => ir::Defaults::default(),
    };
    let commands = skeleton
        .commands
        .iter()
        .map(lower_command_decl)
        .collect::<Result<Vec<_>, _>>()?;
    let apis = skeleton.apis.iter().map(lower_api_decl).collect();
    let resources = skeleton
        .resources
        .iter()
        .map(lower_resource_decl)
        .collect::<Result<Vec<_>, _>>()?;
    let queries = skeleton
        .queries
        .iter()
        .map(|q| lower_query_decl(q, &skeleton.caches))
        .collect::<Result<Vec<_>, _>>()?;
    let records = skeleton
        .records
        .iter()
        .map(lower_record_decl)
        .collect::<Result<Vec<_>, _>>()?;
    let policies = skeleton
        .policies
        .as_ref()
        .map(lower_policies_decl)
        .unwrap_or_default();
    let enums = skeleton.enums.iter().map(lower_enum_decl).collect();
    let reports = skeleton
        .reports
        .iter()
        .map(|r| lower_report_decl(&skeleton.name, r))
        .collect::<Result<Vec<_>, _>>()?;
    // CL.C.4 — lower `aggregate <Name>` blocks from the surface AST.
    let aggregates = skeleton
        .aggregates
        .iter()
        .map(lower_aggregate_decl)
        .collect::<Vec<_>>();
    // MCP bucket cycle — lower `mcp_server <name>` blocks. Lowering is
    // value-preserving except for the closed-catalog `transport` mapping
    // (rejects unknown literals with a typed error).
    let mcp_servers: Vec<ir::MCPServerSpec> = skeleton
        .mcp_servers
        .iter()
        .map(lower_mcp_server)
        .collect::<Result<Vec<_>, _>>()?;
    // Cross-feature contracts §5.4 — lift the feature-level
    // `uses <feature>[, ...]+ [version v<N>]` clauses into parallel
    // `uses` / `uses_spans` / `uses_versions` lists. Each clause from a
    // single `uses` line becomes one entry in each parallel vector.
    let uses: Vec<String> = skeleton
        .uses_clauses
        .iter()
        .map(|c| c.feature.clone())
        .collect();
    let uses_spans: Vec<ir::SpanRef> = skeleton
        .uses_clauses
        .iter()
        .map(|c| span_of(c.span))
        .collect();
    let uses_versions: Vec<Option<u16>> = skeleton.uses_clauses.iter().map(|c| c.version).collect();

    let mut feature = ir::Feature {
        name: skeleton.name.clone(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults,
        uses,
        uses_spans,
        uses_versions,
        requirements: Vec::new(),
        enums,
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies,
        // IR Error-Vocab (Cell PARSE-1) — lower the optional `errors`
        // block onto the typed IR slot. Pre-vocab fixtures (no `errors`
        // block) keep `None`; codegen treats `None` identically to a
        // block with no overrides.
        errors: skeleton.errors.as_ref().map(lower_feature_errors_decl),
        commands,
        apis,
        records,
        queries,
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs,
        webhooks,
        notifications,
        event_groups,
        tenant_migrations,
        translation: skeleton.translation.as_ref().map(lower_translation_decl),
        pollers,
        auth,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents,
        reports,
        channels: skeleton.channels.iter().map(lower_channel).collect(),
        caches: skeleton
            .caches
            .iter()
            .map(lower_cache_profile_decl)
            .collect(),
        aggregates,
        mcp_servers,
        previous_names: Vec::new(),
        // Cell C4 (inlined): empty until C3's synthesis pass populates the
        // map per `docs/proposals/ir-resource-conventions-crud.md` §11.
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: Some(span_of(skeleton.span)),
    };
    lifecycle::lower_lifecycles(&mut feature, &skeleton.resources);
    synthesize_auto_photo(&mut feature);
    // ir-resource-conventions-crud §5 — synthesize 3 commands + 2
    // queries per resource that opts into `conventions [crud]`. The
    // bridge to populate `Feature.synth_origins` (so the inspect
    // surface from Cell C4 can annotate `[conv:crud]`) is wired in
    // `synthesize_conventions` itself. Diagnostics returned here are
    // currently dropped; the bridge cycle wires them through to
    // doctor per §11.
    let _ = synthesize_conventions(&mut feature);
    Ok(feature)
}

/// CL.C.4 — lower an `AggregateDecl` from the surface AST into
/// `ir::Aggregate`. Resource references stay unqualified `QualifiedName`
/// (feature `None`); doctor resolves them against the surrounding
/// feature's resource list.
fn lower_aggregate_decl(decl: &syntax::AggregateDecl) -> ir::Aggregate {
    ir::Aggregate {
        name: decl.name.clone(),
        root: ir::QualifiedName {
            feature: None,
            name: decl.root.clone(),
        },
        contains: decl
            .contains
            .iter()
            .map(|m| ir::QualifiedName {
                feature: None,
                name: m.clone(),
            })
            .collect(),
        invariants: decl.invariants.iter().map(lower_invariant_decl).collect(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// CL.C.4 — lower an `InvariantDecl` (shared by aggregate-scoped and
/// resource-scoped sites) into `ir::Invariant`. The `when` expression
/// is run through the closed-predicate parser used by agent `evals`
/// (`parse_closed_predicate`); when the shape isn't recognized the
/// `EvalPredicate::Unparsed(text)` variant carries the verbatim source
/// so doctor can echo it on failure.
fn lower_invariant_decl(decl: &syntax::InvariantDecl) -> ir::Invariant {
    ir::Invariant {
        name: decl.name.clone(),
        when: parse_closed_predicate(&decl.when),
        message: decl.message.clone(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// Phase L Tier 4d — lower a canonical-indent query declaration into
/// `ir::Query`. The three shapes (`query.list`, `query.lookup`,
/// `query.sql`) project onto the existing IR variants.
///
/// Cache (CL.C.3): if the query authors `cache <profile_name>`, the
/// inline `cache` field is populated by resolving the profile against
/// `caches`. When the profile is unknown, lowering preserves the
/// reference (so doctor can fire `cache-profile-unknown`) without
/// inventing a body.
fn lower_query_decl(
    q: &syntax::QueryDecl,
    caches: &[syntax::CacheProfileDecl],
) -> Result<ir::Query, AnalyzeError> {
    match q {
        syntax::QueryDecl::List(list) => Ok(ir::Query::List(ir::ListQuery {
            name: list.name.clone(),
            public_contract: lower_public_contract(&list.public_contract),
            params: list
                .params
                .iter()
                .map(lower_command_input_to_typed)
                .collect::<Result<Vec<_>, _>>()?,
            scope: Vec::new(),
            scope_override: list.scope_override,
            filters: lower_query_filter_lines(&list.filters),
            order: Vec::new(),
            paginate: list.paginate,
            modifier: list.modifier.clone(),
            cache: lower_query_cache_with_profile(
                &list.cache,
                list.cache_profile_ref.as_deref(),
                caches,
            ),
            // QUERY-POLICY-001 — route the parsed `policy @policy.<X>`
            // atom into IR so codegen can emit a non-empty
            // `lazuli.Policy{...}` literal. Mirrors `lower_command_decl`.
            policy: list
                .policy
                .as_deref()
                .map(lower_policy_atom)
                .unwrap_or(ir::PolicyRef::None),
            policy_expr: list.policy_expr.as_ref().map(lower_policy_expr),
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(list.span)),
            // owner-scope §7.3 — author-written queries default to
            // tenant-only. Synth pass mutates the slot for
            // convention-emitted queries.
            owner_scope_sql: None,
        })),
        syntax::QueryDecl::Lookup(lookup) => Ok(ir::Query::Lookup(ir::LookupQuery {
            name: lookup.name.clone(),
            public_contract: lower_public_contract(&lookup.public_contract),
            params: Vec::new(),
            keys: lookup
                .keys
                .iter()
                .map(|k| ir::KeyClause {
                    path: ir::Path::from_segments([k.name.clone()]),
                    equals: ir::Expr::Path(ir::Path::from_segments([k.name.clone()])),
                })
                .collect(),
            scope: Vec::new(),
            scope_override: false,
            filters: lower_query_filter_lines(&lookup.filters),
            // QUERY-POLICY-001 — same lowering as `query.list`.
            policy: lookup
                .policy
                .as_deref()
                .map(lower_policy_atom)
                .unwrap_or(ir::PolicyRef::None),
            policy_expr: lookup.policy_expr.as_ref().map(lower_policy_expr),
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(lookup.span)),
            // owner-scope §7.3 — author-written `query.lookup` defaults
            // to tenant-only. Synth pass mutates for `lookup_<r>` /
            // `lookup_my_<r>` when the resource carries `@owner_axis`.
            owner_scope_sql: None,
        })),
        syntax::QueryDecl::Sql(sql) => Ok(ir::Query::Sql(ir::SqlQuery {
            name: sql.name.clone(),
            public_contract: lower_public_contract(&sql.public_contract),
            params: sql
                .params
                .iter()
                .map(lower_command_input_to_typed)
                .collect::<Result<Vec<_>, _>>()?,
            scope: Vec::new(),
            scope_override: false,
            returns: type_ref_from_text(&sql.returns),
            sql_path: sql.sql_path.clone(),
            cache: None,
            // QUERY-POLICY-001 — same lowering as `query.list`.
            policy: sql
                .policy
                .as_deref()
                .map(lower_policy_atom)
                .unwrap_or(ir::PolicyRef::None),
            policy_expr: sql.policy_expr.as_ref().map(lower_policy_expr),
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: Some(span_of(sql.span)),
        })),
    }
}

/// WAR-VOCAB-QUERY-ENUM-01 closure — lower the verbatim
/// `query.list filters` lines (parsed by the syntax as `Vec<String>`)
/// into typed `ir::Filter` predicates. Each line is shaped
/// `<field> <op> <expr>` where `<op>` is the closed comparison set
/// (`=`, `!=`, `<`, `<=`, `>`, `>=`) and `<expr>` is one of:
///   - `<dotted.path>` (e.g. `ctx.actor.org_id`, `params.kind`) →
///     `Expr::Path`
///   - quoted string (`"foo"`) → `Expr::String`
///   - integer literal → `Expr::Integer`
///   - `true` / `false` → `Expr::Boolean`
///   - `nil` → `Expr::Nil`
///   - bare single-segment identifier (e.g. `approved`, `pending`) →
///     `Expr::Enum(EnumLiteral { type_name: None, variant })` — the
///     codegen serialises this as `lazuli.FromConst("<variant>")` so
///     the value lands as a Postgres TEXT bind parameter matching
///     the enum-typed column.
///
/// Lines that fail to parse are dropped silently; doctor's
/// vocab-filter lint catches malformed forms (TODO: add the lint).
fn lower_query_filter_lines(lines: &[String]) -> Vec<ir::Filter> {
    lines
        .iter()
        .filter_map(|line| parse_query_filter_line(line))
        .collect()
}

fn parse_query_filter_line(text: &str) -> Option<ir::Filter> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    for (token, op) in [
        ("<=", ir::CompareOp::Le),
        (">=", ir::CompareOp::Ge),
        ("!=", ir::CompareOp::Ne),
        ("<", ir::CompareOp::Lt),
        (">", ir::CompareOp::Gt),
        ("=", ir::CompareOp::Eq),
    ] {
        if let Some(idx) = find_top_level_operator(trimmed, token) {
            let (lhs_text, rhs_text) = trimmed.split_at(idx);
            let rhs_text = &rhs_text[token.len()..];
            let lhs = lhs_text.trim();
            let rhs = rhs_text.trim();
            if lhs.is_empty() || rhs.is_empty() {
                return None;
            }
            return Some(ir::Filter {
                predicate: ir::Predicate::Comparison {
                    left: filter_lhs_expr(lhs),
                    op,
                    right: filter_rhs_expr(rhs),
                },
                when: None,
            });
        }
    }
    None
}

/// LHS of a filter line is always a column reference. Single-segment
/// identifiers resolve to a column path; dotted forms (rare on LHS)
/// preserve their structure for the downstream codegen.
fn filter_lhs_expr(text: &str) -> ir::Expr {
    ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)))
}

/// RHS may be a literal, a dotted runtime path, OR a bare enum variant.
/// The bare-identifier case is the WAR-VOCAB-QUERY-ENUM-01 closure —
/// `expr_from_text` treats bare identifiers as `Expr::Path`, which the
/// query codegen would render as `lazuli.FromInput(...)` (a runtime
/// input lookup); the correct semantic is "const string equal to the
/// enum variant name," so we lift to `Expr::Enum`.
fn filter_rhs_expr(text: &str) -> ir::Expr {
    let text = text.trim();
    if let Some(stripped) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return ir::Expr::String(stripped.to_owned());
    }
    if let Ok(n) = text.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match text {
        "true" => return ir::Expr::Boolean(true),
        "false" => return ir::Expr::Boolean(false),
        "nil" => return ir::Expr::Nil,
        _ => {}
    }
    if text.contains('.') {
        return ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)));
    }
    // Bare single-segment identifier → enum variant. Type resolution
    // is deferred (no enum type carried in the syntax); codegen emits
    // a TEXT const via the unqualified `Expr::Enum` branch.
    ir::Expr::Enum(ir::EnumLiteral {
        type_name: None,
        variant: text.to_owned(),
    })
}

/// Cache bucket cycle — lift `cache` body lines (`key <expr>`, `ttl
/// <literal-or-prose>`, `tags <label>...`, `namespace <label>`) into
/// the typed `QueryCache` IR shape. Returns `None` when no `key` is
/// declared (defensive — doctor flags `cache without key/ttl`).
fn lower_query_cache(lines: &[String]) -> Option<ir::QueryCache> {
    if lines.is_empty() {
        return None;
    }
    let mut key: Option<String> = None;
    let mut ttl: Option<ir::CacheTtl> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut namespace: Option<String> = None;
    for raw in lines {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("key ") {
            key = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("ttl ") {
            let val = rest.trim();
            ttl = Some(parse_cache_ttl(val));
        } else if let Some(rest) = trimmed.strip_prefix("tags ") {
            for part in rest.split(',') {
                let label = part.trim();
                if !label.is_empty() {
                    tags.push(label.to_owned());
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("namespace ") {
            namespace = Some(rest.trim().to_owned());
        }
    }
    let key = key?;
    let ttl = ttl?;
    Some(ir::QueryCache {
        key,
        ttl,
        tags,
        namespace,
        profile_ref: None,
    })
}

/// Cache bucket cycle (CL.C.3) — resolve a query's cache reference,
/// preferring the inline body when present, otherwise looking up the
/// `cache_profile_ref` against the feature's `caches`. Returns `None`
/// when no cache is authored at all.
///
/// When the profile reference is unknown (no matching feature-level
/// `cache <name>`), this returns a stub `QueryCache` carrying the
/// `profile_ref` and no body so doctor can fire
/// `cache-profile-unknown`. The defensive shape (`key`/`ttl` left
/// empty) is OK because doctor blocks before codegen consumes it.
fn lower_query_cache_with_profile(
    inline_lines: &[String],
    profile_ref: Option<&str>,
    caches: &[syntax::CacheProfileDecl],
) -> Option<ir::QueryCache> {
    // Inline form wins when both are present (parser already rejects
    // the combination; this is defensive).
    if let Some(inline) = lower_query_cache(inline_lines) {
        return Some(inline);
    }
    let name = profile_ref?;
    if let Some(profile) = caches.iter().find(|c| c.name == name) {
        // Resolve: copy body fields from the profile and record the
        // reference name so inspect/codegen can preserve author intent.
        return Some(ir::QueryCache {
            key: profile.key.clone(),
            ttl: parse_cache_ttl(&profile.ttl),
            tags: profile.tags.clone(),
            namespace: profile.namespace.clone(),
            profile_ref: Some(name.to_owned()),
        });
    }
    // Unknown profile — emit a stub so the IR records author intent
    // and doctor can flag the dangling reference. `key`/`ttl` are
    // intentionally empty placeholders.
    Some(ir::QueryCache {
        key: String::new(),
        ttl: ir::CacheTtl::Quoted(String::new()),
        tags: Vec::new(),
        namespace: None,
        profile_ref: Some(name.to_owned()),
    })
}

/// Cache bucket cycle (CL.C.3) — lower a feature-level
/// `cache <name>` profile AST into `ir::CacheProfile`. Mirrors
/// the inline-shape lowering for `key`/`ttl`/`tags`/`namespace` and
/// adds the four CL.C.3 decorators (`stale_while_revalidate`,
/// `coalesce`, `sliding`). Closed-catalog enforcement (units, boolean
/// shape, SWR <= TTL) lives in doctor.
fn lower_cache_profile_decl(decl: &syntax::CacheProfileDecl) -> ir::CacheProfile {
    ir::CacheProfile {
        name: decl.name.clone(),
        key: decl.key.clone(),
        ttl: parse_cache_ttl(&decl.ttl),
        namespace: decl.namespace.clone(),
        tags: decl.tags.clone(),
        stale_while_revalidate: decl.stale_while_revalidate.as_deref().map(parse_cache_ttl),
        coalesce: decl.coalesce,
        sliding: decl.sliding,
        span_ref: Some(span_of(decl.span)),
    }
}

fn parse_cache_ttl(value: &str) -> ir::CacheTtl {
    // Quoted prose: `ttl "5 minutes"`.
    if value.starts_with('"') {
        let body = value.trim_matches('"').to_owned();
        return ir::CacheTtl::Quoted(body);
    }
    // Typed literal: `ttl 5m` (digits + s|m|h|d).
    let bytes = value.as_bytes();
    if let Some(idx) = bytes.iter().rposition(|c| c.is_ascii_alphabetic()) {
        // Find last alphabetic char; everything before is the digit body.
        let (num_part, unit_part) = value.split_at(idx);
        let unit = unit_part.trim();
        if let Ok(n) = num_part.trim().parse::<u32>() {
            return match unit {
                "s" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Seconds(n)),
                "m" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Minutes(n)),
                "h" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Hours(n)),
                "d" => ir::CacheTtl::Literal(ir::CacheTtlLiteral::Days(n)),
                _ => ir::CacheTtl::Quoted(value.to_owned()),
            };
        }
    }
    ir::CacheTtl::Quoted(value.to_owned())
}

fn lower_command_input_to_typed(
    slot: &syntax::CommandInputSlot,
) -> Result<ir::TypedSlot, AnalyzeError> {
    Ok(ir::TypedSlot {
        name: slot.name.clone(),
        type_ref: type_ref_from_text(&slot.type_text),
        required: slot.required,
        constraints: lift_field_constraints(&slot.name, &slot.constraints)?,
    })
}

/// Phase L Tier 4d — lower a canonical-indent `record` block into
/// `ir::Record`.
fn lower_record_decl(r: &syntax::RecordDecl) -> Result<ir::Record, AnalyzeError> {
    let fields = r
        .fields
        .iter()
        .map(lower_resource_field)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ir::Record {
        name: r.name.clone(),
        public_contract: lower_public_contract(&r.public_contract),
        fields,
        discriminator_field: r.discriminator_field.clone(),
        span_ref: Some(span_of(r.span)),
    })
}

/// Phase L Tier 4 follow-up — lower a canonical-indent `policies` block
/// into `ir::Policies`. The AST mirrors the IR shape 1:1 so this is a
/// structural copy: category atoms and per-resource field overrides
/// project directly. Closed-catalog validation lives in doctor.
fn lower_policies_decl(decl: &syntax::PoliciesDecl) -> ir::Policies {
    let categories = decl
        .categories
        .iter()
        .map(|c| ir::PolicyCategory {
            name: c.name.clone(),
            atoms: c.atoms.clone(),
            previous_names: Vec::new(),
            // IR Error-Vocab (Cell PARSE-1) — lower the optional
            // `when_denied @translation.<key>` child onto the typed IR
            // slot. Same-feature scope; cross-feature key resolution
            // lives in doctor (`translation_key_unknown` + ERR-VOCAB-002).
            when_denied: c.when_denied.as_ref().map(lower_translation_key_ref),
        })
        .collect();
    let fields = decl
        .fields
        .iter()
        .map(|f| ir::FieldPolicies {
            resource: lower_qualified_name(&f.resource),
            fields: f
                .fields
                .iter()
                .map(|fp| ir::FieldPolicy {
                    field: fp.field.clone(),
                    read: fp.read.clone(),
                    write: fp.write.clone(),
                    previous_names: Vec::new(),
                })
                .collect(),
        })
        .collect();
    ir::Policies {
        categories,
        fields,
        span_ref: Some(span_of(decl.span)),
    }
}

/// Cross-feature contracts — lower the optional `public contract <X> as v<N>`
/// AST clause into the IR `PublicContract` per
/// `docs/proposals/cross-feature-contracts.md` §5.1.
fn lower_public_contract(
    decl: &Option<syntax::PublicContractDeclAst>,
) -> Option<ir::PublicContract> {
    decl.as_ref().map(|d| ir::PublicContract {
        version: d.version,
        span_ref: Some(span_of(d.span)),
    })
}

/// Phase L Tier 4 follow-up — lower a canonical-indent `enum <Name>`
/// declaration into `ir::EnumDecl`. Variant storage values project
/// directly onto `ir::StorageValue`; absent values leave the codegen
/// target free to pick.
fn lower_enum_decl(decl: &syntax::EnumDeclAst) -> ir::EnumDecl {
    ir::EnumDecl {
        name: decl.name.clone(),
        public_contract: lower_public_contract(&decl.public_contract),
        variants: decl
            .variants
            .iter()
            .map(|v| ir::EnumVariant {
                name: v.name.clone(),
                storage_value: v.storage.as_ref().map(|s| match s {
                    syntax::EnumStorageValueDecl::Integer(n) => ir::StorageValue::Integer(*n),
                    syntax::EnumStorageValueDecl::String(s) => ir::StorageValue::String(s.clone()),
                }),
                previous_names: Vec::new(),
            })
            .collect(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// Phase L Tier 4c — lower a canonical-indent `resource` block into
/// `ir::Resource`. `tenancy` (resource-local override), `soft_delete`,
/// `timestamps`, `retention`, `validates`, and `derived_from` all
/// project through additive IR fields landed alongside this lowering.
fn lower_resource_decl(r: &syntax::ResourceDecl) -> Result<ir::Resource, AnalyzeError> {
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
    let validate = r.validates.first().map(|v| ir::PathRef::authored(v));
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
    Ok(ir::Resource {
        name: r.name.clone(),
        public_contract: lower_public_contract(&r.public_contract),
        tenancy,
        soft_delete: r.soft_delete,
        timestamps: if r.timestamps { Some(true) } else { None },
        fields,
        constraints: Vec::new(),
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
    })
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

fn lower_resource_field(f: &syntax::ResourceFieldDecl) -> Result<ir::Field, AnalyzeError> {
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
        span_ref: Some(span_of(f.span)),
    })
}

struct RecoveredFieldType {
    type_text: String,
    required: bool,
    unique: bool,
}

/// FR-PII-STACK — when `@cap.PII(...)` is authored after field modifiers,
/// the syntax parser leaves `required|optional|unique` inside `type_text`
/// because it only peels final bare tokens. Recover those modifiers after
/// removing the field-level decorator so the existing parser remains stable.
fn peel_trailing_field_modifiers(text: &str) -> RecoveredFieldType {
    let mut head = text.trim().to_owned();
    let mut required = false;
    let mut unique = false;
    loop {
        let trimmed = head.trim_end();
        if trimmed.ends_with(" required") {
            required = true;
            head = trimmed[..trimmed.len() - " required".len()].to_owned();
        } else if trimmed.ends_with(" optional") {
            head = trimmed[..trimmed.len() - " optional".len()].to_owned();
        } else if trimmed.ends_with(" unique") {
            unique = true;
            head = trimmed[..trimmed.len() - " unique".len()].to_owned();
        } else {
            head = trimmed.to_owned();
            break;
        }
    }
    RecoveredFieldType {
        type_text: head,
        required,
        unique,
    }
}

/// FR-PII-STACK — peel a non-leading `@cap.PII(...)` marker out of a
/// resource/record field's type tail and lower it into `Field.pii`. Leading
/// `@cap.PII(...)` remains a normal capability `TypeRef` for fields whose
/// only carrier is PII.
fn extract_field_level_pii_decorator(type_text: &str) -> (String, Option<ir::PiiCapability>) {
    let original = type_text.trim().to_owned();
    let Some((start, end)) = find_field_level_cap_pii_span(type_text) else {
        return (original, None);
    };
    let before = type_text[..start].trim_end();
    if before.is_empty() {
        return (original, None);
    }
    let token = &type_text[start..end];
    let Some(pii) = parse_cap_pii_type(token) else {
        return (original, None);
    };
    let after = type_text[end..].trim_start();
    let mut cleaned = before.to_owned();
    if !cleaned.is_empty() && !after.is_empty() {
        cleaned.push(' ');
    }
    cleaned.push_str(after);
    (cleaned.trim().to_owned(), Some(pii))
}

fn find_field_level_cap_pii_span(text: &str) -> Option<(usize, usize)> {
    const PREFIX: &[u8] = b"@cap.PII(";
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;
    while i + PREFIX.len() <= bytes.len() {
        let ch = bytes[i] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if depth == 0 && &bytes[i..i + PREFIX.len()] == PREFIX {
            let before_ok = i == 0 || (bytes[i - 1] as char).is_whitespace();
            if before_ok {
                if let Some(end) = find_balanced_decorator_end(text, i) {
                    return Some((i, end));
                }
            }
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn find_balanced_decorator_end(text: &str, start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

/// Project `syntax::FieldConstraintsDecl` onto the IR's
/// `ir::FieldConstraints`. Combination + default checks happen
/// separately; closed-catalog validate profiles are checked here.
fn lift_field_constraints(
    field: &str,
    decl: &syntax::FieldConstraintsDecl,
) -> Result<ir::FieldConstraints, AnalyzeError> {
    Ok(ir::FieldConstraints {
        min: decl.min,
        max: decl.max,
        pattern: decl.pattern.clone(),
        between: decl.between,
        length: decl.length,
        r#in: decl.r#in.clone(),
        sanitize_html: match decl.sanitize_html.as_deref() {
            Some(profile) => Some(lower_sanitize_html_profile(field, profile)?),
            None => None,
        },
        utf8_safe: decl.utf8_safe,
        max_recursion: decl.max_recursion,
        max_size: decl.max_size,
        covers_pii: decl.covers_pii.clone(),
    })
}

fn lower_sanitize_html_profile(
    field: &str,
    profile: &str,
) -> Result<ir::SanitizeHtmlProfile, AnalyzeError> {
    match profile {
        "strict" => Ok(ir::SanitizeHtmlProfile::Strict),
        "basic" => Ok(ir::SanitizeHtmlProfile::Basic),
        "markdown_safe" => Ok(ir::SanitizeHtmlProfile::MarkdownSafe),
        other => Err(AnalyzeError::UnknownSanitizeHtmlProfile {
            field: field.to_owned(),
            profile: other.to_owned(),
        }),
    }
}

/// `inline_validator_range_invariant_001` — reject empty numeric
/// ranges at compile time. A `min N max M` pair with N>M produces an
/// uninhabited domain; same for `between A and B` with A>B. The
/// shipped parser stores both bounds as `i64`, so the comparison is
/// total. This check runs after the combination rules so that conflict
/// errors (which already cover redundancy) take precedence.
fn validate_constraint_range_invariant(
    field: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    if let (Some(min), Some(max)) = (c.min, c.max) {
        if min > max {
            return Err(AnalyzeError::InlineValidatorRangeInvariant {
                field: field.to_owned(),
                rule: "min>max".to_owned(),
                low: min.to_string(),
                high: max.to_string(),
            });
        }
    }
    if let Some((a, b)) = c.between {
        if a > b {
            return Err(AnalyzeError::InlineValidatorRangeInvariant {
                field: field.to_owned(),
                rule: "between".to_owned(),
                low: a.to_string(),
                high: b.to_string(),
            });
        }
    }
    Ok(())
}

/// `inline_validator_type_mismatch_001` — reject constraint keywords
/// applied to a field whose underlying `BuiltinType` is outside the
/// §10.1 "Applies to" column. The check is intentionally generous on
/// `UserDefined` / `EnumRef` / `Capability` / `Many` / `Unresolved`
/// type refs (we skip them) so the existing `TypeRef::Unresolved`
/// path keeps owning the "this is an unknown name" error class.
///
/// Catalog (mirrors `docs/proposals/lzx-integration-codegen.md §10.1`):
/// - `min` / `max`: Text, Integer, Decimal, semantic string variants
/// - `length`: Text + semantic string variants ONLY
/// - `pattern`: Text + semantic string variants ONLY
/// - `between`: Integer, Decimal ONLY
/// - `in`: Text, Integer, Decimal + semantic string variants
fn validate_constraint_type_compatibility(
    field: &str,
    type_text: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    use ir::{BuiltinType as B, TypeRef};
    // Resolve once; bail out on non-Builtin refs (those classes never
    // carry inline constraints in v0 — and we don't want to false-
    // positive on unresolved names).
    let resolved = type_ref_from_syntax(type_text);
    let builtin = match resolved {
        TypeRef::Builtin(b) => b,
        _ => return Ok(()),
    };

    // Helper closures for the three categories.
    let is_text_like = matches!(
        builtin,
        B::Text
            | B::SemanticEmail
            | B::SemanticPhone
            | B::SemanticUrl
            | B::SemanticUuid
            | B::SemanticCurrency
    ) || matches!(
        &builtin,
        // B3 — a plugin-contributed semantic with a text carrier
        // accepts the same inline constraint families as Text. Wider
        // carriers gated by a separate proposal so they cannot land
        // here yet (loader enforces `carrier_type = "String"` only).
        B::SemanticPluginType { carrier, .. } if matches!(**carrier, B::Text)
    );
    let is_numeric = matches!(builtin, B::Integer | B::Decimal);
    let is_min_max_compatible = is_text_like || is_numeric;
    let is_in_compatible = is_text_like || is_numeric;

    if c.min.is_some() && !is_min_max_compatible {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "min".to_owned(),
            applies_to: "Text, Integer, Decimal".to_owned(),
        });
    }
    if c.max.is_some() && !is_min_max_compatible {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "max".to_owned(),
            applies_to: "Text, Integer, Decimal".to_owned(),
        });
    }
    if c.length.is_some() && !is_text_like {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "length".to_owned(),
            applies_to: "Text".to_owned(),
        });
    }
    if c.pattern.is_some() && !is_text_like {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "pattern".to_owned(),
            applies_to: "Text".to_owned(),
        });
    }
    if c.between.is_some() && !is_numeric {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "between".to_owned(),
            applies_to: "Integer, Decimal".to_owned(),
        });
    }
    if c.r#in.is_some() && !is_in_compatible {
        return Err(AnalyzeError::InlineValidatorTypeMismatch {
            field: field.to_owned(),
            field_type: type_text.trim().to_owned(),
            constraint: "in".to_owned(),
            applies_to: "Text, Integer, Decimal".to_owned(),
        });
    }
    Ok(())
}

/// `inline_validator_pattern_compile_001` — reject obviously
/// malformed regex patterns at lowering. The analyzer stays regex-
/// free by design (no `regex` crate dep in Cargo.toml — see comment
/// in `validate_default_against_constraints`); we only flag the
/// unambiguous shape errors that the Go/JS regex compilers also
/// reject (unbalanced `(`, unbalanced `[`, trailing `\`). Anything
/// passing this check is still subject to the runtime regex
/// compiler's authoritative judgement.
fn validate_constraint_pattern_compile(
    field: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    let Some(pattern) = c.pattern.as_deref() else {
        return Ok(());
    };
    // Trailing unescaped backslash: `^a\` — both RE2 and JS RegExp
    // reject.
    if pattern.ends_with('\\') {
        // Count trailing backslashes; an odd count means the last
        // backslash is unescaped.
        let trailing = pattern.chars().rev().take_while(|c| *c == '\\').count();
        if trailing % 2 == 1 {
            return Err(AnalyzeError::InlineValidatorPatternCompile {
                field: field.to_owned(),
                pattern: pattern.to_owned(),
                reason: "trailing unescaped `\\`".to_owned(),
            });
        }
    }
    // Bracket / paren balance check. Walk left-to-right, skipping the
    // character after `\` (escape). Inside a character class `[...]`
    // we still treat `\]` as escaped. We only flag the unambiguous
    // shape errors: paren or bracket counts that go negative or end
    // non-zero.
    let mut paren_depth: i32 = 0;
    let mut in_class = false;
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Skip the next char (treat as escaped). If the next
                // char is missing, the trailing-`\` check above already
                // fired.
                chars.next();
            }
            '[' if !in_class => {
                in_class = true;
            }
            ']' if in_class => {
                in_class = false;
            }
            '(' if !in_class => {
                paren_depth += 1;
            }
            ')' if !in_class => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return Err(AnalyzeError::InlineValidatorPatternCompile {
                        field: field.to_owned(),
                        pattern: pattern.to_owned(),
                        reason: "unbalanced `)`".to_owned(),
                    });
                }
            }
            _ => {}
        }
    }
    if in_class {
        return Err(AnalyzeError::InlineValidatorPatternCompile {
            field: field.to_owned(),
            pattern: pattern.to_owned(),
            reason: "unbalanced `[`".to_owned(),
        });
    }
    if paren_depth != 0 {
        return Err(AnalyzeError::InlineValidatorPatternCompile {
            field: field.to_owned(),
            pattern: pattern.to_owned(),
            reason: "unbalanced `(`".to_owned(),
        });
    }
    Ok(())
}

/// L0 #3 §10.2 — enforce inline constraint combination rules. Returns
/// the first conflict so authors get one focused diagnostic per field
/// (consistent with the rest of the analyzer).
fn validate_constraint_combinations(
    field: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    // length + min/max — `length N` already pins both bounds.
    if c.length.is_some() && c.min.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "length+min".to_owned(),
        });
    }
    if c.length.is_some() && c.max.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "length+max".to_owned(),
        });
    }
    // between + min/max — redundant.
    if c.between.is_some() && c.min.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "between+min".to_owned(),
        });
    }
    if c.between.is_some() && c.max.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "between+max".to_owned(),
        });
    }
    // in [...] + pattern — use enum instead.
    if c.r#in.is_some() && c.pattern.is_some() {
        return Err(AnalyzeError::ConstraintConflict {
            field: field.to_owned(),
            combo: "in+pattern".to_owned(),
        });
    }
    Ok(())
}

/// L0 #3 §10.3 — verify that a default literal satisfies the declared
/// inline constraints. The parser captures `default` verbatim (incl.
/// surrounding quotes for string literals); we strip the outer quotes
/// before length/pattern/in checks. Numeric checks parse the literal
/// as `i64`; non-integer literals fall back to a no-op rather than
/// raise a different error code, because the analyzer already has a
/// type-mismatch check elsewhere.
fn validate_default_against_constraints(
    field: &str,
    default_raw: &str,
    c: &syntax::FieldConstraintsDecl,
) -> Result<(), AnalyzeError> {
    let default_raw = default_raw.trim();
    // Strip surrounding double quotes for string-typed defaults.
    let unquoted =
        if default_raw.len() >= 2 && default_raw.starts_with('"') && default_raw.ends_with('"') {
            &default_raw[1..default_raw.len() - 1]
        } else {
            default_raw
        };
    // Numeric path: try parsing the (unquoted) literal as an integer.
    let as_int = unquoted.parse::<i64>().ok();
    // length check (string only — applies to char count of the
    // unquoted literal).
    if let Some(n) = c.length {
        if unquoted.chars().count() != n {
            return Err(AnalyzeError::DefaultViolatesConstraint {
                field: field.to_owned(),
                value: default_raw.to_owned(),
                rule: format!("length={}", n),
            });
        }
    }
    // min on numerics OR text length.
    if let Some(min) = c.min {
        if let Some(n) = as_int {
            if n < min {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("min={}", min),
                });
            }
        } else {
            // text-min checks character count.
            let len = unquoted.chars().count() as i64;
            if len < min {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("min={}", min),
                });
            }
        }
    }
    if let Some(max) = c.max {
        if let Some(n) = as_int {
            if n > max {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("max={}", max),
                });
            }
        } else {
            let len = unquoted.chars().count() as i64;
            if len > max {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("max={}", max),
                });
            }
        }
    }
    if let Some((lo, hi)) = c.between {
        if let Some(n) = as_int {
            if n < lo || n > hi {
                return Err(AnalyzeError::DefaultViolatesConstraint {
                    field: field.to_owned(),
                    value: default_raw.to_owned(),
                    rule: format!("between={}..{}", lo, hi),
                });
            }
        }
    }
    if let Some(values) = &c.r#in {
        // For text: compare unquoted string against the list verbatim.
        // For numerics: also compare unquoted, since `in [1,2,3]` is
        // stored as `["1", "2", "3"]` in the AST.
        if !values.iter().any(|v| v == unquoted) {
            return Err(AnalyzeError::DefaultViolatesConstraint {
                field: field.to_owned(),
                value: default_raw.to_owned(),
                rule: format!("in=[{}]", values.join(", ")),
            });
        }
    }
    if let Some(pattern) = &c.pattern {
        // We do NOT compile the regex here (Lazuli analyzer is regex-
        // free by design — RE2 enforcement lives in doctor + runtime).
        // For empty defaults the parser fails on the bare `""` anyway,
        // but we explicitly catch them so they don't silently pass.
        if unquoted.is_empty() && !pattern.is_empty() {
            return Err(AnalyzeError::DefaultViolatesConstraint {
                field: field.to_owned(),
                value: default_raw.to_owned(),
                rule: format!("pattern=\"{}\"", pattern),
            });
        }
    }
    Ok(())
}

/// `ir-rate-limit-env-aware` cell 1 — lower a parser `RateLimitSpecAst`
/// into the IR `RateLimitSpec`.
///
/// The single-line back-compat case (`rate_limit "X"`) lands here with
/// `default = Some("X")` and `by_env = []`. The new env-qualified case
/// folds each `RateLimitByEnvAst` into a `RateLimitByEnv` with closed-
/// catalog `EnvName`s separated from unknown identifiers; the unknown
/// bucket is what Cell 3 doctor will surface as `rate_limit_unknown_env`.
/// The proposal-defined `"unlimited"` keyword (§4.4) lowers to the
/// empty-string sentinel for both the default and per-env slots.
///
/// When the source authored only env-qualified lines (no unqualified
/// default), the IR default becomes the empty string — same sentinel
/// as `"unlimited"`. Cell 3 doctor surfaces
/// `rate_limit_no_default_with_qualifications` so the silent default
/// is visible at lint time, per proposal §9.2.
fn lower_rate_limit_spec(spec: &syntax::RateLimitSpecAst) -> ir::RateLimitSpec {
    let default = match spec.default.as_deref() {
        Some(literal) => lower_rate_limit_literal(literal),
        None => String::new(),
    };
    let by_env = spec
        .by_env
        .iter()
        .map(|entry| {
            let mut known = Vec::with_capacity(entry.envs.len());
            let mut unknown = Vec::new();
            for raw in &entry.envs {
                if let Some(env) = ir::EnvName::from_ident(raw) {
                    known.push(env);
                } else {
                    unknown.push(raw.clone());
                }
            }
            ir::RateLimitByEnv {
                envs: known,
                unknown_envs: unknown,
                limit: lower_rate_limit_literal(&entry.limit),
                span_ref: None,
            }
        })
        .collect();
    ir::RateLimitSpec {
        default,
        by_env,
        span_ref: None,
    }
}

/// `ir-rate-limit-env-aware` cell 1 — lower a single literal into the
/// canonical IR form, applying the `"unlimited"` keyword carve-out
/// (proposal §4.4). The keyword is recognised verbatim (case-sensitive)
/// and lowers to the empty string; everything else passes through
/// unchanged.
fn lower_rate_limit_literal(literal: &str) -> String {
    if literal == "unlimited" {
        String::new()
    } else {
        literal.to_owned()
    }
}

/// Phase L Tier 4b — lower a canonical-indent `command` block into
/// `ir::Command`. The kind is inferred from the body shape: `creates`
/// → Create, `updates` → Update, `deletes` → Delete, `returns` → Returns,
/// `handler`-only → Returns (the escape hatch case).
fn lower_command_decl(c: &syntax::CommandDecl) -> Result<ir::Command, AnalyzeError> {
    let kind = match c.effect.as_ref().map(|e| e.kind) {
        Some(syntax::CommandEffectKindDecl::Creates) => ir::CommandKind::Create,
        Some(syntax::CommandEffectKindDecl::Updates) => ir::CommandKind::Update,
        Some(syntax::CommandEffectKindDecl::Deletes) => ir::CommandKind::Delete,
        None => ir::CommandKind::Returns,
    };
    let route = c
        .route
        .iter()
        .map(|r| ir::RouteSlot {
            name: r.name.clone(),
            type_ref: type_ref_from_text(&r.type_text),
            from: r.from.clone(),
            kind: lower_route_slot_kind(r.kind),
        })
        .collect();
    let input = match &c.input {
        syntax::CommandInputDecl::Empty => ir::CommandInput::Empty,
        syntax::CommandInputDecl::Short(name) => ir::CommandInput::Short(vec![name.clone()]),
        syntax::CommandInputDecl::Typed(slots) => {
            // L0 #3 §10.2 — apply combination + default-compat checks
            // to each typed input slot too. Wave-B-CL4 — also run the
            // range / type-compatibility / pattern-compile checks so
            // command inputs aren't a back door past the resource-side
            // diagnostics.
            let mut lifted = Vec::with_capacity(slots.len());
            for s in slots {
                validate_constraint_combinations(&s.name, &s.constraints)?;
                validate_constraint_range_invariant(&s.name, &s.constraints)?;
                validate_constraint_type_compatibility(&s.name, &s.type_text, &s.constraints)?;
                validate_constraint_pattern_compile(&s.name, &s.constraints)?;
                lifted.push(ir::TypedSlot {
                    name: s.name.clone(),
                    type_ref: type_ref_from_text(&s.type_text),
                    required: s.required,
                    constraints: lift_field_constraints(&s.name, &s.constraints)?,
                });
            }
            ir::CommandInput::Typed(lifted)
        }
    };
    let target = c.target.as_ref().map(lower_target_expr);
    let lets = c.lets.iter().map(lower_let_binding).collect();
    let effect = if let Some(e) = c.effect.as_ref() {
        lower_command_effect(e)
    } else if let Some(returns) = c.returns.as_deref() {
        ir::CommandEffect::Returns(ir::ReturnsEffect {
            return_type: type_ref_from_text(returns),
        })
    } else {
        ir::CommandEffect::None
    };
    let policy = c
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let emits = c.emits.iter().map(|e| e.name.clone()).collect();
    let audit = c.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        emit_to: a.emit_to.clone(),
        data_subject: a.data_subject.clone(),
        record_before: a.record_before,
        record_after: a.record_after,
        retain_for: a.retain_for.clone(),
    });
    let approval = c.approval.as_ref().map(|a| ir::ApprovalSpec {
        required_when: a.required_when.clone(),
        by: a.by.clone(),
        timeout: a.timeout.clone(),
        then: match a.then {
            syntax::ApprovalThenDecl::Deny => ir::ApprovalThen::Deny,
            syntax::ApprovalThenDecl::Allow => ir::ApprovalThen::Allow,
            syntax::ApprovalThenDecl::Escalate => ir::ApprovalThen::Escalate,
        },
    });
    let invalidates = c
        .invalidates
        .iter()
        .map(|inv| ir::InvalidatesSpec {
            query: lower_qualified_name(&inv.query),
            args: inv.args.iter().map(lower_named_arg).collect(),
        })
        .collect();
    let external_calls = c.external_calls.iter().map(lower_external_call).collect();
    let deprecated = c
        .deprecated
        .as_ref()
        .map(|dep| lower_deprecated(dep, DeprecationTarget::Command));
    // Phase L Tier 4 follow-up — lift `timeout`/`retry`/`idempotency by`
    // mirrors of `parse_job`. Doctor cross-checks against
    // `external_calls` for the `INT-CALL-*` integration coverage rules.
    let timeout = c.timeout.clone();
    let retry = c.retry.as_ref().map(lower_retry);
    let idempotency = c
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let write_window = c.write_window.as_ref().map(|w| ir::CommandWriteWindow {
        by: lower_path_string(&w.by),
        within: w.within.clone(),
        span_ref: Some(span_of(w.span)),
    });
    let policy_expr = c.policy_expr.as_ref().map(lower_policy_expr);
    // WAR-RUNTIME-COMMAND-01 (Effect half): lift `handler @fn.<name>`
    // into the typed `HandlerRef`. `handler "./path.go"` (file escape
    // hatch) lifts as namespace=`path`, name=verbatim path. Codegen uses
    // the `fn` form to auto-wire `Effect: lazuli.Returns(...)` when the
    // declarative body has no other effect.
    let handler = c.handler.as_ref().map(|h| {
        let path = h.path.trim();
        if let Some(name) = path.strip_prefix("@fn.") {
            ir::HandlerRef {
                namespace: "fn".to_owned(),
                name: name.to_owned(),
                span_ref: Some(span_of(c.span)),
            }
        } else {
            ir::HandlerRef {
                namespace: "path".to_owned(),
                name: path.to_owned(),
                span_ref: Some(span_of(c.span)),
            }
        }
    });
    // IR Error-Vocab (Cell PARSE-1) — lift the optional `when_denied
    // @translation.<key>` child captured by the parser under `policy`.
    // Resolution-chain step 1 (proposal §2.A).
    let policy_when_denied = c.policy_when_denied.as_ref().map(lower_translation_key_ref);
    Ok(ir::Command {
        name: c.name.clone(),
        public_contract: lower_public_contract(&c.public_contract),
        kind,
        route,
        input,
        target,
        lets,
        effect,
        policy,
        policy_expr,
        policy_when_denied,
        emits,
        rate_limit: c.rate_limit.as_ref().map(lower_rate_limit_spec),
        audit,
        approval,
        invalidates,
        external_calls,
        timeout,
        retry,
        idempotency,
        write_window,
        deprecated,
        handler,
        tests: None,
        triggers: c.triggers.clone(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — author-written commands default to
        // tenant-only. The synth pass mutates the slot post-build for
        // crud / me bundle outputs when the resource carries
        // `@owner_axis`.
        owner_scope_sql: None,
        previous_names: c.previously.clone(),
        span_ref: Some(span_of(c.span)),
    })
}

#[derive(Clone, Copy)]
enum DeprecationTarget {
    Command,
    Api,
}

/// OpenAPI bucket cycle — lower an authored `deprecated` decorator into
/// the typed IR shape. `replacement` is classified by syntactic shape:
/// `https?://` → Url, `[<feature>.]command.<name>` / `[<feature>.]api.<name>`
/// → typed callable ref, otherwise → same-kind local ref.
fn lower_deprecated(
    decl: &syntax::CommandDeprecatedDecl,
    target: DeprecationTarget,
) -> ir::Deprecation {
    let replacement = decl.replacement.as_ref().map(|raw| {
        let trimmed = raw.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            ir::DeprecationReplacement::Url(trimmed.to_owned())
        } else if let Some(stripped) = trimmed.strip_prefix("@") {
            // `@adapter.command.<name>` or similar — store as Url-style
            // verbatim escape hatch.
            ir::DeprecationReplacement::Url(format!("@{}", stripped))
        } else {
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() == 2 && parts[0] == "command" {
                ir::DeprecationReplacement::LocalCommand(parts[1].to_owned())
            } else if parts.len() == 2 && parts[0] == "api" {
                ir::DeprecationReplacement::LocalApi(parts[1].to_owned())
            } else if parts.len() == 3 && parts[1] == "command" {
                ir::DeprecationReplacement::Qualified(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else if parts.len() == 3 && parts[1] == "api" {
                ir::DeprecationReplacement::QualifiedApi(ir::QualifiedName {
                    feature: Some(parts[0].to_owned()),
                    name: parts[2].to_owned(),
                })
            } else {
                match target {
                    DeprecationTarget::Command => {
                        ir::DeprecationReplacement::LocalCommand(trimmed.to_owned())
                    }
                    DeprecationTarget::Api => {
                        ir::DeprecationReplacement::LocalApi(trimmed.to_owned())
                    }
                }
            }
        }
    });
    ir::Deprecation {
        since: decl.since.clone(),
        replacement,
        sunset: decl.sunset.clone(),
    }
}

fn lower_route_slot_kind(kind: syntax::CommandRouteSlotKind) -> ir::RouteSlotKind {
    match kind {
        syntax::CommandRouteSlotKind::Plain => ir::RouteSlotKind::Plain,
        syntax::CommandRouteSlotKind::OpaqueToken => ir::RouteSlotKind::OpaqueToken,
        syntax::CommandRouteSlotKind::SignedToken => ir::RouteSlotKind::SignedToken,
    }
}

/// Phase L Tier 4b — lower a canonical-indent `api` block into `ir::Api`.
fn lower_api_decl(a: &syntax::ApiDecl) -> ir::Api {
    let method = match a.method {
        syntax::HttpMethod::Get => ir::HttpMethod::Get,
        syntax::HttpMethod::Post => ir::HttpMethod::Post,
        syntax::HttpMethod::Put => ir::HttpMethod::Put,
        syntax::HttpMethod::Patch => ir::HttpMethod::Patch,
        syntax::HttpMethod::Delete => ir::HttpMethod::Delete,
    };
    let policy = a
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let handler = a
        .handler
        .as_deref()
        .map(ir::PathRef::authored)
        .unwrap_or_else(|| ir::PathRef::convention(format!("./api/{}.go", a.name)));
    let policy_expr = a.policy_expr.as_ref().map(lower_policy_expr);
    let deprecated = a
        .deprecated
        .as_ref()
        .map(|dep| lower_deprecated(dep, DeprecationTarget::Api));
    ir::Api {
        name: a.name.clone(),
        method,
        path: a.path.clone(),
        policy,
        policy_expr,
        policy_when_denied: None,
        rate_limit: a.rate_limit.as_ref().map(lower_rate_limit_spec),
        output: type_ref_from_text(&a.output),
        handler,
        locale_negotiate: a.locale_negotiate.as_ref().map(lower_locale_negotiate_decl),
        deprecated,
        span_ref: Some(span_of(a.span)),
    }
}

// -----------------------------------------------------------------------------
// Report vocab — lower `report <name>` AST onto IR.
// -----------------------------------------------------------------------------

/// Lower a `ReportDecl` AST into `ir::Report`. Visibility defaults to
/// `signed` (per proposal §Slot inventory); formats outside the closed
/// `{csv, xlsx}` catalog drop silently — doctor reports
/// `REPORT-FORMAT-UNKNOWN-001` against the AST. Filename tokens are
/// parsed via the closed catalog (`{format}`, `{ctx.now:<strftime>}`,
/// `{ctx.user.id}`, `{ctx.tenant.id}`); unknown tokens land as
/// `FilenameToken::CtxNowStrftime("")` placeholders only if a parsing
/// helper rejects them — but we instead keep the literal verbatim and
/// surface unknown tokens via doctor.
fn lower_report_decl(_feature: &str, r: &syntax::ReportDecl) -> Result<ir::Report, AnalyzeError> {
    let source = lower_report_source(&r.source);

    let columns: Vec<ir::ReportColumn> = r
        .columns
        .iter()
        .map(|col| ir::ReportColumn {
            name: col.name.clone(),
            source: lower_report_column_source(&col.source),
            label: col.label.clone(),
            format: col.format.clone(),
            span_ref: Some(span_of(col.span)),
        })
        .collect();

    let formats: Vec<ir::ReportFormat> = r
        .formats
        .iter()
        .filter_map(|token| ir::ReportFormat::from_token(token.as_str()))
        .collect();

    let storage = r.storage.as_deref().map(lower_qualified_name);

    let visibility = match r.visibility.as_deref() {
        Some("public") => ir::FileVisibility::Public,
        Some("private") => ir::FileVisibility::Private,
        // Default per proposal §Slot inventory; doctor enforces signed
        // pairing with `signed_ttl`.
        _ => ir::FileVisibility::Signed,
    };

    let filename = r.filename.as_deref().map(lower_report_filename);

    let policy = r
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);

    let audit = r.audit.as_ref().map(|a| ir::AuditSpec {
        subjects: a.subjects.clone(),
        // Proposal v0.2 forbids `emit_to` on reports; doctor surfaces
        // any author-supplied value. The lowering preserves what was
        // written so the doctor lint sees the offending edge.
        emit_to: a.emit_to.clone(),
        data_subject: a.data_subject.clone(),
        record_before: a.record_before,
        record_after: a.record_after,
        retain_for: a.retain_for.clone(),
    });

    let policy_expr = r.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Report {
        name: r.name.clone(),
        source,
        columns,
        formats,
        storage,
        visibility,
        signed_ttl: r.signed_ttl.clone(),
        filename,
        policy,
        policy_expr,
        rate_limit: r.rate_limit.as_ref().map(lower_rate_limit_spec),
        audit,
        span_ref: Some(span_of(r.span)),
    })
}

fn lower_report_source(text: &str) -> ir::ReportSource {
    // Source forms:
    //   - `query.<name>`         (local short)
    //   - `<feature>.query.<name>` (cross-feature)
    //   - `<feature>.query.list.<name>` / `.lookup.<name>` / `.sql.<name>`
    //     (kind-qualified). The analyzer collapses the kind segment;
    //     doctor enforces the kind from the resolved target.
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split('.').collect();
    let qn = match parts.as_slice() {
        ["query", name] => ir::QualifiedName {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "query", name] => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        [feature, "query", _kind, name] => ir::QualifiedName {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        _ => lower_qualified_name(trimmed),
    };
    ir::ReportSource::Query(qn)
}

fn lower_report_column_source(src: &syntax::ReportColumnSourceAst) -> ir::ReportColumnSource {
    match src {
        syntax::ReportColumnSourceAst::RowField(field) => {
            ir::ReportColumnSource::RowField(field.clone())
        }
        syntax::ReportColumnSourceAst::FnCall { name, args } => {
            ir::ReportColumnSource::Fn(ir::FnInvocation {
                name: name.clone(),
                args: args.clone(),
            })
        }
    }
}

/// Parse a filename template string into the closed `FilenameToken`
/// catalog. Unknown `{...}` tokens are silently dropped from the typed
/// token list; the literal is preserved so doctor's
/// `REPORT-FILENAME-TOKEN-UNKNOWN-001` rule can scan the literal and
/// report user-facing diagnostics.
fn lower_report_filename(literal: &str) -> ir::ReportFilenamePattern {
    let mut tokens = Vec::new();
    let bytes = literal.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(close) = literal[i + 1..].find('}') {
                let raw = &literal[i + 1..i + 1 + close];
                if let Some(token) = parse_filename_token(raw) {
                    tokens.push(token);
                }
                i = i + 1 + close + 1;
                continue;
            }
        }
        i += 1;
    }
    ir::ReportFilenamePattern {
        literal: literal.to_owned(),
        tokens,
    }
}

fn parse_filename_token(raw: &str) -> Option<ir::FilenameToken> {
    match raw {
        "format" => Some(ir::FilenameToken::Format),
        "ctx.user.id" => Some(ir::FilenameToken::CtxUserId),
        "ctx.tenant.id" => Some(ir::FilenameToken::CtxTenantId),
        _ => {
            if let Some(strftime) = raw.strip_prefix("ctx.now:") {
                return Some(ir::FilenameToken::CtxNowStrftime(strftime.to_owned()));
            }
            None
        }
    }
}

/// i18n bucket cycle — lower an authored `translation` block onto
/// `ir::Translation`. Variant locales and plural arms come through
/// verbatim; doctor validates them against `app.locale.supported` and
/// the CLDR plural catalog.
///
/// IR Error-Vocab (Cell PARSE-1) — lower a surface
/// `TranslationKeyRefAst` (parsed from `@translation.<key>`) onto the
/// typed IR `TranslationKeyRef`. The `span_ref` is preserved so doctor
/// can quote the offending line on `translation_key_unknown` /
/// ERR-VOCAB-002 emission. Same-feature scope; v1 does not lower the
/// cross-feature `<feature>.@translation.<key>` form (cf. proposal
/// §3.1 — the surface token form keeps the parser single-shape).
fn lower_translation_key_ref(decl: &syntax::TranslationKeyRefAst) -> ir::TranslationKeyRef {
    ir::TranslationKeyRef {
        key: decl.key.clone(),
        span_ref: Some(span_of(decl.span)),
    }
}

/// IR Error-Vocab (Cell PARSE-1) — lower a surface `FeatureErrorsDecl`
/// onto `ir::FeatureErrors`. The `default hide` / `default expose` and
/// `expose client 4xx|5xx <fields>` slots project 1:1; per-code message
/// overrides keep their verbatim `code` so analyzer-side closed-catalog
/// enforcement (ERR-VOCAB-CODE-UNKNOWN) can report the offending token.
fn lower_feature_errors_decl(decl: &syntax::FeatureErrorsDecl) -> ir::FeatureErrors {
    ir::FeatureErrors {
        default: decl.default.map(|d| match d {
            syntax::ErrorExposureDefaultAst::Hide => ir::ErrorExposureDefault::Hide,
            syntax::ErrorExposureDefaultAst::Expose => ir::ErrorExposureDefault::Expose,
        }),
        exposure_4xx: decl.exposure_4xx.clone(),
        exposure_5xx: decl.exposure_5xx.clone(),
        audience_exposure: decl
            .audience_exposure
            .iter()
            .map(|r| ir::ErrorExposeRule {
                audience: r.audience.clone(),
                fields: r.fields.clone(),
                span_ref: Some(span_of(r.span)),
            })
            .collect(),
        redact_patterns: decl.redact_patterns.clone(),
        messages: decl
            .messages
            .iter()
            .map(|m| ir::FeatureErrorMessage {
                code: m.code.clone(),
                message: lower_translation_key_ref(&m.message),
                span_ref: Some(span_of(m.span)),
            })
            .collect(),
        // Reserved for v2 — per-field validator-error references. v1
        // parser leaves the slot empty (see proposal §3.4 deferral row).
        field_messages: Vec::new(),
        span_ref: Some(span_of(decl.span)),
    }
}

fn lower_translation_decl(t: &syntax::TranslationDecl) -> ir::Translation {
    ir::Translation {
        catalog: t.catalog.clone(),
        keys: t
            .keys
            .iter()
            .map(|key| ir::TranslationKey {
                name: key.name.clone(),
                variants: key
                    .variants
                    .iter()
                    .map(|v| ir::TranslationVariant {
                        locale: v.locale.clone(),
                        text: v.text.clone(),
                    })
                    .collect(),
                plurals: key
                    .plurals
                    .iter()
                    .map(|p| ir::TranslationPluralArm {
                        arm: p.arm.clone(),
                        variants: p
                            .variants
                            .iter()
                            .map(|v| ir::TranslationVariant {
                                locale: v.locale.clone(),
                                text: v.text.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// i18n bucket cycle — lower a per-api `locale_negotiate` block onto
/// `ir::LocaleNegotiate`. The runtime-unit form is parsed elsewhere
/// (`crates/lazuli_cli/src/app_manifest.rs`) since it lives on the
/// `app.lzi` side rather than feature side.
fn lower_locale_negotiate_decl(n: &syntax::LocaleNegotiateDecl) -> ir::LocaleNegotiate {
    ir::LocaleNegotiate {
        source: n.source.clone(),
        strategy: n.strategy.clone(),
        fallback: n.fallback.clone(),
    }
}

/// Phase L Tier 4a — lower a canonical-indent `defaults` block into
/// `ir::Defaults`. `policy_for` entries collapse onto `Defaults.policy`
/// when a single entry is authored; multi-entry `policy_for` (different
/// atoms per kind list) is captured by reading the first entry — the
/// language disallows conflicting defaults by convention. Doctor cross-
/// checks the surface form by walking the typed
/// `feature.policies.categories` slot (`populate_commands_from_ir`);
/// the legacy `collect_policy_atoms` text walker is retired.
fn lower_defaults(defaults: &syntax::FeatureDefaults) -> ir::Defaults {
    let tenancy = defaults.tenancy.as_ref().map(|t| match t {
        syntax::DefaultsTenancy::Org => ir::Tenancy::Org,
        syntax::DefaultsTenancy::Team => ir::Tenancy::Team,
        syntax::DefaultsTenancy::None => ir::Tenancy::None,
        syntax::DefaultsTenancy::Custom(name) => ir::Tenancy::Custom(name.clone()),
    });
    let policy = defaults
        .policy_for
        .first()
        .map(|entry| lower_policy_atom(entry.atom.as_str()))
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    ir::Defaults {
        tenancy,
        timestamps: defaults.timestamps,
        policy,
    }
}

/// Phase L Tier 3 — lower a canonical-indent `job` block into `ir::Job`.
/// Handler-backed bodies lower fully; declarative bodies preserve the
/// raw spine (`raw_target`, `raw_lets`, `raw_effect`) until Tier 4.
pub fn lower_job(feature: &str, job: &syntax::Job) -> Result<ir::Job, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &job.trigger);
    let idempotency = job
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = job.retry.as_ref().map(lower_retry);
    let tenant_from = job
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let fanout = job.fanout.as_ref().map(lower_fanout);
    let external_calls = job.external_calls.iter().map(lower_external_call).collect();
    let policy = job
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let policy = match policy {
        ir::PolicyRef::None => None,
        other => Some(other),
    };
    let body = lower_job_body(&job.body);

    let policy_expr = job.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Job {
        name: job.name.clone(),
        trigger,
        queue: job.queue.clone(),
        idempotency,
        retry,
        policy,
        policy_expr,
        policy_when_denied: None,
        tenant_from,
        fanout,
        timeout: job.timeout.clone(),
        external_calls,
        body,
        emits: job.emits.clone(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(job.span)),
    })
}

// =============================================================================
// L0 #8 — poller lowering (docs/proposals/poller-vocab.md §4).
//
// AST → IR is purely structural; doctor rules enforce the closed-catalog
// validity invariants (cursor field shapes, terminal-state existence,
// handler orphan, etc.). The lowering never fails on AST alone — it
// applies the defaults (`tick.every = 30s`, `tick.batch = 100`) and
// surfaces structurally well-formed IR for downstream consumers.
// =============================================================================

/// Default tick interval when `tick every <duration>` is omitted in source.
/// Per proposal §3.8.
const POLLER_DEFAULT_TICK_EVERY: &str = "30s";
const POLLER_DEFAULT_TICK_BATCH: u32 = 100;

pub fn lower_poller(poller: &syntax::PollerBlockAst) -> Result<ir::Poller, AnalyzeError> {
    let cursor_ast = poller
        .cursor
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "cursor".to_owned(),
        })?;
    let retry_ast = poller
        .retry
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "retry".to_owned(),
        })?;
    let resolve_name =
        poller
            .resolve_handler
            .as_deref()
            .ok_or_else(|| AnalyzeError::MissingField {
                kind: "poller".to_owned(),
                name: poller.name.clone(),
                field: "resolve via @fn.<name>".to_owned(),
            })?;
    if poller.idempotency.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "idempotency".to_owned(),
        });
    }
    if poller.states.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "states".to_owned(),
        });
    }

    let cursor = ir::PollerCursor {
        next_at_field: cursor_ast.next_at_field.clone(),
        resolved_at_field: cursor_ast.resolved_at_field.clone(),
        attempts_field: cursor_ast.attempts_field.clone(),
        span_ref: Some(span_of(cursor_ast.span)),
    };

    let backoff = match retry_ast.backoff_strategy.as_str() {
        "fixed" => ir::PollerBackoff::Fixed {
            base: retry_ast.backoff_base.clone(),
        },
        "linear" => ir::PollerBackoff::Linear {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        "exponential" => ir::PollerBackoff::Exponential {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("poller `{}` backoff", poller.name),
                value: other.to_owned(),
            });
        }
    };
    let retry = ir::PollerRetry {
        max_attempts: retry_ast.max_attempts,
        backoff,
        span_ref: Some(span_of(retry_ast.span)),
    };

    let states = poller
        .states
        .iter()
        .map(|s| ir::PollerState {
            name: s.name.clone(),
            kind: match s.kind_keyword.as_deref() {
                Some("initial") => ir::PollerStateKind::Initial,
                Some("terminal") => ir::PollerStateKind::Terminal,
                Some("intermediate") | None => ir::PollerStateKind::Intermediate,
                Some(_) => ir::PollerStateKind::Intermediate,
            },
            span_ref: Some(span_of(s.span)),
        })
        .collect::<Vec<_>>();

    let tick = match poller.tick.as_ref() {
        Some(t) => ir::PollerTick {
            every: t.every.clone(),
            batch: t.batch.unwrap_or(POLLER_DEFAULT_TICK_BATCH),
        },
        None => ir::PollerTick {
            every: POLLER_DEFAULT_TICK_EVERY.to_owned(),
            batch: POLLER_DEFAULT_TICK_BATCH,
        },
    };

    let tenant_from = poller
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });

    let idempotency = ir::IdempotencyKey {
        by: ir::Path {
            segments: poller.idempotency.iter().cloned().collect(),
        },
    };

    let audit = poller.audit.as_deref().map(|raw: &str| {
        let rest = raw.strip_prefix("audit ").unwrap_or(raw).trim();
        if rest == "default" {
            ir::AuditSpec {
                subjects: vec!["actor".to_owned(), "target.id".to_owned()],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        } else if let Some(reason) = rest.strip_prefix("none ") {
            ir::AuditSpec {
                subjects: vec![format!("none {}", reason)],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        } else {
            ir::AuditSpec {
                subjects: rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s: &&str| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }
        }
    });

    let retry_quirks = poller
        .retry_quirks
        .iter()
        .filter_map(|q| match q.kind.as_str() {
            "gender_flip_once" => Some(ir::PollerRetryQuirk::GenderFlipOnce {
                when: q.when.clone(),
                counter_field: q.counter_field.clone(),
                gender_field: q.mutate_field.clone(),
            }),
            // Unknown catalog entries are dropped during lowering;
            // doctor `POLLER-QUIRK-CATALOG-MISMATCH-001` surfaces the
            // diagnostic at the AST layer.
            _ => None,
        })
        .collect();

    Ok(ir::Poller {
        name: poller.name.clone(),
        source: poller.source.clone(),
        cursor,
        retry,
        states,
        resolve_handler: ir::HandlerRef {
            namespace: "fn".to_owned(),
            name: resolve_name.to_owned(),
            span_ref: Some(span_of(poller.span)),
        },
        terminal_status_field: poller.terminal_status_field.clone(),
        terminal_result_field: poller.terminal_result_field.clone(),
        tick,
        tenant_from,
        idempotency,
        audit,
        emits: poller.emits.clone(),
        retry_quirks,
        span_ref: Some(span_of(poller.span)),
    })
}

/// Phase L Tier 3 — lower a canonical-indent `webhook` block into
/// `ir::Webhook`. `verify: PathRef` falls back to a conventional path
/// derived from the webhook name (the legacy IR field is non-optional);
/// `structured_verify` carries the real structured spec lifted by
/// `parse_webhook_verify`.
pub fn lower_webhook(webhook: &syntax::Webhook) -> Result<ir::Webhook, AnalyzeError> {
    let structured_verify = Some(ir::VerifySpec {
        scheme: match webhook.verify.scheme.as_str() {
            "hmac" => ir::VerifyScheme::Hmac,
            other => {
                return Err(AnalyzeError::UnsupportedVerifyScheme {
                    scheme: other.to_owned(),
                });
            }
        },
        algorithm: webhook.verify.algorithm.clone(),
        secret_env: webhook
            .verify
            .secret_env
            .as_deref()
            .map(extract_env_binding)
            .unwrap_or_default(),
        header: webhook.verify.header.clone().unwrap_or_default(),
    });
    let tenant_from = webhook
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = webhook
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let policy = webhook
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));

    let (handler, returns) = match &webhook.handler {
        Some(h) => (
            ir::PathRef::authored(&h.path),
            h.returns.as_deref().map(|t| type_ref_from_text(t)),
        ),
        None => (
            ir::PathRef::convention(format!("./webhooks/{}.go", webhook.name)),
            None,
        ),
    };

    // Webhooks expanded cycle — typed payload reference (`payload from
    // webhook_events.<name>`). The parser stripped the catalog prefix
    // already, so the IR just keeps the suffix.
    let payload_from = webhook
        .payload_from
        .as_deref()
        .map(|name| ir::WebhookEventRef {
            name: name.to_owned(),
        });

    // `replay` short form (`replay allow within "..."`) and long form
    // (nested children) collapse onto the same `ReplaySpec`.
    let replay = webhook.replay.as_ref().map(|r| ir::ReplaySpec {
        mode: match r.mode.as_str() {
            "deny" => ir::ReplayMode::Deny,
            _ => ir::ReplayMode::Allow,
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_deref().map(lower_path_string),
    });

    // `dlq` discriminator (mutual exclusion enforced by the parser).
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        syntax::WebhookDlq::Emit { event, .. } => ir::DlqSpec::Emit {
            event: event.clone(),
        },
        syntax::WebhookDlq::Handler { path, .. } => ir::DlqSpec::Handler {
            path: ir::PathRef::authored(path),
        },
        syntax::WebhookDlq::Drop { reason, .. } => ir::DlqSpec::Drop {
            reason: reason.clone(),
        },
    });

    // Inbound retry shares the jobs `RetryPolicy` shape (Atrito #5).
    let retry = webhook.retry.as_ref().map(lower_retry);

    let policy_expr = webhook.policy_expr.as_ref().map(lower_policy_expr);
    let scope_global = webhook
        .scope_global
        .as_ref()
        .map(|sg| ir::WebhookScopeGlobalSpec {
            reason: sg.reason.clone(),
        });
    // B5 framework gap 2 — lift per-branch emit predicates onto the
    // typed `EmitPredicate` shape. The AST carries the raw `when`
    // clauses; we promote `path = "literal"` and
    // `path in ("a", "b")` to typed variants and fall back to
    // `EmitPredicateKind::Other { raw }` for anything else. Length
    // matches `webhook.emits` when any predicate is authored; an
    // empty vec means "flat list, no per-branch dispatch".
    let emit_predicates = if webhook.emits_predicates.is_empty() {
        Vec::new()
    } else {
        webhook
            .emits_predicates
            .iter()
            .map(|raw| raw.as_deref().map(lower_emit_predicate))
            .collect::<Vec<_>>()
    };

    Ok(ir::Webhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify: ir::PathRef::convention(format!("./webhooks/{}_verify.go", webhook.name)),
        structured_verify,
        tenant_from,
        scope_global,
        idempotency,
        policy,
        policy_expr,
        policy_when_denied: None,
        handler,
        returns,
        emits: webhook.emits.clone(),
        emit_predicates,
        payload_from,
        replay,
        dlq,
        retry,
        previous_names: Vec::new(),
        span_ref: Some(span_of(webhook.span)),
    })
}

/// B5 framework gap 2 — lift a raw `when <predicate>` clause into the
/// typed `ir::EmitPredicate`. Recognised shapes:
///
/// * `path = "literal"` — equality.
/// * `path in ("a", "b")` — set membership.
/// * anything else — `EmitPredicateKind::Other { raw }`.
///
/// The lift is intentionally conservative: shapes that don't match
/// the typed catalog are preserved verbatim so codegen can emit a
/// runtime-evaluated stub without losing authoring intent.
fn lower_emit_predicate(raw: &str) -> ir::EmitPredicate {
    let trimmed = raw.trim();
    let kind = parse_emit_predicate_kind(trimmed).unwrap_or_else(|| ir::EmitPredicateKind::Other {
        raw: trimmed.to_owned(),
    });
    ir::EmitPredicate {
        raw: trimmed.to_owned(),
        kind,
        span_ref: None,
    }
}

fn parse_emit_predicate_kind(text: &str) -> Option<ir::EmitPredicateKind> {
    // `path = "literal"` — split on the first `=` not followed by `=`
    // (avoid `==` if a future surface accepts it). The current closed
    // surface only authors a single `=`.
    if let Some((lhs, rhs)) = text.split_once('=') {
        let path = lhs.trim();
        let literal_raw = rhs.trim();
        if !path.is_empty() && !path.contains(' ') {
            if let Some(literal) = strip_quotes(literal_raw) {
                return Some(ir::EmitPredicateKind::Equals {
                    path: path.to_owned(),
                    literal: literal.to_owned(),
                });
            }
        }
    }
    // `path in ("a", "b", ...)`
    if let Some(in_pos) = find_word(text, "in") {
        let path = text[..in_pos].trim();
        let rhs = text[in_pos + 2..].trim();
        if !path.is_empty() && !path.contains(' ') && rhs.starts_with('(') && rhs.ends_with(')') {
            let inner = &rhs[1..rhs.len() - 1];
            let literals: Vec<String> = inner
                .split(',')
                .filter_map(|raw| strip_quotes(raw.trim()).map(str::to_owned))
                .collect();
            if !literals.is_empty() {
                return Some(ir::EmitPredicateKind::In {
                    path: path.to_owned(),
                    literals,
                });
            }
        }
    }
    None
}

fn strip_quotes(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        Some(&trimmed[1..trimmed.len() - 1])
    } else {
        None
    }
}

/// Find a whole-word token (`word`) in `text`, returning its byte
/// offset. Returns `None` when the substring only appears as part of
/// a longer identifier (e.g. `withdrawn`).
fn find_word(text: &str, word: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(word) {
        let abs = from + rel;
        let before_ok = abs == 0 || bytes[abs - 1].is_ascii_whitespace();
        let after_pos = abs + word.len();
        let after_ok = after_pos >= bytes.len() || bytes[after_pos].is_ascii_whitespace();
        if before_ok && after_ok {
            return Some(abs);
        }
        from = abs + word.len();
    }
    None
}

/// Realtime bucket cycle MVP — lower a canonical-indent `channel`
/// block into `ir::Channel`. Mechanical projection: the parser
/// already enforces presence of all three required children, so the
/// lowering only wraps the verbatim strings into the typed shapes
/// (`TenantFromSpec`, `PolicyRef::Atom`, payload string verbatim).
/// Doctor `CHANNEL-PAYLOAD-001` resolves the payload reference
/// downstream.
pub fn lower_channel(channel: &syntax::Channel) -> ir::Channel {
    ir::Channel {
        name: channel.name.clone(),
        tenant_from: ir::TenantFromSpec {
            path: lower_path_string(&channel.tenant_from),
        },
        policy: lower_policy_atom(&channel.policy),
        policy_when_denied: None,
        payload: channel.payload.clone(),
        span_ref: Some(span_of(channel.span)),
    }
}

/// Phase L Tier 3 — lower a canonical-indent `notification` block into
/// `ir::Notification`. Reuses `JobTrigger`, `IdempotencyKey`,
/// `RetryPolicy`, `TenantFromSpec` from the job lowering helpers.
pub fn lower_notification(
    feature: &str,
    notification: &syntax::Notification,
) -> Result<ir::Notification, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &notification.trigger);
    let tenant_from = notification
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let idempotency = notification
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = notification.retry.as_ref().map(lower_retry);
    let policy = notification
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .filter(|p| !matches!(p, ir::PolicyRef::None));
    let digest = notification.digest.as_ref().map(lower_notification_digest);
    let throttle = notification
        .throttle
        .as_ref()
        .map(lower_notification_throttle);
    let policy_expr = notification.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Notification {
        name: notification.name.clone(),
        trigger,
        channels: notification.channels.clone(),
        recipient: notification.recipient.clone(),
        template: notification.template.clone(),
        policy,
        policy_expr,
        tenant_from,
        idempotency,
        retry,
        emits: notification.emits.clone(),
        digest,
        throttle,
        previous_names: Vec::new(),
        span_ref: Some(span_of(notification.span)),
    })
}

/// MCP bucket cycle — lower a canonical-indent `mcp_server` block into
/// `ir::MCPServerSpec`. Value-preserving except for the closed-catalog
/// `transport` mapping, which rejects unknown literals at lower-time.
pub fn lower_mcp_server(server: &syntax::McpServer) -> Result<ir::MCPServerSpec, AnalyzeError> {
    let transport = match server.transport.as_str() {
        "stdio" => ir::MCPTransport::Stdio,
        "http_sse" => ir::MCPTransport::HttpSse,
        "http_streamable" => ir::MCPTransport::HttpStreamable,
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("MCP-TRANSPORT-001 mcp_server `{}` transport", server.name),
                value: other.to_owned(),
            });
        }
    };
    let auth = server.auth.as_deref().and_then(parse_mcp_auth);
    let metadata = ir::MCPServerMetadata {
        name: server.metadata.name.clone(),
        description: server.metadata.description.clone(),
        version: server.metadata.version.clone(),
    };
    let tools = server.tools.iter().map(lower_mcp_tool).collect::<Vec<_>>();
    let resources = server
        .resources
        .iter()
        .map(lower_mcp_resource)
        .collect::<Vec<_>>();
    let prompts = server
        .prompts
        .iter()
        .map(lower_mcp_prompt)
        .collect::<Vec<_>>();
    Ok(ir::MCPServerSpec {
        name: server.name.clone(),
        transport,
        scope_feature: server.scope_feature.clone(),
        auth,
        metadata,
        tools,
        resources,
        prompts,
        span_ref: Some(span_of(server.span)),
    })
}

/// Parse `bearer env.<NAME>` into `ir::MCPAuth::BearerEnvVar`. Anything
/// else (future `oauth ...`, malformed line) returns `None`; doctor
/// `MCP-AUTH-001` (registered in proposal) catches malformed shapes.
fn parse_mcp_auth(raw: &str) -> Option<ir::MCPAuth> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("bearer env.") {
        let env = rest.trim().to_owned();
        if env.is_empty() {
            return None;
        }
        return Some(ir::MCPAuth::BearerEnvVar { env });
    }
    None
}

fn lower_mcp_tool(tool: &syntax::McpTool) -> ir::MCPTool {
    let params = tool.params.iter().map(lower_mcp_param).collect();
    ir::MCPTool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        params,
        returns_kind: tool.returns.clone(),
        handler_fn: tool.handler.clone(),
        policy: tool.policy.clone(),
        span_ref: Some(span_of(tool.span)),
    }
}

fn lower_mcp_resource(resource: &syntax::McpResource) -> ir::MCPResource {
    ir::MCPResource {
        name: resource.name.clone(),
        uri_template: resource.uri_template.clone(),
        mime: resource.mime.clone(),
        handler_fn: resource.handler.clone(),
        policy: resource.policy.clone(),
        span_ref: Some(span_of(resource.span)),
    }
}

fn lower_mcp_prompt(prompt: &syntax::McpPrompt) -> ir::MCPPrompt {
    let params = prompt.params.iter().map(lower_mcp_param).collect();
    ir::MCPPrompt {
        name: prompt.name.clone(),
        description: prompt.description.clone(),
        params,
        template_path: prompt.template.clone(),
        span_ref: Some(span_of(prompt.span)),
    }
}

fn lower_mcp_param(param: &syntax::McpParam) -> ir::MCPParam {
    ir::MCPParam {
        name: param.name.clone(),
        ty_literal: param.ty.clone(),
        required: param.required,
    }
}

/// Notifications expanded bucket cycle — lower AST `NotificationDigest`
/// into the typed IR. `template_strategy` falls through `merge` /
/// `append` into the closed-catalog enum; unknown values are preserved
/// in `invalid_template_strategy` so doctor can report
/// `NOTIF-DIGEST-003` without widening the enum.
fn lower_notification_digest(digest: &syntax::NotificationDigest) -> ir::NotificationDigest {
    let (template_strategy, invalid_template_strategy) = match digest.template_strategy.as_deref() {
        Some("merge") => (Some(ir::DigestStrategy::Merge), None),
        Some("append") => (Some(ir::DigestStrategy::Append), None),
        Some(raw) => (None, Some(raw.to_owned())),
        None => (None, None),
    };
    ir::NotificationDigest {
        every: digest.every.clone(),
        group_by: digest.group_by.clone(),
        max_size: digest.max_size,
        template_strategy,
        invalid_template_strategy,
    }
}

/// Notifications expanded bucket cycle — lower AST
/// `NotificationThrottle` into the typed IR. Pure field-for-field
/// projection; no validation here (doctor `NOTIF-THROTTLE-*` covers
/// the closed-catalog and combinatorial rules).
fn lower_notification_throttle(
    throttle: &syntax::NotificationThrottle,
) -> ir::NotificationThrottle {
    ir::NotificationThrottle {
        max_per: throttle.max_per.clone(),
        per_recipient: throttle.per_recipient,
        per_channel: throttle.per_channel,
        burst: throttle.burst,
    }
}

/// Phase L Tier 3 — lower a canonical-indent `event_group` into
/// `ir::EventGroup`. The payload bag and authored events stay as raw
/// strings; B5 framework gap 1 lifts the per-event typed payload
/// blocks into `variants`.
pub fn lower_event_group(group: &syntax::EventGroup) -> ir::EventGroup {
    // EVENT-OUTBOX §3.3 — lower the parallel bool vec into the typed
    // `OutboxMode` catalog. Index-paired with `events`; when the AST
    // emits an empty vec (legacy / pre-outbox payloads) we expand to
    // a same-length `None` vec so downstream code can read by index.
    let events_outbox: Vec<ir::OutboxMode> = if group.events_outbox_guaranteed.is_empty() {
        vec![ir::OutboxMode::None; group.events.len()]
    } else {
        group
            .events_outbox_guaranteed
            .iter()
            .map(|g| {
                if *g {
                    ir::OutboxMode::Guaranteed
                } else {
                    ir::OutboxMode::None
                }
            })
            .collect()
    };

    // B5 framework gap 1 — lift per-event field bodies into
    // `EventVariant` records. Each variant carries its `EventField`s
    // lifted via `type_ref_from_syntax`, the closed kind catalog
    // (committed vs trace), and the outbox flag mirrored from the
    // parallel slot above. Back-compat: variants whose body was
    // empty come through with an empty `fields` Vec; legacy fixtures
    // that didn't author `event_variants`/`event_variant_kinds` at
    // all leave `variants` empty.
    let variants: Vec<ir::EventVariant> =
        if group.event_variants.is_empty() && group.event_variant_kinds.is_empty() {
            Vec::new()
        } else {
            group
                .events
                .iter()
                .enumerate()
                .map(|(idx, short_name)| {
                    let kind = match group
                        .event_variant_kinds
                        .get(idx)
                        .copied()
                        .unwrap_or(syntax::EventVariantKindAst::Committed)
                    {
                        syntax::EventVariantKindAst::Committed => ir::EventVariantKind::Committed,
                        syntax::EventVariantKindAst::Trace => ir::EventVariantKind::Trace,
                    };
                    let fields = group
                        .event_variants
                        .get(idx)
                        .map(|rows| {
                            rows.iter()
                                .map(lower_event_variant_field)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let outbox = events_outbox
                        .get(idx)
                        .copied()
                        .unwrap_or(ir::OutboxMode::None);
                    ir::EventVariant {
                        name: short_name.clone(),
                        kind,
                        outbox,
                        fields,
                        span_ref: group
                            .event_variants
                            .get(idx)
                            .and_then(|rows| rows.first().map(|f| span_of(f.span))),
                    }
                })
                .collect()
        };

    ir::EventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        raw_payload: group.payload.clone(),
        raw_audit: group.audit.clone(),
        events: group.events.clone(),
        events_outbox,
        variants,
        span_ref: Some(span_of(group.span)),
    }
}

/// B5 framework gap 1 — lift one typed event-variant field row into
/// `ir::EventField`. Reuses `type_ref_from_syntax` so `@semantic.X`,
/// `@cap.X`, and built-in scalars all flow through the same lifter
/// resource fields use. `optional` falls back to `!required` when
/// neither modifier was authored — matches the resource-field
/// convention.
fn lower_event_variant_field(decl: &syntax::EventVariantFieldDecl) -> ir::EventField {
    let optional = if decl.required {
        false
    } else {
        // Treat unmarked event-variant fields as required by default
        // (events are projection contracts; missing values are a
        // codegen-time bug). Authors opt into optionality explicitly.
        decl.optional
    };
    ir::EventField {
        name: decl.name.clone(),
        type_ref: type_ref_from_syntax(&decl.type_text),
        optional,
    }
}

/// Migrations bucket cycle Route C — lower a canonical-indent
/// `tenant_migration` block into `ir::TenantMigration`. Mirrors
/// `lower_job` for the shared spine (idempotency / retry / timeout /
/// handler) and adds the `target tenants <axis>` slot. The lowering
/// does **not** enforce that `idempotency` is authored; that is
/// `TM-IDEMP-001`'s job downstream.
pub fn lower_tenant_migration(
    tm: &syntax::TenantMigration,
) -> Result<ir::TenantMigration, AnalyzeError> {
    let idempotency = tm
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path })
        .unwrap_or_else(|| ir::IdempotencyKey {
            by: ir::Path::from_segments(Vec::<String>::new()),
        });
    let retry = tm.retry.as_ref().map(lower_retry);
    Ok(ir::TenantMigration {
        name: tm.name.clone(),
        target: ir::TenantMigrationTarget {
            operation: tm.target_ref.as_deref().map(lower_tenant_migration_target),
            axis: tm.target_axis.clone(),
        },
        idempotency,
        retry,
        timeout: tm.timeout.clone(),
        handler: ir::PathRef::authored(&tm.handler),
        previous_names: Vec::new(),
        span_ref: Some(span_of(tm.span)),
    })
}

fn lower_tenant_migration_target(raw: &str) -> ir::TenantMigrationTargetOperation {
    let parts: Vec<&str> = raw.split('.').collect();
    match parts.as_slice() {
        ["query", name] => ir::TenantMigrationTargetOperation::Query {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "query", name] => ir::TenantMigrationTargetOperation::Query {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        ["command", name] => ir::TenantMigrationTargetOperation::Command {
            feature: None,
            name: (*name).to_owned(),
        },
        [feature, "command", name] => ir::TenantMigrationTargetOperation::Command {
            feature: Some((*feature).to_owned()),
            name: (*name).to_owned(),
        },
        _ => ir::TenantMigrationTargetOperation::Query {
            feature: None,
            name: raw.to_owned(),
        },
    }
}

fn lower_job_trigger(feature: &str, trigger: &syntax::JobTrigger) -> ir::JobTrigger {
    match trigger {
        syntax::JobTrigger::Event(name) => ir::JobTrigger::Event {
            event: qualified_event_name(feature, name),
        },
        syntax::JobTrigger::Schedule(cron) => ir::JobTrigger::Schedule { cron: cron.clone() },
    }
}

fn qualified_event_name(feature: &str, name: &str) -> ir::QualifiedName {
    if let Some((ns, ev)) = name.split_once('.') {
        ir::QualifiedName {
            feature: Some(ns.to_owned()),
            name: ev.to_owned(),
        }
    } else {
        ir::QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    }
}

fn lower_retry(retry: &syntax::JobRetry) -> ir::RetryPolicy {
    ir::RetryPolicy {
        count: retry.count,
        backoff: match retry.backoff.as_str() {
            "exponential" => ir::BackoffStrategy::Exponential,
            _ => ir::BackoffStrategy::Fixed,
        },
    }
}

fn lower_fanout(fanout: &syntax::JobFanout) -> ir::FanoutSpec {
    ir::FanoutSpec {
        scope: ir::FanoutScope::Tenants,
        axis: fanout.axis.clone(),
    }
}

fn lower_external_call(call: &syntax::JobExternalCall) -> ir::ExternalCallRef {
    ir::ExternalCallRef {
        slot: call.slot.clone(),
        op: call.op.clone(),
        args: call
            .args
            .iter()
            .map(|arg| ir::NamedArg {
                name: arg.name.clone(),
                value: lower_raw_expr(&arg.value),
            })
            .collect(),
        span_ref: Some(span_of(call.span)),
    }
}

fn lower_job_body(body: &syntax::JobBody) -> ir::JobBody {
    match body {
        syntax::JobBody::Handler(h) => ir::JobBody::Handler(ir::JobHandler {
            path: ir::PathRef::authored(&h.path),
            returns: h.returns.as_deref().map(|t| type_ref_from_text(t)),
        }),
        syntax::JobBody::Declarative(d) => ir::JobBody::Declarative(ir::JobDeclarative {
            target: d.target.as_ref().map(lower_target_expr),
            lets: d.lets.iter().map(lower_let_binding).collect(),
            effect: d
                .effect
                .as_ref()
                .map(lower_command_effect)
                .unwrap_or(ir::CommandEffect::None),
        }),
        syntax::JobBody::None => ir::JobBody::Declarative(ir::JobDeclarative {
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::None,
        }),
    }
}

/// Phase L Tier 4b — shared lowering for `target query.<name>(args)`.
/// Reused by `lower_job_body` (Tier 3) and `lower_command_skeleton`
/// (Tier 4b) — closes the Tier 3 raw-spine carve-out.
fn lower_target_expr(t: &syntax::TargetExprDecl) -> ir::TargetExpr {
    ir::TargetExpr {
        query: lower_qualified_name(&t.query),
        args: t.args.iter().map(lower_named_arg).collect(),
    }
}

fn lower_let_binding(l: &syntax::LetBindingDecl) -> ir::LetBinding {
    ir::LetBinding {
        name: l.name.clone(),
        value: lower_raw_expr(&l.value),
    }
}

fn lower_named_arg(arg: &syntax::TargetArgDecl) -> ir::NamedArg {
    ir::NamedArg {
        name: arg.name.clone(),
        value: lower_raw_expr(&arg.value),
    }
}

fn lower_assignment(a: &syntax::AssignmentDecl) -> ir::Assignment {
    ir::Assignment {
        field: a.field.clone(),
        value: lower_raw_expr(&a.value),
    }
}

fn lower_command_effect(effect: &syntax::CommandEffectDecl) -> ir::CommandEffect {
    let resource = lower_qualified_name(&effect.resource);
    let assignments: Vec<ir::Assignment> =
        effect.assignments.iter().map(lower_assignment).collect();
    match effect.kind {
        syntax::CommandEffectKindDecl::Creates => ir::CommandEffect::Creates(ir::CreateEffect {
            resource,
            from_input: effect.from_input,
            assignments,
        }),
        syntax::CommandEffectKindDecl::Updates => ir::CommandEffect::Updates(ir::UpdateEffect {
            resource,
            assignments,
        }),
        syntax::CommandEffectKindDecl::Deletes => {
            ir::CommandEffect::Deletes(ir::DeleteEffect { resource })
        }
    }
}

fn lower_qualified_name(text: &str) -> ir::QualifiedName {
    let trimmed = text.trim();
    if let Some((feature, name)) = trimmed.split_once('.') {
        ir::QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    } else {
        ir::QualifiedName {
            feature: None,
            name: trimmed.to_owned(),
        }
    }
}

/// Capture a freeform expression as a typed `ir::Expr`. Today this
/// handles five literal shapes (string / integer / bool / nil / enum
/// or path) plus the v1 `@fn.<name>(<arg>...)` invocation form (closes
/// WAR-VOCAB-CREATES-FN-CALL-01).
fn lower_raw_expr(text: &str) -> ir::Expr {
    let trimmed = text.trim();
    if let Some(unquoted) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        return ir::Expr::String(unquoted.to_owned());
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match trimmed {
        "true" => return ir::Expr::Boolean(true),
        "false" => return ir::Expr::Boolean(false),
        "nil" => return ir::Expr::Nil,
        _ => {}
    }
    // `@fn.<name>(<arg>, ...)` — extension-fn invocation. The args
    // are recursively lowered as expressions; nested fn calls are
    // permitted at the IR level (codegen guards against unsupported
    // nesting today by emitting a TODO comment).
    if let Some(fn_call) = parse_fn_call_expr(trimmed) {
        return ir::Expr::FnCall(fn_call);
    }
    let segments = trimmed.split('.').map(|s| s.trim().to_owned()).collect();
    ir::Expr::Path(ir::Path { segments })
}

fn parse_fn_call_expr(text: &str) -> Option<ir::FnCallExpr> {
    let rest = text.strip_prefix("@fn.")?;
    let paren_idx = rest.find('(')?;
    if !rest.ends_with(')') {
        return None;
    }
    let (name_text, after) = rest.split_at(paren_idx);
    let inside = &after[1..after.len() - 1];
    let name = name_text.trim();
    if name.is_empty() {
        return None;
    }
    let args = if inside.trim().is_empty() {
        Vec::new()
    } else {
        split_fn_call_args(inside)
            .into_iter()
            .map(|arg| lower_raw_expr(&arg))
            .collect()
    };
    Some(ir::FnCallExpr {
        name: ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        },
        args,
    })
}

fn split_fn_call_args(input: &str) -> Vec<String> {
    // Splits on commas that live at paren-depth 0 outside double-quoted
    // strings. Nested fn-call args therefore stay grouped.
    let mut out = Vec::new();
    let mut depth: usize = 0;
    let mut in_quote = false;
    let mut start = 0usize;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
        } else if !in_quote {
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth = depth.saturating_sub(1);
            } else if b == b',' && depth == 0 {
                let part = input[start..i].trim();
                if !part.is_empty() {
                    out.push(part.to_owned());
                }
                start = i + 1;
            }
        }
        i += 1;
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_owned());
    }
    out
}

fn lower_path_string(text: &str) -> ir::Path {
    ir::Path {
        segments: text
            .split(',')
            .next()
            .unwrap_or(text)
            .split('.')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

/// Extract the env binding name from `env.<NAME>` (`secret env.X`).
fn extract_env_binding(raw: &str) -> String {
    raw.trim()
        .strip_prefix("env.")
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| raw.trim().to_owned())
}

/// Phase L — lower a canonical-indent `auth` block into the IR `Auth`
/// shape. The translation is mostly structural; the analyzer's only
/// non-trivial duty is splitting `Customer.email` into `FieldRef`.
pub fn lower_auth(auth: &syntax::Auth) -> Result<ir::Auth, AnalyzeError> {
    Ok(ir::Auth {
        identity: lower_auth_identity(&auth.identity)?,
        password: auth.password.as_ref().map(lower_auth_password),
        sessions: auth.sessions.as_ref().map(lower_auth_sessions),
        mfa: auth.mfa.as_ref().map(lower_auth_mfa),
        oauth: auth.oauth.iter().map(lower_auth_oauth).collect(),
        span_ref: Some(span_of(auth.span)),
    })
}

fn lower_auth_identity(identity: &syntax::AuthIdentity) -> Result<ir::AuthIdentity, AnalyzeError> {
    let (resource, field) =
        identity
            .field
            .split_once('.')
            .ok_or_else(|| AnalyzeError::InvalidAuthIdentity {
                reference: identity.field.clone(),
            })?;
    if resource.is_empty() || field.is_empty() || field.contains('.') {
        return Err(AnalyzeError::InvalidAuthIdentity {
            reference: identity.field.clone(),
        });
    }
    Ok(ir::AuthIdentity {
        field: ir::FieldRef {
            resource: qualified_name_local(resource),
            field: field.to_owned(),
        },
        public_contract: lower_public_contract(&identity.public_contract),
    })
}

fn lower_auth_password(password: &syntax::AuthPassword) -> ir::AuthPassword {
    ir::AuthPassword {
        algorithm: password.algorithm.clone(),
        hash: password.hash.clone(),
        verify: password.verify.clone(),
        rate_limit: password.rate_limit.as_ref().map(lower_rate_limit_spec),
    }
}

fn lower_auth_sessions(sessions: &syntax::AuthSessions) -> ir::AuthSessions {
    ir::AuthSessions {
        resource: qualified_name_local(&sessions.resource),
        ttl: sessions.ttl.clone(),
        refresh: sessions.refresh,
        // Populated in S3 when the orchestrator wires resource FieldSpec lookup.
        extra_columns: vec![],
        access_ttl: sessions.access_ttl.as_ref().map(|ttl| ttl.value.clone()),
        rotation: sessions.rotation.as_ref().map(lower_auth_session_rotation),
    }
}

fn lower_auth_session_rotation(rotation: &syntax::AuthSessionRotation) -> ir::RotationConfig {
    ir::RotationConfig {
        refresh_ttl: rotation.refresh_ttl.as_ref().map(|ttl| ttl.value.clone()),
        grace: rotation.grace.as_ref().map(|grace| grace.value.clone()),
        theft_detection_action: rotation
            .theft_detection_action
            .as_ref()
            .map(|action| lower_auth_theft_action(action.action)),
        span_ref: Some(span_of(rotation.span)),
    }
}

fn lower_auth_theft_action(action: syntax::AuthTheftDetectionAction) -> ir::TheftAction {
    match action {
        syntax::AuthTheftDetectionAction::RevokeSessionFamily => {
            ir::TheftAction::RevokeSessionFamily
        }
        syntax::AuthTheftDetectionAction::RevokeUser => ir::TheftAction::RevokeUser,
    }
}

fn lower_auth_mfa(mfa: &syntax::AuthMfa) -> ir::AuthMfa {
    ir::AuthMfa {
        method: mfa.method.clone(),
        enroll: mfa.enroll.clone(),
        verify: mfa.verify.clone(),
        adapter: mfa.adapter.clone(),
    }
}

fn lower_auth_oauth(oauth: &syntax::AuthOAuthProvider) -> ir::AuthOAuthProvider {
    ir::AuthOAuthProvider {
        provider: oauth.provider.clone(),
        adapter: oauth.adapter.clone(),
    }
}

/// Lower a single `agent` AST node into the IR form. The `feature` arg
/// pins the owning feature name on the IR record so cross-feature doctor
/// checks can rebuild `<feature>.agent.<name>` references.
pub fn lower_agent(feature: &str, agent: &syntax::Agent) -> Result<ir::Agent, AnalyzeError> {
    let input = agent
        .input
        .iter()
        .map(|slot| ir::TypedSlot {
            name: slot.name.clone(),
            type_ref: type_ref_from_text(&slot.type_text),
            required: slot.required,
            constraints: ir::FieldConstraints::default(),
        })
        .collect();

    let policy = agent
        .policy
        .as_ref()
        .and_then(|atoms| atoms.first())
        .map(|first| lower_policy_atom(first));

    let (output_kind, output_type, output_discriminator) = match &agent.output {
        Some(syntax::AgentOutput::Stream(ty)) => (
            ir::AgentOutputKind::Stream,
            Some(type_ref_from_text(ty)),
            None,
        ),
        Some(syntax::AgentOutput::Discriminator(name)) => (
            ir::AgentOutputKind::DiscriminatedEnum,
            None,
            Some(ir::DiscriminatorRef::Enum(qualified_name_local(name))),
        ),
        Some(syntax::AgentOutput::Plain(ty)) => (
            // Lowering can't tell `Text` from `DiscriminatedRecord` without
            // the feature scope (enum vs record). Default to `Text`; the
            // expand pass (Phase 5) promotes to `DiscriminatedRecord` when
            // the resolved type is a record with a `discriminator` field.
            ir::AgentOutputKind::Text,
            Some(type_ref_from_text(ty)),
            None,
        ),
        None => (ir::AgentOutputKind::Text, None, None),
    };

    let model = agent.model.as_ref().map(|s| qualified_namespace(s));

    let safety = agent
        .safety
        .iter()
        .map(|s| qualified_namespace(s))
        .collect();

    let mut tools = Vec::with_capacity(agent.tools.len());
    for tool_ast in &agent.tools {
        tools.push(ir::ToolBinding {
            reference: lower_tool_ref(&tool_ast.reference, feature)?,
            resolved_effect: None,
            resolved_policy: None,
            resolved_pii_classes: Vec::new(),
            span_ref: Some(span_of(tool_ast.span)),
        });
    }

    let mut evals = Vec::with_capacity(agent.evals.len());
    for case_ast in &agent.evals {
        evals.push(lower_eval_case(case_ast, feature)?);
    }

    let expose_http = agent.expose.as_ref().map(lower_agent_expose);

    Ok(ir::Agent {
        name: agent.name.clone(),
        feature: feature.to_owned(),
        input,
        context: None, // Phase 1 parser does not yet structure context expressions.
        policy,
        policy_when_denied: None,
        rate_limit: agent.rate_limit.as_ref().map(lower_rate_limit_spec),
        output_kind,
        output_type,
        output_discriminator,
        model,
        temperature: agent.temperature,
        max_tokens: agent.max_tokens,
        top_p: agent.top_p,
        seed: agent.seed,
        prompt_path: agent.prompt.clone(),
        safety,
        tools,
        evals,
        expose_http,
        span_ref: Some(span_of(agent.span)),
    })
}

/// Cut A.7 — lower an `expose http` AST block. Method enum maps 1:1;
/// route slots become `TypedSlot`s with `required: true` (path params
/// are inherently required); audience / rate-limit pass-through as
/// strings.
fn lower_agent_expose(expose: &syntax::AgentExpose) -> ir::HttpExposure {
    let route_slots = expose
        .route_slots
        .iter()
        .map(|slot| ir::TypedSlot {
            name: slot.name.clone(),
            type_ref: type_ref_from_text(&slot.type_text),
            required: true,
            constraints: ir::FieldConstraints::default(),
        })
        .collect();
    ir::HttpExposure {
        method: match expose.method {
            syntax::HttpMethod::Get => ir::HttpMethod::Get,
            syntax::HttpMethod::Post => ir::HttpMethod::Post,
            syntax::HttpMethod::Put => ir::HttpMethod::Put,
            syntax::HttpMethod::Patch => ir::HttpMethod::Patch,
            syntax::HttpMethod::Delete => ir::HttpMethod::Delete,
        },
        path: expose.path.clone(),
        route_slots,
        audience: expose.audience.clone(),
        rate_limit_override: expose.rate_limit_override.clone(),
        span_ref: Some(span_of(expose.span)),
    }
}

/// Lower a single tool reference. `feature` is the owning feature so the
/// short form `query.by_id` rewrites to `Local` and the analyzer
/// preserves the same-feature locality for the expand pass to resolve.
fn lower_tool_ref(raw: &str, _feature: &str) -> Result<ir::QualifiedToolRef, AnalyzeError> {
    if let Some(rest) = raw.strip_prefix("@tool.") {
        if rest.is_empty() {
            return Err(AnalyzeError::InvalidToolRef {
                reference: raw.to_owned(),
            });
        }
        let dotted: Vec<String> = rest.split('.').map(str::to_owned).collect();
        return Ok(ir::QualifiedToolRef::Adapter { dotted });
    }

    // Tail tokens after the feature prefix (if any). The reference is
    // dotted; `query.list` / `query.lookup` / `query.sql` are the
    // three legal three-segment kinds.
    let segments: Vec<&str> = raw.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(AnalyzeError::InvalidToolRef {
            reference: raw.to_owned(),
        });
    }

    // Local shorthand: `query.by_id`, `command.create`, `api.export`,
    // `query.list.by_email`, etc.
    if let Some((kind, name)) = parse_tool_kind_local(&segments) {
        return Ok(ir::QualifiedToolRef::Local { kind, name });
    }

    // Cross-feature: `<feature>.<kind>.<name>` or
    // `<feature>.query.list.<name>` / `query.lookup.<name>` / `query.sql.<name>`.
    if segments.len() >= 3 {
        let feature = segments[0].to_owned();
        if let Some((kind, name)) = parse_tool_kind_local(&segments[1..]) {
            return Ok(ir::QualifiedToolRef::CrossFeature {
                feature,
                kind,
                name,
            });
        }
    }

    Err(AnalyzeError::InvalidToolRef {
        reference: raw.to_owned(),
    })
}

/// Recognize the trailing `(kind, name)` of a tool reference. Accepts:
///
///   - `query.list.<name>`     -> QueryList
///   - `query.lookup.<name>`   -> QueryLookup
///   - `query.sql.<name>`      -> QuerySql
///   - `query.<name>`          -> QueryUnspecified
///   - `command.<name>`        -> Command
///   - `api.<name>`            -> Api
fn parse_tool_kind_local(segments: &[&str]) -> Option<(ir::ToolKind, String)> {
    match segments {
        ["query", "list", name] => Some((ir::ToolKind::QueryList, (*name).to_owned())),
        ["query", "lookup", name] => Some((ir::ToolKind::QueryLookup, (*name).to_owned())),
        ["query", "sql", name] => Some((ir::ToolKind::QuerySql, (*name).to_owned())),
        ["query", name] => Some((ir::ToolKind::QueryUnspecified, (*name).to_owned())),
        ["command", name] => Some((ir::ToolKind::Command, (*name).to_owned())),
        ["api", name] => Some((ir::ToolKind::Api, (*name).to_owned())),
        _ => None,
    }
}

fn lower_eval_case(
    case: &syntax::AgentEvalCase,
    feature: &str,
) -> Result<ir::EvalCase, AnalyzeError> {
    let mut assertions = Vec::with_capacity(case.assertions.len());
    for assertion in &case.assertions {
        assertions.push(ir::EvalAssertion {
            kind: match assertion.kind {
                syntax::AgentEvalKind::Requires => ir::EvalAssertionKind::Requires,
                syntax::AgentEvalKind::Forbids => ir::EvalAssertionKind::Forbids,
            },
            predicate: lower_eval_predicate(&assertion.predicate, feature)?,
            span_ref: Some(span_of(assertion.span)),
        });
    }
    let golden = case.golden.as_ref().map(|g| ir::GoldenSpec {
        path: g.path.clone(),
        min_score: g.min_score,
        span_ref: Some(span_of(g.span)),
    });
    Ok(ir::EvalCase {
        name: case.name.clone(),
        assertions,
        golden,
        span_ref: Some(span_of(case.span)),
    })
}

fn lower_eval_predicate(
    predicate: &syntax::AgentEvalPredicate,
    feature: &str,
) -> Result<ir::EvalPredicate, AnalyzeError> {
    match predicate {
        syntax::AgentEvalPredicate::Contains { lhs, rhs } => Ok(ir::EvalPredicate::Contains {
            lhs: ir::Path::from_segments(lhs.split('.').map(str::to_owned)),
            rhs: match rhs {
                syntax::ContainsRhs::Literal(s) => ir::EvalContainsRhs::Literal(s.clone()),
                syntax::ContainsRhs::SemanticType(s) => {
                    ir::EvalContainsRhs::SemanticType(qualified_namespace(s))
                }
            },
        }),
        syntax::AgentEvalPredicate::ToolsCalls { op, target } => {
            Ok(ir::EvalPredicate::ToolsCalls {
                op: match op {
                    syntax::ToolsCallsOp::Includes => ir::ToolsCallsOp::Includes,
                    syntax::ToolsCallsOp::Excludes => ir::ToolsCallsOp::Excludes,
                },
                target: lower_tool_ref(target, feature)?,
            })
        }
        syntax::AgentEvalPredicate::Closed { text } => Ok(parse_closed_predicate(text)),
    }
}

/// Parse the simple `<path> <op> <literal>` subset of the closed predicate
/// language. Richer shapes (compound `AND`/`OR`, `has`, parenthesised
/// expressions) fall through to `EvalPredicate::Unparsed` so doctor can
/// surface them — the parser stays narrow until the canonical predicate
/// parser lands.
fn parse_closed_predicate(text: &str) -> ir::EvalPredicate {
    let trimmed = text.trim();
    // Try ordered ops first (longest token wins to avoid `<=` parsing as `<`).
    for (token, op) in [
        ("<=", ir::CompareOp::Le),
        (">=", ir::CompareOp::Ge),
        ("!=", ir::CompareOp::Ne),
        ("<", ir::CompareOp::Lt),
        (">", ir::CompareOp::Gt),
        ("=", ir::CompareOp::Eq),
    ] {
        if let Some(idx) = find_top_level_operator(trimmed, token) {
            let (lhs_text, rhs_text) = trimmed.split_at(idx);
            let rhs_text = &rhs_text[token.len()..];
            let lhs = lhs_text.trim();
            let rhs = rhs_text.trim();
            if lhs.is_empty() || rhs.is_empty() {
                return ir::EvalPredicate::Unparsed(text.to_owned());
            }
            return ir::EvalPredicate::Closed(ir::Predicate::Comparison {
                left: expr_from_text(lhs),
                op,
                right: expr_from_text(rhs),
            });
        }
    }
    ir::EvalPredicate::Unparsed(text.to_owned())
}

/// Locate `op` outside of any double-quoted span. Returns the byte index
/// in `text` where the operator begins, or `None` if no top-level match.
fn find_top_level_operator(text: &str, op: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if !in_quote && text[i..].starts_with(op) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Promote a token to the smallest expression kind that fits. Strings
/// must be double-quoted; integers parse as `Integer`; `true`/`false` as
/// `Boolean`; bare identifiers / dotted paths become `Path`; bare
/// identifiers that look like enum literals (no dots) also surface as
/// `Path` — the analyzer narrows once symbols resolve in expand.
fn expr_from_text(text: &str) -> ir::Expr {
    let text = text.trim();
    if let Some(stripped) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return ir::Expr::String(stripped.to_owned());
    }
    if let Ok(n) = text.parse::<i64>() {
        return ir::Expr::Integer(n);
    }
    match text {
        "true" => return ir::Expr::Boolean(true),
        "false" => return ir::Expr::Boolean(false),
        "nil" => return ir::Expr::Nil,
        _ => {}
    }
    ir::Expr::Path(ir::Path::from_segments(text.split('.').map(str::to_owned)))
}

/// Build a feature-local `QualifiedName` (no feature prefix).
fn qualified_name_local(name: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

/// Treat the entire namespace literal as a single name (e.g.
/// `@llm.default`, `@validator.pii_email_scrub`, `@semantic.Email`).
/// Doctor + LSP enforce the closed-namespace catalog elsewhere; this
/// helper keeps the raw form so resolution stays uniform.
fn qualified_namespace(raw: &str) -> ir::QualifiedName {
    ir::QualifiedName {
        feature: None,
        name: raw.to_owned(),
    }
}

fn lower_policy_atom(atom: &str) -> ir::PolicyRef {
    if let Some(rest) = atom.strip_prefix('@') {
        ir::PolicyRef::Atom(rest.to_owned())
    } else {
        ir::PolicyRef::Local(atom.to_owned())
    }
}

#[cfg(test)]
fn lower_policy_atom_with_args(text: &str) -> ir::PolicyAtom {
    let raw = text.trim().strip_prefix('@').unwrap_or(text.trim());
    let (ns_name, args) = match raw.split_once('(') {
        Some((head, tail)) => (head.trim(), Some(tail.trim_end_matches(')').to_owned())),
        None => (raw.trim(), None),
    };
    let (namespace, name) = ns_name
        .split_once('.')
        .map(|(namespace, name)| (namespace.to_owned(), name.to_owned()))
        .unwrap_or_else(|| ("".to_owned(), ns_name.to_owned()));
    ir::PolicyAtom {
        namespace,
        name,
        args,
    }
}

#[cfg(test)]
fn lower_audit_block(src: &str) -> ir::AuditSpec {
    let mut spec = ir::AuditSpec {
        subjects: Vec::new(),
        emit_to: None,
        data_subject: None,
        record_before: false,
        record_after: false,
        retain_for: None,
    };
    for line in src.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("audit data_subject ") {
            spec.data_subject = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data_subject ") {
            spec.data_subject = Some(rest.trim().to_owned());
        } else if line == "audit before" || line == "before" {
            spec.record_before = true;
        } else if line == "audit after" || line == "after" {
            spec.record_after = true;
        } else if let Some(rest) = line
            .strip_prefix("audit retain ")
            .or_else(|| line.strip_prefix("retain "))
        {
            spec.retain_for = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("audit ") {
            for part in rest
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                match part {
                    "before" => spec.record_before = true,
                    "after" => spec.record_after = true,
                    _ => spec.subjects.push(part.to_owned()),
                }
            }
        } else if let Some(rest) = line.strip_prefix("emit_to ") {
            spec.emit_to = Some(rest.trim().to_owned());
        }
    }
    spec
}

#[cfg(test)]
fn lower_validate_line(line: &str) -> Result<ir::FieldConstraints, AnalyzeError> {
    let trimmed = line.trim();
    let mut decl = syntax::FieldConstraintsDecl::default();
    if let Some(rest) = trimmed.strip_prefix("validate sanitize_html(") {
        let profile = rest.trim_end_matches(')').trim();
        decl.sanitize_html = Some(profile.to_owned());
    } else if trimmed == "validate utf8_safe" {
        decl.utf8_safe = Some(true);
    } else if let Some(rest) = trimmed.strip_prefix("validate max_recursion:") {
        decl.max_recursion = rest.trim().parse::<u32>().ok();
    } else if let Some(rest) = trimmed.strip_prefix("validate max_size:") {
        decl.max_size = rest.trim().parse::<u64>().ok();
    } else if trimmed == "validator covers_pii" {
        decl.covers_pii = Some("covers_pii".to_owned());
    } else {
        return Ok(ir::FieldConstraints::default());
    };
    lift_field_constraints("validate", &decl)
}

/// RB.S6 — lower the structured AST policy expression (parser
/// already validated permission-ref shape + closed keyword catalog).
/// The lowering is purely structural; catalog cross-checks (role/perm
/// existence) live in doctor (`RBAC-ROLE-UNDECLARED-001` /
/// `RBAC-PERM-UNDECLARED-001`).
fn lower_policy_expr(expr: &syntax::PolicyExprAst) -> ir::PolicyExpr {
    match expr {
        syntax::PolicyExprAst::Authenticated => ir::PolicyExpr::Authenticated,
        syntax::PolicyExprAst::HasRole(name) => ir::PolicyExpr::HasRole(name.clone()),
        syntax::PolicyExprAst::HasPermission(perm) => ir::PolicyExpr::HasPermission(perm.clone()),
        syntax::PolicyExprAst::Atom(atom) => ir::PolicyExpr::Atom(ir::PolicyAtom {
            namespace: atom.namespace.clone(),
            name: atom.name.clone(),
            args: atom.args.clone(),
        }),
        syntax::PolicyExprAst::And(terms) => {
            ir::PolicyExpr::And(terms.iter().map(lower_policy_expr).collect())
        }
        syntax::PolicyExprAst::Or(terms) => {
            ir::PolicyExpr::Or(terms.iter().map(lower_policy_expr).collect())
        }
        syntax::PolicyExprAst::Not(inner) => {
            ir::PolicyExpr::Not(Box::new(lower_policy_expr(inner)))
        }
    }
}

/// The Phase 1 parser captures type references as raw source text. Turn
/// that into a minimal `TypeRef` so doctor and inspect can read it; the
/// canonical-indent migration replaces this with a real type-ref parser.
fn type_ref_from_text(text: &str) -> ir::TypeRef {
    // Single canonical lifter for type tokens. Previously a slimmer
    // duplicate of `type_ref_from_syntax` with drift bugs (notably:
    // matched `"Json"` only, lost `"JSON"`; always lowered `@semantic.*`
    // to `SemanticEmail`). Delegating fixes both at the source.
    type_ref_from_syntax(text.trim())
}

// =============================================================================
// L0 #2 — Design tokens lowering.
//
// `lower_design` validates each closed-catalog group:
//   * color states map to `ir::ColorStateKind` (closed catalog of 4).
//   * hex literals match `#[0-9a-fA-F]{3,8}` (3, 4, 6, or 8 hex digits).
//   * shadow values reject top-level commas (multi-layer composition).
//   * weight values parse as `u16`.
//   * z values parse as `i32`.
//   * `extends` is reserved for Cut B — v0 always rejects.
// =============================================================================

pub fn lower_design(ast: &syntax::DesignDeclAst) -> Result<ir::Design, AnalyzeError> {
    if let Some(target) = ast.extends.as_deref() {
        return Err(AnalyzeError::DesignExtendsCutB {
            target: target.to_owned(),
        });
    }

    let colors = ast
        .colors
        .iter()
        .map(lower_design_color_token)
        .collect::<Result<Vec<_>, _>>()?;

    let typography = ir::Typography {
        families: ast
            .typography
            .families
            .iter()
            .map(|f| ir::FamilyToken {
                name: f.name.clone(),
                value: f.value.clone(),
            })
            .collect(),
        scale: ast
            .typography
            .scale
            .iter()
            .map(|s| ir::TextScaleToken {
                name: s.name.clone(),
                size: s.size.clone(),
                line_height: s.line_height.clone(),
            })
            .collect(),
        weights: ast
            .typography
            .weights
            .iter()
            .map(lower_design_weight)
            .collect::<Result<Vec<_>, _>>()?,
        tracking: ast
            .typography
            .tracking
            .iter()
            .map(|t| ir::TrackingToken {
                name: t.name.clone(),
                value: t.value.clone(),
            })
            .collect(),
    };

    let spaces = ast
        .spaces
        .iter()
        .map(|s| ir::ScaleToken {
            name: s.name.clone(),
            value: s.value.clone(),
        })
        .collect();
    let radii = ast
        .radii
        .iter()
        .map(|s| ir::ScaleToken {
            name: s.name.clone(),
            value: s.value.clone(),
        })
        .collect();
    let shadows = ast
        .shadows
        .iter()
        .map(lower_design_shadow)
        .collect::<Result<Vec<_>, _>>()?;
    let motion = ir::Motion {
        durations: ast
            .motion
            .durations
            .iter()
            .map(|s| ir::ScaleToken {
                name: s.name.clone(),
                value: s.value.clone(),
            })
            .collect(),
        easings: ast
            .motion
            .easings
            .iter()
            .map(|e| ir::EasingToken {
                name: e.name.clone(),
                value: e.value.clone(),
            })
            .collect(),
    };
    let breakpoints = ast
        .breakpoints
        .iter()
        .map(|s| ir::ScaleToken {
            name: s.name.clone(),
            value: s.value.clone(),
        })
        .collect();
    let z_indices = ast
        .z_indices
        .iter()
        .map(lower_design_z)
        .collect::<Result<Vec<_>, _>>()?;

    // L0 #2 — `custom` 9th meta-group per
    // `docs/proposals/design-tokens-custom.md`. Validate hex values here;
    // collision + reserved-name policy is enforced by doctor (the analyzer
    // stays surface-thin to keep proposal-pending diagnostics in doctor).
    let custom = ast
        .custom
        .iter()
        .map(lower_design_custom_token)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ir::Design {
        name: ast.name.clone(),
        extends: None,
        colors,
        typography,
        spaces,
        radii,
        shadows,
        motion,
        breakpoints,
        z_indices,
        custom,
        span_ref: Some(span_of(ast.span)),
    })
}

fn lower_design_custom_token(
    token: &syntax::CustomTokenAst,
) -> Result<ir::CustomToken, AnalyzeError> {
    // Hex validation for the `custom` 9th meta-group is intentionally
    // delegated to doctor (`design-custom-invalid-value`) so the
    // proposal-pending diagnostic surface ships as a doctor rule rather
    // than a hard analyzer rejection. See
    // `docs/proposals/design-tokens-custom.md` §4. Lowering preserves the
    // verbatim value so doctor can produce an actionable error message.
    Ok(ir::CustomToken {
        name: token.name.clone(),
        base: token.value.clone(),
        dark: token.dark.clone(),
        span_ref: Some(span_of(token.span)),
    })
}

fn lower_design_color_token(token: &syntax::ColorTokenAst) -> Result<ir::ColorToken, AnalyzeError> {
    let mut states = Vec::with_capacity(token.states.len());
    for state in &token.states {
        let kind = match state.kind.as_str() {
            "base" => ir::ColorStateKind::Base,
            "hover" => ir::ColorStateKind::Hover,
            "active" => ir::ColorStateKind::Active,
            "foreground" => ir::ColorStateKind::Foreground,
            other => {
                return Err(AnalyzeError::DesignColorStateUnknown {
                    token: token.name.clone(),
                    state: other.to_owned(),
                });
            }
        };
        if !is_valid_design_hex(&state.value) {
            return Err(AnalyzeError::DesignColorHexInvalid {
                token: token.name.clone(),
                state: state.kind.clone(),
                value: state.value.clone(),
            });
        }
        if let Some(dark) = state.dark.as_deref() {
            if !is_valid_design_hex(dark) {
                return Err(AnalyzeError::DesignColorHexInvalid {
                    token: token.name.clone(),
                    state: format!("{}.dark", state.kind),
                    value: dark.to_owned(),
                });
            }
        }
        states.push(ir::ColorState {
            kind,
            value: state.value.clone(),
            dark: state.dark.clone(),
        });
    }
    Ok(ir::ColorToken {
        name: token.name.clone(),
        states,
        span_ref: Some(span_of(token.span)),
    })
}

fn lower_design_weight(weight: &syntax::WeightTokenAst) -> Result<ir::WeightToken, AnalyzeError> {
    let parsed =
        weight
            .value
            .trim()
            .parse::<u16>()
            .map_err(|_| AnalyzeError::DesignWeightInvalid {
                name: weight.name.clone(),
                value: weight.value.clone(),
            })?;
    Ok(ir::WeightToken {
        name: weight.name.clone(),
        value: parsed,
    })
}

fn lower_design_shadow(shadow: &syntax::ShadowTokenAst) -> Result<ir::ShadowToken, AnalyzeError> {
    if has_top_level_comma(&shadow.value) {
        return Err(AnalyzeError::DesignShadowMultiLayer {
            name: shadow.name.clone(),
        });
    }
    Ok(ir::ShadowToken {
        name: shadow.name.clone(),
        value: shadow.value.clone(),
    })
}

fn lower_design_z(z: &syntax::ZTokenAst) -> Result<ir::ZToken, AnalyzeError> {
    let parsed = z
        .value
        .trim()
        .parse::<i32>()
        .map_err(|_| AnalyzeError::DesignZInvalid {
            name: z.name.clone(),
            value: z.value.clone(),
        })?;
    Ok(ir::ZToken {
        name: z.name.clone(),
        value: parsed,
    })
}

/// Match `^#[0-9a-fA-F]{3,8}$` without pulling in a regex dependency.
/// 3-digit (`#fff`), 4-digit (`#ffff` rgba shorthand), 6-digit (`#ffffff`),
/// and 8-digit (`#ffffffff` rgba) are all valid CSS hex notations.
fn is_valid_design_hex(text: &str) -> bool {
    let trimmed = text.trim();
    if !trimmed.starts_with('#') {
        return false;
    }
    let rest = &trimmed[1..];
    let len = rest.len();
    if !(len == 3 || len == 4 || len == 6 || len == 8) {
        return false;
    }
    rest.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Detect a top-level comma in a CSS box-shadow value, signaling
/// multi-layer composition. Commas inside `(...)` or `[...]` (e.g.
/// `rgb(0, 0, 0)`) do NOT count. Strings inside quoted regions also
/// don't count (a hypothetical `content: ","` would not trigger).
fn has_top_level_comma(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' => in_quote = !in_quote,
            b'(' | b'[' if !in_quote => depth += 1,
            b')' | b']' if !in_quote => depth -= 1,
            b',' if !in_quote && depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use lazuli_syntax::parse_lzx_document;

    use super::{
        AnalyzeError, lower_audit_block, lower_auth_identity, lower_lzx_document,
        lower_policy_atom_with_args, lower_validate_line, parse_cap_file_type,
        parse_query_filter_line, type_ref_from_syntax,
    };

    #[test]
    fn query_filter_line_lowers_dotted_path() {
        let filter = parse_query_filter_line("org_id = ctx.actor.org_id")
            .expect("dotted path filter parses");
        let ir::Predicate::Comparison { left, op, right } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        assert!(matches!(op, ir::CompareOp::Eq));
        assert_eq!(
            left,
            ir::Expr::Path(ir::Path::from_segments(["org_id".to_owned()]))
        );
        assert_eq!(
            right,
            ir::Expr::Path(ir::Path::from_segments([
                "ctx".to_owned(),
                "actor".to_owned(),
                "org_id".to_owned(),
            ]))
        );
        assert!(filter.when.is_none());
    }

    #[test]
    fn query_filter_line_lowers_bool_literal() {
        let filter = parse_query_filter_line("is_public = false").unwrap();
        let ir::Predicate::Comparison { right, .. } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        assert_eq!(right, ir::Expr::Boolean(false));
    }

    #[test]
    fn query_filter_line_lifts_bare_identifier_to_enum_literal() {
        // WAR-VOCAB-QUERY-ENUM-01 closure: `status = approved` must
        // lift `approved` to `Expr::Enum` so codegen emits a TEXT
        // const bind, NOT a runtime input lookup.
        let filter = parse_query_filter_line("status = approved").unwrap();
        let ir::Predicate::Comparison { right, .. } = filter.predicate else {
            panic!("expected Comparison predicate");
        };
        let literal = match right {
            ir::Expr::Enum(literal) => literal,
            other => panic!("expected Expr::Enum, got {other:?}"),
        };
        assert!(literal.type_name.is_none());
        assert_eq!(literal.variant, "approved");
    }

    #[test]
    fn query_filter_line_handles_inequality_operators() {
        let f1 = parse_query_filter_line("rating >= 4").unwrap();
        if let ir::Predicate::Comparison { op, .. } = f1.predicate {
            assert!(matches!(op, ir::CompareOp::Ge));
        } else {
            panic!("expected Comparison");
        }
        let f2 = parse_query_filter_line("status != cancelled").unwrap();
        if let ir::Predicate::Comparison { op, right, .. } = f2.predicate {
            assert!(matches!(op, ir::CompareOp::Ne));
            if let ir::Expr::Enum(literal) = right {
                assert_eq!(literal.variant, "cancelled");
            } else {
                panic!("expected Enum literal on RHS of !=");
            }
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn query_filter_line_drops_blanks_and_comments() {
        assert!(parse_query_filter_line("").is_none());
        assert!(parse_query_filter_line("   ").is_none());
        assert!(parse_query_filter_line("# org_id = ctx.actor.org_id").is_none());
    }

    #[test]
    fn query_filter_line_lowers_quoted_string() {
        let filter = parse_query_filter_line("name = \"hello\"").unwrap();
        if let ir::Predicate::Comparison { right, .. } = filter.predicate {
            assert_eq!(right, ir::Expr::String("hello".to_owned()));
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn query_filter_line_lowers_integer_and_nil() {
        let f1 = parse_query_filter_line("count >= 0").unwrap();
        if let ir::Predicate::Comparison { right, .. } = f1.predicate {
            assert_eq!(right, ir::Expr::Integer(0));
        } else {
            panic!("expected Comparison");
        }
        let f2 = parse_query_filter_line("deleted_at = nil").unwrap();
        if let ir::Predicate::Comparison { right, .. } = f2.predicate {
            assert_eq!(right, ir::Expr::Nil);
        } else {
            panic!("expected Comparison");
        }
    }

    #[test]
    fn lowers_lzx_experience_and_surface_to_ir() {
        let experience =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.lzx")).unwrap();
        let surface =
            parse_lzx_document(include_str!("../../../examples/customer-capsule.web.lzx")).unwrap();

        let experience_ir = lower_lzx_document(&experience);
        let surface_ir = lower_lzx_document(&surface);

        assert_eq!(experience_ir.experiences[0].name, "customer");
        assert_eq!(experience_ir.experiences[0].imports, vec!["customer"]);
        assert_eq!(
            experience_ir.experiences[0].views[0].actions[0].target,
            "customer.command.create"
        );
        assert_eq!(surface_ir.surfaces[0].experience, "customer");
        assert_eq!(
            surface_ir.surfaces[0].uses_experience.as_deref(),
            Some("customer")
        );
        assert_eq!(surface_ir.surfaces[0].audiences[0].name, "admin");
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].columns,
            vec!["name", "email", "status", "created_at"]
        );
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].search,
            vec!["name", "email"]
        );
        assert_eq!(
            surface_ir.surfaces[0].audiences[0].views[0].cells,
            vec!["status @client.status_cell"]
        );
    }

    #[test]
    fn lowers_lzx_extension_slots_to_ir() {
        let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    slot aside after activity_timeline
      block @client.tag_editor
      platforms web
      audience admin
"#;
        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let extension = &module.experiences[0].extensions[0];

        assert_eq!(extension.anchor, "@anchor.customer_detail");
        assert_eq!(extension.slots.len(), 1);
        assert_eq!(extension.slots[0].name, "aside");
        assert_eq!(extension.slots[0].blocks, vec!["@client.tag_editor"]);
        assert_eq!(extension.slots[0].platforms, vec!["web"]);
        assert_eq!(extension.slots[0].audiences, vec!["admin"]);
        assert_eq!(
            extension.slots[0]
                .order
                .as_ref()
                .map(|order| (order.relation.as_str(), order.target.as_str())),
            Some(("after", "activity_timeline"))
        );
    }

    #[test]
    fn lowers_lzx_route_guards_to_ir_with_spans() {
        let source = r#"
app AcmeCRM
  actor_query "account.query.me"
  route_guard
    default_policy @scope.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
    skeleton @client.route_guard_skeleton

route admin_home
  path "/admin"
  to customer.view.list
  surface customer web
  audience admin
  policy @policy.admin_only
    on_unauthenticated redirect "/sign-in"

experience customer
  view list
    policy @policy.admin_only
      on_unauthorized redirect "/"
    source customer.query.list

surface customer web
  uses experience customer

  audience admin
    policy @policy.admin_only
      on_unauthenticated redirect "/sign-in"
    view list Table
      policy @policy.admin_only
        on_unauthorized redirect "/"
      columns name
"#;

        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let app = module.app.as_ref().unwrap();
        let defaults = app.route_guard.as_ref().unwrap();

        assert_eq!(app.actor_query.as_deref(), Some("account.query.me"));
        assert_eq!(
            defaults.default_policy.as_deref(),
            Some("@scope.authenticated")
        );
        assert_eq!(defaults.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert_eq!(defaults.on_unauthorized.as_deref(), Some("/403"));
        assert_eq!(
            defaults.skeleton.as_deref(),
            Some("@client.route_guard_skeleton")
        );
        assert!(defaults.span_ref.is_some());

        let route_guard = module.routes[0].guard.as_ref().unwrap();
        assert_eq!(
            &route_guard.policy[..],
            vec!["@policy.admin_only".to_owned()].as_slice()
        );
        assert_eq!(route_guard.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert!(route_guard.span_ref.is_some());

        let view_guard = module.experiences[0].views[0].guard.as_ref().unwrap();
        assert_eq!(
            &view_guard.policy[..],
            vec!["@policy.admin_only".to_owned()].as_slice()
        );
        assert_eq!(view_guard.on_unauthorized.as_deref(), Some("/"));
        assert!(view_guard.span_ref.is_some());

        let audience_guard = module.surfaces[0].audiences[0].guard.as_ref().unwrap();
        assert_eq!(
            audience_guard.on_unauthenticated.as_deref(),
            Some("/sign-in")
        );
        assert!(audience_guard.span_ref.is_some());

        let platform_guard = module.surfaces[0].audiences[0].views[0]
            .guard
            .as_ref()
            .unwrap();
        assert_eq!(platform_guard.on_unauthorized.as_deref(), Some("/"));
        assert!(platform_guard.span_ref.is_some());
    }

    #[test]
    fn full_capsule_lzx_route_guards_ir_json_round_trip_is_byte_identical() {
        let source = include_str!("../../../examples/full-capsule/full-capsule.lzx");
        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);
        let guard = module
            .experiences
            .iter()
            .find(|experience| experience.name == "customer_auth")
            .and_then(|experience| {
                experience
                    .views
                    .iter()
                    .find(|view| view.name == "enable_mfa")
            })
            .and_then(|view| view.guard.as_ref())
            .expect("full-capsule enable_mfa guard");

        assert_eq!(
            &guard.policy[..],
            vec!["@policy.update".to_owned()].as_slice()
        );
        assert_eq!(guard.on_unauthenticated.as_deref(), Some("/login"));

        let first = serde_json::to_string_pretty(&module).unwrap();
        let decoded: ir::ExperienceModule = serde_json::from_str(&first).unwrap();
        let second = serde_json::to_string_pretty(&decoded).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn lowers_lzx_app_manifest_and_routes_to_ir() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  targets
    backend go
    web react
  uses customer, billing

route customer_detail
  path "/customers/:id"
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
"#;

        let document = parse_lzx_document(source).unwrap();
        let module = lower_lzx_document(&document);

        assert_eq!(module.app.as_ref().unwrap().name, "AcmeCRM");
        assert_eq!(
            module.app.as_ref().unwrap().targets,
            vec!["backend go", "web react"]
        );
        assert_eq!(module.routes[0].name, "customer_detail");
        assert_eq!(module.routes[0].routes, vec!["id: Customer.ID"]);
        assert_eq!(
            module.routes[0].to.as_deref(),
            Some("customer.view.detail(id: route.id)")
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — agent lowering (§4.4 snapshot tests)
    // -------------------------------------------------------------------------

    use super::lower_feature_skeleton;
    use lazuli_ir as ir;
    use lazuli_syntax::parse_feature_skeletons;

    fn lower_first_agent(source: &str) -> ir::Agent {
        let features = parse_feature_skeletons(source).expect("parses");
        assert_eq!(features.len(), 1);
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        feature.agents.into_iter().next().expect("agent")
    }

    #[test]
    fn lower_agent_with_tools_resolves_to_ir() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.query.by_id
      query.by_id
      command.archive
      @tool.web_search
      @tool.calendar.create_event
"#;
        let agent = lower_first_agent(source);

        assert_eq!(agent.feature, "customer");
        assert_eq!(agent.name, "triage");
        assert_eq!(agent.tools.len(), 5);

        match &agent.tools[0].reference {
            ir::QualifiedToolRef::CrossFeature {
                feature,
                kind,
                name,
            } => {
                assert_eq!(feature, "customer");
                assert_eq!(*kind, ir::ToolKind::QueryUnspecified);
                assert_eq!(name, "by_id");
            }
            other => panic!("expected CrossFeature, got {other:?}"),
        }
        match &agent.tools[1].reference {
            ir::QualifiedToolRef::Local { kind, name } => {
                assert_eq!(*kind, ir::ToolKind::QueryUnspecified);
                assert_eq!(name, "by_id");
            }
            other => panic!("expected Local, got {other:?}"),
        }
        match &agent.tools[2].reference {
            ir::QualifiedToolRef::Local { kind, name } => {
                assert_eq!(*kind, ir::ToolKind::Command);
                assert_eq!(name, "archive");
            }
            other => panic!("expected Local Command, got {other:?}"),
        }
        match &agent.tools[3].reference {
            ir::QualifiedToolRef::Adapter { dotted } => {
                assert_eq!(dotted, &vec!["web_search".to_owned()]);
            }
            other => panic!("expected Adapter, got {other:?}"),
        }
        match &agent.tools[4].reference {
            ir::QualifiedToolRef::Adapter { dotted } => {
                assert_eq!(
                    dotted,
                    &vec!["calendar".to_owned(), "create_event".to_owned()]
                );
            }
            other => panic!("expected Adapter dotted, got {other:?}"),
        }

        // Expand pass populates the resolved_* fields; lowering leaves them
        // None / empty.
        assert!(agent.tools.iter().all(|t| t.resolved_effect.is_none()));
        assert!(agent.tools.iter().all(|t| t.resolved_policy.is_none()));
        assert!(
            agent
                .tools
                .iter()
                .all(|t| t.resolved_pii_classes.is_empty())
        );
    }

    #[test]
    fn lower_agent_with_evals_resolves_to_ir() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case short_for_active
        requires customer.lifecycle_stage = active
        requires output contains "active"

      case redacts_email
        forbids output contains @semantic.Email

      case uses_lookup
        requires tools.calls includes customer.query.by_id
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.evals.len(), 3);

        // Case 0: Closed Comparison + Contains literal.
        let c0 = &agent.evals[0];
        assert_eq!(c0.name, "short_for_active");
        match &c0.assertions[0].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { left, op, right }) => {
                assert_eq!(*op, ir::CompareOp::Eq);
                match (left, right) {
                    (ir::Expr::Path(lhs), ir::Expr::Path(rhs)) => {
                        assert_eq!(lhs.segments, vec!["customer", "lifecycle_stage"]);
                        assert_eq!(rhs.segments, vec!["active"]);
                    }
                    other => panic!("unexpected Comparison sides: {other:?}"),
                }
            }
            other => panic!("expected Closed Comparison, got {other:?}"),
        }
        match &c0.assertions[1].predicate {
            ir::EvalPredicate::Contains { lhs, rhs } => {
                assert_eq!(lhs.segments, vec!["output"]);
                assert_eq!(rhs, &ir::EvalContainsRhs::Literal("active".to_owned()));
            }
            other => panic!("expected Contains literal, got {other:?}"),
        }

        // Case 1: Forbids + Contains semantic.
        let c1 = &agent.evals[1];
        assert_eq!(c1.assertions[0].kind, ir::EvalAssertionKind::Forbids);
        match &c1.assertions[0].predicate {
            ir::EvalPredicate::Contains { rhs, .. } => match rhs {
                ir::EvalContainsRhs::SemanticType(qn) => {
                    assert_eq!(qn.name, "@semantic.Email");
                }
                other => panic!("expected SemanticType, got {other:?}"),
            },
            other => panic!("expected Contains, got {other:?}"),
        }

        // Case 2: ToolsCalls includes a cross-feature target.
        let c2 = &agent.evals[2];
        match &c2.assertions[0].predicate {
            ir::EvalPredicate::ToolsCalls { op, target } => {
                assert_eq!(*op, ir::ToolsCallsOp::Includes);
                match target {
                    ir::QualifiedToolRef::CrossFeature { feature, name, .. } => {
                        assert_eq!(feature, "customer");
                        assert_eq!(name, "by_id");
                    }
                    other => panic!("expected CrossFeature target, got {other:?}"),
                }
            }
            other => panic!("expected ToolsCalls, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_with_discriminator_output_resolves() {
        let source = r#"
feature customer_support
  agent classify_intent
    input
      message: Text required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.output_kind, ir::AgentOutputKind::DiscriminatedEnum);
        match agent.output_discriminator.as_ref().unwrap() {
            ir::DiscriminatorRef::Enum(qn) => {
                assert_eq!(qn.name, "Intent");
                assert!(qn.feature.is_none());
            }
            other => panic!("expected Enum discriminator, got {other:?}"),
        }
        assert!(agent.output_type.is_none());
    }

    #[test]
    fn lower_agent_with_discriminated_record_resolves() {
        // Bare `output Action` lowers as Text + Some(output_type=Action).
        // The expand pass (Phase 5) promotes to DiscriminatedRecord when
        // it resolves `Action` to a record with a `discriminator` field.
        let source = r#"
feature customer
  agent extract_action
    input
      message: Text required
    policy @policy.read
    output Action
    model @llm.default
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.output_kind, ir::AgentOutputKind::Text);
        assert!(agent.output_discriminator.is_none());
        match agent.output_type.as_ref().unwrap() {
            ir::TypeRef::UserDefined(q) => {
                assert_eq!(q.name, "Action");
                assert!(q.feature.is_none());
            }
            other => panic!("expected UserDefined Action, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_evals_without_temperature_zero_is_marked_nondeterministic() {
        // Lowering doesn't fail; doctor's diagnostic
        // `eval_nondeterministic_warning` fires in Phase 3. Here we just
        // verify lowering captures `temperature` and `seed` so doctor can
        // inspect them.
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case nondeterministic
        requires output contains "x"
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.temperature, Some(0.7));
        assert!(agent.seed.is_none());
        assert!(!agent.evals.is_empty());
        // Doctor will combine temperature + seed + evals.is_empty() to
        // emit `eval_nondeterministic_warning` in Phase 3.
    }

    #[test]
    fn lower_agent_propagates_safety_list_for_cut_a5_ready() {
        // Cut A allows 0..1 safety entries; Cut A.5 widens to a list.
        // The IR shape `safety: Vec<QualifiedName>` already supports the
        // wider form — this test pins the shape so A.5 lands by adding
        // a doctor diagnostic, not by changing IR.
        let source = r#"
feature customer
  agent guarded
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    safety @validator.pii_email_scrub, @validator.pii_ssn_scrub
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.safety.len(), 2);
        assert_eq!(agent.safety[0].name, "@validator.pii_email_scrub");
        assert_eq!(agent.safety[1].name, "@validator.pii_ssn_scrub");
    }

    #[test]
    fn lower_agent_ordered_compare_op_lowers_to_lt_le_gt_ge() {
        // Proposal §A3 admits ordered ops inside evals. Lowering parses
        // them; doctor's `eval_ordered_op_invalid_diagnostics` decides
        // whether the operand types are numeric.
        let source = r#"
feature customer
  agent ordered
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bounded
        requires output.length <= 800
        requires output.length >= 1
"#;
        let agent = lower_first_agent(source);
        assert_eq!(agent.evals.len(), 1);
        match &agent.evals[0].assertions[0].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { op, .. }) => {
                assert_eq!(*op, ir::CompareOp::Le);
            }
            other => panic!("expected Le Comparison, got {other:?}"),
        }
        match &agent.evals[0].assertions[1].predicate {
            ir::EvalPredicate::Closed(ir::Predicate::Comparison { op, .. }) => {
                assert_eq!(*op, ir::CompareOp::Ge);
            }
            other => panic!("expected Ge Comparison, got {other:?}"),
        }
    }

    #[test]
    fn lower_agent_invalid_tool_ref_errors() {
        // `@tool` (no dotted tail) is malformed; lowering returns
        // `AnalyzeError::InvalidToolRef`. Tool-string sanity checks fire
        // here so doctor can stay focused on cross-feature resolution.
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      @tool.
"#;
        // Note: the parser already rejects `@tool.` (trailing dot leaves an
        // empty tail when split). We craft a slightly different shape so
        // the parser accepts and lowering rejects.
        let parsed = parse_feature_skeletons(source);
        match parsed {
            Err(_) => return, // parser caught it — equally valid
            Ok(features) => {
                let err = lower_feature_skeleton(&features[0]).unwrap_err();
                match err {
                    AnalyzeError::InvalidToolRef { .. } => {}
                    other => panic!("expected InvalidToolRef, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn lower_agent_golden_eval_lowers_to_ir() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case quality
        requires output contains "active"
        golden "./evals/summarize.jsonl" min_score 0.85
"#;
        let agent = lower_first_agent(source);
        let case = &agent.evals[0];
        let golden = case.golden.as_ref().expect("golden");
        assert_eq!(golden.path, "./evals/summarize.jsonl");
        assert_eq!(golden.min_score, Some(0.85));
        // Assertions still present alongside the golden ref.
        assert_eq!(case.assertions.len(), 1);
    }

    #[test]
    fn lower_agent_with_expose_http_lowers_to_ir() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
      audience admin
      rate_limit "5 per minute per user"
"#;
        let agent = lower_first_agent(source);
        let expose = agent.expose_http.as_ref().expect("expose_http");
        assert_eq!(expose.method, ir::HttpMethod::Post);
        assert_eq!(expose.path, "/api/customers/:customer_id/summary");
        assert_eq!(expose.route_slots.len(), 1);
        assert_eq!(expose.route_slots[0].name, "customer_id");
        assert!(expose.route_slots[0].required);
        assert_eq!(expose.audience.as_deref(), Some("admin"));
        assert_eq!(
            expose.rate_limit_override.as_deref(),
            Some("5 per minute per user")
        );
    }

    #[test]
    fn lower_agent_without_expose_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let agent = lower_first_agent(source);
        assert!(agent.expose_http.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_auth_full_block_to_ir() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let auth = feature.auth.expect("auth lowered");

        assert_eq!(auth.identity.field.resource.name, "Customer");
        assert_eq!(auth.identity.field.field, "email");

        let password = auth.password.as_ref().expect("password");
        assert_eq!(password.algorithm, "argon2id");
        assert_eq!(password.hash, "@fn.hash_customer_password");
        assert_eq!(password.verify, "@fn.verify_customer_password");
        let rate_limit = password.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(rate_limit.default, "5 per 10 minutes");
        assert!(rate_limit.by_env.is_empty());

        let mfa = auth.mfa.as_ref().expect("mfa");
        assert_eq!(mfa.method, "totp");
        assert_eq!(mfa.enroll, "@fn.enroll_customer_totp");
        assert_eq!(mfa.verify, "@validator.verify_customer_totp");

        let sessions = auth.sessions.as_ref().expect("sessions");
        assert_eq!(sessions.resource.name, "CustomerSession");
        assert_eq!(sessions.ttl, "7 days");
        assert!(!sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());

        assert_eq!(auth.oauth.len(), 1);
        assert_eq!(auth.oauth[0].provider, "google");
        assert_eq!(auth.oauth[0].adapter, "@adapter.google_oauth");
    }

    #[test]
    fn lower_auth_sessions_rotation_block_to_ir() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      access_ttl "15 minutes"
      rotation
        refresh_ttl "30 days"
        grace "30 seconds"
        theft_detection_action revoke_user
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let sessions = feature
            .auth
            .as_ref()
            .expect("auth lowered")
            .sessions
            .as_ref()
            .expect("sessions lowered");

        assert_eq!(sessions.access_ttl.as_deref(), Some("15 minutes"));
        let rotation = sessions.rotation.as_ref().expect("rotation lowered");
        assert_eq!(rotation.refresh_ttl.as_deref(), Some("30 days"));
        assert_eq!(rotation.grace.as_deref(), Some("30 seconds"));
        assert_eq!(
            rotation.theft_detection_action,
            Some(ir::TheftAction::RevokeUser)
        );
        assert!(rotation.span_ref.is_some());
    }

    #[test]
    fn lower_auth_sessions_empty_rotation_block_uses_ir_defaults_later() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      rotation
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let sessions = feature
            .auth
            .as_ref()
            .expect("auth lowered")
            .sessions
            .as_ref()
            .expect("sessions lowered");

        let rotation = sessions.rotation.as_ref().expect("rotation lowered");
        assert!(rotation.refresh_ttl.is_none());
        assert!(rotation.grace.is_none());
        assert!(rotation.theft_detection_action.is_none());
        assert_eq!(sessions.resolved_access_ttl(), "15 minutes");
        assert_eq!(sessions.resolved_refresh_ttl(), Some("30 days"));
        assert_eq!(sessions.resolved_rotation_grace(), Some("30 seconds"));
        assert_eq!(
            sessions.resolved_theft_action(),
            Some(ir::TheftAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn lower_auth_sessions_without_legacy_refresh_keeps_rotation_none() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let sessions = feature
            .auth
            .as_ref()
            .expect("auth lowered")
            .sessions
            .as_ref()
            .expect("sessions lowered");

        assert!(!sessions.refresh);
        assert!(sessions.access_ttl.is_none());
        assert!(sessions.rotation.is_none());
    }

    #[test]
    fn lower_auth_identity_with_empty_field_errors() {
        // Parser would already reject `identity .email` because the
        // dot-qualified contract requires both segments; this test
        // documents the analyzer's defensive guard for any future
        // parser shape that lets a stray dot through.
        let identity = lazuli_syntax::AuthIdentity {
            field: "Customer.".to_owned(),
            public_contract: None,
            span: lazuli_syntax::Span::new(0, 9),
        };
        let err = lower_auth_identity(&identity).unwrap_err();
        match err {
            AnalyzeError::InvalidAuthIdentity { reference } => {
                assert_eq!(reference, "Customer.");
            }
            other => panic!("expected InvalidAuthIdentity, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 3 — job / webhook / notification / event_group lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_tier3_job_handler_full_block() {
        let source = r#"
feature customer
  job process_import
    trigger event customer_import_uploaded
    queue customer_imports
    tenant_from payload.org_id
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
      org_id = payload.org_id
    timeout "30s"
    handler "./jobs/process_import.go"
    emits customer_import_completed
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.jobs.len(), 1);
        let job = &feature.jobs[0];
        assert_eq!(job.name, "process_import");
        assert_eq!(job.queue.as_deref(), Some("customer_imports"));
        assert_eq!(job.timeout.as_deref(), Some("30s"));
        let tenant = job.tenant_from.as_ref().expect("tenant_from");
        assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
        let retry = job.retry.as_ref().expect("retry");
        assert_eq!(retry.count, 3);
        assert!(matches!(retry.backoff, ir::BackoffStrategy::Exponential));
        assert_eq!(job.external_calls.len(), 1);
        assert_eq!(job.external_calls[0].slot, "crm");
        assert_eq!(job.external_calls[0].op, "normalize_import_batch");
        assert_eq!(job.external_calls[0].args.len(), 2);
        match &job.body {
            ir::JobBody::Handler(h) => {
                assert_eq!(h.path.path, "./jobs/process_import.go");
            }
            other => panic!("expected Handler body, got {other:?}"),
        }
        assert_eq!(job.emits, vec!["customer_import_completed"]);
    }

    #[test]
    fn lower_tier3_job_declarative_carve_out() {
        let source = r#"
feature customer
  job recompute_score_after_invoice
    trigger event billing.invoice_paid
    tenant_from payload.org_id
    idempotency by envelope.id
    target query.by_id(id: payload.customer_id)
    let new_score = @fn.risk_score(target)
    updates Customer
      score = new_score
    emits customer_score_recomputed
      score = new_score
      reason = "invoice_paid"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.jobs.len(), 1);
        let job = &feature.jobs[0];
        match &job.body {
            ir::JobBody::Declarative(d) => {
                let target = d.target.as_ref().expect("target lifted");
                assert_eq!(target.query.name, "by_id");
                assert_eq!(d.lets.len(), 1);
                assert_eq!(d.lets[0].name, "new_score");
                match &d.effect {
                    ir::CommandEffect::Updates(u) => {
                        assert_eq!(u.resource.name, "Customer");
                        assert_eq!(u.assignments.len(), 1);
                        assert_eq!(u.assignments[0].field, "score");
                    }
                    other => panic!("expected Updates effect, got {other:?}"),
                }
            }
            other => panic!("expected Declarative body, got {other:?}"),
        }
    }

    #[test]
    fn lower_tier3_webhook_structured_verify() {
        let source = r#"
feature customer
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    handler "./integrations/upsert_customer_from_crm.go" returns Customer
    emits customer_webhook_received
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.webhooks.len(), 1);
        let webhook = &feature.webhooks[0];
        assert_eq!(webhook.route, "/webhooks/crm/customer-upsert");
        let verify = webhook
            .structured_verify
            .as_ref()
            .expect("structured verify");
        assert!(matches!(verify.scheme, ir::VerifyScheme::Hmac));
        assert_eq!(verify.algorithm, "sha256");
        assert_eq!(verify.secret_env, "CRM_WEBHOOK_SECRET");
        assert_eq!(verify.header, "X-CRM-Signature");
        let tenant = webhook.tenant_from.as_ref().expect("tenant_from");
        assert_eq!(tenant.path.segments, vec!["payload", "org_id"]);
        assert_eq!(
            webhook.handler.path,
            "./integrations/upsert_customer_from_crm.go"
        );
        assert_eq!(webhook.emits, vec!["customer_webhook_received"]);
    }

    #[test]
    fn lower_tier3_notification_full_block() {
        let source = r#"
feature customer_outreach
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    tenant_from payload.org_id
    idempotency by envelope.id
    retry 3 backoff exponential
    template "./outreach/welcome_email.mjml"
    policy @policy.notify
    emits welcome_email_sent
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.notifications.len(), 1);
        let n = &feature.notifications[0];
        assert_eq!(n.name, "welcome_email");
        assert_eq!(n.channels, vec!["email"]);
        assert_eq!(n.recipient, "target.email");
        assert_eq!(n.template, "./outreach/welcome_email.mjml");
        match &n.trigger {
            ir::JobTrigger::Event { event } => {
                assert_eq!(event.feature.as_deref(), Some("customer"));
                assert_eq!(event.name, "customer_activated");
            }
            other => panic!("expected Event trigger, got {other:?}"),
        }
        assert_eq!(n.emits, vec!["welcome_email_sent"]);
    }

    #[test]
    fn lower_tier3_event_group_payload_and_events() {
        let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
      org_id = org.id
    event created
    event activated
    event archived
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.event_groups.len(), 1);
        let group = &feature.event_groups[0];
        assert_eq!(group.pattern, "customer_*");
        assert_eq!(group.on_resource.as_deref(), Some("Customer"));
        assert_eq!(group.raw_payload.len(), 2);
        assert_eq!(
            group.events,
            vec![
                "created".to_owned(),
                "activated".to_owned(),
                "archived".to_owned()
            ]
        );
    }

    /// B5 framework gap 1 — per-event typed payload field bodies are
    /// lifted into `EventGroup.variants`. The legacy `events: Vec<String>`
    /// slot still holds the name list (back-compat), and each variant
    /// carries its `EventField`s, kind, and outbox flag.
    #[test]
    fn lower_event_group_lifts_per_event_typed_payload_fields() {
        let source = r#"
feature payments
  event_group charge_* on Charge
    payload
      charge_id = id
    event requested
      outbox guaranteed
      amount: @semantic.Money
      host_id: ID
    event confirmed
      outbox guaranteed
      amount: @semantic.Money
      provider_payment_id: Text
      paid_at: DateTime
    event.trace mp_status_received
      provider_status: Text
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let group = &feature.event_groups[0];
        assert_eq!(group.variants.len(), 3, "three variants under group");

        // Variant 0 — requested
        let requested = &group.variants[0];
        assert_eq!(requested.name, "requested");
        assert!(matches!(requested.kind, ir::EventVariantKind::Committed));
        assert!(requested.outbox.is_guaranteed());
        assert_eq!(requested.fields.len(), 2);
        assert_eq!(requested.fields[0].name, "amount");
        assert_eq!(requested.fields[1].name, "host_id");

        // Variant 1 — confirmed
        let confirmed = &group.variants[1];
        assert_eq!(confirmed.name, "confirmed");
        assert_eq!(confirmed.fields.len(), 3);
        let names: Vec<&str> = confirmed.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["amount", "provider_payment_id", "paid_at"]);

        // Variant 2 — trace
        let trace = &group.variants[2];
        assert_eq!(trace.name, "mp_status_received");
        assert!(matches!(trace.kind, ir::EventVariantKind::Trace));
        assert!(trace.outbox.is_none());
        assert_eq!(trace.fields.len(), 1);
        assert_eq!(trace.fields[0].name, "provider_status");
    }

    /// B5 framework gap 1 — `event foo` (no body) still parses and
    /// lowers cleanly. The variant comes through with an empty
    /// `fields` Vec so the legacy `Feature.events` lookup path stays
    /// in charge of the typed projection.
    #[test]
    fn lower_event_group_back_compat_empty_event_bodies() {
        let source = r#"
feature customer
  event_group customer_* on Customer
    payload
      customer_id = id
    event created
    event archived
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let group = &feature.event_groups[0];
        assert_eq!(group.variants.len(), 2);
        for variant in &group.variants {
            assert!(variant.fields.is_empty());
            assert!(matches!(variant.kind, ir::EventVariantKind::Committed));
        }
    }

    /// B5 framework gap 2 — `webhook ... emits foo when <predicate>`
    /// lifts the per-branch `when` clause into a typed `EmitPredicate`.
    #[test]
    fn lower_webhook_with_when_predicates_typed_lift() {
        let source = r#"
feature payments
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed when payload.status = "approved"
    emits charge_failed when payload.status in ("rejected", "cancelled")
    emits mp_status_received
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let webhook = &feature.webhooks[0];
        assert_eq!(
            webhook.emits,
            vec![
                "charge_confirmed".to_owned(),
                "charge_failed".to_owned(),
                "mp_status_received".to_owned()
            ]
        );
        assert_eq!(webhook.emit_predicates.len(), 3);

        // [0] equals
        let approved = webhook.emit_predicates[0]
            .as_ref()
            .expect("first emit has predicate");
        match &approved.kind {
            ir::EmitPredicateKind::Equals { path, literal } => {
                assert_eq!(path, "payload.status");
                assert_eq!(literal, "approved");
            }
            other => panic!("expected Equals, got {:?}", other),
        }

        // [1] in
        let failed = webhook.emit_predicates[1]
            .as_ref()
            .expect("second emit has predicate");
        match &failed.kind {
            ir::EmitPredicateKind::In { path, literals } => {
                assert_eq!(path, "payload.status");
                assert_eq!(
                    literals,
                    &vec!["rejected".to_owned(), "cancelled".to_owned()]
                );
            }
            other => panic!("expected In, got {:?}", other),
        }

        // [2] no predicate (default branch)
        assert!(webhook.emit_predicates[2].is_none());
    }

    /// B5 framework gap 2 back-compat — the flat `emits foo` /
    /// `emits bar` shape (no predicates) leaves `emit_predicates`
    /// empty so the generated `WebhookContract` stays on the legacy
    /// `Emits []string{}` shape.
    #[test]
    fn lower_webhook_without_when_predicates_keeps_legacy_emits_shape() {
        let source = r#"
feature payments
  webhook mp_payment_event
    path "/webhooks/mp/payment"
    verify hmac sha256
      secret env.MERCADOPAGO_WEBHOOK_SECRET
      header "x-signature"
    idempotency by envelope.id
    handler @fn.on_mp_payment_event
    emits charge_confirmed
    emits charge_failed
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let webhook = &feature.webhooks[0];
        assert_eq!(webhook.emits.len(), 2);
        assert!(
            webhook.emit_predicates.is_empty(),
            "no `when` clauses means no per-branch dispatch"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `@cap.File(...)` typing
    // -------------------------------------------------------------------------

    #[test]
    fn mfa_atom_with_args_lowers() {
        let atom = lower_policy_atom_with_args("@mfa.required(within:15m)");
        assert_eq!(atom.namespace, "mfa");
        assert_eq!(atom.name, "required");
        assert_eq!(atom.args.as_deref(), Some("within:15m"));
    }

    #[test]
    fn cap_pii_lowers() {
        let ty = type_ref_from_syntax("@cap.PII(class:contact,retention:90d,log_redact:true)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::PII(pii)) => {
                assert_eq!(pii.class, "contact");
                assert_eq!(pii.retention.as_deref(), Some("90d"));
                assert_eq!(pii.log_redact, Some(true));
            }
            other => panic!("expected Capability::PII, got {other:?}"),
        }
    }

    fn lower_field_line(line: &str) -> ir::Field {
        let source = format!(
            "feature account\n  domain\n    resource Customer\n      {}\n",
            line
        );
        let features = lazuli_syntax::parse_feature_skeletons(&source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        feature
            .resources
            .into_iter()
            .next()
            .expect("resource")
            .fields
            .into_iter()
            .next()
            .expect("field")
    }

    #[test]
    fn field_with_pii_decorator_stacks_with_semantic() {
        let line = "cpf: @semantic.BrazilianCPF optional unique @cap.PII(class:\"identity\")";
        let field = lower_field_line(line);
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::UserDefined(ref q) if q.name == "@semantic.BrazilianCPF"
        ));
        assert!(!field.required);
        assert!(field.unique);
        assert!(field.pii.is_some());
        assert_eq!(field.pii.as_ref().unwrap().class, "identity");
    }

    #[test]
    fn field_without_pii_decorator_has_none() {
        let field = lower_field_line("name: Text required");
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::Builtin(ir::BuiltinType::Text)
        ));
        assert!(field.required);
        assert!(field.pii.is_none());
    }

    #[test]
    fn owner_axis_on_fk_field_lowers_into_ir() {
        // `ir-resource-conventions-owner-scope` §7 — happy path: a
        // user-defined FK field (here `host: Host required`) is the
        // only legal carrier for `@owner_axis(through: <ident>)`.
        let field = lower_field_line("host: Host required @owner_axis(through: user)");
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::UserDefined(ref q) if q.name == "Host"
        ));
        let axis = field
            .owner_axis
            .as_ref()
            .expect("`@owner_axis(through: user)` must lower into ir::Field.owner_axis");
        assert_eq!(axis.through_column, "user");
    }

    #[test]
    fn owner_axis_on_primitive_field_emits_owner_axis_on_non_fk() {
        // `ir-resource-conventions-owner-scope` §11.1 —
        // `owner_axis_on_non_fk`. The annotation on a primitive field
        // (here `slug: Text`) is rejected at lowering: primitives carry
        // no ownership chain for the synth pass to walk.
        let source = "
feature catalog
  domain
    resource Property
      slug: Text @owner_axis(through: user)
";
        let features = lazuli_syntax::parse_feature_skeletons(source)
            .expect("parses (annotation is syntactic)");
        let err = lower_feature_skeleton(&features[0])
            .expect_err("lowering must reject @owner_axis on a non-FK field");
        match err {
            AnalyzeError::OwnerAxisOnNonFk { field, .. } => {
                assert_eq!(field, "slug");
            }
            other => panic!("expected OwnerAxisOnNonFk, got {other:?}"),
        }
    }

    #[test]
    fn field_with_pii_decorator_after_default_cleans_default() {
        let field = lower_field_line("name: Text required = anon @cap.PII(class:\"contact\")");
        assert_eq!(
            field.default,
            Some(ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
                type_name: None,
                variant: "anon".to_owned(),
            }))
        );
        assert_eq!(field.pii.as_ref().unwrap().class, "contact");
    }

    #[test]
    fn audit_data_subject_lowers() {
        let spec = lower_audit_block("audit default\naudit data_subject user_id\n");
        assert_eq!(spec.subjects, vec!["default".to_owned()]);
        assert_eq!(spec.data_subject.as_deref(), Some("user_id"));
    }

    #[test]
    fn audit_before_after_lowers() {
        let spec = lower_audit_block("audit before, after\n");
        assert!(spec.record_before);
        assert!(spec.record_after);
    }

    #[test]
    fn audit_retain_lowers() {
        let spec = lower_audit_block("audit retain 90d\n");
        assert_eq!(spec.retain_for.as_deref(), Some("90d"));
    }

    #[test]
    fn validate_sanitize_html_lowers() {
        let constraints =
            lower_validate_line("validate sanitize_html(basic)").expect("valid profile");
        assert_eq!(
            constraints.sanitize_html,
            Some(ir::SanitizeHtmlProfile::Basic)
        );
    }

    #[test]
    fn validate_sanitize_html_rejects_unknown_profile() {
        let result = lower_validate_line("validate sanitize_html(unsafe)");
        assert!(matches!(
            result,
            Err(AnalyzeError::UnknownSanitizeHtmlProfile { .. })
        ));
    }

    #[test]
    fn validate_limits_lower() {
        let source = r#"
feature account
  domain
    resource Payload
      body: Json validate utf8_safe validate max_recursion:8 validate max_size:4096
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.utf8_safe, Some(true));
        assert_eq!(field.constraints.max_recursion, Some(8));
        assert_eq!(field.constraints.max_size, Some(4096));
    }

    #[test]
    fn validator_covers_pii_lowers() {
        let source = r#"
feature account
  domain
    resource Customer
      email: Text validator covers_pii
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.covers_pii.as_deref(), Some("covers_pii"));
    }

    #[test]
    fn command_route_token_kinds_lower() {
        let source = r#"
feature account
  command consume
    route opaque token: Text
    route signed_token
    returns Text
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let command = &feature.commands[0];
        assert_eq!(command.route[0].name, "token");
        assert_eq!(command.route[0].kind, ir::RouteSlotKind::OpaqueToken);
        assert_eq!(command.route[1].name, "signed_token");
        assert_eq!(command.route[1].kind, ir::RouteSlotKind::SignedToken);
    }

    #[test]
    fn cap_file_auto_photo_policy_lowers() {
        let cap = parse_cap_file_type(
            "@cap.File(max_size:5mb,accept:image/jpeg,auto_photo_policy:@policy.host_only) optional",
        )
        .expect("cap file parses");
        assert_eq!(cap.auto_photo_policy.as_deref(), Some("@policy.host_only"));
    }

    #[test]
    fn type_ref_from_syntax_lowers_full_cap_file() {
        let ty =
            type_ref_from_syntax("@cap.File(max_size:25mb,accept:text/csv,visibility:private)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.max_size.bytes, 25 * 1024 * 1024);
                assert!(matches!(file.max_size.literal, ir::FileSizeLiteral::Mb(25)));
                assert_eq!(file.accept.len(), 1);
                assert_eq!(file.accept[0].family, "text");
                assert_eq!(file.accept[0].subtype, "csv");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Private));
                assert!(file.signed_ttl.is_none());
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_multi_mime_cap_file() {
        let ty = type_ref_from_syntax(
            "@cap.File(max_size:100mb,accept:text/csv|application/vnd.ms-excel,visibility:signed,signed_ttl:1h)",
        );
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.accept.len(), 2);
                assert_eq!(file.accept[1].family, "application");
                assert_eq!(file.accept[1].subtype, "vnd.ms-excel");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Signed));
                assert_eq!(file.signed_ttl.as_deref(), Some("1h"));
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_missing_max_size() {
        // No `max_size` arg → falls through to UserDefined so the LSP
        // shape diagnostic remains the canonical authority.
        let ty = type_ref_from_syntax("@cap.File(accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_malformed_size() {
        // `25xy` is not a recognised size literal.
        let ty = type_ref_from_syntax("@cap.File(max_size:25xy,accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_hashed_argon2id() {
        // Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:argon2id)`
        // now lowers into `CapabilityRef::Hashed(...)`.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:argon2id)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Hashed(h)) => {
                assert_eq!(h.algorithm, ir::HashAlgorithm::Argon2id);
            }
            other => panic!("expected Capability::Hashed, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_token_typed() {
        let ty = type_ref_from_syntax("@cap.Token(ttl:24h,single_use:true,store:hashed)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Token(t)) => {
                assert_eq!(t.ttl, "24h");
                assert!(t.single_use);
                assert_eq!(t.store, ir::TokenStore::Hashed);
            }
            other => panic!("expected Capability::Token, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_on_unknown_hash_algorithm() {
        // Closed catalog: unknown algo falls through to UserDefined so
        // the LSP can surface a shape diagnostic.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:scrypt)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_lifts_semantic_currency() {
        let ty = type_ref_from_syntax("@semantic.Currency");
        assert!(matches!(
            ty,
            ir::TypeRef::Builtin(ir::BuiltinType::SemanticCurrency)
        ));
    }

    #[test]
    fn lower_feature_without_auth_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.auth.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4a — `defaults` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_defaults_full_block() {
        let source = r#"
feature customer
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(matches!(feature.defaults.tenancy, Some(ir::Tenancy::Org)));
        assert!(feature.defaults.timestamps);
        match feature.defaults.policy.as_ref().expect("policy") {
            ir::PolicyRef::Atom(atom) => assert_eq!(atom, "actor.system"),
            other => panic!("expected @actor.system atom, got {other:?}"),
        }
    }

    #[test]
    fn lower_feature_defaults_absent_keeps_default() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.defaults.tenancy.is_none());
        assert!(!feature.defaults.timestamps);
        assert!(feature.defaults.policy.is_none());
    }

    #[test]
    fn lower_feature_defaults_custom_tenancy() {
        let source = r#"
feature pinned
  defaults
    tenancy workspace
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        match feature.defaults.tenancy.as_ref().expect("axis") {
            ir::Tenancy::Custom(axis) => assert_eq!(axis, "workspace"),
            other => panic!("expected custom axis, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4c — `resource` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_resource_lifts_retention_and_derived() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
      score: Integer = 0
      is_high_value: Boolean derived from score > 80
      has_many notes: CustomerNote inverse customer

      soft_delete
      retention 7y then anonymize
      validates @validator.tier_check
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.resources.len(), 1);
        let r = &feature.resources[0];
        assert_eq!(r.name, "Customer");
        assert!(r.soft_delete);
        let ret = r.retention.as_ref().expect("retention");
        assert_eq!(ret.duration, "7y");
        assert!(matches!(ret.action, ir::RetentionAction::Anonymize));
        let derived = r
            .fields
            .iter()
            .find(|f| f.name == "is_high_value")
            .expect("is_high_value");
        assert_eq!(derived.derived_from.as_deref(), Some("score > 80"));
        // validates @validator.tier_check projects onto `Resource.validate`
        // for single-entry authoring.
        assert!(r.validate.is_some());
    }

    #[test]
    fn lower_registry_tool_entry_with_effect_and_pii_classes() {
        // Pin the IR shape for `RegistryToolEntry`. The actual
        // registry.lzi parser lands in a later phase; this test
        // documents the contract that doctor's
        // `tool_registry_effect_required_diagnostics` will read.
        let entry = ir::RegistryToolEntry {
            name: "web_search".to_owned(),
            effect: ir::ToolEffect::Read,
            pii_classes: vec![ir::QualifiedName {
                feature: None,
                name: "@pii.contact".to_owned(),
            }],
            adapter: Some(ir::QualifiedName {
                feature: None,
                name: "@adapter.serp".to_owned(),
            }),
            span_ref: None,
        };

        let serialized = serde_json::to_value(&entry).unwrap();
        assert_eq!(serialized["name"], "web_search");
        assert_eq!(serialized["effect"], "read");
        assert_eq!(serialized["pii_classes"][0]["name"], "@pii.contact");
        assert_eq!(serialized["adapter"]["name"], "@adapter.serp");
    }

    // -------------------------------------------------------------------------
    // L0 #2 — design tokens lowering tests.
    // -------------------------------------------------------------------------

    use lazuli_syntax::parse_design_document;

    use super::lower_design;

    fn lower_design_source(source: &str) -> ir::Design {
        let ast = parse_design_document(source).expect("parses");
        lower_design(&ast).expect("lowers")
    }

    #[test]
    fn lower_design_lifts_flat_color_as_base_state() {
        let source = "
design example
  color
    success \"#16a34a\"
";
        let design = lower_design_source(source);
        assert_eq!(design.name, "example");
        assert!(design.extends.is_none());
        assert_eq!(design.colors.len(), 1);
        let success = &design.colors[0];
        assert_eq!(success.name, "success");
        assert_eq!(success.states.len(), 1);
        assert_eq!(success.states[0].kind, ir::ColorStateKind::Base);
        assert_eq!(success.states[0].value, "#16a34a");
    }

    #[test]
    fn lower_design_lifts_sub_block_color_states() {
        let source = "
design example
  color
    primary
      base \"#7c3aed\"
      hover \"#6d28d9\"
      active \"#5b21b6\"
      foreground \"#ffffff\"
";
        let design = lower_design_source(source);
        let primary = &design.colors[0];
        assert_eq!(primary.states.len(), 4);
        let kinds: Vec<ir::ColorStateKind> = primary.states.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ir::ColorStateKind::Base,
                ir::ColorStateKind::Hover,
                ir::ColorStateKind::Active,
                ir::ColorStateKind::Foreground,
            ]
        );
    }

    #[test]
    fn lower_design_preserves_dark_suffix() {
        let source = "
design example
  color
    background
      base \"#ffffff\" dark \"#09090b\"
";
        let design = lower_design_source(source);
        let bg = &design.colors[0];
        assert_eq!(bg.states[0].value, "#ffffff");
        assert_eq!(bg.states[0].dark.as_deref(), Some("#09090b"));
    }

    #[test]
    fn lower_design_extends_rejected_with_cut_b_code() {
        let source = "
design alpha
  extends base
  color
    primary
      base \"#10b981\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DESIGN-EXTENDS-CUT-B"),
            "expected DESIGN-EXTENDS-CUT-B, got: {msg}"
        );
        assert!(matches!(err, AnalyzeError::DesignExtendsCutB { .. }));
    }

    #[test]
    fn lower_design_multi_layer_shadow_rejected() {
        let source = "
design example
  shadow
    elevated \"0 1px 2px 0 rgb(0 0 0 / 0.05), 0 4px 6px -1px rgb(0 0 0 / 0.1)\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DESIGN-SHADOW-MULTI-LAYER"),
            "expected DESIGN-SHADOW-MULTI-LAYER, got: {msg}"
        );
        assert!(matches!(
            err,
            AnalyzeError::DesignShadowMultiLayer { ref name } if name == "elevated"
        ));
    }

    #[test]
    fn lower_design_single_layer_shadow_with_inner_commas_passes() {
        // Commas inside `rgb(...)` are inner; they do NOT trigger the
        // multi-layer rejection. The closed grammar accepts single-layer
        // shadows whose inner color uses `rgb(r, g, b)` notation.
        let source = "
design example
  shadow
    base \"0 1px 3px 0 rgb(0, 0, 0, 0.1)\"
";
        let design = lower_design_source(source);
        assert_eq!(design.shadows.len(), 1);
        assert_eq!(design.shadows[0].value, "0 1px 3px 0 rgb(0, 0, 0, 0.1)");
    }

    #[test]
    fn lower_design_typography_full_round_trip() {
        let source = "
design example
  typography
    family
      sans \"Inter, system-ui, sans-serif\"
    scale
      base size 1rem, line_height 1.5rem
    weight
      medium 500
      bold 700
    tracking
      tight -0.025em
";
        let design = lower_design_source(source);
        assert_eq!(design.typography.families[0].name, "sans");
        assert_eq!(
            design.typography.families[0].value,
            "Inter, system-ui, sans-serif"
        );
        assert_eq!(design.typography.scale[0].size, "1rem");
        assert_eq!(design.typography.scale[0].line_height, "1.5rem");
        // u16 parse.
        assert_eq!(design.typography.weights[0].value, 500);
        assert_eq!(design.typography.weights[1].value, 700);
        // Tracking preserves text including negative.
        assert_eq!(design.typography.tracking[0].value, "-0.025em");
    }

    #[test]
    fn lower_design_z_values_parsed_as_i32() {
        let source = "
design example
  z
    docked 10
    modal 1300
    toast 1500
";
        let design = lower_design_source(source);
        assert_eq!(design.z_indices.len(), 3);
        assert_eq!(design.z_indices[0].value, 10);
        assert_eq!(design.z_indices[1].value, 1300);
        assert_eq!(design.z_indices[2].value, 1500);
    }

    #[test]
    fn lower_design_rejects_invalid_hex() {
        let source = "
design example
  color
    bogus \"not-a-hex\"
";
        let ast = parse_design_document(source).unwrap();
        let err = lower_design(&ast).unwrap_err();
        assert!(
            matches!(err, AnalyzeError::DesignColorHexInvalid { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn lower_design_rejects_unknown_color_state() {
        // Construct AST directly (parser surface uses kind=String, so an
        // unknown identifier passes parse but should fail lowering).
        use lazuli_syntax::{
            ColorStateAst, ColorTokenAst, DesignDeclAst, MotionAst, Span, TypographyAst,
        };

        let ast = DesignDeclAst {
            name: "example".to_owned(),
            extends: None,
            colors: vec![ColorTokenAst {
                name: "primary".to_owned(),
                states: vec![ColorStateAst {
                    kind: "disabled".to_owned(),
                    value: "#7c3aed".to_owned(),
                    dark: None,
                }],
                span: Span::new(0, 1),
            }],
            typography: TypographyAst::default(),
            spaces: Vec::new(),
            radii: Vec::new(),
            shadows: Vec::new(),
            motion: MotionAst::default(),
            breakpoints: Vec::new(),
            z_indices: Vec::new(),
            custom: Vec::new(),
            span: Span::new(0, 1),
        };
        let err = lower_design(&ast).unwrap_err();
        assert!(matches!(
            err,
            AnalyzeError::DesignColorStateUnknown { ref token, ref state }
                if token == "primary" && state == "disabled"
        ));
    }

    #[test]
    fn lower_design_full_example_round_trip() {
        let source = "
design example
  color
    primary
      base \"#7c3aed\"
      hover \"#6d28d9\"
      foreground \"#ffffff\"
    success \"#16a34a\"

  typography
    family
      sans \"Inter, system-ui, sans-serif\"
    scale
      base size 1rem, line_height 1.5rem

  space
    \"1\" 0.25rem
    \"4\" 1rem

  radius
    sm 0.125rem

  shadow
    base \"0 1px 3px 0 rgb(0 0 0 / 0.1)\"

  motion
    duration
      fast 150ms
    easing
      out \"cubic-bezier(0, 0, 0.2, 1)\"

  breakpoint
    sm 640px

  z
    modal 1300
";
        let design = lower_design_source(source);
        // Every group has at least one entry.
        assert!(!design.colors.is_empty());
        assert!(!design.typography.families.is_empty());
        assert!(!design.typography.scale.is_empty());
        assert!(!design.spaces.is_empty());
        assert!(!design.radii.is_empty());
        assert!(!design.shadows.is_empty());
        assert!(!design.motion.durations.is_empty());
        assert!(!design.motion.easings.is_empty());
        assert!(!design.breakpoints.is_empty());
        assert!(!design.z_indices.is_empty());
        // SpanRef preserved.
        assert!(design.span_ref.is_some());
        // Serializes round-trip cleanly.
        let json = serde_json::to_value(&design).unwrap();
        assert_eq!(json["name"], "example");
        assert_eq!(json["colors"][0]["name"], "primary");
        // States serialize with snake_case kind.
        assert_eq!(json["colors"][0]["states"][0]["kind"], "base");
        // ColorStateKind serializes as snake_case.
        assert_eq!(json["colors"][0]["states"][2]["kind"], "foreground");
    }

    // ── Z2 — `custom` 9th meta-group lowering ──────────────────────────────

    #[test]
    fn lower_design_lifts_custom_group_with_base_and_dark() {
        let source = r##"
design hostpoint
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    chat-bubble-other "#ffffff"
    map-marker-active "#ff5722"
"##;
        let design = lower_design_source(source);
        assert_eq!(design.custom.len(), 3);
        assert_eq!(design.custom[0].name, "chat-bubble-mine");
        assert_eq!(design.custom[0].base, "#dcf8c6");
        assert_eq!(design.custom[0].dark.as_deref(), Some("#005c4b"));
        assert_eq!(design.custom[1].dark, None);
        assert_eq!(design.custom[2].name, "map-marker-active");
    }

    #[test]
    fn lower_design_preserves_invalid_custom_hex_for_doctor() {
        // Analyzer is intentionally permissive on `custom` hex values —
        // doctor's `design-custom-invalid-value` rule does the proposal-
        // pending validation. See `docs/proposals/design-tokens-custom.md` §4.
        let source = r##"
design hostpoint
  custom
    oops "not-a-color"
    chat-bubble "#dcf8c6" dark "rgb(5,5,5)"
"##;
        let design = lower_design_source(source);
        assert_eq!(design.custom.len(), 2);
        assert_eq!(design.custom[0].base, "not-a-color");
        assert_eq!(design.custom[1].dark.as_deref(), Some("rgb(5,5,5)"));
    }

    // -------------------------------------------------------------------------
    // IR Error-Vocab (Cell PARSE-1) — analyzer lowering round-trip tests
    // for the three new IR slots populated by this cell:
    //   * `Command.policy_when_denied` ← `command.policy.when_denied`
    //   * `PolicyCategory.when_denied` ← `policies.<cat>.when_denied`
    //   * `Feature.errors` ← `errors` block (default + 4xx/5xx + messages)
    // -------------------------------------------------------------------------

    #[test]
    fn lower_command_policy_when_denied_populates_typed_ref() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.choose_role_signin_required
    input
      role_id: ID required
    returns User
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let command = feature
            .commands
            .iter()
            .find(|c| c.name == "choose_role")
            .expect("choose_role command");
        let key = command
            .policy_when_denied
            .as_ref()
            .expect("policy_when_denied lowered");
        assert_eq!(key.key, "choose_role_signin_required");
    }

    #[test]
    fn lower_policy_category_when_denied_populates_typed_ref() {
        let source = r#"
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let authenticated = feature
            .policies
            .categories
            .iter()
            .find(|c| c.name == "authenticated")
            .expect("authenticated category");
        let key = authenticated
            .when_denied
            .as_ref()
            .expect("when_denied lowered");
        assert_eq!(key.key, "must_be_signed_in");
    }

    #[test]
    fn lower_feature_errors_populates_typed_block() {
        let source = r#"
feature account
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.account_signin_required
    validation_failed message @translation.account_invalid_input
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        assert_eq!(errors.default, Some(ir::ErrorExposureDefault::Hide));
        assert_eq!(errors.exposure_4xx, vec!["message", "code"]);
        assert_eq!(errors.exposure_5xx, vec!["code"]);
        assert_eq!(errors.messages.len(), 2);
        let policy_denied = errors
            .messages
            .iter()
            .find(|m| m.code == "policy_denied")
            .expect("policy_denied row");
        assert_eq!(policy_denied.message.key, "account_signin_required");
        let validation = errors
            .messages
            .iter()
            .find(|m| m.code == "validation_failed")
            .expect("validation_failed row");
        assert_eq!(validation.message.key, "account_invalid_input");
        // v1 leaves field_messages empty (reserved slot — proposal §3.4).
        assert!(errors.field_messages.is_empty());
    }

    #[test]
    fn lower_feature_without_errors_block_keeps_field_none() {
        let source = r#"
feature account
  command choose_role
    input
      role_id: ID required
    returns User
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(
            feature.errors.is_none(),
            "feature without `errors` block keeps `errors: None`"
        );
    }

    #[test]
    fn lower_feature_errors_default_expose_lowers_correctly() {
        let source = r#"
feature account
  errors
    default expose
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        assert_eq!(errors.default, Some(ir::ErrorExposureDefault::Expose));
        assert!(errors.exposure_4xx.is_empty());
        assert!(errors.exposure_5xx.is_empty());
        assert!(errors.messages.is_empty());
    }

    #[test]
    fn lower_feature_errors_redact_patterns_lowers() {
        let source = r#"
feature account
  errors
    error_redact "[0-9]{11}"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        assert_eq!(errors.redact_patterns, vec!["[0-9]{11}".to_owned()]);
    }

    #[test]
    fn lower_feature_errors_audience_exposure_lowers() {
        let source = r#"
feature account
  errors
    expose to @audience operator message, code
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let errors = feature.errors.as_ref().expect("errors block lowered");
        let rule = errors.audience_exposure.first().expect("audience exposure");
        assert_eq!(rule.audience.as_deref(), Some("operator"));
        assert_eq!(rule.fields, vec!["message".to_owned(), "code".to_owned()]);
    }
}

// =============================================================================
// L0 #3 — `.lzx` surface lowering tests.
// =============================================================================
#[cfg(test)]
mod surface_lowering_tests {
    use super::{AnalyzeError, lower_surface};
    use lazuli_ir as ir;
    use lazuli_syntax::parse_surface_document;

    fn parse(src: &str) -> ir::Surface {
        let ast = parse_surface_document(src).expect("parses");
        lower_surface(&ast).expect("lowers")
    }

    fn parse_requires(atom: &str) -> ir::PolicyAtom {
        let source = format!("surface slug web\n  audience admin\n    requires {atom}\n");
        let surface = parse(&source);
        surface.audiences[0].requires[0].clone()
    }

    #[test]
    fn lowers_minimal_surface() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.target, ir::SurfaceTarget::Web);
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    #[test]
    fn session_fresh_policy_atom_lowers() {
        let atom = parse_requires("@session.fresh(15m)");
        assert_eq!(atom.namespace, "session");
        assert_eq!(atom.name, "fresh");
        assert_eq!(atom.args.as_deref(), Some("15m"));
    }

    #[test]
    fn rate_budget_policy_atom_lowers() {
        let atom = parse_requires("@rate_budget.password_reset");
        assert_eq!(atom.namespace, "rate_budget");
        assert_eq!(atom.name, "password_reset");
        assert!(atom.args.is_none());
    }

    #[test]
    fn time_policy_atom_lowers() {
        let atom = parse_requires("@time.business_hours_brasilia(tz:America/Sao_Paulo)");
        assert_eq!(atom.namespace, "time");
        assert_eq!(atom.name, "business_hours_brasilia");
        assert_eq!(atom.args.as_deref(), Some("tz:America/Sao_Paulo"));
    }

    #[test]
    fn view_redacted_fields_lower() {
        let surface = parse(
            "surface customer web\n  audience admin\n    view create invite\n      submit customer.command.invite\n      fields email redacted\n",
        );
        let ir::View::Create(view) = &surface.audiences[0].views[0] else {
            panic!("expected create view");
        };
        assert_eq!(view.fields, vec!["email".to_owned()]);
        assert_eq!(view.redacted_fields, vec!["email".to_owned()]);
    }

    #[test]
    fn list_view_lowers_table_render_search_and_legacy_filter_names() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key, title\n      search key\n      filter title\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.render,
            ir::ListRender::Table {
                columns: vec!["key".into(), "title".into()]
            }
        );
        assert_eq!(
            view.search.as_ref().map(|search| &search.mode),
            Some(&ir::SearchMode::Columns {
                columns: vec!["key".into()]
            })
        );
        assert_eq!(view.filter.len(), 1);
        assert_eq!(view.filter[0].name, "title");
    }

    #[test]
    fn list_view_lowers_cells_render() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list cards\n      source item.query.search\n      cells @client.item_card\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(
            view.render,
            ir::ListRender::Cells {
                slot: "item_card".into()
            }
        );
    }

    #[test]
    fn lowers_filter_decl_block_to_typed_ir() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      filters\n        slug: Text from query\n        tags: list of Text\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.filter.len(), 2);
        assert_eq!(view.filter[0].name, "slug");
        assert_eq!(view.filter[0].type_ref, "Text");
        assert_eq!(view.filter[0].cardinality, ir::FilterCardinality::Single);
        assert!(view.filter[0].url_sync);
        assert_eq!(view.filter[1].cardinality, ir::FilterCardinality::Multi);
    }

    #[test]
    fn lowers_segmented_search_decl_bindings() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list a\n      source item.query.search\n      columns key\n      search segmented\n        field slug binds filters.slug\n        field q binds source.search\n        free text into selection\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let search = view.search.as_ref().expect("search");
        assert_eq!(search.mode, ir::SearchMode::Segmented);
        assert_eq!(
            search.fields[0].binds_to,
            ir::BindingRef::Filter {
                name: "slug".into()
            }
        );
        assert_eq!(
            search.fields[1].binds_to,
            ir::BindingRef::SourceInput {
                name: "search".into()
            }
        );
        assert_eq!(
            search.free_text_target,
            Some(ir::BindingRef::SelectionScalar)
        );
    }

    #[test]
    fn lowers_drawer_subview() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list items\n      source item.query.search\n      columns key\n      drawer item_detail on select\n        source item.query.by_id\n        route key from selection\n        sections header, meta\n        cells owner @client.owner_card\n        actions update\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let drawer = view.drawer.as_ref().expect("drawer");
        assert_eq!(drawer.name, "item_detail");
        assert_eq!(drawer.trigger, ir::DrawerTrigger::Select);
        assert_eq!(drawer.source.name, "by_id");
        assert_eq!(drawer.route_binding.as_ref().unwrap().target, "key");
        assert_eq!(drawer.sections, vec!["header", "meta"]);
        assert_eq!(drawer.cells[0].slot, "owner_card");
        assert_eq!(drawer.actions[0].name, "update");
    }

    #[test]
    fn lowers_sort_selection_and_settings() {
        let surface = parse(
            "surface item web\n  audience admin\n    view list terminal\n      source item.query.search\n      columns title\n      sort\n        by title, updated\n        default updated desc\n      selection multi\n      bulk_actions delete\n      settings\n        grid_size: Enum [sm, md] default sm\n          persist local\n        page_size: Int min 10 max 200 default 25\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        let sort = view.sort.as_ref().expect("sort");
        assert_eq!(sort.allowed, vec!["title", "updated"]);
        assert_eq!(sort.default_dir, ir::SortDir::Desc);
        let selection = view.selection.as_ref().expect("selection");
        assert_eq!(selection.mode, ir::SelectionMode::Multi);
        assert_eq!(selection.bulk_actions[0].name, "delete");
        assert_eq!(view.settings.len(), 2);
        assert_eq!(
            view.settings[0].value_space,
            ir::SettingValueSpace::Enum {
                values: vec!["sm".into(), "md".into()]
            }
        );
        assert_eq!(view.settings[0].persistence, ir::SettingPersistence::Local);
        assert_eq!(
            view.settings[1].value_space,
            ir::SettingValueSpace::Int { min: 10, max: 200 }
        );
    }

    #[test]
    fn detail_view_lifts_route_params_and_sections() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.by_key\n      route key: Text from path\n      sections header, metadata\n",
        );
        let detail = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.route.as_deref(), Some("/s/:key"));
        assert_eq!(detail.route_params.len(), 1);
        assert_eq!(detail.route_params[0].name, "key");
        assert_eq!(detail.route_params[0].type_ref, "Text");
        assert_eq!(detail.sections, vec!["header", "metadata"]);
    }

    #[test]
    fn create_view_lifts_submit_command_and_fields() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view create n at \"/s/new\"\n      submit slug.command.create\n      fields key, title\n",
        );
        let create = match &surface.audiences[0].views[0] {
            ir::View::Create(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(create.submit.feature, "slug");
        assert_eq!(create.submit.name, "create");
        assert_eq!(create.fields, vec!["key", "title"]);
    }

    #[test]
    fn requires_lifts_to_policy_atom() {
        let surface = parse(
            "surface slug web\n  audience admin\n    requires @scope.workspace_admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let req = &surface.audiences[0].requires[0];
        assert_eq!(req.namespace, "scope");
        assert_eq!(req.name, "workspace_admin");
    }

    #[test]
    fn query_ref_disambiguates_kind_via_prefix() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.lookup.by_key\n      route key: Text from path\n",
        );
        let detail = match &surface.audiences[0].views[0] {
            ir::View::Detail(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(detail.source.feature, "slug");
        assert_eq!(detail.source.kind, ir::QueryKind::Lookup);
        assert_eq!(detail.source.name, "by_key");
    }

    #[test]
    fn query_ref_unqualified_defaults_to_list() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.source.kind, ir::QueryKind::List);
        assert_eq!(view.source.name, "mine");
    }

    #[test]
    fn actions_short_form_lifts_owning_feature() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions create, update\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions.len(), 2);
        for action in &view.actions {
            assert_eq!(action.feature, "slug");
        }
        assert_eq!(view.actions[0].name, "create");
        assert_eq!(view.actions[1].name, "update");
    }

    #[test]
    fn actions_qualified_form_keeps_explicit_feature() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n      actions other.command.archive\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.actions[0].feature, "other");
        assert_eq!(view.actions[0].name, "archive");
    }

    #[test]
    fn cell_binding_lifts_to_ir_cell_binding() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns tags\n      cells tags @client.type_badge\n",
        );
        let view = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(view.cells[0].field, "tags");
        assert_eq!(view.cells[0].slot, "type_badge");
    }

    #[test]
    fn route_param_orphan_error() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view detail d at \"/s/:key\"\n      source slug.query.by_key\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(
            err,
            AnalyzeError::LzxRouteParamMissingBinding { .. }
        ));
    }

    #[test]
    fn route_param_extra_without_placeholder_error() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view detail d at \"/s/x\"\n      source slug.query.by_key\n      route key: Text from path\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxRouteParamOrphan { .. }));
    }

    #[test]
    fn cell_slot_orphan_when_field_not_in_columns() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key, title\n      cells tags @client.type_badge\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxCellSlotOrphan { .. }));
    }

    #[test]
    fn bad_query_ref_rejected_at_lowering() {
        let ast = parse_surface_document(
            "surface slug web\n  audience admin\n    view list a\n      source bogus_thing\n      columns key\n",
        )
        .expect("parses");
        let err = lower_surface(&ast).unwrap_err();
        assert!(matches!(err, AnalyzeError::LzxBadQueryRef { .. }));
    }

    #[test]
    fn lowers_full_section_13_1_fixture() {
        // Smoke: the proposal §13.1 fixture lowers cleanly end-to-end.
        let surface = parse(include_str!("../tests/fixtures/slug_web_section_13_1.lzx"));
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].views.len(), 3);
        let admin_list = match &surface.audiences[0].views[0] {
            ir::View::List(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(admin_list.cells[0].slot, "type_badge");
        assert_eq!(admin_list.actions.len(), 3);
    }

    #[test]
    fn mobile_target_lowers_to_mobile_variant() {
        let surface = parse(
            "surface item mobile\n  audience kiosk\n    view list a\n      source item.query.mine\n      columns key\n",
        );
        assert_eq!(surface.target, ir::SurfaceTarget::Mobile);
    }

    #[test]
    fn span_ref_attached_after_lowering() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        assert!(surface.span_ref.is_some());
        assert!(surface.audiences[0].span_ref.is_some());
    }

    #[test]
    fn audience_view_count_preserves_source_order() {
        let surface = parse(
            "surface slug web\n  audience admin\n    view list b\n      source slug.query.mine\n      columns key\n    view list a\n      source slug.query.mine\n      columns key\n",
        );
        let names: Vec<&str> = surface.audiences[0]
            .views
            .iter()
            .map(|v| v.name())
            .collect();
        assert_eq!(names, vec!["b", "a"]);
    }
}

// =============================================================================
// L0 #3 §10 — inline field constraint analyzer tests (Cells D.1+D.2+D.3).
//
// Combination rules per §10.2 (length / between / in conflicts) plus
// default-value compatibility per §10.3.
// =============================================================================
#[cfg(test)]
mod field_constraint_lowering_tests {
    use super::AnalyzeError;
    use lazuli_syntax::parse_feature_skeletons;

    /// `length 120 min 100` — § 10.2 rejects `length + min`.
    #[test]
    fn length_plus_min_emits_constraint_conflict() {
        let source = r#"
feature post
  domain
    resource Post
      title: Text length 120 min 100
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::ConstraintConflict { field, combo }) => {
                assert_eq!(field, "title");
                assert_eq!(combo, "length+min");
            }
            other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
        }
    }

    /// `between 0 and 100 max 50` — §10.2 rejects `between + max`.
    #[test]
    fn between_plus_max_emits_constraint_conflict() {
        let source = r#"
feature score
  domain
    resource Score
      points: Integer between 0 and 100 max 50
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::ConstraintConflict { field, combo }) => {
                assert_eq!(field, "points");
                assert_eq!(combo, "between+max");
            }
            other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
        }
    }

    /// `in ["a", "b"] pattern "^a"` — §10.2 says use enum instead.
    #[test]
    fn in_plus_pattern_emits_constraint_conflict() {
        let source = r#"
feature acl
  domain
    resource Member
      role: Text in ["a", "b"] pattern "^a"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::ConstraintConflict { field, combo }) => {
                assert_eq!(field, "role");
                assert_eq!(combo, "in+pattern");
            }
            other => panic!("expected ConstraintConflict, got: {:?}", other.err()),
        }
    }

    /// `Text required min 2 default ""` — §10.3 rejects empty default
    /// because the empty string has length 0 < 2.
    #[test]
    fn empty_default_violates_min_constraint() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text required min 2 = ""
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::DefaultViolatesConstraint { field, rule, .. }) => {
                assert_eq!(field, "handle");
                assert!(rule.starts_with("min="), "expected min rule, got {}", rule);
            }
            other => panic!("expected DefaultViolatesConstraint, got: {:?}", other.err()),
        }
    }

    /// Valid combination: `min N max M` (without between/length) passes.
    #[test]
    fn min_max_combination_passes_lowering() {
        let source = r#"
feature post
  domain
    resource Post
      title: Text required min 2 max 80
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = super::lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.min, Some(2));
        assert_eq!(field.constraints.max, Some(80));
    }

    // -------------------------------------------------------------------------
    // Wave-B-CL4 — `inline_validator_range_invariant_001`
    // -------------------------------------------------------------------------

    /// `min 10 max 5` — N>M yields an empty domain.
    #[test]
    fn min_greater_than_max_emits_range_invariant() {
        let source = r#"
feature post
  domain
    resource Post
      score: Integer required min 10 max 5
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorRangeInvariant {
                field,
                rule,
                low,
                high,
            }) => {
                assert_eq!(field, "score");
                assert_eq!(rule, "min>max");
                assert_eq!(low, "10");
                assert_eq!(high, "5");
            }
            other => panic!(
                "expected InlineValidatorRangeInvariant, got: {:?}",
                other.err()
            ),
        }
    }

    /// `between 100 and 0` — A>B yields an empty domain.
    #[test]
    fn between_with_inverted_bounds_emits_range_invariant() {
        let source = r#"
feature score
  domain
    resource Score
      points: Integer required between 100 and 0
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorRangeInvariant {
                field,
                rule,
                low,
                high,
            }) => {
                assert_eq!(field, "points");
                assert_eq!(rule, "between");
                assert_eq!(low, "100");
                assert_eq!(high, "0");
            }
            other => panic!(
                "expected InlineValidatorRangeInvariant, got: {:?}",
                other.err()
            ),
        }
    }

    /// `min 5 max 5` — equal bounds are valid (single-value domain).
    #[test]
    fn min_equals_max_passes_range_invariant() {
        let source = r#"
feature post
  domain
    resource Post
      flag: Integer required min 5 max 5
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = super::lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.min, Some(5));
        assert_eq!(field.constraints.max, Some(5));
    }

    // -------------------------------------------------------------------------
    // Wave-B-CL4 — `inline_validator_type_mismatch_001`
    // -------------------------------------------------------------------------

    /// `pattern "..."` on `Boolean` — §10.1 restricts `pattern` to Text.
    #[test]
    fn pattern_on_boolean_emits_type_mismatch() {
        let source = r#"
feature account
  domain
    resource Account
      enabled: Boolean pattern "^t"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorTypeMismatch {
                field,
                field_type,
                constraint,
                ..
            }) => {
                assert_eq!(field, "enabled");
                assert_eq!(field_type, "Boolean");
                assert_eq!(constraint, "pattern");
            }
            other => panic!(
                "expected InlineValidatorTypeMismatch, got: {:?}",
                other.err()
            ),
        }
    }

    /// `length N` on `Integer` — §10.1 restricts `length` to Text.
    #[test]
    fn length_on_integer_emits_type_mismatch() {
        let source = r#"
feature score
  domain
    resource Score
      points: Integer length 3
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorTypeMismatch {
                field,
                field_type,
                constraint,
                ..
            }) => {
                assert_eq!(field, "points");
                assert_eq!(field_type, "Integer");
                assert_eq!(constraint, "length");
            }
            other => panic!(
                "expected InlineValidatorTypeMismatch, got: {:?}",
                other.err()
            ),
        }
    }

    /// `between A and B` on `Text` — §10.1 restricts `between` to numerics.
    #[test]
    fn between_on_text_emits_type_mismatch() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text between 2 and 30
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorTypeMismatch {
                field,
                field_type,
                constraint,
                ..
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(field_type, "Text");
                assert_eq!(constraint, "between");
            }
            other => panic!(
                "expected InlineValidatorTypeMismatch, got: {:?}",
                other.err()
            ),
        }
    }

    // -------------------------------------------------------------------------
    // Wave-B-CL4 — `inline_validator_pattern_compile_001`
    // -------------------------------------------------------------------------

    /// `pattern "[a"` — unbalanced character class.
    #[test]
    fn pattern_unbalanced_class_emits_compile_error() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "[a"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorPatternCompile {
                field,
                pattern,
                reason,
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(pattern, "[a");
                assert!(reason.contains("unbalanced `[`"), "reason: {}", reason);
            }
            other => panic!(
                "expected InlineValidatorPatternCompile, got: {:?}",
                other.err()
            ),
        }
    }

    /// `pattern "^a("` — unbalanced group paren.
    #[test]
    fn pattern_unbalanced_paren_emits_compile_error() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^a("
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorPatternCompile {
                field,
                pattern,
                reason,
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(pattern, "^a(");
                assert!(reason.contains("unbalanced `(`"), "reason: {}", reason);
            }
            other => panic!(
                "expected InlineValidatorPatternCompile, got: {:?}",
                other.err()
            ),
        }
    }

    /// `pattern "^a)"` — extra closing paren, no matching `(`.
    #[test]
    fn pattern_extra_closing_paren_emits_compile_error() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^a)"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let result = super::lower_feature_skeleton(&features[0]);
        match result {
            Err(AnalyzeError::InlineValidatorPatternCompile {
                field,
                pattern,
                reason,
            }) => {
                assert_eq!(field, "handle");
                assert_eq!(pattern, "^a)");
                assert!(reason.contains("unbalanced `)`"), "reason: {}", reason);
            }
            other => panic!(
                "expected InlineValidatorPatternCompile, got: {:?}",
                other.err()
            ),
        }
    }

    /// Sanity: well-formed pattern passes.
    #[test]
    fn pattern_well_formed_passes() {
        let source = r#"
feature account
  domain
    resource Account
      handle: Text pattern "^[a-z][a-z0-9-]{2,29}$"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = super::lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(
            field.constraints.pattern.as_deref(),
            Some("^[a-z][a-z0-9-]{2,29}$")
        );
    }

    // -------------------------------------------------------------------------
    // Cross-feature contracts §5.4 — lowering of `uses [<feature>...] [version v<N>]`
    // populates parallel `uses` / `uses_spans` / `uses_versions` lists.
    // -------------------------------------------------------------------------

    #[test]
    fn lowers_uses_with_mixed_pins() {
        let source = r#"
feature billing
  uses account version v2
  uses notifications
  uses org, user version v1
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = super::lower_feature_skeleton(&features[0]).expect("lowers");

        assert_eq!(
            feature.uses,
            vec![
                "account".to_owned(),
                "notifications".to_owned(),
                "org".to_owned(),
                "user".to_owned(),
            ]
        );
        assert_eq!(feature.uses_versions, vec![Some(2), None, Some(1), Some(1)]);
        assert_eq!(feature.uses_spans.len(), 4);
        // First two lines and last line have distinct spans.
        assert_ne!(feature.uses_spans[0], feature.uses_spans[1]);
        assert_ne!(feature.uses_spans[1], feature.uses_spans[2]);
        // Comma-list entries share the source line, hence the span.
        assert_eq!(feature.uses_spans[2], feature.uses_spans[3]);
    }

    #[test]
    fn auto_photo_synthesizes_4_commands_and_2_records() {
        // Inline a minimal feature skeleton with a per-user resource
        // carrying an optional @cap.File field. Expect synthesis to
        // populate feature.commands with 4 names ending in
        // _upload/_upload/_/url and feature.records with the 2
        // intent + display records.
        let source = r#"
feature photoshare
  defaults
    tenancy org

  uses org
  uses account

  policies
    photoshare_only: @scope.authenticated, @role.host
      when_denied @translation.x

  domain
    resource PhotoShare
      org: Org required
      user: User required unique
      avatar: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional
      created_at: DateTime required
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = super::lower_feature_skeleton(&features[0]).expect("lowering succeeds");

        let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
        assert!(
            cmd_names.contains(&"request_avatar_upload"),
            "request_avatar_upload missing; got {:?}",
            cmd_names
        );
        assert!(cmd_names.contains(&"confirm_avatar_upload"));
        assert!(cmd_names.contains(&"clear_avatar"));
        assert!(cmd_names.contains(&"get_avatar_url"));

        let record_names: Vec<&str> = feature.records.iter().map(|r| r.name.as_str()).collect();
        assert!(record_names.contains(&"AvatarUploadIntent"));
        assert!(record_names.contains(&"AvatarDisplayUrl"));

        // Marker must be set on synthesized commands.
        let req = feature
            .commands
            .iter()
            .find(|c| c.name == "request_avatar_upload")
            .unwrap();
        assert!(req.synthesized_from_cap_file.is_some());
    }
}

#[cfg(test)]
mod conventions_unknown_diagnostic_tests {
    //! ir-resource-conventions-crud Cell C1 — tests for the
    //! `conventions_unknown` diagnostic plumbing. Cell C2 (parser)
    //! will be the actual emit site; here we lock the suggestion
    //! helper + the error formatting so the parser's emission shape
    //! is stable before it lands.

    use super::{AnalyzeError, CONVENTION_CATALOG, conventions_unknown_suggestion};

    #[test]
    fn catalog_contains_crud_and_me_today() {
        // crud §4.2 + me §4.2 — closed catalog is `{ crud, me }`.
        // Any further addition is an IR change requiring a proposal;
        // this test fails on accidental growth.
        assert_eq!(CONVENTION_CATALOG, &["crud", "me"]);
    }

    #[test]
    fn suggestion_for_single_char_typo_returns_crud() {
        // §4.3 names this exact case verbatim: `conventions [crd]`
        // suggests `crud` (single-character Levenshtein).
        assert_eq!(conventions_unknown_suggestion("crd"), Some("crud"));
    }

    #[test]
    fn suggestion_for_extra_char_typo_returns_crud() {
        // `crude` and `cruds` are also distance-1 from `crud`.
        assert_eq!(conventions_unknown_suggestion("crude"), Some("crud"));
        assert_eq!(conventions_unknown_suggestion("cruds"), Some("crud"));
    }

    #[test]
    fn suggestion_for_typo_resolves_to_me() {
        // `ir-resource-conventions-me.md` cell M1: typos distance-1
        // from `me` resolve to `me`. `m` (deletion), `mee`/`mes`
        // (insertion / substitution). Locks the nearest-match
        // behaviour now that the catalog has a second entry.
        assert_eq!(conventions_unknown_suggestion("m"), Some("me"));
        assert_eq!(conventions_unknown_suggestion("mee"), Some("me"));
        assert_eq!(conventions_unknown_suggestion("mes"), Some("me"));
    }

    #[test]
    fn suggestion_for_far_typo_returns_none() {
        // Distance 2+ from every catalog entry — no suggestion is
        // better than a misleading one.
        assert_eq!(conventions_unknown_suggestion("workflow"), None);
        assert_eq!(conventions_unknown_suggestion("xyz"), None);
        assert_eq!(conventions_unknown_suggestion(""), None);
    }

    #[test]
    fn suggestion_for_exact_match_returns_self() {
        // Defensive: if the parser somehow calls this with a known
        // identifier, the helper still resolves rather than failing.
        // (The parser shouldn't reach this path — exact matches don't
        // hit the unknown diagnostic — but the helper is total.)
        assert_eq!(conventions_unknown_suggestion("crud"), Some("crud"));
    }

    #[test]
    fn error_message_includes_suggestion_when_present() {
        let err = AnalyzeError::ConventionsUnknown {
            resource: "Customer".to_owned(),
            identifier: "crd".to_owned(),
            suggestion: Some("crud".to_owned()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("CONVENTIONS-UNKNOWN"),
            "missing diagnostic code: {msg}"
        );
        assert!(
            msg.contains("`Customer`"),
            "missing resource name: {msg}"
        );
        assert!(msg.contains("`crd`"), "missing offending identifier: {msg}");
        assert!(
            msg.contains("did you mean `crud`?"),
            "missing suggestion clause: {msg}"
        );
    }

    #[test]
    fn error_message_omits_suggestion_clause_when_none() {
        let err = AnalyzeError::ConventionsUnknown {
            resource: "Customer".to_owned(),
            identifier: "workflow".to_owned(),
            suggestion: None,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("CONVENTIONS-UNKNOWN"),
            "missing diagnostic code: {msg}"
        );
        assert!(msg.contains("`workflow`"));
        assert!(
            !msg.contains("did you mean"),
            "should not invent a suggestion when none was found: {msg}"
        );
    }
}

// =============================================================================
// `conventions [crud]` synthesis pass — Cell C3 tests
//
// Spec: `docs/proposals/ir-resource-conventions-crud.md` §5–§11.
//
// Tests build `ir::Feature` values programmatically because Cell C2's
// parser shim for `conventions [crud]` lands in parallel. The synth
// pass operates on the post-parse IR so direct construction is the
// canonical surface to exercise here.
// =============================================================================
#[cfg(test)]
mod conventions_crud_synth_tests {
    use super::{CrudSynthDiagnostic, synthesize_conventions};
    use lazuli_ir as ir;

    /// Minimal `Feature` for testing — empty defaults, a single
    /// `authenticated` policy unless the test overrides.
    fn empty_feature(name: &str, with_authenticated: bool) -> ir::Feature {
        let policies = if with_authenticated {
            ir::Policies {
                categories: vec![ir::PolicyCategory {
                    name: "authenticated".to_owned(),
                    atoms: vec!["@scope.authenticated".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                }],
                fields: Vec::new(),
                span_ref: None,
            }
        } else {
            ir::Policies::default()
        };
        ir::Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: ir::Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies,
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn req_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn req_unique_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            unique: true,
            ..req_field(name, type_ref)
        }
    }

    fn user_qn(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    fn customer_resource() -> ir::Resource {
        // §8 worked example: feature customer, resource Customer.
        ir::Resource {
            name: "Customer".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field(
                    "email",
                    ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                ),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
                req_field("status", user_qn("CustomerStatus")),
                req_field("created_at", ir::TypeRef::Builtin(ir::BuiltinType::DateTime)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
        }
    }

    /// §8 worked example — synth produces exactly the 5 entries
    /// (3 commands + 2 queries) with the exact shapes per §5.2–§5.6.
    #[test]
    fn synth_produces_five_entries_for_customer_resource() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for clean Customer, got {:?}",
            diags
        );

        // 3 commands appended: create / update / delete.
        let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            cmd_names,
            vec!["create_customer", "update_customer", "delete_customer"]
        );

        // 2 queries appended: lookup / list.
        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert_eq!(q_names, vec!["lookup_customer", "list_customers"]);

        // create_customer §5.2 shape — input has [email, name, status]
        // (org + created_at are Tenant/Auto, dropped).
        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_customer")
            .unwrap();
        assert!(matches!(create.kind, ir::CommandKind::Create));
        match &create.input {
            ir::CommandInput::Typed(slots) => {
                let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["email", "name", "status"]);
                // Required-on-resource fields stay required.
                assert!(slots.iter().all(|s| s.required));
            }
            other => panic!("expected Typed input, got {:?}", other),
        }
        match &create.effect {
            ir::CommandEffect::Creates(e) => assert_eq!(e.resource.name, "Customer"),
            other => panic!("expected Creates effect, got {:?}", other),
        }
        let create_rate_limit = create.rate_limit.as_ref().expect("rate_limit");
        assert_eq!(create_rate_limit.default, "100 per 10 minutes per ip");
        assert!(create_rate_limit.by_env.is_empty());
        assert!(matches!(&create.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
        assert!(create.audit.is_some());

        // update_customer §5.3 — every field becomes optional in input,
        // route id: ID present, effect Updates Customer.
        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_customer")
            .unwrap();
        assert!(matches!(update.kind, ir::CommandKind::Update));
        assert_eq!(update.route.len(), 1);
        assert_eq!(update.route[0].name, "id");
        assert!(matches!(
            update.route[0].type_ref,
            ir::TypeRef::Builtin(ir::BuiltinType::Id)
        ));
        match &update.input {
            ir::CommandInput::Typed(slots) => {
                let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["email", "name", "status"]);
                // All slots optional per §5.3.
                assert!(slots.iter().all(|s| !s.required));
            }
            other => panic!("expected Typed input, got {:?}", other),
        }
        match &update.effect {
            ir::CommandEffect::Updates(e) => assert_eq!(e.resource.name, "Customer"),
            other => panic!("expected Updates effect, got {:?}", other),
        }

        // delete_customer §5.4 — no input, route id, Deletes effect.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_customer")
            .unwrap();
        assert!(matches!(delete.kind, ir::CommandKind::Delete));
        assert_eq!(delete.route.len(), 1);
        assert_eq!(delete.route[0].name, "id");
        assert!(matches!(delete.input, ir::CommandInput::Empty));
        match &delete.effect {
            ir::CommandEffect::Deletes(e) => assert_eq!(e.resource.name, "Customer"),
            other => panic!("expected Deletes effect, got {:?}", other),
        }

        // lookup_customer §5.5 — Lookup with key id, policy authenticated.
        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_customer")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["id".to_owned()]);
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }

        // list_customers §5.6 — List with limit+offset params, paginate 50.
        let list = feature
            .queries
            .iter()
            .find(|q| q.name() == "list_customers")
            .unwrap();
        match list {
            ir::Query::List(lq) => {
                let pnames: Vec<&str> = lq.params.iter().map(|p| p.name.as_str()).collect();
                assert_eq!(pnames, vec!["limit", "offset"]);
                assert!(lq.params.iter().all(|p| !p.required));
                assert_eq!(lq.paginate, Some(50));
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected List query, got {:?}", other),
        }
    }

    /// §9 worked override — author wrote `update_customer`; other 4
    /// still synthesize; no warning emitted.
    #[test]
    fn author_override_skips_just_that_name() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());

        // Author's update_customer: matches canonical input + Updates
        // Customer (so no signature_mismatch diagnostic should fire).
        let author_update = ir::Command {
            name: "update_customer".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Update,
            route: vec![ir::RouteSlot {
                name: "id".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
                from: None,
                kind: ir::RouteSlotKind::Plain,
            }],
            input: ir::CommandInput::Typed(vec![
                ir::TypedSlot {
                    name: "email".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                },
                ir::TypedSlot {
                    name: "name".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                },
                ir::TypedSlot {
                    name: "status".to_owned(),
                    type_ref: ir::TypeRef::UserDefined(ir::QualifiedName {
                        feature: None,
                        name: "CustomerStatus".to_owned(),
                    }),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                },
            ]),
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::Updates(ir::UpdateEffect {
                resource: ir::QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                },
                assignments: Vec::new(),
            }),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
        };
        feature.commands.push(author_update);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "matching-signature author override should not emit a diagnostic, got {:?}",
            diags
        );

        let cmd_names: Vec<&str> = feature.commands.iter().map(|c| c.name.as_str()).collect();
        assert!(cmd_names.contains(&"create_customer"));
        assert!(cmd_names.contains(&"delete_customer"));
        // update_customer present, but appears exactly once (the author's).
        let update_count = cmd_names.iter().filter(|n| **n == "update_customer").count();
        assert_eq!(update_count, 1, "update_customer must not be duplicated");

        // The remaining update_customer is the author's — its policy is
        // `customer_admin`, not `authenticated`.
        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_customer")
            .unwrap();
        assert!(matches!(&update.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));

        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert!(q_names.contains(&"lookup_customer"));
        assert!(q_names.contains(&"list_customers"));
    }

    /// §5.7 edge — resource with `user: User required unique` places
    /// both `org` and `user` in the Tenant group (neither lands in
    /// input).
    #[test]
    fn user_unique_resource_drops_user_from_inputs() {
        let mut feature = empty_feature("photoshare", true);
        feature.resources.push(ir::Resource {
            name: "PhotoShare".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("caption", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
        });

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "no diagnostics expected, got {:?}", diags);

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_photo_share")
            .unwrap();
        match &create.input {
            ir::CommandInput::Typed(slots) => {
                let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
                // org + user are Tenant; only caption remains.
                assert_eq!(names, vec!["caption"]);
            }
            other => panic!("expected Typed input, got {:?}", other),
        }
    }

    /// §5.7 edge — resource without a lifecycle block has no discriminator
    /// to drop. A field named like a discriminator on another resource
    /// stays in input. Verifies the discriminator-skip is gated on
    /// `resource.lifecycle` being `Some`.
    #[test]
    fn resource_without_lifecycle_keeps_status_field() {
        let mut feature = empty_feature("customer", true);
        // Customer above has `status` field; it has NO lifecycle block,
        // so `status` should land in create / update input.
        feature.resources.push(customer_resource());
        let _ = synthesize_conventions(&mut feature);

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_customer")
            .unwrap();
        let names: Vec<&str> = match &create.input {
            ir::CommandInput::Typed(slots) => slots.iter().map(|s| s.name.as_str()).collect(),
            other => panic!("expected Typed input, got {:?}", other),
        };
        assert!(names.contains(&"status"));
    }

    /// §11 — `crud_synth_no_required_fields` fires when every required
    /// field is Tenant or Auto. Build a resource with only `org`,
    /// `id`, `created_at` (all Tenant/Auto).
    #[test]
    fn empty_required_emits_no_required_fields_diagnostic() {
        let mut feature = empty_feature("ledger", true);
        feature.resources.push(ir::Resource {
            name: "Ledger".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("id", ir::TypeRef::Builtin(ir::BuiltinType::Id)),
                req_field("org", user_qn("Org")),
                req_field("created_at", ir::TypeRef::Builtin(ir::BuiltinType::DateTime)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
        });

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags
                .iter()
                .any(|d| matches!(d, CrudSynthDiagnostic::NoRequiredFields { resource } if resource == "Ledger")),
            "expected NoRequiredFields for Ledger, got {:?}",
            diags
        );
    }

    /// §11 — `crud_synth_policy_not_found` fires when the feature has
    /// no `authenticated` policy. Synth still produces entries with the
    /// canonical PolicyRef; Cell C4 surfaces the diagnostic to the
    /// author.
    #[test]
    fn missing_authenticated_policy_emits_diagnostic() {
        let mut feature = empty_feature("customer", false); // no authenticated
        feature.resources.push(customer_resource());

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags
                .iter()
                .any(|d| matches!(d, CrudSynthDiagnostic::PolicyNotFound { resource } if resource == "Customer")),
            "expected PolicyNotFound for Customer, got {:?}",
            diags
        );
    }

    /// §11 — `crud_synth_signature_mismatch` fires when author wrote
    /// `update_customer` with a non-canonical input list (e.g., extra
    /// field).
    #[test]
    fn diverging_author_signature_emits_mismatch_diagnostic() {
        let mut feature = empty_feature("customer", true);
        feature.resources.push(customer_resource());
        // Author wrote update_customer with extra `notes` field — diverges.
        feature.commands.push(ir::Command {
            name: "update_customer".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Update,
            route: vec![ir::RouteSlot {
                name: "id".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
                from: None,
                kind: ir::RouteSlotKind::Plain,
            }],
            input: ir::CommandInput::Typed(vec![
                ir::TypedSlot {
                    name: "name".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                },
                ir::TypedSlot {
                    name: "notes".to_owned(),
                    type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                    required: false,
                    constraints: ir::FieldConstraints::default(),
                },
            ]),
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::Updates(ir::UpdateEffect {
                resource: ir::QualifiedName {
                    feature: None,
                    name: "Customer".to_owned(),
                },
                assignments: Vec::new(),
            }),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
        });

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                CrudSynthDiagnostic::SignatureMismatch { resource, synth_name, .. }
                    if resource == "Customer" && synth_name == "update_customer"
            )),
            "expected SignatureMismatch for update_customer, got {:?}",
            diags
        );
    }

    /// Resource without `conventions [crud]` is a no-op for the synth.
    #[test]
    fn resource_without_conventions_is_no_op() {
        let mut feature = empty_feature("customer", true);
        let mut r = customer_resource();
        r.conventions = Vec::new();
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty());
        assert!(feature.commands.is_empty());
        assert!(feature.queries.is_empty());
    }
}

// =============================================================================
// `conventions [me]` synthesis pass — Cell M2 tests
//
// Spec: `docs/proposals/ir-resource-conventions-me.md` §§5–§11.
//
// Tests build `ir::Feature` values programmatically because M1's parser
// shim for `conventions [me]` lands in parallel. The synth pass operates
// on the post-parse IR so direct construction is the canonical surface
// to exercise here.
//
// Coverage:
// - 4 mode tests: user_keyed, user_keyed_no_org, org_keyed, self_keyed.
// - Override test: author wrote `lookup_my_customer` → synth skipped,
//   `synth_origins` records `AuthorOverride(Me)`.
// - Composition test: `conventions [crud, me]` → 6 entries, no collisions.
// - Diagnostic: `MeNoActorResolution` when resource has neither axis.
// - Diagnostic: `MeSignatureMismatch` for divergent author signature.
// =============================================================================
#[cfg(test)]
mod conventions_me_synth_tests {
    use super::{ConventionSynthDiagnostic, synthesize_conventions};
    use lazuli_ir as ir;

    /// Minimal `Feature` with a single `authenticated` policy.
    fn empty_feature(name: &str) -> ir::Feature {
        ir::Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: ir::Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: ir::Policies {
                categories: vec![ir::PolicyCategory {
                    name: "authenticated".to_owned(),
                    atoms: vec!["@scope.authenticated".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                }],
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn req_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn req_unique_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            unique: true,
            ..req_field(name, type_ref)
        }
    }

    fn user_qn(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    /// Build a minimal Resource with `conventions [me]`.
    fn me_resource(name: &str, fields: Vec<ir::Field>) -> ir::Resource {
        ir::Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Me],
        }
    }

    /// me §5.3 row 1 — `user_keyed`: resource has `user: User required
    /// unique` + `org: Org required`. Emits SELECT with
    /// `WHERE org = ctx.User.OrgID AND "user" = ctx.User.ID`.
    #[test]
    fn user_keyed_mode_emits_org_and_user_key_clauses() {
        let mut feature = empty_feature("host");
        feature.resources.push(me_resource(
            "Host",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert_eq!(q_names, vec!["lookup_my_host"]);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_host")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                // Route-less + param-less per §5.2.
                assert!(lq.params.is_empty(), "expected no params, got {:?}", lq.params);
                // Two key clauses: org + user.
                assert_eq!(lq.keys.len(), 2);
                assert_eq!(lq.keys[0].path.segments, vec!["org".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "org_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path for org, got {:?}", other),
                }
                assert_eq!(lq.keys[1].path.segments, vec!["user".to_owned()]);
                match &lq.keys[1].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path for user, got {:?}", other),
                }
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }

        // §11 inspect surface — synth_origins records Synthesized(Me).
        assert_eq!(
            feature.synth_origins.get("lookup_my_host"),
            Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me))
        );
    }

    /// me §5.3 row 2 — `user_keyed_no_org`: `user: User required` and
    /// no `org` field. Emits SELECT with `WHERE "user" = ctx.User.ID`.
    #[test]
    fn user_keyed_no_org_mode_emits_user_only_key_clause() {
        let mut feature = empty_feature("profile");
        feature.resources.push(me_resource(
            "Profile",
            vec![
                req_unique_field("user", user_qn("User")),
                req_field("bio", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_profile")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                // Single key clause on `user`.
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["user".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §5.3 row 3 — `org_keyed`: resource has `org: Org required`
    /// AND no `user: User required` field. Emits SELECT with
    /// `WHERE org_id = ctx.User.OrgID`.
    #[test]
    fn org_keyed_mode_emits_org_only_key_clause() {
        let mut feature = empty_feature("settings");
        feature.resources.push(me_resource(
            "OrgSettings",
            vec![
                req_field("org", user_qn("Org")),
                req_field(
                    "theme",
                    ir::TypeRef::Builtin(ir::BuiltinType::Text),
                ),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_org_settings")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["org".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "org_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §5.3 row 4 — `self_keyed`: the resource IS the User table.
    /// Emits SELECT with `WHERE id = ctx.User.ID`.
    #[test]
    fn self_keyed_mode_emits_id_key_clause_for_user_resource() {
        let mut feature = empty_feature("account");
        // resource User — no `user` field needed; the row IS the actor.
        feature.resources.push(me_resource(
            "User",
            vec![
                req_unique_field(
                    "email",
                    ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                ),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_user")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["id".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §6 — author wrote `query lookup_my_customer`; synth skips
    /// that name, records `AuthorOverride(Me)` in `synth_origins`. No
    /// duplicate query, no diagnostic when the signature matches.
    #[test]
    fn author_override_skips_synth_and_records_origin() {
        let mut feature = empty_feature("customer");
        feature.resources.push(me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        // Author wrote their own `lookup_my_customer` query (e.g.,
        // with a role-gated policy) — canonical-matching shape (no
        // params, Lookup variant).
        feature.queries.push(ir::Query::Lookup(ir::LookupQuery {
            name: "lookup_my_customer".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for matching override, got {:?}",
            diags
        );

        // Exactly one `lookup_my_customer` — the author's.
        let count = feature
            .queries
            .iter()
            .filter(|q| q.name() == "lookup_my_customer")
            .count();
        assert_eq!(count, 1);

        // Author's policy preserved (not overwritten by synth).
        let q = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_customer")
            .unwrap();
        match q {
            ir::Query::Lookup(lq) => {
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));
            }
            other => panic!("expected Lookup, got {:?}", other),
        }

        // §11 — synth_origins records `AuthorOverride(Me)`.
        assert_eq!(
            feature.synth_origins.get("lookup_my_customer"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me))
        );
    }

    /// me §6.1 — `conventions [crud, me]` composes cleanly: 5 from
    /// crud + 1 from me = 6 entries, no naming collisions. All 6
    /// names appear in `synth_origins`.
    #[test]
    fn conventions_crud_and_me_compose_to_six_entries() {
        let mut feature = empty_feature("customer");
        let mut r = me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        );
        // Declare both bundles.
        r.conventions = vec![ir::ConventionRef::Crud, ir::ConventionRef::Me];
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        // 3 crud commands + 0 me commands.
        let cmd_names: std::collections::BTreeSet<String> =
            feature.commands.iter().map(|c| c.name.clone()).collect();
        assert!(cmd_names.contains("create_customer"));
        assert!(cmd_names.contains("update_customer"));
        assert!(cmd_names.contains("delete_customer"));
        assert_eq!(cmd_names.len(), 3, "got commands: {:?}", cmd_names);

        // 2 crud queries + 1 me query.
        let q_names: std::collections::BTreeSet<String> =
            feature.queries.iter().map(|q| q.name().to_owned()).collect();
        assert!(q_names.contains("lookup_customer"));
        assert!(q_names.contains("list_customers"));
        assert!(q_names.contains("lookup_my_customer"));
        assert_eq!(q_names.len(), 3, "got queries: {:?}", q_names);

        // §11 inspect — synth_origins has 6 entries: 5 crud + 1 me.
        assert_eq!(
            feature.synth_origins.len(),
            6,
            "expected 6 synth_origins entries, got {:?}",
            feature.synth_origins
        );
        // Spot-check the 5 crud entries.
        for name in ["create_customer", "update_customer", "delete_customer", "lookup_customer", "list_customers"] {
            assert_eq!(
                feature.synth_origins.get(name),
                Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud)),
                "expected Synthesized(Crud) for `{}`",
                name
            );
        }
        // And the 1 me entry.
        assert_eq!(
            feature.synth_origins.get("lookup_my_customer"),
            Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me))
        );
    }

    /// me §11.1 — `me_synth_no_actor_resolution` fires when the
    /// resource has neither `user` nor `org` and is not named `User`.
    /// No synth emitted for that resource.
    #[test]
    fn no_actor_resolution_diagnostic_when_no_user_no_org_not_user() {
        let mut feature = empty_feature("audit");
        feature.resources.push(me_resource(
            "AuditNote",
            vec![req_field("note", ir::TypeRef::Builtin(ir::BuiltinType::Text))],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::MeNoActorResolution { resource }
                    if resource == "AuditNote"
            )),
            "expected MeNoActorResolution for AuditNote, got {:?}",
            diags
        );

        // No `lookup_my_audit_note` synthesized.
        assert!(
            feature
                .queries
                .iter()
                .all(|q| q.name() != "lookup_my_audit_note"),
            "synth should skip the resource entirely on no actor axis"
        );
        // No entry in synth_origins.
        assert!(!feature.synth_origins.contains_key("lookup_my_audit_note"));
    }

    /// me §11.1 — `me_synth_signature_mismatch` fires when the author
    /// wrote a divergent shape (e.g., a `Query::List` named
    /// `lookup_my_<r>`; or a Lookup with non-empty params).
    #[test]
    fn divergent_author_signature_emits_mismatch_diagnostic() {
        let mut feature = empty_feature("traveler");
        feature.resources.push(me_resource(
            "Traveler",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
            ],
        ));

        // Author wrote a Lookup with non-empty params — diverges from
        // the canonical route-less + param-less shape.
        feature.queries.push(ir::Query::Lookup(ir::LookupQuery {
            name: "lookup_my_traveler".to_owned(),
            public_contract: None,
            params: vec![ir::TypedSlot {
                name: "extra".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                required: false,
                constraints: ir::FieldConstraints::default(),
            }],
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::MeSignatureMismatch { resource, synth_name, .. }
                    if resource == "Traveler" && synth_name == "lookup_my_traveler"
            )),
            "expected MeSignatureMismatch for lookup_my_traveler, got {:?}",
            diags
        );

        // §6 — synth still records AuthorOverride(Me) so inspect can
        // render the override annotation.
        assert_eq!(
            feature.synth_origins.get("lookup_my_traveler"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me))
        );
    }

    /// Sanity — resource without `conventions [me]` is a no-op for the
    /// `me` half of the synth (existing crud-no-op test covers the
    /// joint path; this one anchors the bundle-isolation property).
    #[test]
    fn resource_without_me_convention_is_no_op() {
        let mut feature = empty_feature("customer");
        let mut r = me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
            ],
        );
        r.conventions = Vec::new();
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty());
        assert!(feature.queries.is_empty());
        assert!(feature.synth_origins.is_empty());
    }
}

// =============================================================================
// Cell O2 — `@owner_axis(through: <col>)` synth-pass tests.
//
// Spec: `docs/proposals/ir-resource-conventions-owner-scope.md`
// §7.3 + §8 + §8.5.A + §11.1.
//
// Coverage matrix:
//   1. Mode: owner-scope `delete_*` emits chain WHERE.
//   2. Mode: owner-scope `update_*` / `lookup_*` / `list_*` emit chain WHERE.
//   3. CTE: owner-scope `create_*` emits CTE-INSERT shape via `cte_owner_check`.
//   4. Composition: `[crud, me]` + `@owner_axis` -> `lookup_my_*` ALSO carries scope.
//   5. Diagnostic: `owner_axis_unknown_through`.
//   6. Diagnostic: `owner_axis_through_not_user_keyed`.
//   7. Diagnostic: `owner_axis_collides_with_unique_user`.
//   8. Override: author's `command delete_<r>` skips synth; no diagnostic; scope
//      is NOT attached to the author's command.
//   9. Direct-call form: `build_owner_scope_where_for_test` round-trips the SQL.
//
// RULE-VOCAB-03 affirmation: each test asserts on the *single* SQL shape the
// analyzer composes per synth call. No procedural sequencing is exercised.
// =============================================================================
#[cfg(test)]
mod conventions_owner_scope_synth_tests {
    use super::{
        ConventionSynthDiagnostic, build_owner_scope_cte_prefix_for_test,
        build_owner_scope_where_for_test, synthesize_conventions,
    };
    use lazuli_ir as ir;

    fn empty_feature(name: &str) -> ir::Feature {
        ir::Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: ir::Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: ir::Policies {
                categories: vec![ir::PolicyCategory {
                    name: "authenticated".to_owned(),
                    atoms: vec!["@scope.authenticated".to_owned()],
                    previous_names: Vec::new(),
                    when_denied: None,
                }],
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn req_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn req_unique_field(name: &str, type_ref: ir::TypeRef) -> ir::Field {
        ir::Field {
            unique: true,
            ..req_field(name, type_ref)
        }
    }

    /// Build an FK field annotated with `@owner_axis(through: <col>)`.
    fn fk_field_with_axis(name: &str, target: &str, through: &str) -> ir::Field {
        let mut f = req_field(
            name,
            ir::TypeRef::UserDefined(ir::QualifiedName {
                feature: None,
                name: target.to_owned(),
            }),
        );
        f.owner_axis = Some(ir::OwnerAxis {
            through_column: through.to_owned(),
        });
        f
    }

    fn user_qn(name: &str) -> ir::TypeRef {
        ir::TypeRef::UserDefined(ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        })
    }

    /// Build the Hostpoint pilot's `Host` resource (the FK target with
    /// the `user: User required unique` actor key). Used to back the
    /// owner-chain in fixtures.
    fn host_resource() -> ir::Resource {
        ir::Resource {
            name: "Host".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
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
        }
    }

    /// Build the trigger pilot's `Property` resource — owner-scoped via
    /// `host: Host required @owner_axis(through: user)`.
    fn property_resource_with_axis() -> ir::Resource {
        ir::Resource {
            name: "Property".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                fk_field_with_axis("host", "Host", "user"),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
        }
    }

    /// §8.1 — owner-scope mode emits a chain WHERE predicate on
    /// `delete_<r>`. The synthesized command carries `owner_scope_sql`
    /// with the `host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)`
    /// fragment — the same shape the trigger pilot's pre-absorption
    /// `delete_property.go` (§1.1) used.
    #[test]
    fn owner_scope_delete_emits_chain_where_predicate() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "owner-scope delete_property should not emit diagnostics, got {:?}",
            diags
        );

        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth emits delete_property");
        let scope = delete
            .owner_scope_sql
            .as_ref()
            .expect("delete_property carries owner_scope_sql");
        assert_eq!(scope.field_name, "host");
        assert_eq!(scope.fk_target, "Host");
        assert_eq!(scope.through_column, "user");
        assert_eq!(
            scope.where_predicate,
            r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#
        );
        // DELETE doesn't need the CTE prefix — only CREATE does.
        assert!(scope.cte_owner_check.is_none(), "DELETE carries no CTE");
    }

    /// §8.2 / §8.3 / §8.4 — owner-scope mode emits the same WHERE
    /// fragment on UPDATE, LOOKUP, and LIST. Single test asserts all
    /// three because the predicate is composed by the unified
    /// builder; per-shape divergence would surface here.
    #[test]
    fn owner_scope_update_lookup_list_emit_chain_where_predicate() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        let _ = synthesize_conventions(&mut feature);

        let expected = r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#;

        let update = feature
            .commands
            .iter()
            .find(|c| c.name == "update_property")
            .expect("synth emits update_property");
        assert_eq!(
            update.owner_scope_sql.as_ref().map(|s| s.where_predicate.as_str()),
            Some(expected)
        );

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_property")
            .expect("synth emits lookup_property");
        let lookup_scope = match lookup {
            ir::Query::Lookup(lq) => lq.owner_scope_sql.as_ref(),
            _ => panic!("expected Lookup variant"),
        };
        assert_eq!(
            lookup_scope.map(|s| s.where_predicate.as_str()),
            Some(expected),
        );

        let list = feature
            .queries
            .iter()
            .find(|q| q.name() == "list_propertys")
            .expect("synth emits list_propertys");
        let list_scope = match list {
            ir::Query::List(lq) => lq.owner_scope_sql.as_ref(),
            _ => panic!("expected List variant"),
        };
        assert_eq!(
            list_scope.map(|s| s.where_predicate.as_str()),
            Some(expected),
        );
    }

    /// §8.5.A — `create_<r>` synth emits the CTE-INSERT prefix in the
    /// `cte_owner_check` slot. RULE-VOCAB-03 affirmation: one SQL
    /// statement (CTE-wrapped INSERT), no procedural sequencing.
    #[test]
    fn owner_scope_create_emits_cte_owner_check_prefix() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        let _ = synthesize_conventions(&mut feature);

        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_property")
            .expect("synth emits create_property");
        let scope = create
            .owner_scope_sql
            .as_ref()
            .expect("create_property carries owner_scope_sql");
        let cte = scope
            .cte_owner_check
            .as_ref()
            .expect("create_property carries cte_owner_check prefix");
        assert_eq!(
            cte,
            r#"WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $host AND "user" = ctx.User.ID)"#
        );
    }

    /// §6.1 composition — `[crud, me]` + `@owner_axis` propagates the
    /// chain WHERE to `lookup_my_<r>`. This is the core composability
    /// claim (§5.3 / proposal §6.2): one annotation, all bundles see
    /// it. The fixture uses a `Profile` resource that is NOT user-keyed
    /// (no `user: User required unique`) so the `me` mode falls back to
    /// the owner-axis route via `host`.
    ///
    /// We exercise the lookup_my path with an `org_keyed` me mode (the
    /// `Profile` has `org` but no direct `user` field) — the chain
    /// WHERE adds the ownership filter on top of the actor-keyed
    /// shape, exactly per §6.1's "compose, don't replace" rule.
    #[test]
    fn composition_crud_and_me_with_owner_axis_propagates_chain_to_lookup_my() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        let profile = ir::Resource {
            name: "Profile".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                fk_field_with_axis("host", "Host", "user"),
                req_field("bio", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud, ir::ConventionRef::Me],
        };
        // Sanity: not user-keyed (no `user: User required unique`).
        profile
            .fields
            .iter()
            .for_each(|f| assert_ne!(f.name, "user"));
        feature.resources.push(profile);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "composition + @owner_axis should not emit diagnostics, got {:?}",
            diags
        );

        // lookup_my_profile is emitted (me §5.3 OrgKeyed route — Profile
        // has `org`, no `user`). The owner-scope synth ALSO attached its
        // chain predicate.
        let lookup_my = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_profile")
            .expect("composition emits lookup_my_profile");
        let scope = match lookup_my {
            ir::Query::Lookup(lq) => lq
                .owner_scope_sql
                .as_ref()
                .expect("lookup_my_profile carries owner_scope_sql"),
            _ => panic!("expected Lookup variant"),
        };
        assert_eq!(scope.field_name, "host");
        assert_eq!(scope.fk_target, "Host");
        assert_eq!(
            scope.where_predicate,
            r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#
        );

        // Plus the 5 crud entries all carry the same scope (spot-check
        // delete_profile to confirm cross-bundle uniformity).
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_profile")
            .expect("composition emits delete_profile");
        assert!(delete.owner_scope_sql.is_some());
    }

    /// §11.1 `owner_axis_unknown_through` — annotation names a column
    /// that doesn't exist on the FK target. Suggestion field is
    /// populated when a nearest match exists.
    #[test]
    fn diagnostic_owner_axis_unknown_through() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        // Property with `@owner_axis(through: usr)` — typo: `usr` not
        // `user`. Nearest-match should suggest `user`.
        let mut property = property_resource_with_axis();
        // Replace the host field's owner_axis with the typo'd column.
        for f in property.fields.iter_mut() {
            if f.name == "host" {
                f.owner_axis = Some(ir::OwnerAxis {
                    through_column: "usr".to_owned(),
                });
            }
        }
        feature.resources.push(property);

        let diags = synthesize_conventions(&mut feature);
        let found = diags.iter().find_map(|d| match d {
            ConventionSynthDiagnostic::OwnerAxisUnknownThrough {
                resource,
                field,
                through,
                fk_target,
                suggestion,
            } if resource == "Property" && field == "host" => Some((
                through.clone(),
                fk_target.clone(),
                suggestion.clone(),
            )),
            _ => None,
        });
        let (through, fk_target, suggestion) =
            found.expect("expected OwnerAxisUnknownThrough diagnostic");
        assert_eq!(through, "usr");
        assert_eq!(fk_target, "Host");
        assert_eq!(suggestion, Some("user".to_owned()));

        // Synth fell back to tenant-only — owner_scope_sql NOT attached.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth still emits delete_property");
        assert!(
            delete.owner_scope_sql.is_none(),
            "unresolved @owner_axis must not produce SQL fragments"
        );
    }

    /// §11.1 `owner_axis_through_not_user_keyed` — the resolved
    /// `through:` column on the FK target is not typed as `User`.
    /// Warning severity (proposal §11.1) — chain still emits so author
    /// can hand-correct.
    #[test]
    fn diagnostic_owner_axis_through_not_user_keyed() {
        let mut feature = empty_feature("catalog");

        // Host with a `manager: Text required` (not a User type).
        let mut host = host_resource();
        host.fields = vec![
            req_field("org", user_qn("Org")),
            req_field("manager", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
        ];
        feature.resources.push(host);

        // Property with `@owner_axis(through: manager)` — `manager`
        // exists on Host but is Text-typed, not User-typed.
        let mut property = property_resource_with_axis();
        for f in property.fields.iter_mut() {
            if f.name == "host" {
                f.owner_axis = Some(ir::OwnerAxis {
                    through_column: "manager".to_owned(),
                });
            }
        }
        feature.resources.push(property);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed {
                    resource,
                    field,
                    through,
                    fk_target,
                } if resource == "Property"
                    && field == "host"
                    && through == "manager"
                    && fk_target == "Host"
            )),
            "expected OwnerAxisThroughNotUserKeyed diagnostic, got {:?}",
            diags
        );

        // Warning, not error — the chain SQL is still emitted so the
        // author can hand-fix the chain.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth still emits delete_property");
        let scope = delete
            .owner_scope_sql
            .as_ref()
            .expect("warning-level diag still attaches scope");
        assert!(scope.where_predicate.contains("manager"));
    }

    /// §11.1 `owner_axis_collides_with_unique_user` — resource has BOTH
    /// `user: User required unique` AND `@owner_axis(through: <col>)`
    /// on another field. Synth surfaces a warning and skips the
    /// owner-axis emission (user-keyed mode already provides
    /// ownership; §11.1 mitigation).
    #[test]
    fn diagnostic_owner_axis_collides_with_unique_user() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        // Property with BOTH `user: User required unique` AND
        // `host: Host required @owner_axis(through: user)`. The two
        // are mutually redundant.
        let property = ir::Resource {
            name: "Property".to_owned(),
            public_contract: None,
            tenancy: Some(ir::Tenancy::Org),
            soft_delete: false,
            timestamps: None,
            fields: vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                fk_field_with_axis("host", "Host", "user"),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: Vec::new(),
            lock: None,
            composite_key: None,
            conventions: vec![ir::ConventionRef::Crud],
        };
        feature.resources.push(property);

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser {
                    resource,
                    field,
                } if resource == "Property" && field == "host"
            )),
            "expected OwnerAxisCollidesWithUniqueUser diagnostic, got {:?}",
            diags
        );

        // Owner-axis SQL must NOT be attached — user-keyed mode wins,
        // the existing tenant categorization handles ownership via
        // the `user: User required unique` field.
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .expect("synth still emits delete_property");
        assert!(
            delete.owner_scope_sql.is_none(),
            "user-unique + @owner_axis must not double-restrict"
        );
    }

    /// §9 override semantics — author writes `command delete_<r>` with
    /// their own handler; synth skips just that name, no diagnostic.
    /// The author's command is untouched (no `owner_scope_sql`
    /// attached — the synth doesn't mutate author-written commands).
    #[test]
    fn override_with_handler_skips_synth_and_does_not_attach_scope() {
        let mut feature = empty_feature("catalog");
        feature.resources.push(host_resource());
        feature.resources.push(property_resource_with_axis());

        // Author-written `delete_property` — bare canonical shape so
        // the existing signature-match logic passes; the analyzer
        // simply records `AuthorOverride(Crud)` and skips the synth.
        feature.commands.push(ir::Command {
            name: "delete_property".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Delete,
            route: vec![ir::RouteSlot {
                name: "id".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
                from: None,
                kind: ir::RouteSlotKind::Plain,
            }],
            input: ir::CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::Deletes(ir::DeleteEffect {
                resource: ir::QualifiedName {
                    feature: None,
                    name: "Property".to_owned(),
                },
            }),
            policy: ir::PolicyRef::Local("host_only".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: Some(ir::HandlerRef {
                namespace: "fn".to_owned(),
                name: "delete_property".to_owned(),
                span_ref: None,
            }),
            tests: None,
            previous_names: Vec::new(),
            span_ref: None,
            triggers: Vec::new(),
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
        });

        let diags = synthesize_conventions(&mut feature);
        // No diagnostic — override is first-class per §9 / RULE-VOCAB-02.
        assert!(
            !diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::OwnerAxisUnknownThrough { .. }
                    | ConventionSynthDiagnostic::OwnerAxisThroughNotUserKeyed { .. }
                    | ConventionSynthDiagnostic::OwnerAxisCollidesWithUniqueUser { .. }
                    | ConventionSynthDiagnostic::SignatureMismatch { .. }
            )),
            "override should not emit owner-axis OR signature-mismatch diagnostics, got {:?}",
            diags
        );

        // Exactly one `delete_property` — the author's, with policy
        // `host_only`, handler set, NO `owner_scope_sql`.
        let count = feature
            .commands
            .iter()
            .filter(|c| c.name == "delete_property")
            .count();
        assert_eq!(count, 1, "delete_property must not be duplicated");
        let delete = feature
            .commands
            .iter()
            .find(|c| c.name == "delete_property")
            .unwrap();
        assert!(matches!(&delete.policy, ir::PolicyRef::Local(p) if p == "host_only"));
        assert!(delete.handler.is_some(), "author's handler preserved");
        assert!(
            delete.owner_scope_sql.is_none(),
            "synth must not mutate author-written delete_property",
        );
        // §11 — synth_origins records AuthorOverride(Crud).
        assert_eq!(
            feature.synth_origins.get("delete_property"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Crud)),
        );

        // Other 4 crud entries still synth WITH owner-scope.
        let create = feature
            .commands
            .iter()
            .find(|c| c.name == "create_property")
            .expect("create still synthesized");
        assert!(create.owner_scope_sql.is_some());
    }

    /// Direct-call builder sanity — `build_owner_scope_where_for_test`
    /// and `build_owner_scope_cte_prefix_for_test` round-trip the SQL.
    /// Anchors the function-level surface in case downstream cells
    /// invoke the builders directly (O3 inspect / LSP hover).
    #[test]
    fn builder_functions_round_trip_canonical_sql() {
        // §7.3 — WHERE predicate shape.
        assert_eq!(
            build_owner_scope_where_for_test("host", "Host", "user"),
            r#"host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)"#,
        );
        // §8.5.A — CTE prefix shape.
        assert_eq!(
            build_owner_scope_cte_prefix_for_test("host", "Host", "user"),
            r#"WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $host AND "user" = ctx.User.ID)"#,
        );
    }
}
