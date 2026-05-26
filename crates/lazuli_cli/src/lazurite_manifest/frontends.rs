//! `[frontends.<name>]` block schema — one entry per frontend
//! deployable. Two canonical `target` variants are supported today:
//! `expo` (React Native via Expo Router) and `tanstack-vite`
//! (TanStack-Router + Vite for the web).

use serde::{Deserialize, Serialize};

/// One `[frontends.<name>]` block — a single frontend deployable.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Frontend {
    /// Closed catalog of frontend runtimes.
    pub target: FrontendTarget,
    /// Optional override for the source directory under the project
    /// root (defaults to `app/<name>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Generated-output directory under the project root.
    pub out: String,
    /// Names of the audiences (declared in `.lzx`) this frontend
    /// renders. Empty means "render every audience".
    pub audiences: Vec<String>,
}

/// Closed catalog of supported frontend runtimes.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum FrontendTarget {
    /// React Native via Expo Router.
    Expo,
    /// TanStack-Router + Vite (web).
    TanstackVite,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_target_round_trips_kebab_case() {
        let json = serde_json::to_string(&FrontendTarget::TanstackVite).unwrap();
        assert_eq!(json, "\"tanstack-vite\"");
        let parsed: FrontendTarget = serde_json::from_str("\"expo\"").unwrap();
        assert!(matches!(parsed, FrontendTarget::Expo));
    }
}
