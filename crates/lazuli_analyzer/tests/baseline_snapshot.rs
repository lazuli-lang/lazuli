//! Baseline snapshots that lock the current shape of the parser AST and the
//! IR for `examples/crm.lzi` before the Phase 1a refactor.
//!
//! These are intentionally not aspirational. They capture today's behavior so
//! we can detect drift while reshaping `lazuli_ir`. When the refactor lands,
//! these snapshots will move or be replaced by canonical-shape baselines; they
//! exist now as a safety net.

use lazuli_analyzer::lower_document;
use lazuli_syntax::parse_document;

const CRM_SOURCE: &str = include_str!("../../../examples/anti-patterns/crm-aggregate-dialect.lzi");

#[test]
fn crm_ast_baseline() {
    let document = parse_document(CRM_SOURCE).expect("crm.lzi must parse");
    insta::assert_json_snapshot!("crm_ast", document);
}

#[test]
fn crm_ir_baseline() {
    let document = parse_document(CRM_SOURCE).expect("crm.lzi must parse");
    let app = lower_document(&document).expect("crm.lzi must lower");
    insta::assert_json_snapshot!("crm_ir", app);
}
