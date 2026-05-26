//! Cell G2b -- `Webhook` kind emission. Walks every `Webhook`
//! declared on a feature and emits the v0 Lazuli Go
//! `webhooks.WebhookContract` value into `<feature>/webhook.gen.go`.
//!
//! The Lazuli Go runtime carries the webhook spine plus expanded
//! webhook slots: `PayloadFrom`, `Replay`, `DLQ`, and `Retry`.
//! `Retry` reuses `jobs.RetryPolicy`, so the jobs runtime import is
//! included only when a feature declares webhook retry policy.
//!
//! Determinism: webhooks are sorted by name before emission. Imports
//! flow through `ImportSet`, and type strings for `handler returns`
//! reuse `types::go_type_for` so cross-feature names render the same
//! way as resource/command emitters.

mod contract;
mod emit_bindings;
mod format;
mod specs;

use lazuli_ir::{Feature, Webhook};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::printer::GoPrinter;
use super::types::TypeCtx;

use contract::emit_webhook;

/// Emit `<feature>/webhook.gen.go` for a feature, or `None` when the
/// feature declares no webhooks.
pub fn emit_webhook_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.webhooks.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut webhooks: Vec<&Webhook> = feature.webhooks.iter().collect();
    webhooks.sort_by(|a, b| a.name.cmp(&b.name));

    imports.add("context");
    imports.add("lazuli.dev/runtime/lazuli");
    imports.add("lazuli.dev/runtime/lazuli/webhooks");
    if webhooks.iter().any(|webhook| webhook.retry.is_some()) {
        imports.add("lazuli.dev/runtime/lazuli/jobs");
    }
    // PG.C.2 — gated webhooks carry a `Prelude: []billing.GateRef{...}`
    // field on the WebhookContract value; the receiver runs it via
    // the runner the `billing` package registers on `webhooks` at
    // init. Import `billing` only when any webhook in the file
    // declares gates.
    let any_gated = webhooks
        .iter()
        .any(|w| !emit_ctx.gates_for("webhook", &w.name).is_empty());
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for webhook in &webhooks {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_webhook(&mut p, feature, webhook, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}


#[cfg(test)]
mod feature_emit_tests;
