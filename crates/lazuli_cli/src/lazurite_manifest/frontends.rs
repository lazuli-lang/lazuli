//! `[frontends.<name>]` block schema — one entry per frontend
//! deployable. Two canonical `target` variants are supported today:
//! `expo` (React Native via Expo Router) and `tanstack-vite`
//! (TanStack-Router + Vite for the web).

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Frontend {
    pub target: FrontendTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub out: String,
    pub audiences: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum FrontendTarget {
    Expo,
    TanstackVite,
}
