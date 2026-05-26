//! Registry of [`FixAction`] instances keyed by rule code.
//!
//! Both the CLI and the LSP dispatch through a [`FixRegistry`]. The
//! default registry registers every built-in action from
//! [`crate::actions`]; tests can build an empty registry and register
//! individual actions in isolation.

use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::actions::{FixAction, insert_tests_block, scaffold_errors_block};
use crate::{FixOutcome, FixRequest, FixResult};

/// Map of rule code → boxed [`FixAction`] handler.
///
/// `Send + Sync` bounds are required because the LSP shares one
/// registry across request-handler threads.
pub struct FixRegistry {
    actions: HashMap<String, Box<dyn FixAction + Send + Sync>>,
}

impl FixRegistry {
    /// Build an empty registry. Use [`FixRegistry::register`] to add
    /// actions, or prefer [`FixRegistry::default`] for the full
    /// built-in catalogue.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_fix::FixRegistry;
    ///
    /// let reg = FixRegistry::new();
    /// assert!(reg.supported_rules().is_empty());
    /// ```
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    /// Insert an action under its [`FixAction::rule_code`].
    ///
    /// If a different action is already registered for that code it is
    /// replaced — last-writer-wins. Used by [`FixRegistry::default`]
    /// to seed the built-in catalogue and by tests to inject mocks.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_fix::FixRegistry;
    /// use lazuli_fix::actions::insert_tests_block::InsertTestsBlock;
    ///
    /// let mut reg = FixRegistry::new();
    /// reg.register(Box::new(InsertTestsBlock));
    /// assert!(reg.supported_rules().contains(&"TEST-MISSING-AUTHORED-001".to_owned()));
    /// ```
    pub fn register(&mut self, action: Box<dyn FixAction + Send + Sync>) {
        let key = action.rule_code().to_string();
        self.actions.insert(key, action);
    }

    /// Look up the action for `request.rule` and run it.
    ///
    /// If no action is registered for the rule, returns a
    /// [`FixOutcome::Skipped`] result with a human-readable note
    /// rather than an `Err`. This keeps the LSP surface non-fatal for
    /// rules whose fix action is still in the migration backlog.
    ///
    /// ## Examples
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use lazuli_fix::{FixRegistry, FixRequest};
    ///
    /// let reg = FixRegistry::default();
    /// let request = FixRequest {
    ///     rule: "TEST-MISSING-AUTHORED-001".to_owned(),
    ///     path: PathBuf::from("examples/full-capsule/full-capsule.lzi"),
    ///     line: 1,
    ///     column: 1,
    ///     apply: false,
    /// };
    /// let _ = reg.execute(&request);
    /// ```
    pub fn execute(&self, request: &FixRequest) -> Result<FixResult> {
        match self.actions.get(&request.rule) {
            Some(action) => action.execute(request),
            None => Ok(FixResult {
                outcome: FixOutcome::Skipped,
                preview: String::new(),
                note: Some(format!(
                    "no fix action registered for rule {} (Wave 2.3 ships 2 actions; \
                     full migration is the follow-up cell)",
                    request.rule
                )),
            }),
        }
    }

    /// Returns the list of rule codes the registry knows how to fix.
    /// Used by `lazuli fix --list` and by tests.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_fix::FixRegistry;
    ///
    /// let codes = FixRegistry::default().supported_rules();
    /// assert!(codes.contains(&"TEST-MISSING-AUTHORED-001".to_owned()));
    /// assert!(codes.contains(&"ERROR-VOCAB-MISSING-001".to_owned()));
    /// ```
    pub fn supported_rules(&self) -> Vec<String> {
        let mut codes: Vec<String> = self.actions.keys().cloned().collect();
        codes.sort();
        codes
    }
}

impl Default for FixRegistry {
    fn default() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(insert_tests_block::InsertTestsBlock));
        reg.register(Box::new(scaffold_errors_block::ScaffoldErrorsBlock));
        reg
    }
}

#[allow(dead_code)]
fn ensure_action_present(reg: &FixRegistry, code: &str) -> Result<()> {
    if !reg.actions.contains_key(code) {
        return Err(anyhow!("no fix action for rule {code}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_registry_includes_both_built_in_actions() {
        let codes = FixRegistry::default().supported_rules();
        assert!(codes.contains(&"TEST-MISSING-AUTHORED-001".to_owned()));
        assert!(codes.contains(&"ERROR-VOCAB-MISSING-001".to_owned()));
    }

    #[test]
    fn unknown_rule_returns_skipped_not_error() {
        let reg = FixRegistry::default();
        let request = FixRequest {
            rule: "DOES-NOT-EXIST-999".to_owned(),
            path: PathBuf::from("/tmp/whatever.lzi"),
            line: 1,
            column: 1,
            apply: false,
        };
        let result = reg.execute(&request).expect("unknown rule should not error");
        assert!(matches!(result.outcome, FixOutcome::Skipped));
    }
}
