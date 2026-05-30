/// Migrations bucket cycle Route C — lower a canonical-indent
/// `tenant_migration` block into `ir::TenantMigration`. Mirrors
/// `lower_job` for the shared spine (idempotency / retry / timeout /
/// handler) and adds the `target tenants <axis>` slot. The lowering
/// does **not** enforce that `idempotency` is authored; that is
/// `TM-IDEMP-001`'s job downstream.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::events::lower_tenant_migration;
/// use lazuli_syntax::TenantMigration;
///
/// let tm: TenantMigration = unimplemented!("from canonical-indent parse");
/// let lowered = lower_tenant_migration(&tm)?;
/// assert!(!lowered.name.is_empty());
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn lower_tenant_migration(
    tm: &syntax::TenantMigration,
) -> Result<ir::TenantMigration, AnalyzeError> {
    let idempotency = tm
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path })
        .unwrap_or_else(|| ir::IdempotencyKey {
            by: ir::Path::from_segments(Vec::<String>::new()),
        });
    let retry = tm.retry.as_ref().map(lower_retry);
    Ok(ir::TenantMigration {
        name: tm.name.clone(),
        target: ir::TenantMigrationTarget {
            operation: tm.target_ref.as_deref().map(lower_tenant_migration_target),
            axis: tm.target_axis.clone(),
        },
        idempotency,
        retry,
        timeout: tm.timeout.clone(),
        handler: ir::PathRef::authored(&tm.handler),
        previous_names: Vec::new(),
        span_ref: Some(span_of(tm.span)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mcp_auth_lifts_bearer_env_form() {
        let auth = parse_mcp_auth("bearer env.MCP_TOKEN").expect("bearer env. form parses");
        // `MCPAuth::BearerEnvVar` is the sole variant today — exhaustive match.
        let ir::MCPAuth::BearerEnvVar { env } = auth;
        assert_eq!(env, "MCP_TOKEN");
    }

    #[test]
    fn parse_mcp_auth_rejects_malformed_shape() {
        assert!(parse_mcp_auth("oauth client_id=x").is_none());
        assert!(parse_mcp_auth("bearer env.").is_none());
    }
}
