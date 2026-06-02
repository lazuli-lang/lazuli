fn is_plain_route_slot_kind(kind: &RouteSlotKind) -> bool {
    matches!(kind, RouteSlotKind::Plain)
}

/// Shape of a command's `input` declaration. `Short` is the field-name
/// shortcut (`input { name, email }`); `Typed` is the explicit
/// name/type form (`input { name: Text }`); `Empty` covers commands
/// without an `input` block (typical for `delete`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandInput {
    /// Short list — every entry maps 1:1 to a field on the command's local
    /// `creates`/`updates` resource.
    Short(Vec<String>),
    /// Typed block — explicit name/type pairs.
    Typed(Vec<TypedSlot>),
    /// Empty inputs (`delete` commands often have none).
    Empty,
}

/// One name/type entry inside a typed [`CommandInput::Typed`] block.
/// Mirrors `Field` shape — same constraints catalog flows to the
/// generated Zod / Go validator / OpenAPI schemas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedSlot {
    pub name: String,
    pub type_ref: TypeRef,
    pub required: bool,
    /// L0 #3 §10 — inline constraints carried on command input
    /// slots (`input` block). Mirrors `Field::constraints` so Zod
    /// schemas for command inputs, Go validator tags on
    /// `<Cmd>Input` structs, and OpenAPI parameter schemas pick up
    /// the same six-keyword catalog without a parallel field.
    #[serde(default, skip_serializing_if = "FieldConstraints::is_empty")]
    pub constraints: FieldConstraints,
    /// LAZ-SEMANTIC-AUTO-VALIDATE W2 — `@validate.skip` annotation on
    /// the slot. Codegen skips emitting the semantic-scalar runtime
    /// validation pre-pass for this field, even when the
    /// `@semantic.X` type declares a validator. Used for migration /
    /// legacy import flows where authors knowingly accept invalid
    /// scalar values. Doctor SEMANTIC-PLUGIN-002 stays silent when set.
    #[serde(default, skip_serializing_if = "is_false")]
    pub validate_skip: bool,
}

/// `target <query>(arg: value, ...)` — names the row(s) a non-create
/// command writes against. Carries a qualified query reference plus the
/// explicit named-argument bindings the analyzer resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetExpr {
    pub query: QualifiedName,
    pub args: Vec<NamedArg>,
}

/// One `name: value` pair inside a `target(...)` or `invalidates(...)`
/// argument list. `value` is a lowered `Expr` so route params, literals,
/// and field paths share the same shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedArg {
    pub name: String,
    pub value: Expr,
}

/// `let <name> = <expr>` binding inside a command body. Resolved during
/// lowering; downstream codegens may inline it or emit it as a typed
/// local depending on the target language idiom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
}

/// The write effect a command performs. `Creates` / `Updates` / `Deletes`
/// each carry typed sub-payloads naming the resource and assignments;
/// `Returns` is the pure-read variant for `returns <Type>` commands;
/// `None` is the legacy lowering placeholder before effect inference ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CommandEffect {
    /// `creates <Resource>` — see [`CreateEffect`].
    Creates(CreateEffect),
    /// `updates <target>` — see [`UpdateEffect`].
    Updates(UpdateEffect),
    /// `deletes <target>` — see [`DeleteEffect`].
    Deletes(DeleteEffect),
    /// Pure request/response command — declares `returns` instead of an effect.
    Returns(ReturnsEffect),
    /// W4 GAP-REORDER-01 — `reorder <Resource> by <position>` — see
    /// [`ReorderEffect`].
    Reorders(ReorderEffect),
    /// No effect declared yet (legacy lowering path).
    None,
}

/// `creates <Resource>` effect — inserts a new row with optional
/// `from input` shortcut (auto-binds same-named fields from input) and
/// explicit assignments for the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEffect {
    pub resource: QualifiedName,
    /// True when the command body uses `creates X from input`.
    pub from_input: bool,
    pub assignments: Vec<Assignment>,
}

