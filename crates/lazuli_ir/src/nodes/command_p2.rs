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

/// One parsed entry of a GAP-09 *command-level* predicate-gated policy
/// reference list: `@policy.<name> [when <predicate>]`. Unlike
/// [`ConditionalPolicyAtom`] (which gates a raw atom declared INSIDE a
/// `policies` category), this gates a *reference to a category* on the
/// command/query/api `policy` line itself.
///
/// `name` is the bare category name (the `<name>` of `@policy.<name>`).
/// `when` is the verbatim predicate text after ` when ` (`None` for an
/// unconditional ref).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalPolicyRef {
    /// Bare policy-category name (`admin`, `finance`, ...).
    pub name: String,
    /// Verbatim `when <predicate>` text, or `None` if unconditional.
    pub when: Option<String>,
}

/// GAP-09 — split a single comma-separated policy entry into
/// `(ref_text, when_text)` when it carries a standalone ` when ` tail,
/// else `(ref_text, None)`. Mirrors the category parser's
/// `split_when_clause` (lazuli_syntax) so the command-level conditional
/// form resolves the same way the in-category form does.
fn split_ref_when(entry: &str) -> (&str, Option<&str>) {
    let mut search = 0usize;
    while let Some(rel) = entry[search..].find(" when ") {
        let idx = search + rel;
        let atom = entry[..idx].trim();
        let when = entry[idx + " when ".len()..].trim();
        if !atom.is_empty() && !when.is_empty() {
            return (atom, Some(when));
        }
        search = idx + " when ".len();
    }
    (entry.trim(), None)
}

/// Split a raw policy payload on TOP-LEVEL commas — commas inside a
/// double-quoted string literal (e.g. a `when ... == "a,b"` value) do not
/// separate entries.
fn split_top_level_commas(raw: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut in_str = false;
    for (i, ch) in raw.char_indices() {
        match ch {
            '"' => in_str = !in_str,
            ',' if !in_str => {
                parts.push(&raw[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&raw[start..]);
    parts
}

/// Parse a raw `policy` payload as a GAP-09 *conditional / comma-list*
/// reference form: one or more `@policy.<name> [when <predicate>]` atoms
/// separated by top-level commas.
///
/// Returns `Some(refs)` ONLY when the payload genuinely has this shape —
/// i.e. it contains more than one comma-separated `@policy.<name>` entry,
/// or a single `@policy.<name> when <predicate>` entry. A bare single
/// `@policy.<name>` (or any payload whose first entry is not a
/// `@policy.`/`policy.` reference, e.g. a `@role.*` atom or a structured
/// `has_role` expression) returns `None`, so every existing single-atom /
/// structured path is left exactly as-is.
///
/// Each entry must be a `@policy.<name>` (or `policy.<name>`) reference;
/// if ANY entry is not, the whole payload is rejected (`None`) and the
/// caller falls through to its normal resolution / deny path. The parser
/// does not validate that the names resolve to categories — that is the
/// caller's job (codegen resolves+emits; doctor resolves+flags).
///
/// ## Examples
///
/// ```
/// use lazuli_ir::parse_conditional_policy_refs;
///
/// // Conditional comma form → parsed atoms.
/// let refs = parse_conditional_policy_refs(
///     "policy.admin when input.scope == \"Production\", @policy.finance when input.scope == \"MediaPlacement\"",
/// )
/// .expect("conditional form");
/// assert_eq!(refs.len(), 2);
/// assert_eq!(refs[0].name, "admin");
/// assert_eq!(refs[0].when.as_deref(), Some("input.scope == \"Production\""));
/// assert_eq!(refs[1].name, "finance");
///
/// // Bare single atom → not the conditional form.
/// assert!(parse_conditional_policy_refs("policy.admin").is_none());
/// // Non-policy atom → rejected.
/// assert!(parse_conditional_policy_refs("role.ADMIN, role.MANAGER").is_none());
/// ```
pub fn parse_conditional_policy_refs(raw: &str) -> Option<Vec<ConditionalPolicyRef>> {
    let entries = split_top_level_commas(raw);
    let mut refs: Vec<ConditionalPolicyRef> = Vec::with_capacity(entries.len());
    let mut any_when = false;
    for entry in entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return None;
        }
        let (ref_text, when_text) = split_ref_when(trimmed);
        // Each entry MUST be a `@policy.<name>` / `policy.<name>` reference.
        let stripped = ref_text.strip_prefix('@').unwrap_or(ref_text);
        let name = stripped.strip_prefix("policy.")?;
        if name.is_empty() || name.contains(char::is_whitespace) {
            return None;
        }
        if when_text.is_some() {
            any_when = true;
        }
        refs.push(ConditionalPolicyRef {
            name: name.to_owned(),
            when: when_text.map(str::to_owned),
        });
    }
    // Only claim this payload when it is genuinely the conditional/comma
    // form: a single unconditional `@policy.<name>` is NOT — leave it to
    // the existing single-ref resolution path.
    if refs.len() >= 2 || any_when {
        Some(refs)
    } else {
        None
    }
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
