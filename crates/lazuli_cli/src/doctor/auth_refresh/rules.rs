use lazuli_ir::{BuiltinType, TheftAction, TypeRef};

use super::super::aggregators::auth::resolve_resource_for_feature;
use super::{
    AuthRefreshContext, AuthRefreshDiagnostic, DEFAULT_REFRESH_TTL, DEFAULT_ROTATION_GRACE,
    DEFAULT_THEFT_ACTION, Finding,
};

const REQUIRED_COLUMNS: &[&str] = &[
    "refresh_token_hash",
    "refresh_expires_at",
    "parent_session_id",
    "theft_detected_at",
];
const THIRTY_DAYS_SECONDS: u64 = 30 * 24 * 60 * 60;

pub(super) fn check(ctx: &AuthRefreshContext<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    let markers = ProjectMarkers::from_context(ctx);
    for fact in ctx.auth_facts {
        let Some(sessions) = fact.auth.sessions.as_ref() else {
            continue;
        };
        if !sessions.is_rotation_enabled() {
            continue;
        }
        out.extend(rule_001_secret_provider(fact, &markers));
        out.extend(rule_002_grace_vs_refresh(fact));
        out.extend(rule_003_columns(ctx, fact));
        out.extend(rule_004_revoke_user_fk(ctx, fact));
        out.extend(rule_005_refresh_ttl_long(fact));
        out.extend(rule_006_refresh_failure(fact, &markers));
        out.extend(rule_007_auto_promotion(fact));
        out.extend(rule_008_auto_refresh(fact, &markers));
        out.extend(rule_009_cookie_domain(fact, &markers));
    }
    out
}

fn rule_001_secret_provider(
    fact: &super::super::AuthFacts,
    markers: &ProjectMarkers,
) -> Option<Finding> {
    (!markers.secret_provider).then(|| finding(fact, AuthRefreshDiagnostic::MissingSecretProvider, format!(
        "auth.sessions.rotation on feature `{}` requires a secret_provider capability so refresh tokens can be signed.",
        fact.feature
    )))
}

fn rule_002_grace_vs_refresh(fact: &super::super::AuthFacts) -> Option<Finding> {
    let sessions = fact.auth.sessions.as_ref()?;
    let grace = resolved_grace(sessions);
    let refresh = resolved_refresh_ttl(sessions);
    (duration_seconds(grace)? > duration_seconds(refresh)?).then(|| finding(fact, AuthRefreshDiagnostic::GraceExceedsRefreshTtl, format!(
        "auth.sessions.rotation on feature `{}` has rotation_grace `{grace}` > refresh_ttl `{refresh}`; grace must not outlive the refresh token.",
        fact.feature
    )))
}

fn rule_003_columns(
    ctx: &AuthRefreshContext<'_>,
    fact: &super::super::AuthFacts,
) -> Option<Finding> {
    let resource = session_resource(ctx, fact)?;
    let sessions = fact.auth.sessions.as_ref()?;
    let missing: Vec<&str> = REQUIRED_COLUMNS
        .iter()
        .copied()
        .filter(|name| !resource.fields.contains_key(*name))
        .collect();
    (!missing.is_empty()).then(|| finding(fact, AuthRefreshDiagnostic::MissingRotationColumns, format!(
        "session resource `{}` lacks required columns for rotation: `{}`. Run `lazuli migrate` to add them, or remove `rotation` if the rotation discipline is not desired.",
        sessions.resource.name,
        missing.join("`, `")
    )))
}

fn rule_004_revoke_user_fk(
    ctx: &AuthRefreshContext<'_>,
    fact: &super::super::AuthFacts,
) -> Option<Finding> {
    let sessions = fact.auth.sessions.as_ref()?;
    if !matches!(
        sessions.rotation.as_ref()?.theft_detection_action,
        Some(TheftAction::RevokeUser)
    ) || session_resource(ctx, fact)
        .map(has_user_fk)
        .unwrap_or(false)
    {
        return None;
    }
    Some(finding(
        fact,
        AuthRefreshDiagnostic::RevokeUserWithoutUserFk,
        format!(
            "auth.sessions.rotation on feature `{}` declares theft_detection_action revoke_user, but session resource `{}` has no user_id FK; revoke_session_family is safer unless the user relationship is modeled.",
            fact.feature, sessions.resource.name
        ),
    ))
}

fn rule_005_refresh_ttl_long(fact: &super::super::AuthFacts) -> Option<Finding> {
    let refresh = resolved_refresh_ttl(fact.auth.sessions.as_ref()?);
    (duration_seconds(refresh)? > THIRTY_DAYS_SECONDS).then(|| finding(fact, AuthRefreshDiagnostic::RefreshTtlLong, format!(
        "auth.sessions.rotation on feature `{}` has refresh_ttl `{refresh}` > 30d; long-lived refresh tokens increase replay exposure.",
        fact.feature
    )))
}

fn rule_006_refresh_failure(
    fact: &super::super::AuthFacts,
    markers: &ProjectMarkers,
) -> Option<Finding> {
    (!markers.on_refresh_failure).then(|| finding(fact, AuthRefreshDiagnostic::MissingRefreshFailureHandler, format!(
        "auth.sessions.rotation on feature `{}` has no app-level on_refresh_failure capability; clients may not know when a forced logout happens.",
        fact.feature
    )))
}

