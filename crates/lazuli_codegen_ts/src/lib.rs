//! TypeScript code generation for Lazuli.
//!
//! ## Cut A acknowledgment (acknowledged, not yet generated)
//!
//! Cut A introduces `Agent` / `ToolBinding` / `EvalCase` to the IR
//! (see `lazuli_ir::Agent`, schema bump to `0.4.0`). The TS client
//! codegen does not yet emit agent bindings; a separate runtime phase
//! lands them. When agent codegen arrives here, the generated TS
//! surface materialises:
//!
//!   - typed client method per agent with input/output narrowed by
//!     `output_kind` (text vs. stream vs. discriminated_{enum,record})
//!   - tool-result typing pulled from `RegistryToolEntry` once Cut A.6
//!     ships `tools.<x>.<field>` references
//!   - a tagged-union response type for discriminated outputs so
//!     consumers branch statically
//!
//! Plan reference: `docs/proposals/ai-primitives-v0-implementation.md`
//! §9.1. Runtime team: `docs/runtime-handoff.md`.

pub mod cap_file_hooks;
pub mod design;
pub mod lifecycle_gate_emit;
pub mod lzx;
pub mod lzx_route_params;
pub mod lzx_audience_slot;
pub mod mobile_runtime;
pub mod mobile_view_scaffold;
pub mod playwright;
pub mod plural;
pub mod preflight;
pub mod rbac;
pub mod routes;
pub mod runtime;
mod scaffold;
pub mod semantic_formatters;
pub mod zod_constraints;

pub use cap_file_hooks::emit_cap_file_hooks_ts;
pub use playwright::emit_playwright_api_policy;
pub use plural::pluralize;
pub use preflight::{emit_preflight_index_ts, emit_preflight_ts};
pub use runtime::{emit_feature_ts, emit_lifecycle_route_helpers_ts, lower_camel_export};
pub use semantic_formatters::emit_semantic_formatters_ts;
pub use zod_constraints::{is_numeric, zod_constraint_chain, zod_enum_replacement};

use lazuli_ir::Module;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

pub fn generate(module: &Module) -> Vec<GeneratedFile> {
    let display_name = display_name(module);

    let mut files = vec![
        GeneratedFile {
            path: "frontend/package.json".to_owned(),
            contents: scaffold::generate_package_json(&display_name),
        },
        GeneratedFile {
            path: "frontend/index.html".to_owned(),
            contents: scaffold::generate_index_html(&display_name),
        },
        GeneratedFile {
            path: "frontend/tsconfig.json".to_owned(),
            contents: scaffold::generate_tsconfig(),
        },
        GeneratedFile {
            path: "frontend/vite.config.ts".to_owned(),
            contents: scaffold::generate_vite_config(),
        },
        GeneratedFile {
            path: "frontend/src/main.tsx".to_owned(),
            contents: scaffold::generate_main_tsx(),
        },
        GeneratedFile {
            path: "frontend/src/App.tsx".to_owned(),
            contents: scaffold::generate_app_tsx(),
        },
        GeneratedFile {
            path: "frontend/src/lazuli.generated.ts".to_owned(),
            contents: generate_schema_ts(module, &display_name),
        },
        GeneratedFile {
            path: "frontend/src/styles.css".to_owned(),
            contents: scaffold::generate_styles_css(),
        },
    ];

    if let Some(design) = &module.design {
        files.extend([
            GeneratedFile {
                path: "dist/ts-web/design/tokens.ts".to_owned(),
                contents: design::emit_tokens_ts(design),
            },
            GeneratedFile {
                path: "dist/ts-web/design/tokens.css".to_owned(),
                contents: design::emit_tokens_css(design),
            },
            GeneratedFile {
                path: "dist/ts-web/design/tailwind.gen.ts".to_owned(),
                contents: design::emit_tailwind_v3_preset(design),
            },
            GeneratedFile {
                path: "dist/ts-web/design/tailwind.theme.css".to_owned(),
                contents: design::emit_tailwind_v4_theme(design),
            },
            GeneratedFile {
                path: "dist/ts-web/design/allowlist.json".to_owned(),
                contents: design::emit_allowlist_json(design),
            },
        ]);
    }

    files.extend(lzx_audience_slot::emit_route_guard_artifacts(
        module.app.as_ref(),
        &[],
        &[],
        &[],
        &module.features,
        lzx_audience_slot::RouteGuardTarget::Web,
    ));
    files.extend(lifecycle_gate_emit::emit_lifecycle_gate_artifacts(
        module,
        lifecycle_gate_emit::LifecycleGateTarget::Web,
        lifecycle_gate_emit::LifecycleGateIntegration::TanStack,
    ));

    // RB.C — emit `dist/ts-web/rbac/rbac.gen.ts` when the package
    // declares a `permission` / `role` catalog. Audience-scoping is
    // deferred per docs/proposals/rbac-catalog-vocab.md §Codegen-TS;
    // v0.1 ships the full catalog to every frontend.
    if let Some(contents) = rbac::emit_rbac_ts(module) {
        files.push(GeneratedFile {
            path: "dist/ts-web/rbac/rbac.gen.ts".to_owned(),
            contents,
        });
    }

    files
}

