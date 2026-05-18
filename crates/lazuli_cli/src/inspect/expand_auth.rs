use lazuli_ir::{Auth, Feature, Module, SpanRef};
use serde_json::{Map, Value, json};

pub fn expand_auth(module: &Module) -> Value {
    let features = module
        .features
        .iter()
        .filter_map(expand_feature_auth)
        .collect::<Vec<_>>();

    json!({ "features": features })
}

fn expand_feature_auth(feature: &Feature) -> Option<Value> {
    feature.auth.as_ref().map(|auth| {
        json!({
            "name": feature.name,
            "auth": expand_auth_block(&feature.name, auth),
        })
    })
}

fn expand_auth_block(feature_name: &str, auth: &Auth) -> Value {
    let mut block = Map::new();
    block.insert("origin".to_owned(), origin(feature_name, auth.span_ref));
    block.insert(
        "identity".to_owned(),
        with_origin(
            serde_json::to_value(&auth.identity).expect("AuthIdentity serializes"),
            feature_name,
            auth.span_ref,
        ),
    );

    if let Some(password) = auth.password.as_ref() {
        block.insert(
            "password".to_owned(),
            with_origin(
                serde_json::to_value(password).expect("AuthPassword serializes"),
                feature_name,
                auth.span_ref,
            ),
        );
    }

    if let Some(sessions) = auth.sessions.as_ref() {
        block.insert(
            "sessions".to_owned(),
            with_origin(
                serde_json::to_value(sessions).expect("AuthSessions serializes"),
                feature_name,
                auth.span_ref,
            ),
        );
    }

    if let Some(mfa) = auth.mfa.as_ref() {
        block.insert(
            "mfa".to_owned(),
            with_origin(
                serde_json::to_value(mfa).expect("AuthMfa serializes"),
                feature_name,
                auth.span_ref,
            ),
        );
    }

    if !auth.oauth.is_empty() {
        block.insert(
            "oauth".to_owned(),
            Value::Array(
                auth.oauth
                    .iter()
                    .map(|provider| {
                        with_origin(
                            serde_json::to_value(provider).expect("AuthOAuthProvider serializes"),
                            feature_name,
                            auth.span_ref,
                        )
                    })
                    .collect(),
            ),
        );
    }

    Value::Object(block)
}

fn with_origin(mut value: Value, feature_name: &str, span_ref: Option<SpanRef>) -> Value {
    if let Value::Object(object) = &mut value {
        object.insert("origin".to_owned(), origin(feature_name, span_ref));
    }
    value
}

pub(crate) fn origin(feature_name: &str, span_ref: Option<SpanRef>) -> Value {
    let mut origin = Map::new();
    origin.insert("feature".to_owned(), Value::String(feature_name.to_owned()));
    if let Some(span_ref) = span_ref {
        origin.insert("line".to_owned(), json!(span_ref.start));
    }
    Value::Object(origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AuthIdentity, AuthMfa, AuthOAuthProvider, AuthPassword, AuthSessions, Defaults, FieldRef,
        Policies, QualifiedName,
    };

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    fn feature(name: &str, auth: Option<Auth>) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            auth,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn auth(span_ref: Option<SpanRef>) -> Auth {
        Auth {
            identity: AuthIdentity {
                field: FieldRef {
                    resource: QualifiedName {
                        feature: None,
                        name: "Customer".to_owned(),
                    },
                    field: "email".to_owned(),
                },
                public_contract: None,
            },
            password: Some(AuthPassword {
                algorithm: "argon2id".to_owned(),
                hash: "@fn.hash_customer_password".to_owned(),
                verify: "@fn.verify_customer_password".to_owned(),
                rate_limit: Some("5 per 10 minutes".to_owned()),
            }),
            sessions: Some(AuthSessions {
                resource: QualifiedName {
                    feature: None,
                    name: "CustomerSession".to_owned(),
                },
                ttl: "7 days".to_owned(),
                refresh: false,
                extra_columns: vec![],
            }),
            mfa: Some(AuthMfa {
                method: "totp".to_owned(),
                enroll: "@fn.enroll_customer_totp".to_owned(),
                verify: "@validator.verify_customer_totp".to_owned(),
                adapter: None,
            }),
            oauth: vec![AuthOAuthProvider {
                provider: "google".to_owned(),
                adapter: "@adapter.google_oauth".to_owned(),
            }],
            span_ref,
        }
    }

    #[test]
    fn expand_auth_emits_full_auth_block() {
        let output = expand_auth(&module_with_features(vec![feature(
            "customer_auth",
            Some(auth(Some(SpanRef {
                start: 497,
                end: 545,
            }))),
        )]));

        let auth = &output["features"][0]["auth"];
        assert_eq!(output["features"][0]["name"], "customer_auth");
        assert_eq!(
            auth["origin"],
            json!({"feature": "customer_auth", "line": 497})
        );
        assert_eq!(auth["identity"]["field"]["resource"]["name"], "Customer");
        assert_eq!(auth["identity"]["field"]["field"], "email");
        assert_eq!(auth["identity"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["password"]["algorithm"], "argon2id");
        assert_eq!(auth["password"]["rate_limit"], "5 per 10 minutes");
        assert_eq!(auth["sessions"]["resource"]["name"], "CustomerSession");
        assert_eq!(auth["sessions"]["refresh"], false);
        assert_eq!(auth["mfa"]["method"], "totp");
        assert_eq!(auth["oauth"][0]["provider"], "google");
        assert_eq!(auth["oauth"][0]["origin"]["line"], 497);
    }

    #[test]
    fn expand_auth_emits_partial_password_only_block() {
        let mut auth = auth(None);
        auth.sessions = None;
        auth.mfa = None;
        auth.oauth = vec![];
        auth.password.as_mut().unwrap().rate_limit = None;

        let output = expand_auth(&module_with_features(vec![feature(
            "customer_auth",
            Some(auth),
        )]));
        let auth = &output["features"][0]["auth"];

        assert_eq!(auth["origin"], json!({"feature": "customer_auth"}));
        assert_eq!(auth["password"]["algorithm"], "argon2id");
        assert!(auth.get("sessions").is_none());
        assert!(auth.get("mfa").is_none());
        assert!(auth.get("oauth").is_none());
    }

    #[test]
    fn expand_auth_skips_empty_module_and_features_without_auth() {
        assert_eq!(
            expand_auth(&module_with_features(vec![])),
            json!({ "features": [] })
        );

        assert_eq!(
            expand_auth(&module_with_features(vec![feature("catalog", None)])),
            json!({ "features": [] })
        );
    }
}
