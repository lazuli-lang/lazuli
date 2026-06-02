//! Tests for `checks/lifecycle_transition` — extracted from `mod.rs`
//! (Rails-style R9 split). Production code stays in the parent module.

use super::*;

fn feature(commands: Vec<CommandFacts>) -> CheckInput {
    CheckInput {
        features: vec![FeatureFacts {
            name: "billing".to_owned(),
            uses: Vec::new(),
            resources: vec![resource_with_lifecycle()],
            commands,
        }],
    }
}

fn resource_with_lifecycle() -> ResourceFacts {
    ResourceFacts {
        name: "Payment".to_owned(),
        lifecycle: Some(LifecycleFacts {
            discriminator_field: "lifecycle_state".to_owned(),
            transitions: vec![
                transition("authorize", &["draft"], "authorized"),
                transition("capture", &["authorized"], "captured"),
                transition("refund", &["captured"], "refunded"),
                transition("cancel", &["draft"], "cancelled"),
            ],
        }),
    }
}

fn resource_without_lifecycle() -> ResourceFacts {
    ResourceFacts {
        name: "Payment".to_owned(),
        lifecycle: None,
    }
}

fn transition(name: &str, from: &[&str], to: &str) -> TransitionFacts {
    TransitionFacts {
        name: name.to_owned(),
        from: from.iter().map(|state| (*state).to_owned()).collect(),
        to: to.to_owned(),
    }
}

fn update_command(name: &str, triggers: &[&str], assignments: &[&str]) -> CommandFacts {
    CommandFacts {
        feature: "billing".to_owned(),
        name: name.to_owned(),
        triggers: triggers
            .iter()
            .map(|trigger| (*trigger).to_owned())
            .collect(),
        effect: CommandEffectFacts::Updates {
            resource: qname("Payment"),
            assignments: assignments
                .iter()
                .map(|field| (*field).to_owned())
                .collect(),
        },
        span: None,
    }
}

fn qname(name: &str) -> QualifiedName {
    QualifiedName {
        feature: None,
        name: name.to_owned(),
    }
}

fn returns_command(name: &str, triggers: &[&str]) -> CommandFacts {
    CommandFacts {
        feature: "billing".to_owned(),
        name: name.to_owned(),
        triggers: triggers
            .iter()
            .map(|trigger| (*trigger).to_owned())
            .collect(),
        effect: CommandEffectFacts::Other,
        span: None,
    }
}

fn codes(input: &CheckInput) -> Vec<&'static str> {
    check_input(input)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn lifecycle_transition_001_unknown_trigger_name() {
    let input = feature(vec![update_command("settle", &["bogus"], &["note"])]);
    assert_eq!(codes(&input), vec!["LIFECYCLE-TRANSITION-001"]);
}

/// The LIFECYCLE-TRANSITION-001 message must name the offending command, the
/// bad trigger, the target resource, AND list the declared transition names so
/// the author sees the valid options to pick from (the "many faces" fix —
/// runtime `.Apply()` would 500 on the unknown name).
#[test]
fn lifecycle_transition_001_message_lists_declared_transitions() {
    let input = feature(vec![update_command("settle", &["bogus"], &["note"])]);
    let diags = check_input(&input);
    assert_eq!(diags.len(), 1);
    let message = &diags[0].message;
    assert!(message.contains("command `settle`"), "{message}");
    assert!(message.contains('`') && message.contains("bogus"), "{message}");
    assert!(message.contains("resource `Payment`"), "{message}");
    // declared transitions on Payment: authorize, capture, refund, cancel.
    assert!(message.contains("Declared transitions:"), "{message}");
    for declared in ["authorize", "capture", "refund", "cancel"] {
        assert!(
            message.contains(&format!("`{declared}`")),
            "message should list declared transition `{declared}`: {message}"
        );
    }
}

#[test]
fn lifecycle_transition_002_resource_has_no_lifecycle() {
    let mut input = feature(vec![update_command("settle", &["capture"], &["note"])]);
    input.features[0].resources = vec![resource_without_lifecycle()];
    assert_eq!(codes(&input), vec!["LIFECYCLE-TRANSITION-002"]);
}

#[test]
fn lifecycle_transition_003_same_feature_overlap_warns() {
    let input = feature(vec![
        update_command("settle", &["authorize", "capture"], &["note"]),
        update_command("capture_now", &["capture"], &["note"]),
    ]);
    assert_eq!(codes(&input), vec!["LIFECYCLE-TRANSITION-003"]);
}

#[test]
fn lifecycle_transition_004_trigger_without_updates() {
    let input = feature(vec![returns_command("preview", &["authorize"])]);
    assert_eq!(codes(&input), vec!["LIFECYCLE-TRANSITION-004"]);
}

#[test]
fn lifecycle_transition_005_manual_lifecycle_assignment() {
    let input = feature(vec![update_command(
        "settle",
        &["authorize"],
        &["lifecycle_state"],
    )]);
    assert_eq!(codes(&input), vec!["LIFECYCLE-TRANSITION-005"]);
}

#[test]
fn lifecycle_transition_006_non_contiguous_chain() {
    let input = feature(vec![update_command(
        "settle",
        &["authorize", "refund"],
        &["note"],
    )]);
    assert_eq!(codes(&input), vec!["LIFECYCLE-TRANSITION-006"]);
}

#[test]
fn lifecycle_transition_happy_path() {
    let input = feature(vec![update_command(
        "settle",
        &["authorize", "capture"],
        &["note"],
    )]);
    assert!(check_input(&input).is_empty());
}
