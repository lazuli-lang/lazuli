//! Path-object emission — one operation per IR primitive (`command`,
//! `api`, `agent expose http`, `webhook`). Each emitter writes one path
//! entry under `paths:` and recursively renders its parameters, request
//! body, responses, and `x-lazuli-*` extensions.
//!
//! Helpers for path/method derivation, policy rendering, and
//! deprecation replacement live here because they're shared across the
//! four operation flavours.

use lazuli_ir as ir;

use crate::extensions::{emit_approval, emit_dlq, emit_replay, emit_retry, emit_verify};
use crate::schemas::{emit_command_input_schema, emit_schema_inline};
use crate::yaml::{YamlEmitter, quote_key, quote_value};

pub(crate) fn emit_command(out: &mut YamlEmitter, feature: &str, cmd: &ir::Command) {
    let path = command_path(feature, cmd);
    let method = command_method(cmd);
    out.line(&format!("{}:", quote_key(&path)));
    out.indent();
    out.line(&format!("{}:", method));
    out.indent();
    out.kv("operationId", &format!("{}_{}", feature, cmd.name));
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "command");
    if let Some(rl) = &cmd.rate_limit {
        // `ir-rate-limit-env-aware` cell 1 — openapi shim: emit only the
        // default literal. Cell 3 (inspect) extends the projection to
        // surface the env-qualified shape.
        out.kv_quoted("x-lazuli-rate-limit", &rl.default);
    }
    if !cmd.emits.is_empty() {
        out.kv(
            "x-lazuli-emits",
            &format!(
                "[{}]",
                cmd.emits.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    if let Some(audit) = &cmd.audit {
        out.kv(
            "x-lazuli-audit",
            &format!(
                "[{}]",
                audit
                    .subjects
                    .iter()
                    .map(|s| quote_value(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        if let Some(emit_to) = &audit.emit_to {
            out.kv_quoted("x-lazuli-audit-emit-to", emit_to);
        }
    }
    if let Some(approval) = &cmd.approval {
        emit_approval(out, approval);
    }
    if let Some(dep) = &cmd.deprecated {
        out.line("deprecated: true");
        if let Some(since) = &dep.since {
            out.kv_quoted("x-lazuli-deprecated-since", since);
        }
        if let Some(rep) = &dep.replacement {
            out.kv_quoted(
                "x-lazuli-deprecated-replacement",
                &render_deprecation_replacement(feature, rep),
            );
        }
        if let Some(sunset) = &dep.sunset {
            out.kv_quoted("x-lazuli-deprecated-sunset", sunset);
        }
    }
    let policy_str = render_policy(&cmd.policy);
    if !policy_str.is_empty() {
        out.kv_quoted("x-lazuli-policy", &policy_str);
    }
    // Path parameters
    if !cmd.route.is_empty() {
        out.line("parameters:");
        out.indent();
        for slot in &cmd.route {
            out.line(&format!("- name: {}", slot.name));
            out.indent();
            out.line("in: path");
            out.line("required: true");
            out.line("schema:");
            out.indent();
            emit_schema_inline(out, &slot.type_ref);
            out.dedent();
            out.dedent();
        }
        out.dedent();
    }
    // Request body
    match &cmd.input {
        ir::CommandInput::Empty => {}
        ir::CommandInput::Short(_) | ir::CommandInput::Typed(_) => {
            out.line("requestBody:");
            out.indent();
            out.line("required: true");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            emit_command_input_schema(out, cmd);
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
    }
    // Responses
    out.line("responses:");
    out.indent();
    match &cmd.effect {
        ir::CommandEffect::Creates(e) => {
            out.line("'201':");
            out.indent();
            out.line("description: created");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            out.line(&format!("$ref: '#/components/schemas/{}'", e.resource.name));
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
        ir::CommandEffect::Updates(e) => {
            out.line("'200':");
            out.indent();
            out.line("description: updated");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            out.line(&format!("$ref: '#/components/schemas/{}'", e.resource.name));
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
        ir::CommandEffect::Deletes(_) => {
            out.line("'204':");
            out.indent();
            out.line("description: deleted");
            out.dedent();
        }
        ir::CommandEffect::Returns(e) => {
            out.line("'200':");
            out.indent();
            out.line("description: ok");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            emit_schema_inline(out, &e.return_type);
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
        ir::CommandEffect::None => {
            out.line("'200':");
            out.indent();
            out.line("description: ok");
            out.dedent();
        }
    }
    out.line("'4XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.line("'5XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

pub(crate) fn emit_api(out: &mut YamlEmitter, feature: &str, api: &ir::Api, strict: bool) {
    if strict {
        return;
    }
    out.line(&format!("{}:", quote_key(&api.path)));
    out.indent();
    out.line(&format!("{}:", http_method_name(api.method)));
    out.indent();
    out.kv("operationId", &format!("{}_{}", feature, api.name));
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "api");
    if let Some(rl) = &api.rate_limit {
        // `ir-rate-limit-env-aware` cell 1 — openapi shim: emit only the
        // default literal. Cell 3 (inspect) extends the projection to
        // surface the env-qualified shape.
        out.kv_quoted("x-lazuli-rate-limit", &rl.default);
    }
    if let Some(dep) = &api.deprecated {
        out.line("deprecated: true");
        if let Some(since) = &dep.since {
            out.kv_quoted("x-lazuli-deprecated-since", since);
        }
        if let Some(rep) = &dep.replacement {
            out.kv_quoted(
                "x-lazuli-deprecated-replacement",
                &render_deprecation_replacement(feature, rep),
            );
        }
        if let Some(sunset) = &dep.sunset {
            out.kv_quoted("x-lazuli-deprecated-sunset", sunset);
        }
    }
    let policy_str = render_policy(&api.policy);
    if !policy_str.is_empty() {
        out.kv_quoted("x-lazuli-policy", &policy_str);
    }
    out.line("responses:");
    out.indent();
    out.line("'200':");
    out.indent();
    out.line("description: ok");
    out.line("content:");
    out.indent();
    out.line("application/json:");
    out.indent();
    out.line("schema:");
    out.indent();
    emit_schema_inline(out, &api.output);
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.line("'4XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

pub(crate) fn emit_agent_expose(
    out: &mut YamlEmitter,
    feature: &str,
    agent: &str,
    expose: &ir::HttpExposure,
) {
    out.line(&format!("{}:", quote_key(&expose.path)));
    out.indent();
    out.line(&format!("{}:", http_method_name(expose.method)));
    out.indent();
    out.kv("operationId", &format!("{}_{}", feature, agent));
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "agent");
    out.line("responses:");
    out.indent();
    out.line("'200':");
    out.indent();
    out.line("description: agent response");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

pub(crate) fn emit_webhook(out: &mut YamlEmitter, feature: &str, webhook: &ir::Webhook) {
    out.line(&format!("{}:", quote_key(&webhook.route)));
    out.indent();
    out.line("post:");
    out.indent();
    out.kv(
        "operationId",
        &format!("{}_{}_webhook", feature, webhook.name),
    );
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "webhook");
    out.kv_quoted("x-lazuli-webhook", &webhook.name);
    out.kv_quoted("x-lazuli-handler", &webhook.handler.path);
    if let Some(verify) = &webhook.structured_verify {
        emit_verify(out, verify);
    } else {
        out.kv_quoted("x-lazuli-verify-path", &webhook.verify.path);
    }
    if let Some(tenant_from) = &webhook.tenant_from {
        out.kv_quoted("x-lazuli-tenant-from", &render_path(&tenant_from.path));
    }
    if let Some(idempotency) = &webhook.idempotency {
        out.kv_quoted("x-lazuli-idempotency-by", &render_path(&idempotency.by));
    }
    if let Some(policy) = &webhook.policy {
        let policy_str = render_policy(policy);
        if !policy_str.is_empty() {
            out.kv_quoted("x-lazuli-policy", &policy_str);
        }
    }
    if !webhook.emits.is_empty() {
        out.kv("x-lazuli-emits", &quoted_list(&webhook.emits));
    }
    if let Some(payload_from) = &webhook.payload_from {
        out.kv_quoted(
            "x-lazuli-payload-from",
            &format!("webhook_events.{}", payload_from.name),
        );
    }
    if let Some(retry) = &webhook.retry {
        emit_retry(out, retry);
    }
    if let Some(replay) = &webhook.replay {
        emit_replay(out, replay);
    }
    if let Some(dlq) = &webhook.dlq {
        emit_dlq(out, dlq);
    }
    out.line("requestBody:");
    out.indent();
    out.line("required: true");
    out.line("content:");
    out.indent();
    out.line("application/json:");
    out.indent();
    out.line("schema:");
    out.indent();
    out.line("type: object");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.line("responses:");
    out.indent();
    out.line("'200':");
    out.indent();
    out.line("description: accepted");
    if let Some(return_type) = &webhook.returns {
        out.line("content:");
        out.indent();
        out.line("application/json:");
        out.indent();
        out.line("schema:");
        out.indent();
        emit_schema_inline(out, return_type);
        out.dedent();
        out.dedent();
        out.dedent();
    }
    out.dedent();
    out.line("'4XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.line("'5XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

fn command_path(feature: &str, cmd: &ir::Command) -> String {
    let mut path = format!("/api/{}/{}", feature, cmd.name);
    for slot in &cmd.route {
        path.push_str(&format!("/{{{}}}", slot.name));
    }
    path
}

fn command_method(cmd: &ir::Command) -> &'static str {
    match cmd.kind {
        ir::CommandKind::Create => "post",
        ir::CommandKind::Update => "patch",
        ir::CommandKind::Delete => "delete",
        ir::CommandKind::Returns => "post",
    }
}

fn http_method_name(m: ir::HttpMethod) -> &'static str {
    match m {
        ir::HttpMethod::Get => "get",
        ir::HttpMethod::Post => "post",
        ir::HttpMethod::Put => "put",
        ir::HttpMethod::Patch => "patch",
        ir::HttpMethod::Delete => "delete",
    }
}

fn render_policy(p: &ir::PolicyRef) -> String {
    match p {
        ir::PolicyRef::None => String::new(),
        ir::PolicyRef::Local(name) => format!("@policy.{}", name),
        ir::PolicyRef::Atom(a) => a.clone(),
        ir::PolicyRef::External { feature, name } => format!("{}.policy.{}", feature, name),
        ir::PolicyRef::Unresolved(s) => s.clone(),
    }
}

fn render_deprecation_replacement(feature: &str, rep: &ir::DeprecationReplacement) -> String {
    match rep {
        ir::DeprecationReplacement::LocalCommand(name) => {
            format!("{}.command.{}", feature, name)
        }
        ir::DeprecationReplacement::LocalApi(name) => {
            format!("{}.api.{}", feature, name)
        }
        ir::DeprecationReplacement::Qualified(qn) => match &qn.feature {
            Some(feature) => format!("{}.command.{}", feature, qn.name),
            None => format!("command.{}", qn.name),
        },
        ir::DeprecationReplacement::QualifiedApi(qn) => match &qn.feature {
            Some(feature) => format!("{}.api.{}", feature, qn.name),
            None => format!("api.{}", qn.name),
        },
        ir::DeprecationReplacement::Url(u) => u.clone(),
    }
}

pub(crate) fn render_path(path: &ir::Path) -> String {
    path.segments.join(".")
}

fn quoted_list(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| quote_value(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
