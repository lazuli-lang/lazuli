//! SPEC-01 drift gate — the single-source scalar / semantic catalogs in
//! `lazuli_keywords` must stay in lockstep with the analyzer's type resolver.
//!
//! Each catalog name must resolve to a `TypeRef::Builtin` (a catalog entry with
//! no resolver arm, or a resolver arm dropped from the catalog, fails here), and
//! each historically-silent alias must resolve identically to its canonical
//! spelling (documenting the behaviour `VOCAB-SCALAR-ALIAS-001` flags + `lazuli
//! fmt` normalizes).

use lazuli_analyzer::type_ref_from_syntax_public;
use lazuli_ir::TypeRef;
use lazuli_keywords::{SCALAR_ALIASES, SCALAR_TYPES, SEMANTIC_TYPES};

#[test]
fn every_scalar_type_resolves_to_a_builtin() {
    for name in SCALAR_TYPES {
        assert!(
            matches!(type_ref_from_syntax_public(name), TypeRef::Builtin(_)),
            "scalar `{name}` is in the catalog but the analyzer does not resolve it to a Builtin",
        );
    }
}

#[test]
fn every_semantic_type_resolves_to_a_builtin() {
    for name in SEMANTIC_TYPES {
        let syntax = format!("@semantic.{name}");
        assert!(
            matches!(type_ref_from_syntax_public(&syntax), TypeRef::Builtin(_)),
            "semantic `{syntax}` is in the catalog but the analyzer does not resolve it to a Builtin",
        );
    }
}

#[test]
fn every_alias_resolves_like_its_canonical() {
    for (alias, canonical) in SCALAR_ALIASES {
        assert_eq!(
            type_ref_from_syntax_public(alias),
            type_ref_from_syntax_public(canonical),
            "alias `{alias}` must resolve identically to canonical `{canonical}`",
        );
    }
}
