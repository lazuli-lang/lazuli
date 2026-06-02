//! CREATES-EMPTY-BINDINGS-001 — a `creates` / `updates` effect that binds
//! ZERO author columns lowers to a degenerate, no-op write.
//!
//! ## The bug class
//!
//! A `creates <Resource>` effect emits `lazuli.Creates(&row,
//! lazuli.Bindings{ <one row per assignment> })`. The Go emitter populates
//! that `Bindings{}` map **only** from the effect's `assignments`
//! (`crates/lazuli_codegen_go/src/emitter/command/effects.rs`):
//!
//! - `creates X from input` with no assignments → the emitter writes an
//!   empty `lazuli.Bindings{}` plus a `// TODO(from-input)` placeholder
//!   (the synth path deliberately materialises one `<field> = input.<field>`
//!   assignment per slot precisely *because* an empty binding map "tripped
//!   runtime panics at first call" — see `conventions/crud.rs`).
//! - `creates X` with no assignments → falls through to the same
//!   `lazuli.Bindings{}` with no rows.
//!
//! Either way the resulting INSERT writes an all-default / empty row: the
//! command author forgot to bind any `input → column` (the W4-6
//! `create_agency {}` class, caught here at the DSL-declaration level).
//! The `updates` twin is a no-op UPDATE — an `updates X` whose SET map
//! (also built solely from `assignments`) is empty changes nothing.
//!
//! ## What counts as a "meaningful write"
//!
//! A binding is a meaningful write unless its column is one of the
//! framework-**auto-managed** columns the runtime stamps on every write:
//! the identity (`id`) and the timestamp axis (`created_at`, `updated_at`).
//! Those are derived from the shared SoT
//! [`lazuli_ir::synthesized_columns`] (`IDENTITY` + `TIMESTAMPS`) so the
//! split can never drift from the codegen / schema-diff layer.
//!
//! The **soft-delete** columns (`deleted_at`, `deleted_by`) — also in
//! `synthesized_columns`, but as the `SOFT_DELETE` axis — DO count as a
//! meaningful write: a `delete_x` command lowered as `updates X` that binds
//! `deleted_at = ctx.now` / `deleted_by = ctx.actor.id` is a real soft
//! delete, not a no-op. Excluding them would false-positive on every
//! soft-delete in the codebase (the pauta `delete_*` family). So only
//! `IDENTITY` + `TIMESTAMPS` are filtered out of the count, not the whole
//! `is_synthesized_column` set.
//!
//! A `creates` / `updates` whose *only* bindings are auto-managed columns
//! (e.g. an `updates X` that binds nothing but `updated_at = ctx.now` while
//! declaring a dozen unused `input` slots) writes no domain data — that IS
//! the missing-bindings authoring bug, so it fires.
//!
//! ## Precisely-scoped escape hatches (no false positives)
//!
//! - **Handler-backed commands** (`handler @fn.<name>` / `handler "./…"`):
//!   the row is filled by user Go code, not the declarative binding map, so
//!   an empty `assignments` list is legitimate. Skipped.
//! - **Lifecycle-transition commands** (`triggers` non-empty): an
//!   `updates X` that fires a state transition writes the discriminator
//!   column through the lifecycle machinery even with zero SET assignments.
//!   Skipped for the `updates` arm.
//! - **`from_input` does NOT save a `creates`**: codegen does not expand
//!   bindings from the flag alone (it emits the empty-map + TODO), and the
//!   `conventions [crud]` synth proves the flag must be paired with explicit
//!   per-slot assignments to produce a populated INSERT. So a `creates X
//!   from input` with zero assignments is exactly the no-op INSERT this
//!   rule targets.
//!
//! Fires when a `creates` (or `updates`) command effect has zero author
//! bindings after the synthesized-column filter and is not handler-backed /
//! lifecycle-transition driven. See the `#[cfg(test)] mod tests` fixtures
//! for the firing and non-firing shapes.
//!
//! Severity: **error**. A no-op INSERT / UPDATE is a concrete, reproducible
//! bug (the row is written all-default, or nothing is updated), not style
//! drift — it joins the other Correctness `error` peers.

use std::path::{Path, PathBuf};

use lazuli_ir::{Command, CommandEffect, Feature, QualifiedName, synthesized_columns};

/// The effect kind a CREATES-EMPTY-BINDINGS-001 finding flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// `creates <Resource>` → degenerate / empty INSERT.
    Creates,
    /// `updates <Resource>` → no-op UPDATE (empty SET map).
    Updates,
}

impl EffectKind {
    /// DSL keyword (`creates` / `updates`) for the diagnostic message.
    fn keyword(self) -> &'static str {
        match self {
            EffectKind::Creates => "creates",
            EffectKind::Updates => "updates",
        }
    }

    /// The degenerate SQL the empty binding map lowers to.
    fn degenerate(self) -> &'static str {
        match self {
            EffectKind::Creates => "an empty/degenerate INSERT (an all-default row)",
            EffectKind::Updates => "a no-op UPDATE (the SET map is empty)",
        }
    }
}

