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

pub mod design;
pub mod lifecycle_gate_emit;
pub mod lzx;
pub mod lzx_route_params;
pub mod lzx_audience_slot;
pub mod mobile_runtime;
pub mod mobile_view_scaffold;
pub mod playwright;
pub mod rbac;
pub mod runtime;
pub mod zod_constraints;

pub use playwright::emit_playwright_api_policy;
pub use runtime::{emit_feature_ts, lower_camel_export};
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
            contents: generate_package_json(&display_name),
        },
        GeneratedFile {
            path: "frontend/index.html".to_owned(),
            contents: generate_index_html(&display_name),
        },
        GeneratedFile {
            path: "frontend/tsconfig.json".to_owned(),
            contents: generate_tsconfig(),
        },
        GeneratedFile {
            path: "frontend/vite.config.ts".to_owned(),
            contents: generate_vite_config(),
        },
        GeneratedFile {
            path: "frontend/src/main.tsx".to_owned(),
            contents: generate_main_tsx(),
        },
        GeneratedFile {
            path: "frontend/src/App.tsx".to_owned(),
            contents: generate_app_tsx(),
        },
        GeneratedFile {
            path: "frontend/src/lazuli.generated.ts".to_owned(),
            contents: generate_schema_ts(module, &display_name),
        },
        GeneratedFile {
            path: "frontend/src/styles.css".to_owned(),
            contents: generate_styles_css(),
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

fn generate_package_json(display_name: &str) -> String {
    format!(
        r#"{{
  "name": "{}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {{
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  }},
  "dependencies": {{
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  }},
  "devDependencies": {{
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^5.0.0",
    "typescript": "^5.8.0",
    "vite": "^7.0.0"
  }}
}}
"#,
        to_kebab_case(display_name)
    )
}

fn generate_index_html(display_name: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{display_name}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#
    )
}

fn generate_tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["DOM", "DOM.Iterable", "ES2022"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Node",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src"],
  "references": []
}
"#
    .to_owned()
}

fn generate_vite_config() -> String {
    r#"import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': 'http://localhost:8080'
    }
  }
});
"#
    .to_owned()
}

