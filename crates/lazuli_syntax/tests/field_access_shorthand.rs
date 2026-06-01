//! 0005 field-policy `access:` shorthand — symmetric read/write desugaring.
//!
//! `access: P` is a field-policy shorthand that desugars to `read: P` +
//! `write: P` at parse time, so the resulting [`FieldPolicyDecl`] (and thus
//! every downstream IR / codegen consumer) is byte-identical to the explicit
//! two-line form. The asymmetric minority keeps explicit `read:`/`write:`;
//! mixing `access:` with either on one field is a parse error.
//!
//! These tests pin the surface (parser → AST). The AST-identity assertion is
//! the migration safety net: because `access:` lowers into the SAME
//! `FieldPolicyDecl.read`/`.write` slots the explicit form fills, the analyzer
//! (`crates/lazuli_analyzer/src/lib.rs` field-policy lowering, which just
//! clones `read`/`write`) produces identical `ir::FieldPolicy` for both.

use lazuli_syntax::{FieldPolicyDecl, parse_feature_skeletons};

/// Pull the single field-policy decl for `<Resource>.<field>` out of the
/// first feature's `policies > fields` block.
fn field_policy<'a>(source: &str) -> FieldPolicyDecl {
    let features = parse_feature_skeletons(source).unwrap();
    let policies = features[0].policies.as_ref().expect("policies block");
    let fp = policies
        .fields
        .iter()
        .flat_map(|f| f.fields.iter())
        .next()
        .expect("at least one field policy");
    fp.clone()
}

#[test]
fn access_shorthand_parses() {
    let source = "
feature billing
  resource Customer
    legal_name: Text
  policies
    fields Customer
      legal_name
        access: @role.ADMIN
";
    let fp = field_policy(source);
    assert_eq!(fp.field, "legal_name");
    assert_eq!(fp.read.as_deref(), Some(&["@role.ADMIN".to_owned()][..]));
    assert_eq!(fp.write.as_deref(), Some(&["@role.ADMIN".to_owned()][..]));
}

#[test]
fn access_desugars_to_read_write() {
    let access_src = "
feature billing
  resource Customer
    legal_name: Text
  policies
    fields Customer
      legal_name
        access: @role.ADMIN | @role.MANAGER
";
    let explicit_src = "
feature billing
  resource Customer
    legal_name: Text
  policies
    fields Customer
      legal_name
        read: @role.ADMIN | @role.MANAGER
        write: @role.ADMIN | @role.MANAGER
";
    let from_access = field_policy(access_src);
    let from_explicit = field_policy(explicit_src);

    // The desugaring guarantee: identical read + write allow-lists. (Spans
    // differ — explicit form spans two lines — so we compare the policy
    // payload, which is what lowers to `ir::FieldPolicy`.)
    assert_eq!(from_access.field, from_explicit.field);
    assert_eq!(from_access.read, from_explicit.read);
    assert_eq!(from_access.write, from_explicit.write);
    // And `access:` actually filled BOTH axes.
    assert!(from_access.read.is_some());
    assert!(from_access.write.is_some());
    assert_eq!(from_access.read, from_access.write);
}

#[test]
fn mixing_access_and_read_errors() {
    let source = "
feature billing
  resource Customer
    legal_name: Text
  policies
    fields Customer
      legal_name
        access: @role.ADMIN
        read: @role.MANAGER
";
    let err = parse_feature_skeletons(source).expect_err("mixing access: + read: must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("access"),
        "diagnostic should mention `access`; got: {msg}"
    );
}

#[test]
fn mixing_access_and_write_errors() {
    let source = "
feature billing
  resource Customer
    legal_name: Text
  policies
    fields Customer
      legal_name
        write: @role.ADMIN
        access: @role.MANAGER
";
    let err = parse_feature_skeletons(source).expect_err("mixing write: + access: must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("access"),
        "diagnostic should mention `access`; got: {msg}"
    );
}

#[test]
fn explicit_read_write_still_parses() {
    // Asymmetric field — the legitimate minority. Must keep both axes
    // independent and unaffected by the `access:` addition.
    let source = "
feature billing
  resource Customer
    cnpj: Text
  policies
    fields Customer
      cnpj
        read: @role.ADMIN | @role.MANAGER
        write: @role.ADMIN
";
    let fp = field_policy(source);
    assert_eq!(fp.field, "cnpj");
    assert_eq!(
        fp.read.as_deref(),
        Some(&["@role.ADMIN | @role.MANAGER".to_owned()][..])
    );
    assert_eq!(fp.write.as_deref(), Some(&["@role.ADMIN".to_owned()][..]));
    assert_ne!(fp.read, fp.write, "asymmetric field must stay asymmetric");
}
