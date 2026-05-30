//! Registry `ALL` section 10/11 (SPEC-19 split; concatenated in `registry::ALL`).
#![allow(clippy::all, unused_imports)]

use crate::{CapabilitySpec, Context, DiagnosticFacet, SemanticToken, Sigil, Surface};
use super::super::builders::*;
use super::super::facets::*;

pub(crate) const ROWS: &[CapabilitySpec] = &[
    // ════════════════════════════════════════════════════════════════
    // plan block
    // ════════════════════════════════════════════════════════════════
    kw(
        "features",
        Context::Plan,
        SECTION,
        "Plan feature entitlements.",
    ),
    // H2 reconcile: the grammar's plan-block alternation colors
    // `features | limits | trial` at the section leaf; promote `trial` off the
    // generic statement leaf so `#kw-plan-section` is faithful.
    kw("trial", Context::Plan, SECTION, "Plan trial-window block."),
    CapabilitySpec {
        literal: "then",
        context: Context::Plan,
        scope: "keyword.other.plan.lazuli",
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover: "Plan upgrade target.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "unlimited",
        context: Context::Plan,
        scope: "keyword.other.plan.lazuli",
        token: SemanticToken::Keyword,
        surface: Surface::Lzi,
        sigil: None,
        hover: "Unlimited plan quota.",
        produces: &[],
    },
    // ════════════════════════════════════════════════════════════════
    // Cross-cutting modifiers — storage.modifier.lazuli
    // ════════════════════════════════════════════════════════════════
    modifier("required", "Field is required."),
    modifier("optional", "Field is optional."),
    modifier("readonly", "Field is read-only."),
    modifier("raw", "Raw/unprocessed value."),
    modifier("override", "Override a base declaration."),
    modifier("per", "Rate/quota unit connector."),
    modifier("at", "Position/time connector."),
    modifier("from", "Source connector."),
    modifier("to", "Target connector."),
    modifier("by", "Agent/grouping connector."),
    modifier("on", "Relation/event connector."),
    modifier("provides", "Provides connector."),
    modifier("cascade", "On-delete cascade."),
    modifier("restrict", "On-delete restrict."),
    modifier("nullify", "On-delete set-null."),
    modifier("terminal", "Terminal lifecycle state."),
    modifier("initial", "Initial lifecycle state."),
    modifier("sync", "Synchronous mode."),
    modifier("after", "Slot-position after."),
    modifier("before", "Slot-position before."),
    modifier("using", "Using connector."),
    modifier("inherits", "Inherits connector."),
    modifier("parent", "Parent reference."),
    modifier("name", "Name connector."),
    modifier("description", "Description connector."),
    modifier("filename", "Filename connector."),
    modifier("mime", "MIME-type connector."),
    modifier("size", "Size connector."),
    modifier("attempts", "Attempts connector."),
    modifier("max_attempts", "Max-attempts connector."),
    modifier("max_recursion", "Max-recursion connector."),
    modifier("signed_ttl", "Signed-URL TTL."),
    modifier("accept", "Accepted MIME types."),
    modifier("uri_template", "URI template."),
    modifier("uses", "Uses connector."),
    modifier("data_subject", "GDPR data-subject."),
    modifier("retain", "Retention connector."),
    modifier("opaque", "Opaque-token flag."),
    modifier("terminal_result_field", "Terminal result field."),
    modifier("terminal_status_field", "Terminal status field."),
    modifier("invariant_handler", "Invariant handler reference."),
    modifier("derived_from", "Derived-from source."),
    modifier("resolve", "Resolve-via connector."),
    modifier("states", "Lifecycle states block."),
    modifier("state", "Lifecycle state."),
    modifier("transition", "Lifecycle transition."),
    modifier("lifecycle_routes", "Lifecycle route bindings."),
    modifier("lifecycle_stage", "Lifecycle-stage marker."),
    modifier("when_denied_route", "Route when policy denied."),
    modifier("free", "`free text into` sugar."),
    modifier("list", "`list of` type-constructor head."),
    // ════════════════════════════════════════════════════════════════
    // Type constructors — support.function.type-constructor.lazuli
    // ════════════════════════════════════════════════════════════════
    CapabilitySpec {
        literal: "many",
        context: Context::Expression,
        scope: TYPE_CTOR,
        token: SemanticToken::Type,
        surface: Surface::Lzi,
        sigil: None,
        hover: "`many` relation type constructor.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "list_of",
        context: Context::Expression,
        scope: TYPE_CTOR,
        token: SemanticToken::Type,
        surface: Surface::Lzi,
        sigil: None,
        hover: "`list_of` collection type constructor.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "ref",
        context: Context::Expression,
        scope: TYPE_CTOR,
        token: SemanticToken::Type,
        surface: Surface::Lzi,
        sigil: None,
        hover: "`ref` reference type constructor.",
        produces: &[],
    },
    // ════════════════════════════════════════════════════════════════
    // Filter / policy / expression operators + predicates
    // ════════════════════════════════════════════════════════════════
    op("and", OP_LOGICAL),
    op("or", OP_LOGICAL),
    op("not", OP_LOGICAL),
    op("has", OP_PREDICATE),
    op("in", OP_PREDICATE),
    op("exists", OP_PREDICATE),
    op("matches", OP_PREDICATE),
    op("is", OP_PREDICATE),
    op("between", OP_PREDICATE),
    op("contains", OP_PREDICATE),
    op("length", OP_PREDICATE),
    op("pattern", OP_PREDICATE),
    op("min", OP_PREDICATE),
    op("max", OP_PREDICATE),
    op("excludes", OP_PREDICATE),
    op("includes", OP_PREDICATE),
    op("covers_pii", OP_PREDICATE),
    op("guaranteed", OP_PREDICATE),
    op("behind", "keyword.operator.plan-and-gate.lazuli"),
    op("quota", "keyword.operator.plan-and-gate.lazuli"),
    // ════════════════════════════════════════════════════════════════
    // Decorator namespaces — entity.name.tag.decorator.lazuli
    // ════════════════════════════════════════════════════════════════
    produces(
        decorator(
            "@semantic",
            "Semantic-scalar decorator (`@semantic.HexColor`).",
        ),
        P_SEMANTIC,
    ),
    produces(
        decorator("@cap", "Capability decorator (`@cap.File`)."),
        P_CAP,
    ),
    decorator("@pii", "PII-classification decorator."),
    decorator("@key", "Encryption-key decorator."),
    decorator("@slug", "Slug field decorator."),
    produces(
        decorator("@full_text", "Full-text-index decorator."),
        P_FULL_TEXT,
    ),
    produces(
        decorator("@owner_axis", "Ownership-axis decorator."),
        P_OWNER_AXIS,
    ),
    decorator("@llm", "LLM decorator."),
    decorator("@tool", "Tool decorator."),
    decorator("@adapter", "Adapter decorator."),
    decorator("@policy", "Policy reference decorator."),
    decorator("@scope", "Scope reference decorator."),
    decorator("@role", "Role reference decorator."),
    decorator("@actor", "Actor reference decorator."),
    decorator("@anchor", "Anchor reference decorator."),
    decorator("@client", "Client extension decorator."),
    produces(
        decorator("@fn", "Custom-function reference decorator."),
        P_FN,
    ),
    produces(decorator("@hook", "Hook reference decorator."), P_HOOK),
    decorator("@validator", "Validator reference decorator."),
    decorator("@query_modifier", "Query-modifier reference decorator."),
    decorator("@translation", "Translation reference decorator."),
    decorator("@command", "Command reference decorator."),
    decorator("@file", "File reference decorator."),
    decorator("@feature", "Feature reference decorator."),
    decorator("@resume", "Resume reference decorator."),
    decorator("@audience", "Audience reference decorator."),
    // ════════════════════════════════════════════════════════════════
    // design.lzi token catalog — DESIGN_KEYWORDS
    // ════════════════════════════════════════════════════════════════
    design("color", "Closed design token group for colors."),
    design("typography", "Closed design token group for type."),
    design("space", "Spacing scale token group."),
    design("radius", "Border-radius token group."),
    design("shadow", "Box-shadow elevation token group."),
    design("motion", "Transition/animation token group."),
    design("breakpoint", "Responsive breakpoint token group."),
    design("z", "Stacking-order token group."),
    design("family", "Font-family sub-group."),
    design("scale", "Type-scale sub-group."),
    design("weight", "Font-weight sub-group."),
    design("tracking", "Letter-spacing sub-group."),
    design("duration", "Motion duration sub-group."),
    design("easing", "Motion easing sub-group."),
    design("line_height", "Type line-height field."),
    CapabilitySpec {
        literal: "base",
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover: "Required default color state.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "foreground",
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover: "Foreground color when used as background.",
        produces: &[],
    },
    CapabilitySpec {
        literal: "dark",
        context: Context::Design,
        scope: SECTION,
        token: SemanticToken::Keyword,
        surface: Surface::Design,
        sigil: None,
        hover: "Dark-theme color suffix.",
        produces: &[],
    },
    // ════════════════════════════════════════════════════════════════
    // Closed-catalog VALUES — constant.language.<catalog>.lazuli
    // (carried so the proven-complete scan resolves match-arm value
    //  literals; these are EnumMember tokens, not keywords.)
    // ════════════════════════════════════════════════════════════════
    // boolean
    value("true", "constant.language.boolean.lazuli"),
    value("false", "constant.language.boolean.lazuli"),
    value("nil", "constant.language.boolean.lazuli"),
    value("null", "constant.language.boolean.lazuli"),
    // notification channels
    value("email", "constant.language.channel.lazuli"),
    value("push", "constant.language.channel.lazuli"),
    value("sms", "constant.language.channel.lazuli"),
    value("in_app", "constant.language.channel.lazuli"),
    // cookie same_site
    value("lax", "constant.language.cookie.lazuli"),
    value("strict", "constant.language.cookie.lazuli"),
    value("none", "constant.language.cookie.lazuli"),
    // deploy strategy
    value("rolling", "constant.language.deploy.lazuli"),
    value("blue_green", "constant.language.deploy.lazuli"),
    value("canary", "constant.language.deploy.lazuli"),
    // dlq mode
    value("emit", "constant.language.dlq.lazuli"),
    value("drop", "constant.language.dlq.lazuli"),
    // http methods
    value("GET", "constant.language.http-method.lazuli"),
    value("POST", "constant.language.http-method.lazuli"),
    value("PUT", "constant.language.http-method.lazuli"),
    value("PATCH", "constant.language.http-method.lazuli"),
    value("DELETE", "constant.language.http-method.lazuli"),
    value("OPTIONS", "constant.language.http-method.lazuli"),
    value("HEAD", "constant.language.http-method.lazuli"),
    // lock strategy
    value("optimistic", "constant.language.lock.lazuli"),
    value("pessimistic", "constant.language.lock.lazuli"),
    value("row_level", "constant.language.lock.lazuli"),
    // log level / format
    value("debug", "constant.language.log-level.lazuli"),
    value("info", "constant.language.log-level.lazuli"),
    value("warn", "constant.language.log-level.lazuli"),
];