/// `updates <target>` effect — mutates a single resolved row. The
/// `resource` is the resource type, paired with the command's `target`
/// expression that names the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateEffect {
    pub resource: QualifiedName,
    pub assignments: Vec<Assignment>,
    /// Explicit `where <col> = <expr>` row-scoping bindings authored in
    /// the `updates` block. Each entry's `field` is the WHERE column and
    /// `value` the RHS source (`ctx.actor.id`, `route.id`, `input.x`, a
    /// literal …). When non-empty, codegen builds the `Updates` WHERE
    /// map from these instead of the legacy route/input/`id` fallback.
    /// Empty when the author wrote no `where` (legacy fallback applies).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub where_clause: Vec<Assignment>,
}

/// `deletes <target>` effect — removes a single resolved row. No
/// assignments because the row is going away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteEffect {
    pub resource: QualifiedName,
    /// Explicit `where <col> = <expr>` row-scoping bindings authored in
    /// the `deletes` block — same shape/semantics as
    /// [`UpdateEffect::where_clause`]. When non-empty, codegen builds the
    /// `Deletes` WHERE map from these instead of the legacy fallback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub where_clause: Vec<Assignment>,
}

/// `returns <Type>` effect — pure command with no DB mutation. Carries
/// the declared return type so codegen can emit a typed response shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnsEffect {
    pub return_type: TypeRef,
}

/// W4 GAP-REORDER-01 — `reorder <Resource> by <position>` effect. The
/// command takes an ordered list of row ids and emits a single batch UPDATE
/// of the `position_field` column (CASE-based / `VALUES` join — wire-thin
/// SQL, no homegrown ordering). Doctor `REORDER-POSITION-FIELD-001`
/// verifies `position_field` is a declared `Integer` field on `resource`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReorderEffect {
    /// Target resource whose rows are reordered.
    pub resource: QualifiedName,
    /// Integer position column the batch UPDATE rewrites.
    pub position_field: String,
}

/// One `field = expr` assignment inside a `creates` / `updates` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub field: String,
    pub value: Expr,
}

/// Policy reference. `Local` = feature-local policy category. `Atom` = closed
/// `@role.*`/`@scope.*`/`@actor.*` namespace. `External` = `<feature>.<name>`.
/// `Unresolved` covers legacy strings until full lowering lands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "value")]
pub enum PolicyRef {
    Local(String),
    Atom(String),
    External {
        feature: String,
        name: String,
    },
    Unresolved(String),
    #[default]
    None,
}

impl PolicyRef {
    /// Returns `true` when the policy is unset. Used by serde's
    /// `skip_serializing_if` on query / command IR fields so absent
    /// per-callable policies serialize cleanly and round-trip back to
    /// `PolicyRef::None` (the explicit "feature-default applies" marker).
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::PolicyRef;
    ///
    /// assert!(PolicyRef::None.is_none());
    /// assert!(!PolicyRef::Atom("@role.host".into()).is_none());
    /// ```
    pub fn is_none(&self) -> bool {
        matches!(self, PolicyRef::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_ref_default_is_none() {
        let p = PolicyRef::default();
        assert!(p.is_none());
    }

    #[test]
    fn policy_ref_round_trips_atom() {
        let v = PolicyRef::Atom("@role.host".into());
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"Atom\""));
        let back: PolicyRef = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn approval_then_serialises_snake_case() {
        let v = ApprovalThen::Escalate;
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "\"escalate\"");
    }

    #[test]
    fn deprecation_default_is_all_none() {
        let d = Deprecation::default();
        let s = serde_json::to_string(&d).expect("serialize");
        assert_eq!(s, "{}");
    }

    #[test]
    fn route_slot_kind_default_is_plain() {
        assert_eq!(RouteSlotKind::default(), RouteSlotKind::Plain);
    }

    #[test]
    fn command_kind_round_trips() {
        let v = CommandKind::Create;
        let s = serde_json::to_string(&v).expect("serialize");
        let back: CommandKind = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
