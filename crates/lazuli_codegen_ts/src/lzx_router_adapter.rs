//! Router adapter — emits the per-target `useParams` import line for
//! `view detail` hooks. Lives next to the view emitters because every
//! detail view emission needs it.
//!
//! Per `docs/proposals/lzx-integration-codegen.md` §6.2 the import line
//! switches off the frontend `target` setting in `lazurite.toml`:
//!
//! | target       | Import emitted                                                     |
//! |--------------|--------------------------------------------------------------------|
//! | `vite-react` | `import { useParams } from "@tanstack/react-router";`              |
//! | `nextjs`     | `import { useParams } from "next/navigation";`                     |
//! | `expo`       | `import { useLocalSearchParams as useParams } from "expo-router";` |
//! | `tauri`      | (same as `vite-react`)                                             |
//!
//! `cli` is intentionally absent — headless CLI frontends do not support
//! `view detail` and the emission walker rejects that target upstream.

/// Frontend router target — mirrors `lazurite.toml [frontends.<x>] target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterTarget {
    /// Vite + React + TanStack Router (the canonical web target).
    ViteReact,
    /// Next.js App Router.
    NextJs,
    /// Expo Router (React Native).
    Expo,
    /// Tauri (desktop) — same router as `ViteReact`.
    Tauri,
}

/// Emit the single import line that puts a `useParams` hook in scope.
///
/// The returned slice ends with a newline; emitters can write it
/// verbatim into the import block.
pub fn router_useparams_import(target: RouterTarget) -> &'static str {
    match target {
        RouterTarget::ViteReact | RouterTarget::Tauri => {
            "import { useParams } from \"@tanstack/react-router\";\n"
        }
        RouterTarget::NextJs => "import { useParams } from \"next/navigation\";\n",
        RouterTarget::Expo => {
            "import { useLocalSearchParams as useParams } from \"expo-router\";\n"
        }
    }
}

/// Emit the import line for the target's search-param hook.
pub fn router_usesearchparams_import(target: RouterTarget) -> &'static str {
    match target {
        RouterTarget::ViteReact | RouterTarget::Tauri => {
            "import { useSearchParams } from \"@tanstack/react-router\";\n"
        }
        RouterTarget::NextJs => "import { useSearchParams } from \"next/navigation\";\n",
        RouterTarget::Expo => {
            "import { useLocalSearchParams as useSearchParams } from \"expo-router\";\n"
        }
    }
}

/// Translate a portable `.lzx` route path (Express-style `:param`) into
/// the syntax expected by the target router. Authors write `:key`; each
/// router consumes its own dialect.
///
/// | target       | `:param` becomes |
/// |--------------|------------------|
/// | `vite-react` | `$param` (TanStack Router) |
/// | `tauri`      | `$param` (TanStack Router) |
/// | `nextjs`     | `[param]` (App Router segment) |
/// | `expo`       | `[param]` (Expo Router segment) |
pub fn translate_route_path(target: RouterTarget, route: &str) -> String {
    let mut out = String::with_capacity(route.len());
    let bytes = route.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            if j > start {
                let name = &route[start..j];
                match target {
                    RouterTarget::ViteReact | RouterTarget::Tauri => {
                        out.push('$');
                        out.push_str(name);
                    }
                    RouterTarget::NextJs | RouterTarget::Expo => {
                        out.push('[');
                        out.push_str(name);
                        out.push(']');
                    }
                }
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vite_react_emits_tanstack_router() {
        assert_eq!(
            router_useparams_import(RouterTarget::ViteReact),
            "import { useParams } from \"@tanstack/react-router\";\n"
        );
    }

    #[test]
    fn tauri_matches_vite_react() {
        assert_eq!(
            router_useparams_import(RouterTarget::Tauri),
            router_useparams_import(RouterTarget::ViteReact)
        );
    }

    #[test]
    fn nextjs_emits_next_navigation() {
        assert_eq!(
            router_useparams_import(RouterTarget::NextJs),
            "import { useParams } from \"next/navigation\";\n"
        );
    }

    #[test]
    fn expo_emits_expo_router_alias() {
        assert_eq!(
            router_useparams_import(RouterTarget::Expo),
            "import { useLocalSearchParams as useParams } from \"expo-router\";\n"
        );
    }

    #[test]
    fn translate_route_path_tanstack_uses_dollar() {
        assert_eq!(
            translate_route_path(RouterTarget::ViteReact, "/slugs/:key"),
            "/slugs/$key"
        );
        assert_eq!(
            translate_route_path(RouterTarget::Tauri, "/orgs/:org_id/users/:user_id"),
            "/orgs/$org_id/users/$user_id"
        );
    }

    #[test]
    fn translate_route_path_next_and_expo_use_brackets() {
        assert_eq!(
            translate_route_path(RouterTarget::NextJs, "/slugs/:key"),
            "/slugs/[key]"
        );
        assert_eq!(
            translate_route_path(RouterTarget::Expo, "/users/:id"),
            "/users/[id]"
        );
    }

    #[test]
    fn translate_route_path_passes_through_static_segments() {
        for t in [
            RouterTarget::ViteReact,
            RouterTarget::Tauri,
            RouterTarget::NextJs,
            RouterTarget::Expo,
        ] {
            assert_eq!(translate_route_path(t, "/slugs/new"), "/slugs/new");
            assert_eq!(translate_route_path(t, "/"), "/");
        }
    }

    #[test]
    fn every_import_ends_with_newline() {
        for target in [
            RouterTarget::ViteReact,
            RouterTarget::NextJs,
            RouterTarget::Expo,
            RouterTarget::Tauri,
        ] {
            assert!(router_useparams_import(target).ends_with('\n'));
        }
    }
}