/// One CREATES-EMPTY-BINDINGS-001 finding — a `creates` / `updates`
/// command effect that binds zero author columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Feature owning the command.
    pub feature: String,
    /// Command whose effect has no author bindings (diagnostic anchor).
    pub command: String,
    /// Target resource the empty effect writes.
    pub resource: String,
    /// Whether the offending effect is a `creates` or an `updates`.
    pub kind: EffectKind,
    /// Source `.lzi` file the command lives in (diagnostic anchor).
    pub path: PathBuf,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "CREATES-EMPTY-BINDINGS-001";

    /// Render the "no author bindings → degenerate write" message naming
    /// the command, the effect keyword, and the resource.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::creates_empty_bindings_001::{EffectKind, Finding};
    ///
    /// let f = Finding {
    ///     feature: "billing".into(),
    ///     command: "create_invoice".into(),
    ///     resource: "Invoice".into(),
    ///     kind: EffectKind::Creates,
    ///     path: PathBuf::from("billing.lzi"),
    /// };
    /// assert!(f.message().contains("create_invoice"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "command '{}.{}' declares `{} {}` but binds no meaningful columns, so \
             it lowers to {}. Add `<column> = <input.field | ctx.* | literal>` \
             bindings, use a `handler @fn.<name>` escape hatch if a Go handler \
             fills the row, or remove the effect. (Auto-managed columns — id, \
             created_at, updated_at — do not count; soft-delete columns \
             deleted_at/deleted_by DO count as a real write.)",
            self.feature,
            self.command,
            self.kind.keyword(),
            self.resource,
            self.kind.degenerate(),
        )
    }
}

/// Run CREATES-EMPTY-BINDINGS-001 over one feature.
///
/// Walks every command and emits a finding for each `creates` / `updates`
/// effect that binds zero author columns. Handler-backed commands and
/// lifecycle-transition `updates` are skipped (see module header).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::creates_empty_bindings_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a `creates` command");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for command in &feature.commands {
        // Handler-backed commands route the write to user Go code; an empty
        // declarative binding map is legitimate there (the handler fills the
        // row). This is the sanctioned escape hatch — never false-positive.
        if command.handler.is_some() {
            continue;
        }

        match &command.effect {
            CommandEffect::Creates(effect) => {
                if author_binding_count(effect.assignments.iter().map(|a| a.field.as_str())) == 0 {
                    findings.push(make_finding(
                        feature,
                        command,
                        &effect.resource,
                        EffectKind::Creates,
                        path,
                    ));
                }
            }
            CommandEffect::Updates(effect) => {
                // A lifecycle-transition command writes the discriminator
                // column through the `triggers` machinery even with zero SET
                // assignments, so it is NOT a no-op UPDATE. Skip it.
                if !command.triggers.is_empty() {
                    continue;
                }
                if author_binding_count(effect.assignments.iter().map(|a| a.field.as_str())) == 0 {
                    findings.push(make_finding(
                        feature,
                        command,
                        &effect.resource,
                        EffectKind::Updates,
                        path,
                    ));
                }
            }
            _ => {}
        }
    }

    findings
}

/// Count `assignments` that write a MEANINGFUL column — i.e. anything that
/// is not a framework-auto-managed column (`id` identity + `created_at` /
/// `updated_at` timestamps). Soft-delete columns (`deleted_at` /
/// `deleted_by`) are NOT filtered: a `delete_x` lowered as `updates X`
/// binding only soft-delete columns is a real soft delete, not a no-op.
/// Both column sets are read from the shared SoT
/// [`lazuli_ir::synthesized_columns`] so the split can never drift from the
/// codegen / schema-diff layer.
fn author_binding_count<'a>(fields: impl Iterator<Item = &'a str>) -> usize {
    fields.filter(|field| !is_auto_managed_column(field)).count()
}

/// `true` when `name` is a framework-auto-managed column the runtime stamps
/// on every write (identity or timestamp axis) — these never count as a
/// meaningful author write. Soft-delete columns are deliberately excluded
/// here (they ARE a meaningful write; see [`author_binding_count`]).
fn is_auto_managed_column(name: &str) -> bool {
    name == synthesized_columns::IDENTITY || synthesized_columns::is_timestamp_column(name)
}

