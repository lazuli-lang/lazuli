//! Registry of `FixAction` instances keyed by rule code.

use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::actions::{FixAction, insert_tests_block, scaffold_errors_block};
use crate::{FixOutcome, FixRequest, FixResult};

pub struct FixRegistry {
    actions: HashMap<String, Box<dyn FixAction + Send + Sync>>,
}

impl FixRegistry {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    pub fn register(&mut self, action: Box<dyn FixAction + Send + Sync>) {
        let key = action.rule_code().to_string();
        self.actions.insert(key, action);
    }

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
