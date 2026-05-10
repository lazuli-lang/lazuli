pub mod runtime;

pub use runtime::emit_feature_go;

use lazuli_ir::{BuiltinType, CommandInput, Field, Module, Resource, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

pub fn generate(module: &Module) -> Vec<GeneratedFile> {
    let module_name = module
        .features
        .first()
        .map(|feature| feature.name.clone())
        .unwrap_or_else(|| "lazuli_app".to_owned());

    vec![
        GeneratedFile {
            path: "backend/go.mod".to_owned(),
            contents: generate_go_mod(&module_name),
        },
        GeneratedFile {
            path: "backend/main.go".to_owned(),
            contents: generate_main_go(module),
        },
        GeneratedFile {
            path: "backend/internal/lazuli/models.go".to_owned(),
            contents: generate_models_go(module),
        },
    ]
}

fn generate_go_mod(module_name: &str) -> String {
    format!(
        "module lazuli/{module}\n\ngo 1.24\n",
        module = to_kebab_case(module_name)
    )
}

fn generate_main_go(module: &Module) -> String {
    let mut routes = String::new();
    let mut handlers = String::new();

    for resource in iter_resources(module) {
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

fn generate_models_go(module: &Module) -> String {
    let mut output = "package lazuli\n\n".to_owned();

    for feature in &module.features {
        for resource in &feature.resources {
            output.push_str(&format!(
                "type {} struct {{\n",
                to_pascal_case(&resource.name)
            ));
            output.push_str("\tID string `json:\"id\"`\n");

            for field in &resource.fields {
                output.push_str(&format!(
                    "\t{} {} `json:\"{}\"`\n",
                    to_pascal_case(&field.name),
                    go_type(&field.type_ref),
                    field.name
                ));
            }

            output.push_str("}\n\n");

            for command in &feature.commands {
                if !command_targets_resource(command, &resource.name) {
                    continue;
                }

                output.push_str(&format!(
                    "type {}{}Input struct {{\n",
                    to_pascal_case(&command.name),
                    to_pascal_case(&resource.name)
                ));

                if let CommandInput::Short(inputs) = &command.input {
                    for input in inputs {
                        if let Some(field) =
                            resource.fields.iter().find(|field| &field.name == input)
                        {
                            output.push_str(&format!(
                                "\t{} {} `json:\"{}\"`\n",
                                to_pascal_case(&field.name),
                                go_type(&field.type_ref),
                                field.name
                            ));
                        }
                    }
                }

                output.push_str("}\n\n");
            }
        }
    }

    output
}

fn command_targets_resource(command: &lazuli_ir::Command, resource_name: &str) -> bool {
    use lazuli_ir::CommandEffect::*;
    match &command.effect {
        Creates(effect) => effect.resource.name == resource_name,
        Updates(effect) => effect.resource.name == resource_name,
        Deletes(effect) => effect.resource.name == resource_name,
        Returns(_) | None => false,
    }
}

fn iter_resources(module: &Module) -> impl Iterator<Item = &Resource> {
    module
        .features
        .iter()
        .flat_map(|feature| feature.resources.iter())
}

fn sample_go_value(field: &Field) -> &'static str {
    match &field.type_ref {
        TypeRef::Builtin(BuiltinType::Boolean) => "true",
        TypeRef::Builtin(BuiltinType::Integer) => "1",
        TypeRef::Builtin(BuiltinType::Decimal) => "1.0",
        _ => "\"sample\"",
    }
}

fn go_type(type_ref: &TypeRef) -> &'static str {
    match type_ref {
        TypeRef::Builtin(BuiltinType::Boolean) => "bool",
        TypeRef::Builtin(BuiltinType::Integer) => "int",
        TypeRef::Builtin(BuiltinType::Decimal) => "float64",
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
        let module = lower_document(&document).unwrap();
        let files = generate(&module);

        assert!(files.iter().any(|file| file.path == "backend/main.go"));
        assert!(
            files
                .iter()
                .any(|file| file.contents.contains("handleCustomerList"))
        );
    }
}