fn display_name(module: &Module) -> String {
    module
        .features
        .first()
        .map(|feature| feature.name.clone())
        .unwrap_or_else(|| "lazuli_app".to_owned())
}

fn generate_schema_ts(module: &Module, display_name: &str) -> String {
    let json = serde_json::to_string_pretty(module).expect("IR must serialize");

    format!(
        r#"// Generated from the Lazuli IR. Read-only — regenerate via `lazuli generate ts`.
// The IR shape is the public contract; see docs/ir-abi.md.

export type LazuliModule = {{
  features: LazuliFeature[];
}};

export type LazuliFeature = {{
  name: string;
  purpose: string | null;
  uses: string[];
  enums: unknown[];
  resources: LazuliResource[];
  commands: LazuliCommand[];
  queries: unknown[];
}};

export type LazuliResource = {{
  name: string;
  fields: LazuliField[];
}};

export type LazuliField = {{
  name: string;
  type_ref: unknown;
  required: boolean;
  unique: boolean;
  default?: unknown;
}};

export type LazuliCommand = {{
  name: string;
  kind: 'Create' | 'Update' | 'Delete' | 'Returns';
  input: unknown;
  effect: unknown;
  policy: unknown;
  emits: string[];
}};

export const lazuliDisplayName = '{display_name}' as const;
export const lazuliModule = {json} as const;
"#
    )
}

#[cfg(test)]
mod tests {
    use lazuli_ir::{
        ColorState, ColorStateKind, ColorToken, Design, Module, Motion, ScaleToken, Typography,
    };

    use super::generate;

    fn minimal_design() -> Design {
        Design {
            name: "example".to_owned(),
            extends: None,
            colors: vec![ColorToken {
                name: "primary".to_owned(),
                states: vec![ColorState {
                    kind: ColorStateKind::Base,
                    value: "#7c3aed".to_owned(),
                    dark: None,
                }],
                span_ref: None,
            }],
            typography: Typography::default(),
            spaces: vec![ScaleToken {
                name: "4".to_owned(),
                value: "1rem".to_owned(),
            }],
            radii: vec![],
            shadows: vec![],
            motion: Motion::default(),
            breakpoints: vec![],
            z_indices: vec![],
            custom: vec![],
            span_ref: None,
        }
    }

    fn module_with_design(design: Option<Design>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design,
            rbac: None,
            features: vec![],
        }
    }

    #[test]
    fn generate_emits_design_files_when_module_has_design() {
        let module = module_with_design(Some(minimal_design()));
        let files = generate(&module);
        let design_files: Vec<_> = files
            .iter()
            .filter(|file| file.path.starts_with("dist/ts-web/design/"))
            .collect();

        assert!(design_files.len() >= 5);
        assert!(
            files
                .iter()
                .any(|file| file.path == "dist/ts-web/design/tokens.ts")
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "dist/ts-web/design/tokens.css")
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "dist/ts-web/design/tailwind.gen.ts")
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "dist/ts-web/design/tailwind.theme.css")
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "dist/ts-web/design/allowlist.json")
        );
    }

    #[test]
    fn generate_skips_design_files_when_module_design_is_none() {
        let module = module_with_design(None);
        let files = generate(&module);

        assert!(
            !files
                .iter()
                .any(|file| file.path.starts_with("dist/ts-web/design/"))
        );
    }
}
