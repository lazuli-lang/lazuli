//! `allow_registry` — the typed read API over `Module.doctor_allows` (spec 0028).
//!
//! Spec 0028 captures every `@doctor.allow(CODE, reason: "...")` node (and
//! lifted legacy `# doctor:allow` comment) into
//! [`lazuli_ir::Module::doctor_allows`]. This module is the canonical read seam
//! for rules that hold a parsed `&Module`: ask "is CODE waived (and why / at
//! which line)?" instead of re-grepping source.
//!
//! Rules that only hold a `path`/`source` string keep using the back-compat
//! bridge [`crate::allow_comment::source_contains_doctor_allow`], which honors
//! both waiver forms at the string level. This registry is the structured
//! upgrade path for consumers that already have the module in hand.
//!
//! Matching on `code` is case-insensitive (mirrors the legacy scanner).

use lazuli_ir::{DoctorAllowScope, Module};

/// True when any captured waiver in `module` waives `code` (any scope).
/// Case-insensitive on `code`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_ir::{DoctorAllow, DoctorAllowScope, Module};
/// use lazuli_doctor::allow_registry::module_allows;
///
/// let mut module = Module {
///     workspace: None, contracts: vec![], app: None, registry: None,
///     profiles: vec![], design: None, rbac: None, features: vec![],
///     doctor_allows: vec![DoctorAllow {
///         code: "LZI-FILE-SIZE-001".into(), reason: Some("gen".into()),
///         scope: DoctorAllowScope::File, legacy: false, span: None,
///     }],
/// };
/// assert!(module_allows(&module, "lzi-file-size-001"));
/// assert!(!module_allows(&module, "OTHER-001"));
/// ```
pub fn module_allows(module: &Module, code: &str) -> bool {
    module
        .doctor_allows
        .iter()
        .any(|w| w.code.eq_ignore_ascii_case(code))
}

/// The reason on the first waiver in `module` matching `code`, if any.
/// Case-insensitive on `code`.
pub fn module_allow_reason<'m>(module: &'m Module, code: &str) -> Option<&'m str> {
    module
        .doctor_allows
        .iter()
        .find(|w| w.code.eq_ignore_ascii_case(code))
        .and_then(|w| w.reason.as_deref())
}

/// True when `code` is waived at `line` — a `File`-scoped waiver covers ANY
/// line; a `Construct { line }` waiver covers only its own line.
/// Case-insensitive on `code`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_ir::{DoctorAllow, DoctorAllowScope, Module};
/// use lazuli_doctor::allow_registry::module_allows_at;
///
/// let module = Module {
///     workspace: None, contracts: vec![], app: None, registry: None,
///     profiles: vec![], design: None, rbac: None, features: vec![],
///     doctor_allows: vec![DoctorAllow {
///         code: "X-1".into(), reason: None,
///         scope: DoctorAllowScope::Construct { line: 12 },
///         legacy: false, span: None,
///     }],
/// };
/// assert!(module_allows_at(&module, "X-1", 12));
/// assert!(!module_allows_at(&module, "X-1", 13));
/// ```
pub fn module_allows_at(module: &Module, code: &str, line: usize) -> bool {
    module.doctor_allows.iter().any(|w| {
        w.code.eq_ignore_ascii_case(code)
            && match w.scope {
                DoctorAllowScope::File => true,
                DoctorAllowScope::Construct { line: l } => l == line,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{DoctorAllow, DoctorAllowScope};

    fn module_with(allows: Vec<DoctorAllow>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            doctor_allows: allows,
            features: vec![],
        }
    }

    #[test]
    fn module_allows_matches_code_case_insensitive() {
        let m = module_with(vec![DoctorAllow {
            code: "LZI-FILE-SIZE-001".into(),
            reason: None,
            scope: DoctorAllowScope::File,
            legacy: false,
            span: None,
        }]);
        assert!(module_allows(&m, "lzi-file-size-001"));
        assert!(module_allows(&m, "LZI-FILE-SIZE-001"));
        assert!(!module_allows(&m, "NOPE-001"));
    }

    #[test]
    fn reason_returns_first_match() {
        let m = module_with(vec![DoctorAllow {
            code: "X-1".into(),
            reason: Some("because".into()),
            scope: DoctorAllowScope::File,
            legacy: false,
            span: None,
        }]);
        assert_eq!(module_allow_reason(&m, "X-1"), Some("because"));
        assert_eq!(module_allow_reason(&m, "Y-2"), None);
    }

    #[test]
    fn file_scope_covers_any_line() {
        let m = module_with(vec![DoctorAllow {
            code: "X-1".into(),
            reason: None,
            scope: DoctorAllowScope::File,
            legacy: false,
            span: None,
        }]);
        assert!(module_allows_at(&m, "X-1", 1));
        assert!(module_allows_at(&m, "X-1", 999));
    }

    #[test]
    fn construct_scope_is_line_keyed() {
        let m = module_with(vec![DoctorAllow {
            code: "X-1".into(),
            reason: None,
            scope: DoctorAllowScope::Construct { line: 12 },
            legacy: false,
            span: None,
        }]);
        assert!(module_allows_at(&m, "X-1", 12));
        assert!(!module_allows_at(&m, "X-1", 11));
    }
}
