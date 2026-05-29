//! Doctor rule `pages-bypass`.
//!
//! Routing in Lazurite comes from `.lzx` `view ... at "<route>"`
//! declarations, lowered to `dist/ts-web/<audience>/routes.gen.tsx`. Filesystem
//! routing (Next.js pages/, app/(routes)/) bypasses the
//! audience-scoping and typed-route-param contracts. This rule fires
//! when filesystem routing is detected at the project root.
//!
//! Detection: presence of `pages/` directory OR `app/(<group>)/`
//! pattern (App Router route group syntax) at project root. The
//! canonical Lazurite `app/` product-kernel subdirs (`features`) and
//! frontend adapter subdirs (`frontends/<target>/shell`) are NOT route
//! groups and don't fire.

use std::fs;
use std::path::{Path, PathBuf};

/// One `pages-bypass` finding — a filesystem path that bypasses the
/// Lazurite typed-routing contract.
pub struct Finding {
    /// Suspect directory.
    pub path: PathBuf,
    /// Pre-rendered diagnostic message.
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "pages-bypass";

    /// Render the diagnostic for the suspect `path` and the matched
    /// bypass style.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::Path;
    /// // let msg = Finding::message(Path::new("pages"), kind);
    /// ```
    pub fn message(path: &Path, kind: BypassKind) -> String {
        let style = match kind {
            BypassKind::PagesDir => "Next.js Pages Router (`pages/` directory)",
            BypassKind::AppRouteGroup => "Next.js App Router route group (`app/(...)/`)",
        };
        format!(
            "`{}` looks like {}. Lazurite routing comes from `.lzx` \
             `view ... at \"<route>\"` declarations + emitted \
             `dist/ts-web/<audience>/routes.gen.tsx`. Move route definitions to \
             `<feature>.web.lzx`.",
            path.display(),
            style
        )
    }
}

/// Style of routing bypass detected at the project root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassKind {
    /// Next.js Pages Router — `pages/` directory at the project root.
    PagesDir,
    /// Next.js App Router route group — `app/(<group>)/` directory.
    AppRouteGroup,
}

/// Inspect `root` for routing-bypass markers. Returns a Finding per
/// suspect path (the `pages/` dir itself, OR each `app/(<group>)/` dir).
///
/// Does NOT recurse into the suspect dir — one Finding per bypass kind
/// per location keeps output noise-free.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::doctor::folder::pages_bypass::check;
///
/// // let findings = check(Path::new("."));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    let pages_dir = root.join("pages");
    if pages_dir.is_dir() && contains_ts_or_tsx_file(&pages_dir) {
        findings.push(finding(pages_dir, BypassKind::PagesDir));
    }

    let app_dir = root.join("app");
    let Ok(entries) = fs::read_dir(&app_dir) else {
        return findings;
    };

    let mut route_groups: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_route_group_name)
        })
        .collect();
    route_groups.sort();

    findings.extend(
        route_groups
            .into_iter()
            .map(|path| finding(path, BypassKind::AppRouteGroup)),
    );

    findings
}

fn finding(path: PathBuf, kind: BypassKind) -> Finding {
    Finding {
        message: Finding::message(&path, kind),
        path,
    }
}

fn contains_ts_or_tsx_file(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .any(|path| {
            path.is_file()
                && matches!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("ts" | "tsx")
                )
        })
}

fn is_route_group_name(name: &str) -> bool {
    name.len() > 2 && name.starts_with('(') && name.ends_with(')')
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use tempfile::TempDir;

    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap();
    }

    #[test]
    fn pages_dir_with_tsx_fires() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("pages/index.tsx"));

        let findings = check(temp.path());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, temp.path().join("pages"));
        assert_eq!(
            findings[0].message,
            Finding::message(&findings[0].path, BypassKind::PagesDir)
        );
        assert_eq!(Finding::CODE, "pages-bypass");
    }

    #[test]
    fn pages_dir_empty_does_not_fire() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("pages/.gitkeep"));

        let findings = check(temp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn app_route_group_fires() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("app/(auth)/login.tsx"));

        let findings = check(temp.path());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, temp.path().join("app/(auth)"));
        assert_eq!(
            findings[0].message,
            Finding::message(&findings[0].path, BypassKind::AppRouteGroup)
        );
    }

    #[test]
    fn app_shell_does_not_fire() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("frontends/web/shell/root.tsx"));

        let findings = check(temp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn app_ui_does_not_fire() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("frontends/web/ui/button.tsx"));

        let findings = check(temp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_app_route_groups_fire_separately() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("app/(dashboard)/page.tsx"));
        touch(&temp.path().join("app/(auth)/login.tsx"));

        let findings = check(temp.path());

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].path, temp.path().join("app/(auth)"));
        assert_eq!(findings[1].path, temp.path().join("app/(dashboard)"));
    }

    #[test]
    fn no_app_dir_does_not_fire() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("features/account/account.lzi"));

        let findings = check(temp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn deterministic_order() {
        let temp = TempDir::new().unwrap();
        touch(&temp.path().join("pages/index.tsx"));
        touch(&temp.path().join("app/(dashboard)/page.tsx"));
        touch(&temp.path().join("app/(auth)/login.tsx"));

        let first: Vec<PathBuf> = check(temp.path()).into_iter().map(|f| f.path).collect();
        let second: Vec<PathBuf> = check(temp.path()).into_iter().map(|f| f.path).collect();

        assert_eq!(first, second);
        assert_eq!(
            first,
            vec![
                temp.path().join("pages"),
                temp.path().join("app/(auth)"),
                temp.path().join("app/(dashboard)"),
            ]
        );
    }
}
