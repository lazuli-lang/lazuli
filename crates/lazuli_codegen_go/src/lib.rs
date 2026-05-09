use lazuli_ir::{Application, Field, Resource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

pub fn generate(app: &Application) -> Vec<GeneratedFile> {
    vec![
        GeneratedFile {
            path: "backend/go.mod".to_owned(),
            contents: generate_go_mod(app),
        },
        GeneratedFile {
            path: "backend/main.go".to_owned(),
            contents: generate_main_go(app),
        },
        GeneratedFile {
            path: "backend/internal/lazuli/models.go".to_owned(),
            contents: generate_models_go(app),
        },
    ]
}

fn generate_go_mod(app: &Application) -> String {
    format!(
        "module {}\n\ngo 1.24\n",
        format!("lazuli/{}", to_kebab_case(&app.name))
    )
}

fn generate_main_go(app: &Application) -> String {
    let mut routes = String::new();
    let mut handlers = String::new();

    for resource in &app.resources {
        let route = format!("/api/{}", plural_route(&resource.name));
        let handler = format!("handle{}List", to_pascal_case(&resource.name));

        routes.push_str(&format!("\tmux.HandleFunc(\"{route}\", {handler})\n"));
        handlers.push_str(&generate_list_handler(resource, &handler));
    }

    format!(
        r#"package main

import (
	"encoding/json"
	"log/slog"
	"net/http"
)

func main() {{
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {{
		_ = json.NewEncoder(w).Encode(map[string]string{{"status": "ok"}})
	}})
{routes}
	addr := ":8080"
	slog.Info("lazuli backend listening", "addr", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {{
		panic(err)
	}}
}}

{handlers}"#
    )
}

fn generate_list_handler(resource: &Resource, handler: &str) -> String {
    let sample = resource
        .fields
        .iter()
        .map(|field| format!("\t\t\t\"{}\": {},\n", field.name, sample_go_value(field)))
        .collect::<String>();

    format!(
        r#"func {handler}(w http.ResponseWriter, r *http.Request) {{
	w.Header().Set("Content-Type", "application/json")
	if r.Method != http.MethodGet {{
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}}

	rows := []map[string]any{{
		{{
{sample}		}},
	}}

	_ = json.NewEncoder(w).Encode(rows)
}}

"#
    )
}

fn generate_models_go(app: &Application) -> String {
    let mut output = "package lazuli\n\n".to_owned();

    for resource in &app.resources {
        output.push_str(&format!(
            "type {} struct {{\n",
            to_pascal_case(&resource.name)
        ));
        output.push_str("\tID string `json:\"id\"`\n");

        for field in &resource.fields {
            output.push_str(&format!(
                "\t{} {} `json:\"{}\"`\n",
                to_pascal_case(&field.name),
                go_type(&field.kind),
                field.name
            ));
        }

        output.push_str("}\n\n");

        for command in &resource.commands {
            output.push_str(&format!(
                "type {}{}Input struct {{\n",
                to_pascal_case(&command.name),
                to_pascal_case(&resource.name)
            ));

            for input in &command.input {
                if let Some(field) = resource.fields.iter().find(|field| &field.name == input) {
                    output.push_str(&format!(
                        "\t{} {} `json:\"{}\"`\n",
                        to_pascal_case(&field.name),
                        go_type(&field.kind),
                        field.name
                    ));
                }
            }

            output.push_str("}\n\n");
        }
    }

    output
}

fn sample_go_value(field: &Field) -> &'static str {
    match field.kind.as_str() {
        "Boolean" | "Bool" => "true",
        "Int" | "Integer" => "1",
        "Float" | "Decimal" | "Money" => "1.0",
        _ => "\"sample\"",
    }
}

fn go_type(kind: &str) -> &'static str {
    match kind {
        "Boolean" | "Bool" => "bool",
        "Int" | "Integer" => "int",
        "Float" | "Decimal" | "Money" => "float64",
        _ => "string",
    }
}

fn plural_route(name: &str) -> String {
    let kebab = to_kebab_case(name);
    if kebab.ends_with('s') {
        kebab
    } else {
        format!("{kebab}s")
    }
}

fn to_pascal_case(value: &str) -> String {
    split_words(value)
        .into_iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect()
}

fn to_kebab_case(value: &str) -> String {
    if value.chars().any(|ch| ch.is_alphabetic())
        && value
            .chars()
            .all(|ch| !ch.is_lowercase() && ch != '_' && ch != '-' && ch != ' ')
    {
        return value.to_ascii_lowercase();
    }

    split_words(value).join("-").to_ascii_lowercase()
}

fn split_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            continue;
        }

        if ch.is_uppercase() && !current.is_empty() {
            words.push(current.clone());
            current.clear();
        }

        current.push(ch);
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

#[cfg(test)]
mod tests {
    use lazuli_analyzer::lower_document;
    use lazuli_syntax::parse_document;

    use super::generate;

    #[test]
    fn generates_go_backend_files() {
        let document = parse_document(include_str!("../../../examples/crm.lzi")).unwrap();
        let app = lower_document(&document).unwrap();
        let files = generate(&app);

        assert!(files.iter().any(|file| file.path == "backend/main.go"));
        assert!(
            files
                .iter()
                .any(|file| file.contents.contains("handleCustomerList"))
        );
    }
}