fn rule_007_auto_promotion(fact: &super::super::AuthFacts) -> Option<Finding> {
    let sessions = fact.auth.sessions.as_ref()?;
    let rotation = sessions.rotation.as_ref()?;
    (sessions.access_ttl.is_none()
        && rotation.refresh_ttl.is_none()
        && rotation.grace.is_none()
        && rotation.theft_detection_action.is_none())
        .then(|| finding(fact, AuthRefreshDiagnostic::AutoPromotionApplied, format!(
            "auth.sessions.rotation on feature `{}` was auto-promoted to defaults: refresh_ttl {DEFAULT_REFRESH_TTL}, rotation_grace {DEFAULT_ROTATION_GRACE}, theft_detection_action {DEFAULT_THEFT_ACTION}. Add any slot inside rotation to override.",
            fact.feature
        )))
}

fn rule_008_auto_refresh(
    fact: &super::super::AuthFacts,
    markers: &ProjectMarkers,
) -> Option<Finding> {
    (!markers.enable_auto_refresh).then(|| finding(fact, AuthRefreshDiagnostic::AutoRefreshNotSurfaced, format!(
        "auth.sessions.rotation on feature `{}` generated a refresh contract, but no audience client surfaces enableAutoRefresh.",
        fact.feature
    )))
}

fn rule_009_cookie_domain(
    fact: &super::super::AuthFacts,
    markers: &ProjectMarkers,
) -> Option<Finding> {
    (markers.refresh_storage_cookie && !markers.cookie_domain).then(|| finding(fact, AuthRefreshDiagnostic::CookieDomainMissing, format!(
        "auth.sessions.rotation on feature `{}` stores refresh tokens in cookies, but app cookie config has no domain; cross-subdomain refresh may break.",
        fact.feature
    )))
}

fn finding(
    fact: &super::super::AuthFacts,
    diagnostic: AuthRefreshDiagnostic,
    message: String,
) -> Finding {
    Finding {
        diagnostic,
        path: fact.path.clone(),
        line: fact.sessions_line.unwrap_or(fact.line),
        message,
    }
}

fn session_resource<'a>(
    ctx: &'a AuthRefreshContext<'_>,
    fact: &super::super::AuthFacts,
) -> Option<&'a super::super::ResourceFact> {
    let sessions = fact.auth.sessions.as_ref()?;
    resolve_resource_for_feature(
        &fact.feature,
        &sessions.resource.name,
        ctx.feature_resources,
        ctx.feature_uses,
    )
}

fn has_user_fk(resource: &super::super::ResourceFact) -> bool {
    resource.fields.iter().any(|(name, field)| {
        name == "user_id"
            || (name == "user" && matches!(&field.type_ref, TypeRef::UserDefined(_)))
            || (name == "user_id" && matches!(&field.type_ref, TypeRef::Builtin(BuiltinType::Id)))
    })
}

fn resolved_refresh_ttl(sessions: &lazuli_ir::AuthSessions) -> &str {
    sessions
        .rotation
        .as_ref()
        .and_then(|r| r.refresh_ttl.as_deref())
        .unwrap_or(DEFAULT_REFRESH_TTL)
}

fn resolved_grace(sessions: &lazuli_ir::AuthSessions) -> &str {
    sessions
        .rotation
        .as_ref()
        .and_then(|r| r.grace.as_deref())
        .unwrap_or(DEFAULT_ROTATION_GRACE)
}

fn duration_seconds(raw: &str) -> Option<u64> {
    let trimmed = raw.trim().trim_matches('"').trim();
    let digit_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let value = trimmed.get(..digit_end)?.parse::<u64>().ok()?;
    let unit = trimmed.get(digit_end..)?.trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        _ => return None,
    };
    value.checked_mul(multiplier)
}

#[derive(Default)]
struct ProjectMarkers {
    secret_provider: bool,
    on_refresh_failure: bool,
    enable_auto_refresh: bool,
    refresh_storage_cookie: bool,
    cookie_domain: bool,
}

impl ProjectMarkers {
    fn from_context(ctx: &AuthRefreshContext<'_>) -> Self {
        let mut markers = Self::from_app(ctx.app);
        for line in source_lines(ctx.files) {
            markers.secret_provider |= starts_with_token(line, "secret_provider");
            markers.on_refresh_failure |= starts_with_token(line, "on_refresh_failure");
            markers.enable_auto_refresh |= line.contains("enableAutoRefresh");
            markers.refresh_storage_cookie |= line.contains("refresh_token_storage cookie");
            markers.cookie_domain |= starts_with_token(line, "cookie_domain");
        }
        markers
    }

    fn from_app(app: Option<&super::super::DoctorAppManifest>) -> Self {
        let mut out = Self::default();
        let Some(app) = app else { return out };
        for cap in &app.manifest.capabilities {
            out.secret_provider |= cap.name == "secret_provider";
            out.on_refresh_failure |= cap.name == "on_refresh_failure";
            out.enable_auto_refresh |= cap.name == "enableAutoRefresh";
            out.refresh_storage_cookie |=
                cap.name == "refresh_token_storage" && cap.value == "cookie";
            out.cookie_domain |= cap.name == "cookie_domain";
        }
        out
    }
}

fn source_lines<'a>(files: &'a [super::super::DoctorFile]) -> impl Iterator<Item = &'a str> {
    files.iter().flat_map(|file| {
        file.source.lines().filter_map(|line| {
            let trimmed = line.trim_start();
            (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some(trimmed)
        })
    })
}

fn starts_with_token(line: &str, token: &str) -> bool {
    line.strip_prefix(token)
        .and_then(|rest| rest.chars().next())
        .map(|c| c.is_whitespace() || c == ':')
        .unwrap_or(false)
}
