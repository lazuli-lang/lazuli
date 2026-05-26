//! TranslationKeyRef + ErrorExposureDefault round-trips.

use lazuli_ir::{ErrorExposureDefault, TranslationKeyRef};

use super::key_ref;

#[test]
fn translation_key_ref_round_trips_through_json() {
    let key = key_ref("must_be_signed_in", 0);
    let json = serde_json::to_string(&key).expect("serialize TranslationKeyRef");
    let back: TranslationKeyRef =
        serde_json::from_str(&json).expect("deserialize TranslationKeyRef");
    assert_eq!(key, back);
    assert!(
        json.contains("\"key\":\"must_be_signed_in\""),
        "key field must serialize verbatim: {json}"
    );
}

#[test]
fn translation_key_ref_omits_span_when_none() {
    let key = TranslationKeyRef {
        key: "compact".to_owned(),
        span_ref: None,
    };
    let json = serde_json::to_string(&key).expect("serialize compact TranslationKeyRef");
    assert!(
        !json.contains("span_ref"),
        "skip_serializing_if = Option::is_none should drop span_ref: {json}"
    );
}

#[test]
fn error_exposure_default_round_trips_snake_case() {
    let hide_json = serde_json::to_string(&ErrorExposureDefault::Hide).expect("serialize");
    assert_eq!(hide_json, "\"hide\"");
    let expose_json = serde_json::to_string(&ErrorExposureDefault::Expose).expect("serialize");
    assert_eq!(expose_json, "\"expose\"");

    let back: ErrorExposureDefault = serde_json::from_str(&hide_json).expect("deserialize hide");
    assert_eq!(back, ErrorExposureDefault::Hide);
}
