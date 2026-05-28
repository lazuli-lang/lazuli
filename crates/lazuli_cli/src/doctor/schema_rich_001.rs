//! Doctor rule `SCHEMA-RICH-001`.
//!
//! Generated command Zod schemas must not fall back to `z.unknown()` for
//! type axes the TS generator now knows how to lower precisely: enums and
//! the closed semantic string carriers handled by Wave A.4.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, QualifiedName, TypeRef};

/// One `SCHEMA-RICH-001` finding — a generated Zod schema slot that
/// silently fell back to `z.unknown()` despite a typed axis being
/// available.
pub struct Finding {
    /// Source `.gen.ts` path where the `z.unknown()` slot appears.
    pub path: PathBuf,
    /// 1-based line number of the offending slot.
    pub line: usize,
    /// Pre-rendered diagnostic message.
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "SCHEMA-RICH-001";
}

/// Walk every feature's command Zod schemas under
/// `<project_root>/dist/ts-*` and emit a `Finding` for each slot that
/// fell back to `z.unknown()` despite a typed axis being available.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::doctor::schema_rich_001::check;
///
/// // let findings = check(Path::new("."));
/// ```
pub fn check(project_root: &Path) -> Vec<Finding> {
    let Ok(module) = crate::build_module_from_path(project_root) else {
        return Vec::new();
    };

    let mut findings = Vec::new();
    for feature in &module.features {
        if feature.commands.is_empty() {
            continue;
        }

        let generated_files = zod_file_candidates(project_root, &feature.name);
        for path in generated_files {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };

            let feature_pascal = crate::pascal_case(&feature.name);
            for command in &feature.commands {
                let schema_ident = crate::command_schema_ident(&command.name, &feature_pascal);
                for slot in crate::command_zod_slots(feature, command, &module) {
                    let Some(type_label) = rich_schema_type_label(&slot.type_ref, &module) else {
                        continue;
                    };
                    let slot_name = lazuli_codegen_ts::lower_camel_export(&slot.name);
                    let Some(line) = schema_slot_unknown_line(&source, &schema_ident, &slot_name)
                    else {
                        continue;
                    };

                    findings.push(Finding {
                        path: path.clone(),
                        line,
                        message: format!(
                            "generated Zod schema `{schema_ident}` slot `{slot_name}` for `{type_label}` still emits `z.unknown()`; regenerate with rich schema codegen or fix `zod_base_for_type_ref`."
                        ),
                    });
                }
            }
        }
    }

    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.message.cmp(&right.message))
    });
    findings.dedup_by(|left, right| {
        left.path == right.path && left.line == right.line && left.message == right.message
    });
    findings
}

fn zod_file_candidates(project_root: &Path, feature_name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for target in ["ts-web", "ts-mobile"] {
        let dir = project_root.join("dist").join(target).join(feature_name);
        out.push(dir.join(format!("{feature_name}.zod.ts")));
        out.push(dir.join(format!("{feature_name}.zod.gen.ts")));
    }
    out
}

fn rich_schema_type_label(type_ref: &TypeRef, module: &lazuli_ir::Module) -> Option<String> {
    match type_ref {
        TypeRef::Many(inner) => {
            rich_schema_type_label(inner, module).map(|label| format!("list<{label}>"))
        }
        TypeRef::EnumRef(name) => Some(format!("enum {}", qualified_type_label(name))),
        TypeRef::UserDefined(name) if crate::find_enum_decl(module, name).is_some() => {
            Some(format!("enum {}", qualified_type_label(name)))
        }
        TypeRef::Unresolved(raw) if !raw.starts_with('@') => {
            let synthetic = QualifiedName {
                feature: None,
                name: raw.clone(),
            };
            crate::find_enum_decl(module, &synthetic).map(|_| format!("enum {raw}"))
        }
        TypeRef::Builtin(builtin) => rich_schema_builtin_label(builtin),
        _ => None,
    }
}

fn rich_schema_builtin_label(builtin: &BuiltinType) -> Option<String> {
    match builtin {
        BuiltinType::SemanticEmail => Some("@semantic.Email".to_owned()),
        BuiltinType::SemanticPhone => Some("@semantic.Phone".to_owned()),
        BuiltinType::SemanticUuid => Some("@semantic.Uuid".to_owned()),
        BuiltinType::SemanticUrl => Some("@semantic.Url".to_owned()),
        BuiltinType::SemanticCurrency => Some("@semantic.Currency".to_owned()),
        BuiltinType::SemanticHexColor => Some("@semantic.HexColor".to_owned()),
        BuiltinType::SemanticPercentage => Some("@semantic.Percentage".to_owned()),
        BuiltinType::SemanticMoney { currency } => Some(format!("@semantic.Money({currency:?})")),
        BuiltinType::SemanticPluginType { name, .. } => Some(format!("@semantic.{name}")),
        _ => None,
    }
}

fn schema_slot_unknown_line(source: &str, schema_ident: &str, slot_name: &str) -> Option<usize> {
    let schema_start = format!("export const {schema_ident} = z.object({{");
    let slot_prefix = format!("{slot_name}:");
    let mut in_schema = false;

    for (idx, line) in source.lines().enumerate() {
        if !in_schema {
            if line.trim_start().starts_with(&schema_start) {
                in_schema = true;
            }
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with("});") {
            in_schema = false;
            continue;
        }
        if trimmed.starts_with(&slot_prefix) && trimmed.contains("z.unknown()") {
            return Some(idx + 1);
        }
    }

    None
}

fn qualified_type_label(name: &QualifiedName) -> String {
    match &name.feature {
        Some(feature) => format!("{feature}.{}", name.name),
        None => name.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn schema_rich_001_fires_for_generated_unknown_enum_slot() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "catalog.lzi",
            r#"
feature catalog
  domain
    enum Role
      admin
      viewer

  command set_role
    input
      role: Role required
    returns Text
"#,
        );
        write(
            dir.path(),
            "dist/ts-web/catalog/catalog.zod.ts",
            r#"import { z } from "zod";

export const setCatalogRoleInputSchema = z.object({
  role: z.unknown(),
});
"#,
        );

        let findings = check(dir.path());

        assert_eq!(findings.len(), 1, "findings: {}", findings.len());
        assert_eq!(findings[0].line, 4);
        assert!(findings[0].message.contains("enum Role"));
    }

    #[test]
    fn schema_rich_001_accepts_generated_enum_schema() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "catalog.lzi",
            r#"
feature catalog
  domain
    enum Role
      admin
      viewer

  command set_role
    input
      role: Role required
    returns Text
"#,
        );
        write(
            dir.path(),
            "dist/ts-web/catalog/catalog.zod.ts",
            r#"import { z } from "zod";

export const setCatalogRoleInputSchema = z.object({
  role: z.enum(["admin", "viewer"]),
});
"#,
        );

        assert!(check(dir.path()).is_empty());
    }
}
