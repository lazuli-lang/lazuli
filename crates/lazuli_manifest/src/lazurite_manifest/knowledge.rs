//! `[knowledge]` block schema — declares CUSTOM knowledge sectors a
//! project recognizes on top of the closed core catalog.
//!
//! Features author `knowledge <sector>` to draw authoring knowledge from a
//! `knowledge/<sector>/` document vault. `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001`
//! treats a sector as KNOWN when it is in the closed core catalog
//! (`decisions`, `changes`, `gaps`, `lazuli-way`), declared here under
//! `[knowledge.sectors]`, OR backed by a `knowledge/<sector>/` folder.
//!
//! The design is a CLOSED CORE plus GOVERNED flexibility: a project may
//! declare extra sectors (so the rule recognizes a slug before its folder
//! is scaffolded), but the declaration is explicit — not a free-for-all
//! dialect.
//!
//! ```toml
//! [knowledge.sectors]
//! billing = "Revenue, invoicing, reconciliation"
//! compliance = "KYC / audit / regulatory"
//! ```
//!
//! `_inbox` is a staging directory, not a sector — it is neither in the
//! core catalog nor a valid declaration target.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level `[knowledge]` block. Optional; absent on most projects, which
/// then rely solely on the core catalog + on-disk folders.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Knowledge {
    /// `[knowledge.sectors]` — a table of custom sector slugs to optional
    /// human descriptions. Keys are the declared sector slugs; values are
    /// free-text descriptions (or empty). The descriptions are advisory
    /// only — the doctor keys solely on the presence of the slug.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sectors: BTreeMap<String, String>,
}

impl Knowledge {
    /// The declared custom sector slugs, in deterministic (sorted) order.
    /// Threaded to `VOCAB-KNOWLEDGE-SECTOR-UNKNOWN-001` as the "declared"
    /// leg of its known-sector check.
    pub fn declared_sectors(&self) -> Vec<String> {
        self.sectors.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Shim {
        #[serde(default)]
        knowledge: Option<Knowledge>,
    }

    #[test]
    fn parses_sectors_table_with_descriptions() {
        let toml = r#"
[knowledge.sectors]
billing = "Revenue, invoicing, reconciliation"
compliance = "KYC / audit / regulatory"
"#;
        let shim: Shim = toml::from_str(toml).expect("deserialize");
        let k = shim.knowledge.expect("knowledge block");
        assert_eq!(k.declared_sectors(), vec!["billing", "compliance"]);
        assert_eq!(
            k.sectors.get("billing").map(String::as_str),
            Some("Revenue, invoicing, reconciliation"),
        );
    }

    #[test]
    fn empty_value_description_is_allowed() {
        let toml = "[knowledge.sectors]\nbilling = \"\"\n";
        let shim: Shim = toml::from_str(toml).expect("deserialize");
        let k = shim.knowledge.expect("knowledge block");
        assert_eq!(k.declared_sectors(), vec!["billing"]);
    }

    #[test]
    fn absent_block_yields_no_declared_sectors() {
        let shim: Shim = toml::from_str("").expect("deserialize");
        assert!(shim.knowledge.is_none());
        assert!(Knowledge::default().declared_sectors().is_empty());
    }
}
