//! `enum <Name>` declaration surface AST (Phase L Tier 4 follow-up).
//!
//! Authored inside `domain` at indent 4. Variants at indent 6 are either
//! bare identifiers (`free`) or `<name> = <value>` (storage value is `i64`
//! or quoted string).

use serde::{Deserialize, Serialize};

use super::super::Span;
use super::contracts::PublicContractDeclAst;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDeclAst {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub variants: Vec<EnumVariantDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariantDecl {
    pub name: String,
    /// `None` when no `= <value>` is authored. `Some(Integer(_))` for
    /// `<name> = <number>`; `Some(String(_))` for `<name> = "<text>"`.
    pub storage: Option<EnumStorageValueDecl>,
    /// Optional enum metadata parsed from
    /// `<variant>: label @translation.<key>, hint @translation.<key>, icon "<name>"`.
    /// Stored as opaque strings; validation against translation/icon catalogs
    /// belongs to app tooling/doctor, not the parser.
    pub label_key: Option<String>,
    pub hint_key: Option<String>,
    pub icon_key: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EnumStorageValueDecl {
    Integer(i64),
    String(String),
}
