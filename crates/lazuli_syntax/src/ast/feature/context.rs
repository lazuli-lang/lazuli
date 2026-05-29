//! Iron-hand context-vocabulary — feature-scoped intent declarations.
//!
//! `purpose "<sentence>"`, `non_goals { "..." }`, `attach_ctx "<path>"` —
//! surfaced by `VOCAB-CONTEXT-*` lints. The `tdd-iron-hand` preset
//! promotes the lints from warn to error. See
//! `docs/canonical-semantics.md#feature-context-vocabulary`.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// Iron-hand `purpose "<sentence>"` line. The string is whatever the
/// author wrote between the quotes; empty / whitespace-only is allowed
/// at parse time so the lint can fire a precise diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeaturePurpose {
    pub text: String,
    pub span: Span,
}

/// Iron-hand `non_goals` block. Children are one-quoted-string-per-line
/// entries (mirrors how `uses` lists work for the wire-thin slice).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureNonGoals {
    pub entries: Vec<String>,
    pub span: Span,
}

/// Iron-hand `attach_ctx "<relative-path>"` line. Path is verbatim;
/// resolution against the project root happens in the doctor lint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureAttachCtx {
    pub path: String,
    pub span: Span,
}

/// Iron-hand `knowledge <sector>` line. The sector is a bareword slug
/// (kebab/snake, e.g. `billing`) — NOT a quoted string — naming the
/// `knowledge/<sector>/` vault the feature draws from. Resolution
/// against the on-disk vault happens in the `VOCAB-KNOWLEDGE-*` doctor
/// lints (a later stage); the parser only captures the slug verbatim. See
/// `docs/proposals/knowledge-sector-field.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LziFeatureKnowledge {
    pub sector: String,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_text_preserved_verbatim() {
        let p = LziFeaturePurpose {
            text: "Ship the canonical pilot reservations".into(),
            span: Span::new(0, 0),
        };
        assert!(p.text.contains("the canonical pilot"));
    }

    #[test]
    fn non_goals_entries_can_be_empty() {
        let n = LziFeatureNonGoals {
            entries: vec![],
            span: Span::new(0, 0),
        };
        assert!(n.entries.is_empty());
    }

    #[test]
    fn knowledge_sector_preserved_verbatim() {
        let k = LziFeatureKnowledge {
            sector: "billing".into(),
            span: Span::new(0, 0),
        };
        assert_eq!(k.sector, "billing");
    }
}
