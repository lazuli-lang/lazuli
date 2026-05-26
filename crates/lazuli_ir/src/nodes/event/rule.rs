//! `Rule` — declarative deny clause.
//!
//! [`Rule`] is the "this command/transition cannot happen when X" form. It
//! declares an [`OperationRef`] target and a [`crate::Predicate`] over the
//! operation's input. The message is either a literal string or a
//! [`crate::TranslationKeyRef`] via `message @translation.<key>`. Rules are
//! evaluated at the operation boundary; codegen emits them as pre-execution
//! gates.

use serde::{Deserialize, Serialize};

use crate::{Predicate, QualifiedName, SpanRef, TestBlock};

/// `rule "title" { denies … when … message … }` — declarative deny
/// clause. Evaluated at the operation boundary; codegen emits it as a
/// pre-execution gate that returns the rendered message on hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Author's prose title: `rule "archived customers cannot be reassigned"`.
    pub title: String,
    pub denies: OperationRef,
    pub when: Predicate,
    pub message: String,
    /// i18n bucket cycle — `message @translation.<key>` form. When set,
    /// `message` is the empty string; the runtime resolves the typed
    /// key at render time using `ctx.locale`. Doctor cross-checks the
    /// reference against the surrounding feature's `Translation.keys`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `denies <resource>.<op>` reference resolved to a typed operation
/// kind during lowering. Used by [`Rule`] to bind the deny clause to
/// the right boundary (command vs lifecycle transition).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRef {
    pub resource: QualifiedName,
    pub op_name: String,
    pub kind: OperationKind,
}

/// Closed catalog of operations a rule can deny. `Unresolved` is the
/// legacy lowering default for fixtures pre-dating resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    /// `<resource>.<command>` — a declarative write.
    Command,
    /// `<resource>.<transition>` — a lifecycle transition.
    Transition,
    /// Resolution deferred to the analyzer; default for legacy lowering.
    Unresolved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_kind_round_trips() {
        let s = serde_json::to_string(&OperationKind::Command).unwrap();
        let back: OperationKind = serde_json::from_str(&s).unwrap();
        assert_eq!(back, OperationKind::Command);
    }
}
