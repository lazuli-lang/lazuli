fn report_local_policy_atoms(feature: &Feature, name: &str) -> Option<Vec<String>> {
    let category = feature
        .policies
        .categories
        .iter()
        .find(|category| category.name == name)?;
    let mut out = Vec::new();
    for atom in &category.atoms {
        if let Some(expr) = report_policy_atom_expr(atom) {
            walk_report_policy_expr_atoms(&expr, &mut out);
        }
    }
    Some(out)
}

fn report_policy_atom_literal(atom: &str) -> Option<String> {
    let expr = report_policy_atom_expr(atom)?;
    let mut out = Vec::new();
    walk_report_policy_expr_atoms(&expr, &mut out);
    out.into_iter().next()
}

fn report_policy_atom_expr(atom: &str) -> Option<PolicyExpr> {
    let stripped = atom.strip_prefix('@').unwrap_or(atom);
    if stripped.starts_with("policy.") {
        return None;
    }
    let mut parts = stripped.splitn(2, '.');
    let namespace = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if namespace.is_empty() || name.is_empty() {
        return None;
    }
    Some(PolicyExpr::Atom(PolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args: None,
    }))
}

fn walk_report_policy_expr_atoms(expr: &PolicyExpr, out: &mut Vec<String>) {
    match expr {
        PolicyExpr::Authenticated => {
            out.push("{Namespace: \"predicate\", Name: \"authenticated\"},".to_owned());
        }
        PolicyExpr::HasRole(name) => {
            out.push(format!("{{Namespace: \"rbac.role\", Name: {:?}}},", name));
        }
        PolicyExpr::HasPermission(perm) => {
            out.push(format!(
                "{{Namespace: \"rbac.permission\", Name: {:?}}},",
                perm
            ));
        }
        PolicyExpr::Atom(atom) => {
            out.push(format!(
                "{{Namespace: {:?}, Name: {:?}}},",
                atom.namespace, atom.name
            ));
        }
        PolicyExpr::And(terms) => {
            out.push("{Namespace: \"predicate\", Name: \"(\"},".to_owned());
            for (i, term) in terms.iter().enumerate() {
                if i > 0 {
                    out.push("{Namespace: \"predicate\", Name: \"and\"},".to_owned());
                }
                walk_report_policy_expr_atoms(term, out);
            }
            out.push("{Namespace: \"predicate\", Name: \")\"},".to_owned());
        }
        PolicyExpr::Or(terms) => {
            out.push("{Namespace: \"predicate\", Name: \"(\"},".to_owned());
            for (i, term) in terms.iter().enumerate() {
                if i > 0 {
                    out.push("{Namespace: \"predicate\", Name: \"or\"},".to_owned());
                }
                walk_report_policy_expr_atoms(term, out);
            }
            out.push("{Namespace: \"predicate\", Name: \")\"},".to_owned());
        }
        PolicyExpr::Not(inner) => {
            out.push("{Namespace: \"predicate\", Name: \"not\"},".to_owned());
            walk_report_policy_expr_atoms(inner, out);
        }
    }
}


#[cfg(test)]
mod tests;
