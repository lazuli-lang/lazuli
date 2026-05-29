//! Tier-3 cross-feature event-payload index helpers.
//!
//! `build_event_payload_index` is the once-per-doctor-run map keyed on
//! `<feature>.<event-name>` so `NOTIF-DIGEST-001`'s `group_by`
//! resolution stays constant-time. `leading_assignment_lhs` is the
//! tiny LHS extractor used by the index builder to fish field names
//! out of `event_group.raw_payload` lines.
//!
//! Lifted from the parent `tier3` god-file in the rails-style split.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::Tier3FeatureFacts;

/// Notifications expanded bucket cycle — cross-feature event-payload
/// index keyed on `<feature>.<event-name>`. Each entry stores the
/// union of (a) event-specific typed payload fields, (b) `event_group`
/// `raw_payload` lines that apply to the event via the group's glob
/// pattern. Built once per doctor run so `NOTIF-DIGEST-001` is
/// constant-time per notification.
pub(crate) fn build_event_payload_index(
    facts: &[Tier3FeatureFacts],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for feature in facts {
        // (a) Typed events lifted on the feature (legacy flow may
        //     populate `Feature.events` in the future; today the
        //     canonical-indent slice leaves it empty, but the loop is
        //     here so the index stays correct when the legacy lifter
        //     catches up).
        for event in &feature.events {
            let key = format!("{}.{}", feature.feature, event.name);
            let fields: BTreeSet<String> = event.payload.iter().map(|f| f.name.clone()).collect();
            index.entry(key).or_default().extend(fields);
        }

        // (b) Concrete events authored under `event_group <prefix>*`
        //     blocks. The lift stores them as short names; the
        //     qualified event name a notification references is
        //     `<feature>.<prefix><short>`. The payload set is the
        //     union of the group's `payload` block (raw `<name> =
        //     <expr>` lines, plus payload-shaped lines like
        //     `customer_id`).
        for group in &feature.event_groups {
            let prefix = group.pattern.strip_suffix('*').unwrap_or(&group.pattern);
            let mut group_fields: BTreeSet<String> = BTreeSet::new();
            for raw in &group.raw_payload {
                if let Some(name) = leading_assignment_lhs(raw) {
                    group_fields.insert(name.to_owned());
                }
            }
            for short_name in &group.events {
                // Avoid double-prefixing when the author already wrote
                // the full prefixed name (`event customer_archived`
                // instead of `event archived` under `customer_*`).
                let qualified = if short_name.starts_with(prefix) {
                    format!("{}.{}", feature.feature, short_name)
                } else {
                    format!("{}.{}{}", feature.feature, prefix, short_name)
                };
                index
                    .entry(qualified)
                    .or_default()
                    .extend(group_fields.iter().cloned());
            }
        }
    }

    index
}

/// Notifications expanded bucket cycle — extract the LHS of an
/// `<name> = <expr>` assignment captured in `event_group.raw_payload`.
/// Returns the bare field name or `None` if the line is not an
/// assignment (e.g. a deeper `audit ...` or comment leftover).
pub(crate) fn leading_assignment_lhs(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let (lhs, _rest) = trimmed.split_once('=')?;
    let lhs = lhs.trim();
    if lhs.is_empty() || lhs.contains(char::is_whitespace) {
        return None;
    }
    Some(lhs)
}
