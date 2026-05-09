use lazuli_ir::Application;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

pub fn generate(app: &Application) -> Vec<GeneratedFile> {
    vec![
        GeneratedFile {
            path: "frontend/package.json".to_owned(),
            contents: generate_package_json(app),
        },
        GeneratedFile {
            path: "frontend/index.html".to_owned(),
            contents: generate_index_html(app),
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
            contents: generate_app_tsx(app),
        },
        GeneratedFile {
            path: "frontend/src/lazuli.generated.ts".to_owned(),
            contents: generate_schema_ts(app),
        },
        GeneratedFile {
            path: "frontend/src/styles.css".to_owned(),
            contents: generate_styles_css(),
        },
    ]
}

fn generate_package_json(app: &Application) -> String {
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
        to_kebab_case(&app.name)
    )
}

fn generate_index_html(app: &Application) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#,
        app.name
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

fn generate_schema_ts(app: &Application) -> String {
    let json = serde_json::to_string_pretty(app).expect("IR must serialize");

    format!(
        r#"export type LazuliField = {{
  name: string;
  kind: string;
  required: boolean;
  unique: boolean;
  default: string | null;
}};

export type LazuliResource = {{
  name: string;
  fields: LazuliField[];
  commands: Array<{{
    name: string;
    input: string[];
    policy: string | null;
    emits: string[];
  }}>;
  queries: Array<{{
    name: string;
    search: string[];
    filters: string[];
  }}>;
  surfaces: Array<{{
    name: string;
    list_columns: string[];
    form_fields: string[];
    detail_fields: string[];
  }}>;
}};

export type LazuliApplication = {{
  name: string;
  resources: LazuliResource[];
}};

export const lazuliApp = {json} satisfies LazuliApplication;
"#
    )
}

fn generate_app_tsx(_app: &Application) -> String {
    r#"import { useMemo, useState } from 'react';
import { LazuliResource, lazuliApp } from './lazuli.generated';

function routeFor(resource: LazuliResource) {
  return `/api/${resource.name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase()}s`;
}

function ResourcePanel({ resource }: { resource: LazuliResource }) {
  const surface = resource.surfaces[0];
  const columns = surface?.list_columns.length ? surface.list_columns : resource.fields.map((field) => field.name);
  const formFields = surface?.form_fields.length ? surface.form_fields : resource.fields.map((field) => field.name);
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
            <span>{resource.commands[0]?.name ?? 'Draft'}</span>
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
  const resource = lazuliApp.resources[selected] ?? lazuliApp.resources[0];

  return (
    <div className="appShell">
      <aside>
        <div className="brand">
          <span>LZ</span>
          <strong>{lazuliApp.name}</strong>
        </div>
        <nav>
          {lazuliApp.resources.map((item, index) => (
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
    use lazuli_analyzer::lower_document;
    use lazuli_syntax::parse_document;

    use super::generate;

    #[test]
    fn generates_react_frontend_files() {
        let document = parse_document(include_str!("../../../examples/crm.lzi")).unwrap();
        let app = lower_document(&document).unwrap();
        let files = generate(&app);

        assert!(files.iter().any(|file| file.path == "frontend/src/App.tsx"));
        assert!(files.iter().any(|file| file.contents.contains("lazuliApp")));
    }
}
