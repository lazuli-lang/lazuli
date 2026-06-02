//! `agent_discriminator_*` diagnostics — target / field validity for
//! `output discriminator <enum>` and `output <Record>` shapes.
//!
//! Diagnostic ids:
//!   - `agent_discriminator_target_invalid_diagnostics` (error) —
//!     `output discriminator <X>` references an unknown enum, or
//!     `output <X>` references neither an enum nor a record.
//!   - `agent_discriminator_field_invalid_diagnostics` (error) — the
//!     discriminator marker on a record points at a field whose type
//!     does not resolve to an enum.
//!
//! Walks `Tier3FeatureFacts.{enums, records}` (typed IR lift) — the
//! pre-Phase-L-Tier-4 text walkers are gone.

use lazuli_ir as ir;
use lazuli_ir::Agent;

use crate::doctor::parsers::type_ref_name;
use crate::doctor::{AgentFacts, DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Phase L Tier 4 follow-up — fully IR-driven replacement for the
/// records/enums branches of `scan_feature_range`. Records read from
/// `Tier3FeatureFacts.records` (typed `ir::Record` lift); enums read
/// from `Tier3FeatureFacts.enums` (typed `ir::EnumDecl` lift). The
/// retired `FeatureSymbols.enums` text walker is gone.
pub(crate) fn agent_discriminator_diagnostics(
    agents: &[AgentFacts],
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let any_enum = |name: &str| -> bool {
        tier3_facts
            .iter()
            .any(|f| f.enums.iter().any(|e| e.name == name))
    };
    let any_record = |name: &str| -> bool {
        tier3_facts
            .iter()
            .any(|f| f.records.iter().any(|r| r.name == name))
    };

    for fact in agents {
        let agent = &fact.agent;
        match (&agent.output_kind, agent.output_discriminator.as_ref()) {
            (ir::AgentOutputKind::DiscriminatedEnum, Some(ir::DiscriminatorRef::Enum(qn))) => {
                let enum_name = qn.name.as_str();
                if !any_enum(enum_name) {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_discriminator_target_invalid_diagnostics".to_owned(),
                        message: format!(
                            "agent `{}` declares `output discriminator {}` but no enum named `{}` exists in any reachable feature.",
                            agent.name, enum_name, enum_name,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
            // `output <Record>` lowering produces output_kind=Text +
            // output_type=Unresolved("X") today; a future expand pass would
            // resolve the discriminator field (unbuilt). For now we still
            // report when the bare `output <Record>` references an unknown
            // record so the author gets a fast signal.
            (ir::AgentOutputKind::Text, _) => {
                if let Some(ir::TypeRef::Unresolved(name)) = agent.output_type.as_ref() {
                    // Heuristic: titlecase first letter means it's an
                    // intended record/enum reference (vs `Text`/`Integer`
                    // which match Builtin earlier).
                    let first = name.chars().next();
                    if first.is_some_and(|c| c.is_ascii_uppercase()) {
                        if !any_record(name) && !any_enum(name) {
                            diagnostics.push(DoctorDiagnostic {
                                path: fact.path.clone(),
                                line: fact.line,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "agent_discriminator_target_invalid_diagnostics".to_owned(),
                                message: format!(
                                    "agent `{}` declares `output {}` but no enum or record named `{}` exists in any reachable feature.",
                                    agent.name, name, name,
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                            continue;
                        }
                        // Validate field-level discriminator marker on
                        // records — proposal §A2 requires exactly one
                        // field carrying the marker, and its type must
                        // resolve to an enum.
                        for facts in tier3_facts {
                            if let Some(record) = facts.records.iter().find(|r| r.name == *name) {
                                diagnostics.extend(check_record_discriminator(
                                    fact,
                                    agent,
                                    name,
                                    record,
                                    tier3_facts,
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

/// Phase L Tier 4 follow-up — typed `check_record_discriminator` that
/// consumes `ir::Record` directly. The discriminator-marker count
/// comes from `Record.discriminator_field` (typed `Option<String>`);
/// the discriminator field's type is read by name from the record's
/// typed field list. The enum lookup walks `Tier3FeatureFacts.enums`
/// (typed `ir::EnumDecl` lift); the legacy `FeatureSymbols.enums`
/// text walker is gone.
pub(crate) fn check_record_discriminator(
    fact: &AgentFacts,
    agent: &Agent,
    record_name: &str,
    record: &lazuli_ir::Record,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let Some(field_name) = record.discriminator_field.as_deref() else {
        // No discriminator: it's a legacy `output <Record>` shape (no
        // discriminator field). Cut A's soft-warn for legacy output is
        // emitted in the LSP file-local layer (Phase 4); nothing to do
        // here.
        return Vec::new();
    };

    let mut diagnostics = Vec::new();

    // The IR currently captures only one discriminator field per record
    // (`discriminator_field: Option<String>`), so the "multiple
    // markers" branch from the legacy walker is structurally
    // unreachable; the parser would reject the duplicate before
    // lowering. We preserve the slot for forward compatibility but
    // skip the check.
    let Some(field) = record.fields.iter().find(|f| f.name == field_name) else {
        return diagnostics;
    };
    let type_name = type_ref_name(&field.type_ref);
    let enum_exists = tier3_facts
        .iter()
        .any(|f| f.enums.iter().any(|e| e.name == type_name));
    if !enum_exists {
        diagnostics.push(DoctorDiagnostic {
            path: fact.path.clone(),
            line: fact.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "agent_discriminator_field_invalid_diagnostics".to_owned(),
            message: format!(
                "agent `{}` references record `{}` whose discriminator field `{}` has type `{}`, but no enum by that name exists; the marked field must resolve to an enum.",
                agent.name, record_name, field_name, type_name,
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    diagnostics
}
