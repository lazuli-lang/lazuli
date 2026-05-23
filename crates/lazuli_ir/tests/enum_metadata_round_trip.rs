use lazuli_ir::{EnumVariant, StorageValue};

#[test]
fn enum_variant_metadata_round_trips_as_optional_strings() {
    let variant = EnumVariant {
        name: "prefer_not".to_owned(),
        storage_value: Some(StorageValue::String("prefer_not".to_owned())),
        label_key: Some("gender_prefer_not".to_owned()),
        hint_key: Some("gender_prefer_not_hint".to_owned()),
        icon_key: Some("eye-off".to_owned()),
        previous_names: vec![],
    };

    let json = serde_json::to_string(&variant).expect("serialize enum variant");
    assert!(json.contains("\"label_key\":\"gender_prefer_not\""));
    assert!(json.contains("\"hint_key\":\"gender_prefer_not_hint\""));
    assert!(json.contains("\"icon_key\":\"eye-off\""));

    let back: EnumVariant = serde_json::from_str(&json).expect("deserialize enum variant");
    assert_eq!(back, variant);
}

#[test]
fn enum_variant_metadata_absent_fields_stay_omitted() {
    let variant = EnumVariant {
        name: "male".to_owned(),
        storage_value: None,
        label_key: None,
        hint_key: None,
        icon_key: None,
        previous_names: vec![],
    };

    let json = serde_json::to_string(&variant).expect("serialize enum variant");
    assert!(!json.contains("label_key"));
    assert!(!json.contains("hint_key"));
    assert!(!json.contains("icon_key"));

    let back: EnumVariant = serde_json::from_str(&json).expect("deserialize enum variant");
    assert_eq!(back, variant);
}
