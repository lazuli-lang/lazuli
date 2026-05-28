//! Workflow + Lifecycle state-machine IR.
//!
//! Lazuli has two related-but-distinct state-machine primitives:
//!
//! - **`workflow`** — author-defined transitions over an arbitrary
//!   discriminator field. Originally the only state-machine primitive;
//!   retained for explicit `from -> to` flows with custom predicates.
//! - **`lifecycle`** — sugar over the workflow shape that bundles a
//!   discriminator field, auto-generated enum, state catalog, and
//!   transition graph in one block. The closed catalog
//!   ([`LifecycleInvariant`]) is enforceable cold; the open form
//!   ([`HandlerRef`] under `invariant_handler @fn.<name>`) is the only
//!   escape hatch.
//!
//! Both forms share [`FieldRef`] (the discriminator pointer) and
//! [`Transition`] / [`LifecycleTransition`] (the transition row). The
//! types live together because doctor's transition validators walk
//! workflow and lifecycle in one pass.
//!
//! ## Why `Lifecycle` exists alongside `Workflow`
//!
//! A pilot's typical state machine — `draft -> published -> archived`
//! with timestamps and policies — exploded into three or four sibling
//! declarations in workflow form: an enum, a workflow block, a per-state
//! audit hook, and a per-transition policy override. The `lifecycle`
//! sugar collapses this into a single block where the LLM can read the
//! whole shape in one pass. `workflow` is still available for advanced
//! flows; doctor surfaces a hint when the workflow shape obviously fits
//! the lifecycle mold.
//!
//! ## Closed-catalog invariants
//!
//! [`LifecycleInvariant`] is intentionally narrow. Three forms ship:
//! `terminal_immutable`, `single <state> per <scope_field>`, and
//! `no_jump_more_than_one`. Adding forms requires a proposal — the
//! catalog is the spec contract authors can rely on. The escape hatch
//! is `invariant_handler @fn.<name>`, which surfaces as a
//! [`HandlerRef`] in [`Lifecycle::invariant_handlers`].
//!
//! ## Handler escape hatch
//!
//! [`HandlerRef`] is the only non-closed slot. It's a typed reference
//! to an extension function (e.g. `@fn.check_archive_window`) that the
//! runtime invokes for invariants the closed catalog can't express.
//! The namespace + name pair is captured verbatim; doctor checks
//! existence in the feature's extension contract.
//!
//! ## See also
//!
//! - `docs/proposals/lifecycle-vocab.md` — original `lifecycle` design.
//! - [`crate::AuditSpec`] — audit subjects carried by transitions.
//! - [`crate::PolicyRef`] — policy gates on transitions.
//! - [`crate::TestBlock`] — inline tests on transitions.
//! - [`crate::TranslationKeyRef`] — `policy_when_denied` slot.

use serde::{Deserialize, Serialize};

use crate::{AuditSpec, PolicyRef, QualifiedName, SpanRef, TestBlock, TranslationKeyRef};

/// Pre-lifecycle workflow declaration (legacy `workflow <name>` form).
/// Carries the discriminator field, default policy/emits, and the
/// list of [`Transition`]s. The richer typed [`Lifecycle`] shape
/// supersedes this for new authoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub on: FieldRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_emits: Vec<String>,
    pub transitions: Vec<Transition>,
    /// IR Error-Vocab — reserved-slot per-workflow override for the
    /// `policy_denied` error message. v1 codegen does not consume this
    /// slot (workflow transitions are author-driven, not end-user HTTP
    /// boundaries); the IR shape exists so v2 promotion is purely
    /// additive. See `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `lifecycle <field>` declaration on a resource — the typed
/// state-machine spine. Auto-generates the enum type, enumerates
/// states + transitions, and pins invariants (terminal immutability,
/// scope uniqueness, etc.) the runtime enforces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lifecycle {
    /// Name of the discriminator field on the parent Resource.
    pub discriminator_field: String,

    /// Auto-generated enum name (e.g. "PublicationStatus" for
    /// `resource Publication { lifecycle status }`). Doctor enforces no
    /// sibling `enum` of the same name.
    pub generated_enum: String,

    /// One per `state <name> [initial|terminal]` child. Order preserved
    /// from source so doctor can reason about "linear chain" for
    /// no_jump_more_than_one.
    pub states: Vec<LifecycleState>,

    /// One per `transition <name> ... ` child.
    pub transitions: Vec<LifecycleTransition>,

    /// One per `invariant <form>` child — closed catalog (§3.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<LifecycleInvariant>,

    /// One per `invariant_handler @fn.<name>` escape-hatch child.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariant_handlers: Vec<HandlerRef>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Typed `@<namespace>.<name>` extension reference (e.g. `@fn.<name>`).
/// Used by invariant handlers and other extension hooks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandlerRef {
    /// Extension namespace, e.g. `fn` for `@fn.<name>`.
    pub namespace: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `state <name> [initial|terminal]` entry inside [`Lifecycle`].
/// Order is preserved from source so doctor can reason about linear
/// progression invariants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleState {
    pub name: String,
    pub kind: LifecycleStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of lifecycle state roles. `Initial` is the starting
