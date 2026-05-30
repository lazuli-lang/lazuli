//! COMPOSE-NULLABILITY-MISMATCH-001 — nullable source into a required field.
//!
//! ## Rule statement
//!
//! Fires when a `query.compose` projects a **nullable** source into a
//! **non-optional** return-record field:
//!
//! - the projection reads `<alias>.<col>` from an `optional` (LEFT) join — a
//!   LEFT join can produce NULL for the joined columns; OR
//! - the projection reads a `latest` subselect — a `latest <col> of <R>` is
//!   `NULL` when the child set is empty.
//!
//! …and the matching field on the return record is declared non-`optional`.
//! Compose makes nullability a typed contract rather than the per-column
//! `COALESCE(x,'')` plumbing a hand-written query author may forget
//! (`list_chat_inbox.go:28` in the audit). The fix is to mark the record field
//! `optional` (the honest type) — never to silently coalesce.
//!
//! ## Severity profile
//!
//! Severity: `warning` (strict + production). It is a typed-contract hygiene
//! nudge: the read runs, but the generated type claims non-null for a value
//! that can be null, which the TS/Go consumer then mishandles.
//!
//! ## Fixture example (fires)
//!
//! ```lzi
//! record ChatInboxRow
//!   counterpart_name: Text          # required, but the source is LEFT-joined
//! query.compose chat_inbox
//!   from Chat
//!   join chat.counterpart as cp optional
//!   select
//!     counterpart_name = cp.name
//!   returns ChatInboxRow
//! ```
//!
//! ## Proposal anchor
//!
//! `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §7
//! (`COMPOSE-NULLABILITY-MISMATCH-001`). Diagnostic ID / code constant:
//! `COMPOSE-NULLABILITY-MISMATCH-001`.

use std::path::{Path, PathBuf};

use lazuli_ir::{
    ComposeQuery, Feature, ProjectionSource, Record, SubselectKind, TypeRef,
};

use super::composes_of;

/// One COMPOSE-NULLABILITY-MISMATCH-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the compose was authored in.
    pub path: PathBuf,
    /// Feature owning the compose.
    pub feature: String,
    /// `query.compose <name>`.
    pub query_name: String,
    /// The projection (= record field) whose nullability is mismatched.
    pub field: String,
    /// Why the source is nullable — `"optional (LEFT) join `cp`"` or
    /// `"`latest` subselect"`.
    pub source_reason: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSE-NULLABILITY-MISMATCH-001";

    /// Render the nullability-mismatch message.
    pub fn message(&self) -> String {
        format!(
            "query.compose {} projects `{}` from a nullable source ({}) into a non-optional record \
             field. A LEFT-joined column / empty `latest` is NULL at runtime. Mark the record \
             field `optional` so the generated type is honest, instead of relying on COALESCE.",
            self.query_name, self.field, self.source_reason
        )
    }
}

/// Run COMPOSE-NULLABILITY-MISMATCH-001 over one feature.
///
/// Enforced only when the return record is an in-feature `record` whose field
/// can be inspected; a generated / cross-feature return type defers.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::compose::nullability_mismatch_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a query.compose");
/// let _ = check(&feature, Path::new("messaging.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for compose in composes_of(feature) {
        let Some(record) = return_record(compose, &feature.records) else {
            continue;
        };
        for proj in &compose.projections {
            let Some(reason) = nullable_source_reason(compose, &proj.source) else {
                continue;
            };
            // The matching record field must be declared non-optional
            // (`required`) to be a mismatch.
            if record
                .fields
                .iter()
                .any(|f| f.name == proj.name && f.required)
            {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    query_name: compose.name.clone(),
                    field: proj.name.clone(),
                    source_reason: reason,
                });
            }
        }
    }
    findings
}

/// `Some(reason)` when a projection source is nullable (optional LEFT join, or
/// a `latest` subselect); `None` otherwise.
fn nullable_source_reason(compose: &ComposeQuery, source: &ProjectionSource) -> Option<String> {
    match source {
        ProjectionSource::Joined(alias, _) => {
            let join = compose.joins.iter().find(|j| &j.alias == alias)?;
            join.nullable
                .then(|| format!("optional (LEFT) join `{alias}`"))
        }
        ProjectionSource::Subselect(name) => {
            let sub = compose.subselects.iter().find(|s| &s.name == name)?;
            matches!(sub.kind, SubselectKind::Latest { .. })
                .then(|| "`latest` subselect (NULL on empty set)".to_owned())
        }
        // `self.<col>` nullability follows the root field's own `optional`
        // declaration — record/field shape coherence is a record-shape concern,
        // not the compose-specific LEFT-join / latest hazard this rule owns.
        ProjectionSource::SelfCol(_) => None,
    }
}

/// Resolve the compose's return record to an in-feature `record`. `None` for a
/// cross-feature return type or a name with no matching `record`.
fn return_record<'a>(compose: &ComposeQuery, records: &'a [Record]) -> Option<&'a Record> {
    let TypeRef::UserDefined(qname) = &compose.returns else {
        return None;
    };
    if qname.feature.is_some() {
        return None;
    }
    records.iter().find(|r| r.name == qname.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn left_join_into_required_field_fires() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
    resource User
      org: Org required
      name: Text required
    record ChatInboxRow
      chat_id: ID
      counterpart_name: Text required
    query.compose chat_inbox
      from Chat
      join chat.counterpart as cp optional
      select
        chat_id = self.id
        counterpart_name = cp.name
      returns ChatInboxRow
"#,
        );

        let findings = check(&feature, Path::new("messaging.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "counterpart_name");
        assert!(findings[0].source_reason.contains("LEFT"));
        assert_eq!(Finding::CODE, "COMPOSE-NULLABILITY-MISMATCH-001");
    }

    #[test]
    fn latest_subselect_into_required_field_fires() {
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
    resource ChatMessage
      chat: Chat required
      body: Text required
      created_at: DateTime required
    record ChatInboxRow
      chat_id: ID
      last_message_preview: Text required
    query.compose chat_inbox
      from Chat
      subselect last_message_preview = latest body of ChatMessage
        related_by chat_message.chat
        order created_at desc
      select
        chat_id = self.id
        last_message_preview = last_message_preview
      returns ChatInboxRow
"#,
        );

        let findings = check(&feature, Path::new("messaging.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "last_message_preview");
        assert!(findings[0].source_reason.contains("latest"));
    }

    #[test]
    fn optional_record_field_does_not_fire() {
        // Same LEFT join, but the record field is `optional` — the honest type.
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
    resource User
      org: Org required
      name: Text required
    record ChatInboxRow
      chat_id: ID
      counterpart_name: Text optional
    query.compose chat_inbox
      from Chat
      join chat.counterpart as cp optional
      select
        chat_id = self.id
        counterpart_name = cp.name
      returns ChatInboxRow
"#,
        );

        assert!(check(&feature, Path::new("messaging.lzi")).is_empty());
    }

    #[test]
    fn inner_join_into_required_field_does_not_fire() {
        // INNER join (no `optional`) → not nullable → no mismatch.
        let feature = lower(
            r#"
feature messaging
  domain
    resource Chat
      org: Org required
      counterpart: User
    resource User
      org: Org required
      name: Text required
    record ChatInboxRow
      chat_id: ID
      counterpart_name: Text required
    query.compose chat_inbox
      from Chat
      join chat.counterpart as cp
      select
        chat_id = self.id
        counterpart_name = cp.name
      returns ChatInboxRow
"#,
        );

        assert!(check(&feature, Path::new("messaging.lzi")).is_empty());
    }
}
