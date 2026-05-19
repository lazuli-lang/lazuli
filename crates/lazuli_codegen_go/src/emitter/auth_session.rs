//! Cell S3 — per-feature session shim emitter. Emits
//! `<feature>/session.gen.go` only when the `auth sessions` block
//! carries `extra_columns` (multi-tenant). Single-tenant features
//! (empty `extra_columns`) return `None` so the existing
//! `auth.IssueSession` call path continues to work unchanged.
//!
//! Proposal: `docs/proposals/auth-session-tenant-pin.md` §S3.
//! Wired into the feature walker by the orchestrator post-merge
//! (Cell S3 block in `module.rs`, alphabetical entry in `mod.rs`).

use lazuli_ir::{AuthSessions, Feature, SessionExtraColumn};

use super::casing::{lower_camel, pascal_case};
use super::imports::ImportSet;
use super::printer::GoPrinter;

/// Emit `<feature>/session.gen.go`, or `None` when:
/// - the feature has no `auth` block,
/// - the `auth` block has no `sessions` declaration, or
/// - the sessions resource has no extra columns (single-tenant).
pub fn emit_auth_session_file(source_label: &str, feature: &Feature) -> Option<String> {
    let sessions = feature.auth.as_ref()?.sessions.as_ref()?;
    if sessions.extra_columns.is_empty() {
        return None;
    }

    let resource = pascal_case(&sessions.resource.name);
    let table = to_snake(&sessions.resource.name);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("time");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/auth");

    p.banner(source_label, &super::casing::gen_package_name(&feature.name));
    imports.emit(&mut p);
    p.blank();

    emit_input_struct(&mut p, &resource, &sessions.extra_columns);
    p.blank();
    emit_issue_fn(&mut p, &resource, &table, sessions);
    p.blank();
    emit_resolve_fn(&mut p, &resource, &table, sessions);
    p.blank();
    emit_invalidate_fn(&mut p, &resource, &table);
    p.blank();
    emit_contract_var(&mut p, &resource, sessions);

    Some(p.finish())
}

fn emit_input_struct(p: &mut GoPrinter, resource: &str, extra: &[SessionExtraColumn]) {
    p.line(&format!("type Issue{resource}Input struct {{"));
    p.indent();
    // UserID is the authenticated identity; extra columns are the tenant axes.
    let mut rows: Vec<(String, String)> = vec![("UserID".to_owned(), "lazuli.ID".to_owned())];
    for col in extra {
        rows.push((pascal_case(&col.column_name), col.go_type.clone()));
    }
    let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (field, typ) in &rows {
        let pad = " ".repeat(key_width.saturating_sub(field.len()));
        p.line(&format!("{field}{pad} {typ}"));
    }
    p.dedent();
    p.line("}");
}

fn emit_issue_fn(p: &mut GoPrinter, resource: &str, table: &str, sessions: &AuthSessions) {
    let contract_var = contract_var_name(resource);
    let extra = &sessions.extra_columns;

    // Column order: extra_columns then user_id, token_hash, expires_at.
    let col_names: Vec<&str> = extra
        .iter()
        .map(|c| c.column_name.as_str())
        .chain(["user_id", "token_hash", "expires_at"])
        .collect();
    let placeholders: Vec<String> = (1..=col_names.len()).map(|i| format!("${i}")).collect();
    let sql = format!(
        "INSERT INTO \"{table}\" ({cols}) VALUES ({vals})",
        cols = col_names.join(", "),
        vals = placeholders.join(", "),
    );

    // Exec args: extra_col fields from input, then UserID, tokenHash, expiresAt.
    let mut args: Vec<String> = extra
        .iter()
        .map(|c| format!("input.{}", pascal_case(&c.column_name)))
        .collect();
    args.push("input.UserID".to_owned());
    args.push("tokenHash".to_owned());
    args.push("expiresAt".to_owned());

    p.line(&format!(
        "func Issue{resource}(ctx *lazuli.Ctx, input Issue{resource}Input) (string, time.Time, error) {{"
    ));
    p.indent();
    p.line(&format!(
        "token, tokenHash, expiresAt, err := auth.MintSessionToken(ctx, {contract_var}.TTL)"
    ));
    p.line("if err != nil {");
    p.indent();
    p.line("return \"\", time.Time{}, err");
    p.dedent();
    p.line("}");
    p.line(&format!("const sql = `{sql}`"));
    p.line(&format!(
        "if _, err := auth.SessionDB(ctx).Exec(ctx, sql, {}); err != nil {{",
        args.join(", ")
    ));
    p.indent();
    p.line("return \"\", time.Time{}, err");
    p.dedent();
    p.line("}");
    p.line("return token, expiresAt, nil");
    p.dedent();
    p.line("}");
}

