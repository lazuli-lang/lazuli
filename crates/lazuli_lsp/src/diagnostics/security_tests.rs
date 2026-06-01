// Inline tests for the `field-security-policy` source walker (sibling of
// `security.rs`; included via `#[cfg(test)] mod ... { include!(...) }`).
//
// 0005 — the `access:` symmetric shorthand must satisfy the
// `field-security-policy` contract exactly as the explicit `read:` + `write:`
// pair does, since it desugars to both axes.
//
// Sensitive fields are detected only inside a `domain` block (indent 4
// `resource`, indent 6 fields) — see `collect_sensitive_fields`. The
// fixtures below mirror that real shape.

use super::field_security_policy_diagnostics;

/// A sensitive field whose policy is declared with the `access:` shorthand
/// must NOT trip `field-security-policy` — `access:` covers both read+write.
#[test]
fn access_shorthand_satisfies_field_security_policy() {
    let source = "
feature billing
  domain
    resource Customer
      legal_name: Text @pii.Name
  policies
    fields Customer
      legal_name
        access: @role.ADMIN | @role.MANAGER
";
    let diags = field_security_policy_diagnostics(source);
    assert!(
        diags.is_empty(),
        "`access:` should satisfy the read+write contract; got: {diags:?}"
    );
}

/// The explicit symmetric pair still satisfies the contract (no regression).
#[test]
fn explicit_read_write_satisfies_field_security_policy() {
    let source = "
feature billing
  domain
    resource Customer
      legal_name: Text @pii.Name
  policies
    fields Customer
      legal_name
        read: @role.ADMIN | @role.MANAGER
        write: @role.ADMIN | @role.MANAGER
";
    let diags = field_security_policy_diagnostics(source);
    assert!(diags.is_empty(), "explicit pair should satisfy; got: {diags:?}");
}

/// A sensitive field whose policy declares only ONE axis via `read:`
/// still trips the rule — the `access:` change must not mask a
/// genuinely half-declared (asymmetric-but-incomplete) policy.
#[test]
fn half_declared_policy_still_fires() {
    let source = "
feature billing
  domain
    resource Customer
      legal_name: Text @pii.Name
  policies
    fields Customer
      legal_name
        read: @role.ADMIN
";
    let diags = field_security_policy_diagnostics(source);
    assert_eq!(
        diags.len(),
        1,
        "a sensitive field with only `read:` (no write) must still fire; got: {diags:?}"
    );
}
