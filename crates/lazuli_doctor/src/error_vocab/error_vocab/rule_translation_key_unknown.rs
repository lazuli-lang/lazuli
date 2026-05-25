//! ERR-VOCAB-002 — `@translation.<key>` does not resolve to a declared key.
//!
//! Sites checked:
//! * `Command.policy_when_denied`        (per-command override)
//! * `PolicyCategory.when_denied`        (per-policy override)
//! * `FeatureErrors.messages[].message`  (feature-level catch-all)
//!
//! Resolution scope: same feature first; falls back to features named in
//! `Feature.uses` (cross-feature lookup). The legacy
//! `rule_message_ref_unresolved` rule covers `Rule.message_ref` strings;
//! this rule extends the cross-check to the new typed surfaces.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 ERR-VOCAB-002.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, SpanRef, TranslationKeyRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUnknownFinding {
    pub path: PathBuf,
    pub feature: String,
    pub key: String,
    /// Where the reference was authored — e.g. `command capture_lead.when_denied`,
    /// `policies update.when_denied`, `errors policy_denied`. Improves the
    /// rendered message.
    pub site: String,
    pub span: Option<SpanRef>,
}

impl KeyUnknownFinding {
    pub const CODE: &'static str = "ERR-VOCAB-002";

    pub fn message(&self, declared: &[&str]) -> String {
        let declared_text = if declared.is_empty() {
            "<none declared>".to_owned()
        } else {
            declared.join(", ")
        };
        format!(
            "`@translation.{}` referenced from {} does not resolve in feature `{}`. Declared \
             keys: {}.",
            self.key, self.site, self.feature, declared_text
        )
    }
}

/// Run ERR-VOCAB-002 over one feature. `keys_by_feature` maps every
/// feature name to its declared translation key catalog so cross-feature
/// `uses` lookup works without re-walking the package.
pub fn check_translation_key_unknown(
    feature: &Feature,
    path: &Path,
    keys_by_feature: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<KeyUnknownFinding> {
    let mut findings = Vec::new();
    let visible = visible_translation_keys(feature, keys_by_feature);

    // Site 1 — per-command `when_denied`.
    for cmd in &feature.commands {
        if let Some(reference) = &cmd.policy_when_denied {
            ensure_resolves(
                reference,
                &visible,
                feature,
                path,
                format!("command `{}.{}.when_denied`", feature.name, cmd.name),
                &mut findings,
            );
        }
    }
    // Site 2 — per-policy `when_denied`.
    for category in &feature.policies.categories {
        if let Some(reference) = &category.when_denied {
            ensure_resolves(
                reference,
                &visible,
                feature,
                path,
                format!("policies `{}.{}.when_denied`", feature.name, category.name),
                &mut findings,
            );
        }
    }
    // Site 3 — feature-level `errors <code> message @translation.<key>`.
    if let Some(errors) = &feature.errors {
        for entry in &errors.messages {
            ensure_resolves(
                &entry.message,
                &visible,
                feature,
                path,
                format!("errors `{}.{}`", feature.name, entry.code),
                &mut findings,
            );
        }
    }
    findings
}

fn ensure_resolves(
    reference: &TranslationKeyRef,
    visible: &BTreeSet<String>,
    feature: &Feature,
    path: &Path,
    site: String,
    out: &mut Vec<KeyUnknownFinding>,
) {
    if visible.contains(&reference.key) {
        return;
    }
    out.push(KeyUnknownFinding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        key: reference.key.clone(),
        site,
        span: reference.span_ref,
    });
}

fn visible_translation_keys(
    feature: &Feature,
    keys_by_feature: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut visible = BTreeSet::new();
    // Same-feature keys take precedence.
    if let Some(translation) = &feature.translation {
        for key in &translation.keys {
            visible.insert(key.name.clone());
        }
    }
    // Cross-feature lookup through `uses` — the resolution chain (proposal
    // §2.E step 3) allows a feature to reference translation keys from any
    // feature it explicitly consumes via `uses`. The mapping is package-
    // wide because `Feature.uses` lists feature names only (no per-key
    // visibility surface in v1).
    for used in &feature.uses {
        if let Some(declared) = keys_by_feature.get(used) {
            for key in declared {
                visible.insert(key.clone());
            }
        }
    }
    visible
}