/// state; `Terminal` is the final / absorbing state; everything else
/// is `Intermediate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStateKind {
    /// Starting state for newly-created rows.
    Initial,
    /// Standard intermediate state.
    Intermediate,
    /// Absorbing final state.
    Terminal,
}

/// One `transition <name> ... ` entry inside [`Lifecycle`]. Carries the
/// `from`/`to` states (multi-`from` = fan-in), policy/audit overrides,
/// optional timestamp field, emitted events, and inline tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleTransition {
    pub name: String,
    /// One or more source state names. Multi = fan-in.
    pub from: Vec<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
    /// Name of the DateTime resource field stamped by this transition.
    /// Lowering auto-emits the field on the parent resource if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<String>,
    /// Emitted events — same shape as Command.emits (verbatim strings;
    /// the existing emit-string lowering handles `<event>` /
    /// `<event> payload <fields>` / `<event> from updates`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// `requires @policy.<name>` — raises the bar above the lifecycle
    /// default, mirrors Transition::requires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<PolicyRef>,
    /// `tests` block — mirrors Transition::tests (v0.2 §3.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog (§3.4). `serde(tag = "kind", content = "value")` keeps
/// the JSON projection self-describing for inspect consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum LifecycleInvariant {
    /// `invariant terminal_immutable`
    #[serde(rename = "terminal_immutable")]
    TerminalImmutable,
    /// `invariant single <state> per <scope_field>`
    #[serde(rename = "single_state_per_scope")]
    SingleStatePerScope { state: String, scope_field: String },
    /// `invariant no_jump_more_than_one`
    #[serde(rename = "no_jump_more_than_one")]
    NoJumpMoreThanOne,
}

/// Typed `<resource>.<field>` reference, used by [`Workflow::on`] to
/// name the lifecycle discriminator field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef {
    pub resource: QualifiedName,
    pub field: String,
}

/// One transition entry inside a legacy [`Workflow`]. Carries the
/// from/to state names + the standard policy/emit decorators. Newer
/// authoring uses [`LifecycleTransition`] instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub from: String,
    pub to: String,
    /// `requires @policy.<category>` raises the policy bar for this transition
    /// above the workflow default (e.g. `requires @policy.delete`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn lifecycle_state_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(LifecycleStateKind::Initial).unwrap(),
            json!("initial")
        );
        assert_eq!(
            serde_json::to_value(LifecycleStateKind::Terminal).unwrap(),
            json!("terminal")
        );
    }

    #[test]
    fn lifecycle_invariant_terminal_immutable_serializes_with_kind_tag() {
        let inv = LifecycleInvariant::TerminalImmutable;
        let value = serde_json::to_value(&inv).unwrap();
        assert_eq!(value["kind"], json!("terminal_immutable"));
        let back: LifecycleInvariant = serde_json::from_value(value).unwrap();
        assert_eq!(back, inv);
    }

    #[test]
    fn lifecycle_invariant_single_state_per_scope_round_trips() {
        let inv = LifecycleInvariant::SingleStatePerScope {
            state: "active".to_owned(),
            scope_field: "tenant_id".to_owned(),
        };
        let value = serde_json::to_value(&inv).unwrap();
        assert_eq!(value["kind"], json!("single_state_per_scope"));
        assert_eq!(value["value"]["state"], json!("active"));
        let back: LifecycleInvariant = serde_json::from_value(value).unwrap();
        assert_eq!(back, inv);
    }

    #[test]
    fn handler_ref_round_trips() {
        let h = HandlerRef {
            namespace: "fn".to_owned(),
            name: "check_archive".to_owned(),
            span_ref: None,
        };
        let json = serde_json::to_value(&h).unwrap();
        let back: HandlerRef = serde_json::from_value(json).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn field_ref_round_trips() {
        let f = FieldRef {
            resource: QualifiedName {
                feature: None,
                name: "Publication".to_owned(),
            },
            field: "status".to_owned(),
        };
        let json = serde_json::to_value(&f).unwrap();
        let back: FieldRef = serde_json::from_value(json).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn workflow_round_trips_with_empty_optional_slots() {
        let wf = Workflow {
            name: "publication".to_owned(),
            on: FieldRef {
                resource: QualifiedName {
                    feature: None,
                    name: "Publication".to_owned(),
                },
                field: "status".to_owned(),
            },
            default_policy: None,
            default_emits: vec![],
            transitions: vec![Transition {
                name: "publish".to_owned(),
                from: "draft".to_owned(),
                to: "published".to_owned(),
                requires: None,
                emits: vec![],
                tests: None,
                previous_names: vec![],
                span_ref: None,
            }],
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
        };
        let json = serde_json::to_value(&wf).unwrap();
        let back: Workflow = serde_json::from_value(json).unwrap();
        assert_eq!(back, wf);
    }

    #[test]
    fn lifecycle_state_round_trips() {
        let s = LifecycleState {
            name: "draft".to_owned(),
            kind: LifecycleStateKind::Initial,
            span_ref: None,
        };
        let json = serde_json::to_value(&s).unwrap();
        let back: LifecycleState = serde_json::from_value(json).unwrap();
        assert_eq!(back, s);
    }
}
