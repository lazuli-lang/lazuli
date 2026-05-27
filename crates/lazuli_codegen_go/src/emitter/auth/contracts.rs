//! Auth runtime-contract emitters.
//!
//! Each function emits one `auth.*Contract{...}` Go literal. These are the
//! per-block walkers consumed by the orchestrator in `mod.rs`.

use lazuli_ir::{Auth, AuthMfa, AuthOAuthProvider, AuthPassword, AuthSessions};

use super::super::printer::GoPrinter;
use super::format::{
    duration_expr, duration_expr_for, escape_string, mfa_method_expr, pascal_case,
    password_algorithm_expr, qualified_resource_name, theft_action_expr, write_aligned_kv_rows,
};

pub(super) fn emit_identity(p: &mut GoPrinter, identity_var: &str, auth_block: &Auth) {
    let field = &auth_block.identity.field;
    p.line(&format!(
        "var {identity_var} = auth.FieldRef{{Resource: \"{}\", Field: \"{}\"}}",
        escape_string(&qualified_resource_name(&field.resource)),
        escape_string(&field.field),
    ));
}

pub(super) fn emit_password(
    p: &mut GoPrinter,
    feature_pascal: &str,
    identity_var: &str,
    password: &AuthPassword,
) {
    p.line(&format!(
        "var {feature_pascal}AuthPassword = auth.PasswordContract{{"
    ));
    p.indent();
    let mut rows = vec![
        ("Identity:".to_owned(), format!("{identity_var},")),
        (
            "Algorithm:".to_owned(),
            format!("{},", password_algorithm_expr(&password.algorithm)),
        ),
        (
            "HashFn:".to_owned(),
            format!("\"{}\",", escape_string(&password.hash)),
        ),
        (
            "VerifyFn:".to_owned(),
            format!("\"{}\",", escape_string(&password.verify)),
        ),
    ];
    if let Some(rate_limit) = &password.rate_limit {
        // Auth subpackage's PasswordContract.RateLimit remains a plain
        // string (its consumer is `runtime/go/lazuli/auth.PasswordSpec`,
        // not the env-aware `lazuli.RateLimit` struct). Per
        // `ir-rate-limit-env-aware` Cell 2 §scope, only Command / Api /
        // Agent / Report / Query thread the struct shape; auth's
        // PasswordContract has its own contract.
        rows.push((
            "RateLimit:".to_owned(),
            format!("\"{}\",", escape_string(&rate_limit.default)),
        ));
    }
    write_aligned_kv_rows(p, &rows);
    p.dedent();
    p.line("}");
}

pub(super) fn emit_sessions(p: &mut GoPrinter, feature_pascal: &str, sessions: &AuthSessions) {
    let (ttl_expr, ttl_todo) = duration_expr(&sessions.ttl);
    p.line(&format!(
        "var {feature_pascal}AuthSessions = auth.SessionsContract{{"
    ));
    p.indent();
    let mut rows = vec![
        (
            "Resource:".to_owned(),
            format!(
                "\"{}\",",
                escape_string(&qualified_resource_name(&sessions.resource))
            ),
        ),
        ("TTL:".to_owned(), format!("{ttl_expr},")),
    ];
    let mut todos = Vec::new();
    if let Some(todo) = ttl_todo {
        todos.push(todo);
    }
    if sessions.is_rotation_enabled() {
        let (access_ttl, access_ttl_todo) =
            duration_expr_for(sessions.resolved_access_ttl(), "AuthSessions.access_ttl");
        // Invariant: `is_rotation_enabled()` returned true above, so these
        // resolvers return Some. If a future regression breaks that, fall
        // back to the empty literal — `duration_expr_for` emits a TODO row.
        let (refresh_ttl, refresh_ttl_todo) = duration_expr_for(
            sessions.resolved_refresh_ttl().unwrap_or(""),
            "AuthSessions.rotation.refresh_ttl",
        );
        let (rotation_grace, rotation_grace_todo) = duration_expr_for(
            sessions.resolved_rotation_grace().unwrap_or(""),
            "AuthSessions.rotation.grace",
        );
        rows.push(("AccessTTL:".to_owned(), format!("{access_ttl},")));
        rows.push(("RefreshTTL:".to_owned(), format!("{refresh_ttl},")));
        rows.push(("Rotation:".to_owned(), "true,".to_owned()));
        rows.push(("RotationGrace:".to_owned(), format!("{rotation_grace},")));
        rows.push((
            "TheftDetectionAction:".to_owned(),
            format!(
                "{},",
                theft_action_expr(sessions.resolved_theft_action().unwrap_or_default()),
            ),
        ));
        rows.push(("Refresh:".to_owned(), "true,".to_owned()));
        todos.extend(
            [access_ttl_todo, refresh_ttl_todo, rotation_grace_todo]
                .into_iter()
                .flatten(),
        );
    } else {
        rows.push(("Refresh:".to_owned(), format!("{},", sessions.refresh)));
    }
    write_aligned_kv_rows(p, &rows);
    for todo in todos {
        p.line(&todo);
    }
    p.dedent();
    p.line("}");
}

pub(super) fn emit_oauth(p: &mut GoPrinter, feature_pascal: &str, oauth: &AuthOAuthProvider) {
    p.line(&format!(
        "var {feature_pascal}AuthOAuth{} = auth.OAuthContract{{",
        pascal_case(&oauth.provider)
    ));
    p.indent();
    write_aligned_kv_rows(
        p,
        &[
            (
                "Provider:".to_owned(),
                format!("\"{}\",", escape_string(&oauth.provider)),
            ),
            (
                "AdapterRef:".to_owned(),
                format!("\"{}\",", escape_string(&oauth.adapter)),
            ),
        ],
    );
    p.dedent();
    p.line("}");
}

pub(super) fn emit_mfa(p: &mut GoPrinter, feature_pascal: &str, mfa: &AuthMfa) {
    p.line(&format!("var {feature_pascal}AuthMfa = auth.MfaContract{{"));
    p.indent();
    let mut rows = vec![
        (
            "Method:".to_owned(),
            format!("{},", mfa_method_expr(&mfa.method)),
        ),
        (
            "EnrollFn:".to_owned(),
            format!("\"{}\",", escape_string(&mfa.enroll)),
        ),
        (
            "VerifyFn:".to_owned(),
            format!("\"{}\",", escape_string(&mfa.verify)),
        ),
    ];
    if let Some(adapter) = &mfa.adapter {
        rows.push((
            "AdapterRef:".to_owned(),
            format!("\"{}\",", escape_string(adapter)),
        ));
    }
    write_aligned_kv_rows(p, &rows);
    p.dedent();
    p.line("}");
}
