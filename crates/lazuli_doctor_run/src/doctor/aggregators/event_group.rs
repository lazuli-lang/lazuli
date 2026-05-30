//! `event_group` pattern-prefix aggregator (row 34).
//!
//! Two warnings on the canonical-indent `event_group` lift:
//!
//! - `EVENTGROUP-NESTING-001` — two sibling groups in the same feature
//!   share a prefix (one fully contains the other). The author probably
//!   meant to nest, or to rename one of the two patterns.
//! - `EVENTGROUP-PREFIX-001` — an event authored under group `A_*`
//!   carries a name that matches a sibling group `B_*`'s prefix. The
//!   event is almost certainly under the wrong group.
//!
//! Both rules operate on the lifted `EventGroup` IR carried by
//! `Tier3FeatureFacts`; the analyzer promotes short event names with
//! the group prefix at lowering time, so this checker only fires when
//! the author wrote an event whose name does NOT match its own group
//! prefix AND another group's prefix DOES match.

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(crate) fn diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for feature in facts {
        for group in &feature.event_groups {
            let line = feature
                .event_group_lines
                .get(&group.pattern)
                .copied()
                .unwrap_or(feature.feature_line);

            // EVENTGROUP-NESTING-001: parse_event_group records nested
            // `event_group` headers as raw child lines today, so we
            // scan `raw_payload` + adjacent groups; for now we surface
            // the case where two groups share the same parent feature
            // *and* one pattern fully contains another's prefix.
            for other in &feature.event_groups {
                if other.pattern == group.pattern {
                    continue;
                }
                if let (Some(group_prefix), Some(other_prefix)) = (
                    group.pattern.strip_suffix('*'),
                    other.pattern.strip_suffix('*'),
                ) && other_prefix.starts_with(group_prefix)
                    && other_prefix != group_prefix
                {
                    diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Warning,
                            code: "EVENTGROUP-NESTING-001".to_owned(),
                            message: format!(
                                "event_group `{}` in feature `{}` is a prefix of `{}` — nest in the more specific group or rename one pattern.",
                                group.pattern, feature.feature, other.pattern
                            ),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                }
            }

            // Pattern-prefix rule (row 34). Strip trailing `*` to get
            // the group prefix. Short event names are *promoted* by
            // the group's prefix at lowering time — `event created`
            // under `customer_*` becomes the qualified event
            // `customer_created`. Authored event names are short names
            // by default in canonical Lazuli; the rule only fires
            // when the same feature declares *another* group whose
            // prefix matches the event — then the author probably
            // wrote the event under the wrong group.
            if let Some(prefix) = group.pattern.strip_suffix('*')
                && !prefix.is_empty()
            {
                for event_name in &group.events {
                    if event_name.starts_with(prefix) {
                        continue;
                    }
                    // Look for another group whose prefix the
                    // event matches; only then is misrouting likely.
                    let other_owner = feature.event_groups.iter().find(|other| {
                        if other.pattern == group.pattern {
                            return false;
                        }
                        let Some(other_prefix) = other.pattern.strip_suffix('*') else {
                            return false;
                        };
                        !other_prefix.is_empty() && event_name.starts_with(other_prefix)
                    });
                    if let Some(other) = other_owner {
                        diagnostics.push(DoctorDiagnostic {
                                path: feature.path.clone(),
                                line,
                                column: 1,
                                severity: DoctorSeverity::Warning,
                                code: "EVENTGROUP-PREFIX-001".to_owned(),
                                message: format!(
                                    "event `{}` authored under group `{}` matches group `{}`'s prefix — move it to the matching group or rename.",
                                    event_name, group.pattern, other.pattern
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                    }
                }
            }
        }
    }
    diagnostics
}
