//! ENC-E2EE-EVENT-001 — an event payload field carries an
//! `@cap.E2ee(key:@key.<scope>)` capability. E2ee fields must not
//! appear in event payloads — the server cannot decrypt them and
//! consumers would see opaque ciphertext.
//!
//! Severity: `error` (strict + production).
//! Reference: docs/proposals/encryption-vocab.md §Doctor diagnostics.

use std::path::{Path, PathBuf};

use lazuli_ir::{CapabilityRef, Feature, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

/// One ENC-E2EE-EVENT-001 finding — an event payload field is tagged
/// with the `@cap.E2ee` capability. Event payloads cross consumer
/// boundaries the server cannot decrypt for, so the only safe shape is
/// to keep the ciphertext off the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file that declared the offending event.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Event the field belongs to.
    pub event: String,
    /// Payload field carrying the E2ee capability.
    pub field: String,
    /// `@key.<scope>` reference, verbatim.
    pub key_scope: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ENC-E2EE-EVENT-001";

    /// Render the "E2ee in event payload" message. Includes the key
    /// scope so authors recognise WHICH key boundary the field is
    /// attached to and can plan a `@cap.Encrypted(key:@key.tenant)`
    /// switch or a separate consumer-side fetch.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::encryption::e2ee_event::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     event: "message_sent".into(),
    ///     field: "body".into(),
    ///     key_scope: "@key.user".into(),
    /// };
    /// assert!(f.message().contains("E2ee"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "event payload `{}.{}` exposes `{}` declared as `@cap.E2ee(key:{})` — E2ee fields must not appear in event payloads (the server cannot decrypt them and consumers see ciphertext)",
            self.feature, self.event, self.field, self.key_scope
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Walk every event in `feature` and emit a finding for each payload
/// field tagged with `@cap.E2ee`. `path` anchors the resulting findings
/// to the source `.lzi` so the LSP / CLI can surface a clickable
/// `<file>:<line>:` line.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::encryption::e2ee_event::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with @cap.E2ee on an event payload");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for event in &feature.events {
        for field in &event.payload {
            let TypeRef::Capability(CapabilityRef::E2ee(e2ee)) = &field.type_ref else {
                continue;
            };
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                event: event.name.clone(),
                field: field.name.clone(),
                key_scope: e2ee.key.clone(),
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::test_support::*;
    use lazuli_ir::{CapabilityRef, E2eeCapability, EncryptedCapability};

    #[test]
    fn positive_e2ee_in_event_payload_fires() {
        let mut feature = empty_feature("messages");
        let event = event_with_payload(
            "message_sent",
            vec![event_field(
                "body",
                TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
                    key: "@key.user".into(),
                })),
            )],
        );
        feature.events.push(event);

        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].event, "message_sent");
        assert_eq!(findings[0].field, "body");
        assert_eq!(findings[0].key_scope, "@key.user");
        assert!(findings[0].message().contains("E2ee"));
    }

    #[test]
    fn negative_encrypted_field_in_payload_passes() {
        // `@cap.Encrypted` is server-readable; passing it through an
        // event payload is allowed (codegen decrypts before serialise).
        let mut feature = empty_feature("customer");
        let event = event_with_payload(
            "customer_updated",
            vec![event_field(
                "external_id",
                TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
                    key: "@key.tenant".into(),
                })),
            )],
        );
        feature.events.push(event);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_no_events_passes() {
        let feature = empty_feature("customer");
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
