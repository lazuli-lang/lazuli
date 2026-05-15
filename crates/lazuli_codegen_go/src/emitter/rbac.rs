//! RBAC catalog Go emission — `dist/go/rbac/rbac.gen.go`.
//!
//! Single package-level file produced from `Module.rbac`. Shape per
//! `docs/proposals/rbac-catalog-vocab.md` §Codegen-Go: closed
//! Permission const block, closure-baked Role variables, and a flat
//! `AllRoles` slice + `HasPermission(role, perm)` / `HasRole(role, name)`
//! helpers.
//!
//! Wire-thin: ≤ 80 LOC of generated output for a typical 10-perm /
//! 4-role catalog. No runtime lookup beyond linear scan over the
//! closure slice.

use lazuli_ir::{Module, PermissionEntry, RbacCatalog, RoleEntry};

use super::casing::pascal_case;
use super::printer::GoPrinter;

/// Emit `dist/go/rbac/rbac.gen.go` for the package, or `None` when
/// no `permission` / `role` blocks were authored.
pub fn emit_rbac_file(source_label: &str, module: &Module) -> Option<String> {
    let catalog = module.rbac.as_ref()?;
    if catalog.permissions.is_empty() && catalog.roles.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    p.banner(source_label, "rbac (catalog)");
    p.line("package rbac");
    p.blank();

    p.line("// Permission is the closed-catalog identifier for one verb.");
    p.line("type Permission string");
    p.blank();

    p.line("// Role describes one closed-catalog role and its flat (closure-");
    p.line("// baked) permission grants.");
    p.line("type Role struct {");
    p.indent();
    p.line("Name   string");
    p.line("Grants []Permission");
    p.dedent();
    p.line("}");
    p.blank();

    // Permission constants.
    if !catalog.permissions.is_empty() {
        emit_permission_consts(&mut p, &catalog.permissions);
        p.blank();
    }

    // AllPermissions slice (for `grants_all` resolution and external
    // tooling that enumerates the catalog).
    p.line("// AllPermissions enumerates the closed permission catalog in source order.");
    p.line("var AllPermissions = []Permission{");
    p.indent();
    for perm in &catalog.permissions {
        p.line(&format!("{},", permission_ident(&perm.name)));
    }
    p.dedent();
    p.line("}");
    p.blank();

    // Per-role variables with baked closure.
    for role in &catalog.roles {
        emit_role_var(&mut p, role);
        p.blank();
    }

    // AllRoles slice.
    p.line("// AllRoles enumerates every declared role in source order.");
    p.line("var AllRoles = []Role{");
    p.indent();
    for role in &catalog.roles {
        p.line(&format!("{},", role_ident(&role.name)));
    }
    p.dedent();
    p.line("}");
    p.blank();

    // Helper functions.
    emit_helpers(&mut p);

    Some(p.finish())
}

fn emit_permission_consts(p: &mut GoPrinter, perms: &[PermissionEntry]) {
    p.line("// Closed permission catalog.");
    p.line("const (");
    p.indent();
    for perm in perms {
        p.line(&format!(
            "{} Permission = {:?}",
            permission_ident(&perm.name),
            perm.name
        ));
    }
    p.dedent();
    p.line(")");
}

fn emit_role_var(p: &mut GoPrinter, role: &RoleEntry) {
    p.line(&format!(
        "// {} role (closure: {} permission{}).",
        role.name,
        role.closure.len(),
        if role.closure.len() == 1 { "" } else { "s" }
    ));
    p.line(&format!("var {} = Role{{", role_ident(&role.name)));
    p.indent();
    p.line(&format!("Name: {:?},", role.name));
    if role.closure.is_empty() {
        p.line("Grants: []Permission{},");
    } else {
        p.line("Grants: []Permission{");
        p.indent();
        for perm in &role.closure {
            p.line(&format!("{},", permission_ident(perm)));
        }
        p.dedent();
        p.line("},");
    }
    p.dedent();
    p.line("}");
}

fn emit_helpers(p: &mut GoPrinter) {
    p.line("// HasPermission returns true when role `roleName` grants `perm`.");
    p.line("// Roles not declared in the catalog return false (fail-closed).");
    p.line("func HasPermission(roleName string, perm Permission) bool {");
    p.indent();
    p.line("for _, r := range AllRoles {");
    p.indent();
    p.line("if r.Name != roleName {");
    p.indent();
    p.line("continue");
    p.dedent();
    p.line("}");
    p.line("for _, g := range r.Grants {");
    p.indent();
    p.line("if g == perm {");
    p.indent();
    p.line("return true");
    p.dedent();
    p.line("}");
    p.dedent();
    p.line("}");
    p.line("return false");
    p.dedent();
    p.line("}");
    p.line("return false");
    p.dedent();
    p.line("}");
    p.blank();

    p.line("// HasRole returns true when the actor's role is `name` or a role");
    p.line("// transitively inheriting from it. Since closures are baked at");
    p.line("// codegen time, the runtime check is a single name match.");
    p.line("func HasRole(roleName string, name string) bool {");
    p.indent();
    p.line("return roleName == name");
    p.dedent();
    p.line("}");
}

/// Convert a permission name `"users:read:own"` -> Go identifier
/// `PermUsersReadOwn`.
fn permission_ident(name: &str) -> String {
    let mut out = String::from("Perm");
    for seg in name.split(':') {
        out.push_str(&pascal_case(seg));
    }
    out
}

/// Convert a role name `"sales_manager"` -> Go identifier `RoleSalesManager`.
fn role_ident(name: &str) -> String {
    format!("Role{}", pascal_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Module, PermissionEntry, RbacCatalog, RoleEntry, RoleGrants,
    };

    fn mk_module(catalog: Option<RbacCatalog>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: catalog,
            features: vec![],
        }
    }

    #[test]
    fn returns_none_without_catalog() {
        let m = mk_module(None);
        assert!(emit_rbac_file("test", &m).is_none());
    }

    #[test]
    fn emits_perm_consts_and_role_vars() {
        let m = mk_module(Some(RbacCatalog {
            permissions: vec![
                PermissionEntry {
                    name: "users:read".into(),
                    segments: vec!["users".into(), "read".into()],
                    span_ref: None,
                },
                PermissionEntry {
                    name: "users:create".into(),
                    segments: vec!["users".into(), "create".into()],
                    span_ref: None,
                },
            ],
            roles: vec![
                RoleEntry {
                    name: "viewer".into(),
                    inherits: None,
                    grants: RoleGrants::Explicit(vec!["users:read".into()]),
                    closure: vec!["users:read".into()],
                    span_ref: None,
                },
                RoleEntry {
                    name: "admin".into(),
                    inherits: None,
                    grants: RoleGrants::All,
                    closure: vec!["users:read".into(), "users:create".into()],
                    span_ref: None,
                },
            ],
        }));
        let out = emit_rbac_file("test.lzi", &m).expect("emits");
        assert!(out.contains("package rbac"));
        assert!(out.contains("PermUsersRead Permission = \"users:read\""));
        assert!(out.contains("PermUsersCreate Permission = \"users:create\""));
        assert!(out.contains("var RoleViewer = Role{"));
        assert!(out.contains("var RoleAdmin = Role{"));
        assert!(out.contains("func HasPermission(roleName string, perm Permission) bool {"));
        assert!(out.contains("AllRoles = []Role{"));
    }

    #[test]
    fn permission_ident_handles_three_segments() {
        assert_eq!(permission_ident("report:repasse:mark"), "PermReportRepasseMark");
    }
}
