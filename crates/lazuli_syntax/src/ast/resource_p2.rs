/// W4 GAP-08 — AST mirror of `lazuli_ir::ComputedDateBase`. Selects the
/// base `Date` of a [`ComputedDateAst`] either by a same-resource field
/// name (W3 `computed_date`) or by a rule-enum value resolved through a
/// bound `@fn` (W4 `schedule_rule`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputedDateBaseAst {
    /// `computed_date from <field> ...` — bare same-resource `Date` field.
    Field(String),
    /// `schedule_rule from @fn.<name>(<rule_arg>) ...` — the base date is
    /// produced by the bound `@fn`, which selects it from the rule arg.
    Rule {
        /// The rule-enum argument inside the `@fn(...)` call (verbatim).
        rule: String,
        /// Bare binding-fn name (the `<name>` in `@fn.<name>`).
        fn_ref: String,
    },
}

/// W3 GAP-03 — the `offset` operand of a [`ComputedDateAst`]. Either a
/// same-resource `Integer` field name or an inline integer literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputedDateOffsetAst {
    /// `offset <field>` — bare identifier naming an `Integer` field.
    Field(String),
    /// `offset <int>` — integer-literal day count.
    Literal(i64),
}

/// `ir-resource-conventions-owner-scope` §7.1 — AST-level mirror of
/// `ir::OwnerAxis`. `through_column` carries the bare identifier the
/// author wrote between the parens (e.g. `user` for the canonical pilot's
/// `Property → Host → User` chain). String-literal arguments are a
/// parse error (per §7.1, the value is a syntactic identifier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerAxisAst {
    /// Bare identifier inside the parens of `@owner_axis(through: <ident>)`.
    pub through_column: String,
}

pub(crate) fn is_false_bool(value: &bool) -> bool {
    !*value
}

/// L0 #3 §10 — parser-side capture of the 6 inline field constraints.
/// Mirrors `ir::FieldConstraints` but stays in the AST layer so the
/// analyzer can apply combination + default-compat checks before
/// projecting into the IR. `r#in` values are stored verbatim (no
/// surrounding quotes for string literals; numerics as their text
/// form) — the analyzer / emitters interpret per type.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FieldConstraintsDecl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<(i64, i64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub r#in: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitize_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utf8_safe: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recursion: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_pii: Option<String>,
}

impl FieldConstraintsDecl {
    /// `true` when none of the constraint slots were authored. Used as
    /// the `skip_serializing_if` guard so untouched fields don't ship
    /// an empty constraints object in JSON.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::FieldConstraintsDecl;
    ///
    /// let empty = FieldConstraintsDecl::default();
    /// assert!(empty.is_empty());
    ///
    /// let with_min = FieldConstraintsDecl { min: Some(0), ..Default::default() };
    /// assert!(!with_min.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.min.is_none()
            && self.max.is_none()
            && self.pattern.is_none()
            && self.between.is_none()
            && self.length.is_none()
            && self.r#in.is_none()
            && self.sanitize_html.is_none()
            && self.utf8_safe.is_none()
            && self.max_recursion.is_none()
            && self.max_size.is_none()
            && self.covers_pii.is_none()
    }
}

/// One `has_many <name>: <Resource> [inverse <field>]` row on a [`ResourceDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHasMany {
    pub name: String,
    /// Resource type reference, e.g. `CustomerNote`.
    pub type_text: String,
    /// `inverse <field>` clause — captured verbatim.
    pub inverse: Option<String>,
    pub span: Span,
}

/// Spec 0014 — one `restrict on_delete references <relation> via <fk>
/// [where <predicate>]` clause on a [`ResourceDecl`]. Declares that the
/// protected resource cannot be deleted while live rows of `relation`
/// still point at it through column `fk`. Lowers to a tenant-scoped,
/// soft-delete-aware `EXISTS` precondition before every delete/destructive
/// mutate of the resource.
///
/// `extra_where` is the optional `where <predicate>` subset filter
/// (captured verbatim) for guards that only count a *subset* of references
/// (e.g. only *open* activities). The tenant-scope (`tenant_id = …`) and
/// soft-delete (`deleted_at IS NULL`) predicates are NOT author-supplied —
/// they are derived by the analyzer from the referencing relation's schema
/// so they can never be forgotten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRestrictOnDelete {
    /// Referencing relation name (e.g. `invoice`), captured verbatim.
    pub relation: String,
    /// Foreign-key column on `relation` pointing at this resource's id
    /// (e.g. `billing_type_id`).
    pub fk: String,
    /// `where <predicate>` subset filter, verbatim. `None` for the common
    /// "any live reference" case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_where: Option<String>,
    /// Spec 0014 GAP-2 — optional `error <CODE>` clause pinning a per-guard
    /// domain error code (e.g. `CATEGORY_HAS_CUSTOMERS`) the emitter rejects
    /// with instead of the bare `runtime.ErrReferencedInUse` sentinel.
    /// `None` keeps the back-compat sentinel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub span: Span,
}

/// `retention <duration> then <action>` row on a [`ResourceDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRetention {
    /// Duration literal, e.g. `7y`, `30d`. Captured verbatim.
    pub duration: String,
    /// `Anonymize | Delete | Archive` closed catalog.
    pub action: ResourceRetentionAction,
    pub span: Span,
}

/// Closed three-arm catalog for the `then <action>` clause of
/// [`ResourceRetention`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRetentionAction {
    /// Strip PII fields, keep the row.
    Anonymize,
    /// Hard-delete the row.
    Delete,
    /// Move the row to cold storage / archive table.
    Archive,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_constraints_decl_default_is_empty() {
        assert!(FieldConstraintsDecl::default().is_empty());
    }

    #[test]
    fn resource_retention_action_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(ResourceRetentionAction::Anonymize).unwrap(),
            serde_json::json!("anonymize")
        );
    }

    #[test]
    fn resource_index_method_ast_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(ResourceIndexMethodAst::Btree).unwrap(),
            serde_json::json!("btree")
        );
        assert_eq!(
            serde_json::to_value(ResourceIndexMethodAst::Gin).unwrap(),
            serde_json::json!("gin")
        );
    }

    #[test]
    fn resource_lock_optimistic_carries_version_field() {
        let l = ResourceLock::Optimistic {
            version_field: "lock_version".into(),
        };
        let v = serde_json::to_value(&l).unwrap();
        assert_eq!(v["kind"], "Optimistic");
        assert_eq!(v["version_field"], "lock_version");
    }
}