fn make_finding(
    feature: &Feature,
    command: &Command,
    resource: &QualifiedName,
    kind: EffectKind,
    path: &Path,
) -> Finding {
    Finding {
        feature: feature.name.clone(),
        command: command.name.clone(),
        resource: resource.name.clone(),
        kind,
        path: path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        assert_eq!(features.len(), 1, "fixture should contain one feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    fn check_source(source: &str) -> Vec<Finding> {
        let feature = lower(source);
        check(&feature, Path::new("billing.lzi"))
    }

    #[test]
    fn creates_with_zero_bindings_fires() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    creates Invoice
"#,
        );

        assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
        let f = &findings[0];
        assert_eq!(f.feature, "billing");
        assert_eq!(f.command, "create_invoice");
        assert_eq!(f.resource, "Invoice");
        assert_eq!(f.kind, EffectKind::Creates);
        assert_eq!(Finding::CODE, "CREATES-EMPTY-BINDINGS-001");
        let msg = f.message();
        assert!(msg.contains("command 'billing.create_invoice'"), "{msg}");
        assert!(msg.contains("creates Invoice"), "{msg}");
        assert!(msg.contains("INSERT"), "{msg}");
    }

    #[test]
    fn creates_binding_real_column_does_not_fire() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    input
      amount: Integer required
    creates Invoice
      amount = input.amount
"#,
        );

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn creates_binding_only_synthesized_columns_fires() {
        // The all-synthesized-only edge: the author bound only framework
        // columns (id / timestamps), so no domain data is written — still
        // a degenerate INSERT. That IS the bug.
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    creates Invoice
      id = input.id
      created_at = input.created_at
      updated_at = input.updated_at
"#,
        );

        assert_eq!(
            findings.len(),
            1,
            "synthesized-only bindings should still fire: {findings:?}"
        );
        assert_eq!(findings[0].kind, EffectKind::Creates);
    }

    #[test]
    fn creates_from_input_with_zero_assignments_fires() {
        // `creates X from input` with no explicit assignments lowers to the
        // empty `lazuli.Bindings{}` + TODO path (the synth must materialise
        // per-slot assignments to avoid the runtime panic), so the flag
        // alone does NOT save it.
        let mut feature = lower(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    creates Invoice
"#,
        );
        // Force the `from_input` flag on with still-empty assignments to
        // model `creates X from input` written with no body.
        if let CommandEffect::Creates(effect) = &mut feature.commands[0].effect {
            effect.from_input = true;
            effect.assignments.clear();
        }

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "from_input flag alone must not suppress the empty-INSERT finding: {findings:?}"
        );
    }

    #[test]
    fn creates_with_handler_does_not_fire() {
        // A handler-backed command fills the row in user Go code; the empty
        // declarative binding map is the sanctioned escape hatch.
        let mut feature = lower(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    creates Invoice
"#,
        );
        feature.commands[0].handler = Some(lazuli_ir::HandlerRef {
            namespace: "fn".into(),
            name: "create_invoice_impl".into(),
            span_ref: None,
        });

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "handler-backed command should not fire: {findings:?}"
        );
    }

    #[test]
    fn updates_with_zero_set_bindings_fires() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command touch_invoice
    route id: ID
    updates Invoice
"#,
        );

        assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
        let f = &findings[0];
        assert_eq!(f.command, "touch_invoice");
        assert_eq!(f.kind, EffectKind::Updates);
        let msg = f.message();
        assert!(msg.contains("updates Invoice"), "{msg}");
        assert!(msg.contains("no-op UPDATE"), "{msg}");
    }

    #[test]
    fn updates_soft_delete_columns_does_not_fire() {
        // A `delete_x` command lowered as `updates X` that binds the
        // soft-delete columns is a REAL soft delete — not a no-op. It must
        // not false-positive (the pauta `delete_*` family).
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command delete_invoice
    route id: ID
    updates Invoice
      where id = route.id
      deleted_at = ctx.now
      deleted_by = ctx.actor.id
"#,
        );

        assert!(
            findings.is_empty(),
            "soft-delete update should not fire: {findings:?}"
        );
    }

    #[test]
    fn updates_only_updated_at_fires() {
        // The real pauta `update_*` bug: declares many input slots but the
        // `updates` body binds only `updated_at = ctx.now`, so none of the
        // inputs are written — a silent no-op UPDATE.
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command update_invoice
    route id: ID
    input
      amount: Integer optional
    updates Invoice
      where id = route.id
      updated_at = ctx.now
"#,
        );

        assert_eq!(
            findings.len(),
            1,
            "updated_at-only update should fire: {findings:?}"
        );
        assert_eq!(findings[0].kind, EffectKind::Updates);
    }

    #[test]
    fn updates_binding_real_column_does_not_fire() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command update_invoice
    route id: ID
    input
      amount: Integer required
    updates Invoice
      amount = input.amount
"#,
        );

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn updates_with_lifecycle_trigger_does_not_fire() {
        // A lifecycle transition writes the discriminator column through the
        // `triggers` machinery even with no SET assignments — not a no-op.
        let mut feature = lower(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command publish_invoice
    route id: ID
    updates Invoice
"#,
        );
        feature.commands[0].triggers = vec!["publish".into()];

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "lifecycle-transition update should not fire: {findings:?}"
        );
    }

    #[test]
    fn deletes_and_returns_do_not_fire() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required

  command delete_invoice
    route id: ID
    deletes Invoice

  command preview_invoice
    returns Text
"#,
        );

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }
}