fn emit_resolve_fn(p: &mut GoPrinter, resource: &str, table: &str, sessions: &AuthSessions) {
    let extra = &sessions.extra_columns;

    // Return type: (lazuli.ID, lazuli.ID × N, error) where N = extra_cols.
    let id_returns: Vec<&str> = std::iter::once("lazuli.ID")
        .chain(extra.iter().map(|_| "lazuli.ID"))
        .collect();
    let return_type = id_returns.join(", ");

    // Zero values for error-path returns.
    let zeros: Vec<&str> = id_returns.iter().map(|_| "0").collect();
    let zero_prefix = zeros.join(", ");

    // SELECT: user_id, extra_col..., expires_at.
    let select_cols: Vec<&str> = std::iter::once("user_id")
        .chain(extra.iter().map(|c| c.column_name.as_str()))
        .chain(std::iter::once("expires_at"))
        .collect();
    let sql = format!(
        "SELECT {cols} FROM \"{table}\" WHERE token_hash = $1 LIMIT 1",
        cols = select_cols.join(", ")
    );

    // Extra-column variable names for Scan and return.
    let extra_var_names: Vec<String> = extra
        .iter()
        .map(|c| lower_camel(&c.column_name))
        .collect();
    let scan_args: Vec<String> = std::iter::once("&userID".to_owned())
        .chain(extra_var_names.iter().map(|v| format!("&{v}")))
        .chain(std::iter::once("&expiresAt".to_owned()))
        .collect();
    let ret_vals: Vec<String> = std::iter::once("userID".to_owned())
        .chain(extra_var_names.iter().cloned())
        .chain(std::iter::once("nil".to_owned()))
        .collect();

    p.line(&format!(
        "func Resolve{resource}(ctx *lazuli.Ctx, token string) ({return_type}, error) {{"
    ));
    p.indent();
    p.line("tokenHash, err := auth.HashSessionToken(token)");
    p.line("if err != nil {");
    p.indent();
    p.line(&format!("return {zero_prefix}, err"));
    p.dedent();
    p.line("}");
    p.line(&format!("const sql = `{sql}`"));
    p.line("var userID lazuli.ID");
    for v in &extra_var_names {
        p.line(&format!("var {v} lazuli.ID"));
    }
    p.line("var expiresAt time.Time");
    p.line(&format!(
        "if err := auth.SessionDB(ctx).QueryRow(ctx, sql, tokenHash).Scan({}); err != nil {{",
        scan_args.join(", ")
    ));
    p.indent();
    p.line(&format!("return {zero_prefix}, auth.MapSessionResolveError(err)"));
    p.dedent();
    p.line("}");
    p.line("if !expiresAt.After(time.Now()) {");
    p.indent();
    p.line(&format!("return {zero_prefix}, auth.ErrSessionExpired"));
    p.dedent();
    p.line("}");
    p.line("ctx.User = &lazuli.User{ID: userID}");
    if let Some(first) = extra.first() {
        let v = lower_camel(&first.column_name);
        p.line(&format!("ctx.Tenant = &lazuli.Tenant{{OrgID: {v}}}"));
    }
    p.line(&format!("return {}", ret_vals.join(", ")));
    p.dedent();
    p.line("}");
}

fn emit_invalidate_fn(p: &mut GoPrinter, resource: &str, table: &str) {
    let sql = format!("DELETE FROM \"{table}\" WHERE token_hash = $1");
    p.line(&format!(
        "func Invalidate{resource}(ctx *lazuli.Ctx, token string) error {{"
    ));
    p.indent();
    p.line("tokenHash, err := auth.HashSessionToken(token)");
    p.line("if err != nil {");
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line(&format!("const sql = `{sql}`"));
    p.line("_, err = auth.SessionDB(ctx).Exec(ctx, sql, tokenHash)");
    p.line("return err");
    p.dedent();
    p.line("}");
}

fn emit_contract_var(p: &mut GoPrinter, resource: &str, sessions: &AuthSessions) {
    let var_name = contract_var_name(resource);
    p.line(&format!("var {var_name} = auth.SessionsContract{{"));
    p.indent();
    p.line(&format!("Resource: \"{}\",", sessions.resource.name));
    p.line(&format!("TTL:      \"{}\",", sessions.ttl));
    p.line(&format!("Refresh:  {},", sessions.refresh));
    p.dedent();
    p.line("}");
}

/// `"UserSession"` → `"userSessionContract"`
fn contract_var_name(resource: &str) -> String {
    format!("{}Contract", lower_camel(resource))
}

