//! FeatureErrors aggregate round-trip + default constructor checks.

use lazuli_ir::{
    ErrorExposureDefault, FeatureErrorMessage, FeatureErrors, FeatureFieldError, SpanRef,
};

use super::key_ref;

#[test]
fn feature_errors_round_trips_with_all_subshapes() {
    let errors = FeatureErrors {
        default: Some(ErrorExposureDefault::Hide),
        exposure_4xx: vec![
            "message".to_owned(),
            "code".to_owned(),
            "message_key".to_owned(),
        ],
        exposure_5xx: vec!["code".to_owned()],
        messages: vec![
            FeatureErrorMessage {
                code: "policy_denied".to_owned(),
                message: key_ref("account_signin_required", 100),
                span_ref: Some(SpanRef {
                    start: 100,
                    end: 132,
                }),
            },
            FeatureErrorMessage {
                code: "validation_failed".to_owned(),
                message: key_ref("account_invalid_input", 140),
                span_ref: None,
            },
        ],
        field_messages: vec![FeatureFieldError {
            resource: "Customer".to_owned(),
            field: "email".to_owned(),
            code: "format_invalid".to_owned(),
            message: key_ref("customer_email_format", 200),
            span_ref: Some(SpanRef {
                start: 200,
                end: 240,
            }),
        }],
        audience_exposure: Vec::new(),
        redact_patterns: Vec::new(),
        span_ref: Some(SpanRef {
            start: 80,
            end: 260,
        }),
    };
    let json = serde_json::to_string(&errors).expect("serialize FeatureErrors");
    let back: FeatureErrors = serde_json::from_str(&json).expect("deserialize FeatureErrors");
    assert_eq!(errors, back);
}

#[test]
fn feature_errors_default_constructor_is_empty() {
    let empty = FeatureErrors::default();
    assert!(empty.default.is_none());
    assert!(empty.exposure_4xx.is_empty());
    assert!(empty.exposure_5xx.is_empty());
    assert!(empty.messages.is_empty());
    assert!(empty.field_messages.is_empty());
    assert!(empty.span_ref.is_none());
}
