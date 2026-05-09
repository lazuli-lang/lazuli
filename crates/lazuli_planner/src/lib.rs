use lazuli_ir::Application;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub app_name: String,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub label: String,
    pub risk: Risk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    Low,
    Medium,
    High,
}

pub fn plan_initial_generation(app: &Application) -> Plan {
    Plan {
        app_name: app.name.clone(),
        steps: vec![
            PlanStep {
                label: "Generate Go backend target".to_owned(),
                risk: Risk::Low,
            },
            PlanStep {
                label: "Generate React frontend target".to_owned(),
                risk: Risk::Low,
            },
        ],
    }
}