/// `"UserSession"` → `"user_session"` (for SQL table names).
fn to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower = false;
        } else {
            out.push(ch);
            prev_lower = ch.is_ascii_lowercase();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Auth, AuthIdentity, AuthSessions, Defaults, FieldRef, Policies, QualifiedName, SessionExtraColumn};

    fn base_feature(name: &str) -> lazuli_ir::Feature {
        lazuli_ir::Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults { tenancy: None, timestamps: false, policy: None },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies { categories: Vec::new(), fields: Vec::new(), span_ref: None },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn qname(name: &str) -> QualifiedName {
        QualifiedName { feature: None, name: name.to_owned() }
    }

    fn minimal_auth(sessions: Option<AuthSessions>) -> Auth {
        Auth {
            identity: AuthIdentity {
                field: FieldRef {
                    resource: qname("User"),
                    field: "email".to_owned(),
                },
            },
            password: None,
            sessions,
            mfa: None,
            oauth: Vec::new(),
            span_ref: None,
        }
    }

    fn org_extra_column() -> SessionExtraColumn {
        SessionExtraColumn {
            field_name: "org".to_owned(),
            column_name: "org_id".to_owned(),
            go_type: "lazuli.ID".to_owned(),
            references: Some("Org".to_owned()),
            required: true,
        }
    }

    #[test]
    fn no_auth_returns_none() {
        let feature = base_feature("account");
        assert!(emit_auth_session_file("examples/x.lzi", &feature).is_none());
    }

    #[test]
    fn single_tenant_empty_extra_columns_returns_none() {
        let mut feature = base_feature("account");
        feature.auth = Some(minimal_auth(Some(AuthSessions {
            resource: qname("UserSession"),
            ttl: "7 days".to_owned(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: None,
            rotation: None,
        })));
        assert!(emit_auth_session_file("examples/x.lzi", &feature).is_none());
    }

    #[test]
    fn multi_tenant_org_axis_emits_session_shim() {
        let mut feature = base_feature("account");
        feature.auth = Some(minimal_auth(Some(AuthSessions {
            resource: qname("UserSession"),
            ttl: "7 days".to_owned(),
            refresh: false,
            extra_columns: vec![org_extra_column()],
            access_ttl: None,
            rotation: None,
        })));

        let out = emit_auth_session_file("features/account/account.lzi", &feature)
            .expect("must emit for multi-tenant");

        // Banner and package.
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package accountgen"));

        // Imports.
        assert!(out.contains("\"time\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/auth\""));

        // Input struct.
        assert!(out.contains("type IssueUserSessionInput struct {"));
        assert!(out.contains("UserID lazuli.ID"));
        assert!(out.contains("OrgID  lazuli.ID"));

        // IssueUserSession signature and SQL.
        assert!(out.contains(
            "func IssueUserSession(ctx *lazuli.Ctx, input IssueUserSessionInput) (string, time.Time, error) {"
        ));
        assert!(out.contains("auth.MintSessionToken(ctx, userSessionContract.TTL)"));
        assert!(out.contains(
            "INSERT INTO \"user_session\" (org_id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)"
        ));
        assert!(out.contains("auth.SessionDB(ctx).Exec(ctx, sql, input.OrgID, input.UserID, tokenHash, expiresAt)"));

        // ResolveUserSession signature and SQL.
        assert!(out.contains(
            "func ResolveUserSession(ctx *lazuli.Ctx, token string) (lazuli.ID, lazuli.ID, error) {"
        ));
        assert!(out.contains(
            "SELECT user_id, org_id, expires_at FROM \"user_session\" WHERE token_hash = $1 LIMIT 1"
        ));
        assert!(out.contains("auth.SessionDB(ctx).QueryRow(ctx, sql, tokenHash).Scan(&userID, &orgID, &expiresAt)"));
        assert!(out.contains("ctx.User = &lazuli.User{ID: userID}"));
        assert!(out.contains("ctx.Tenant = &lazuli.Tenant{OrgID: orgID}"));
        assert!(out.contains("return userID, orgID, nil"));

        // InvalidateUserSession.
        assert!(out.contains(
            "func InvalidateUserSession(ctx *lazuli.Ctx, token string) error {"
        ));
        assert!(out.contains("DELETE FROM \"user_session\" WHERE token_hash = $1"));
        assert!(out.contains("auth.SessionDB(ctx).Exec(ctx, sql, tokenHash)"));

        // Contract var.
        assert!(out.contains("var userSessionContract = auth.SessionsContract{"));
        assert!(out.contains("Resource: \"UserSession\","));
        assert!(out.contains("TTL:      \"7 days\","));
        assert!(out.contains("Refresh:  false,"));
    }

    #[test]
    fn resource_name_flows_into_function_names_and_sql() {
        let mut feature = base_feature("billing");
        feature.auth = Some(minimal_auth(Some(AuthSessions {
            resource: qname("CustomerSession"),
            ttl: "1 hour".to_owned(),
            refresh: true,
            extra_columns: vec![org_extra_column()],
            access_ttl: None,
            rotation: None,
        })));

        let out = emit_auth_session_file("features/billing/billing.lzi", &feature)
            .expect("must emit");

        assert!(out.contains("type IssueCustomerSessionInput struct {"));
        assert!(out.contains("func IssueCustomerSession("));
        assert!(out.contains("func ResolveCustomerSession("));
        assert!(out.contains("func InvalidateCustomerSession("));
        assert!(out.contains("\"customer_session\""));
        assert!(out.contains("var customerSessionContract = auth.SessionsContract{"));
        assert!(out.contains("Resource: \"CustomerSession\","));
        assert!(out.contains("TTL:      \"1 hour\","));
        assert!(out.contains("Refresh:  true,"));
    }
}