fn generate_main_tsx() -> String {
    r#"import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
"#
    .to_owned()
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

fn generate_app_tsx() -> String {
    r#"import { useMemo, useState } from 'react';
import { lazuliDisplayName, lazuliModule, type LazuliResource } from './lazuli.generated';

const resources: LazuliResource[] = lazuliModule.features.flatMap((feature) => feature.resources);

function routeFor(resource: LazuliResource) {
  return `/api/${resource.name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase()}s`;
}

function ResourcePanel({ resource }: { resource: LazuliResource }) {
  const columns = resource.fields.map((field) => field.name);
  const formFields = resource.fields.map((field) => field.name);
  const requiredFields = useMemo(
    () => new Set(resource.fields.filter((field) => field.required).map((field) => field.name)),
    [resource]
  );

  return (
    <main className="workspace">
      <section className="toolbar">
        <div>
          <p className="eyebrow">Resource</p>
          <h1>{resource.name}</h1>
        </div>
        <code>{routeFor(resource)}</code>
      </section>

      <section className="grid">
        <div className="panel">
          <div className="panelHeader">
            <h2>List</h2>
            <span>{columns.length} columns</span>
          </div>
          <table>
            <thead>
              <tr>
                {columns.map((column) => (
                  <th key={column}>{column}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              <tr>
                {columns.map((column) => (
                  <td key={column}>sample</td>
                ))}
              </tr>
            </tbody>
          </table>
        </div>

        <form className="panel formPanel">
          <div className="panelHeader">
            <h2>Create</h2>
            <span>Draft</span>
          </div>
          {formFields.map((fieldName) => (
            <label key={fieldName}>
              <span>
                {fieldName}
                {requiredFields.has(fieldName) ? ' *' : ''}
              </span>
              <input placeholder={fieldName} />
            </label>
          ))}
          <button type="button">Create {resource.name}</button>
        </form>
      </section>
    </main>
  );
}

export default function App() {
  const [selected, setSelected] = useState(0);
  const resource = resources[selected] ?? resources[0];

  return (
    <div className="appShell">
      <aside>
        <div className="brand">
          <span>LZ</span>
          <strong>{lazuliDisplayName}</strong>
        </div>
        <nav>
          {resources.map((item, index) => (
            <button
              className={index === selected ? 'active' : ''}
              key={item.name}
              onClick={() => setSelected(index)}
              type="button"
            >
              {item.name}
            </button>
          ))}
        </nav>
      </aside>
      {resource ? <ResourcePanel resource={resource} /> : <main className="workspace">No resources generated.</main>}
    </div>
  );
}
"#
    .to_owned()
}

fn generate_styles_css() -> String {
    r#":root {
  color: #172026;
  background: #f6f7f2;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
}

button,
input {
  font: inherit;
}

.appShell {
  display: grid;
  min-height: 100vh;
  grid-template-columns: 248px 1fr;
}

aside {
  border-right: 1px solid #d7ddd8;
  background: #101820;
  color: #f8fbf8;
  padding: 24px 16px;
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 28px;
}

.brand span {
  display: grid;
  width: 36px;
  height: 36px;
  place-items: center;
  border-radius: 8px;
  background: #4db6ac;
  color: #071312;
  font-weight: 800;
}

nav {
  display: grid;
  gap: 6px;
}

nav button {
  width: 100%;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: #dbe7e3;
  cursor: pointer;
  padding: 10px 12px;
  text-align: left;
}

nav button.active,
nav button:hover {
  background: #23313b;
  color: white;
}

.workspace {
  padding: 32px;
}

.toolbar {
  display: flex;
  align-items: end;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 24px;
}

.eyebrow {
  margin: 0 0 4px;
  color: #567066;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0;
  text-transform: uppercase;
}

h1,
h2 {
  margin: 0;
}

h1 {
  font-size: 32px;
}

code {
  border: 1px solid #d7ddd8;
  border-radius: 8px;
  background: white;
  padding: 8px 10px;
}

.grid {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) minmax(320px, 0.6fr);
  gap: 20px;
}

.panel {
  border: 1px solid #d7ddd8;
  border-radius: 8px;
  background: white;
  padding: 18px;
}

.panelHeader {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
}

.panelHeader span {
  color: #5f7069;
  font-size: 14px;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  border-bottom: 1px solid #e7ebe7;
  padding: 12px 8px;
  text-align: left;
}

th {
  color: #567066;
  font-size: 13px;
}

.formPanel {
  display: grid;
  align-content: start;
  gap: 14px;
}

label {
  display: grid;
  gap: 6px;
}

label span {
  color: #3d5048;
  font-size: 14px;
  font-weight: 650;
}

input {
  width: 100%;
  border: 1px solid #cad3cd;
  border-radius: 8px;
  padding: 10px 12px;
}

.formPanel button {
  border: 0;
  border-radius: 8px;
  background: #172026;
  color: white;
  cursor: pointer;
  margin-top: 4px;
  padding: 11px 14px;
}

@media (max-width: 820px) {
  .appShell {
    grid-template-columns: 1fr;
  }

  aside {
    border-right: 0;
    padding: 16px;
  }

  nav {
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  }

  .workspace {
    padding: 20px;
  }

  .toolbar {
    align-items: start;
    flex-direction: column;
  }

  .grid {
    grid-template-columns: 1fr;
  }
}
"#
    .to_owned()
}

fn to_kebab_case(value: &str) -> String {
    if value.chars().any(|ch| ch.is_alphabetic())
        && value
            .chars()
            .all(|ch| !ch.is_lowercase() && ch != '_' && ch != '-' && ch != ' ')
    {
        return value.to_ascii_lowercase();
    }

    let mut output = String::new();

    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            output.push('-');
        } else if ch == '_' || ch == ' ' {
            output.push('-');
            continue;
        }

        output.push(ch.to_ascii_lowercase());
    }

    output
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
