//! `enum <Name>` declaration surface AST (Phase L Tier 4 follow-up).
//!
//! Authored inside `domain` at indent 4. Variants at indent 6 are either
//! bare identifiers (`free`) or `<name> = <value>` (storage value is `i64`
//! or quoted string).

use serde::{Deserialize, Serialize};

use super::super::Span;
use super::contracts::PublicContractDeclAst;

/// `enum <Name>` declaration authored inside a feature's `domain` block.
///
/// Variants live in [`EnumVariantDecl`] children; storage values land in
/// [`EnumStorageValueDecl`]. Optional `public_contract` opts the enum
/// into the cross-feature contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDeclAst {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub variants: Vec<EnumVariantDecl>,
    pub span: Span,
}

/// One variant row authored inside an [`EnumDeclAst`].
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

/// Storage value behind an enum variant — closed two-arm catalog.
///
/// Authored as `<variant> = <i64>` (Integer) or `<variant> = "<text>"`
/// (String). Codegen picks the corresponding wire format per target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EnumStorageValueDecl {
    /// `<variant> = <number>`.
    Integer(i64),
    /// `<variant> = "<text>"`.
    String(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_storage_integer_serde_token_tagged() {
        let s = EnumStorageValueDecl::Integer(7);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["kind"], "Integer");
        assert_eq!(v["value"], 7);
    }

    #[test]
    fn enum_storage_string_serde_token_tagged() {
        let s = EnumStorageValueDecl::String("active".into());
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["kind"], "String");
        assert_eq!(v["value"], "active");
    }
}
