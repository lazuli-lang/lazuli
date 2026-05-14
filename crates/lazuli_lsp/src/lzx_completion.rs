//! LSP completion provider for `.lzx` v2 grammar.
//!
//! Indent-aware context detection + closed-catalog snippets for the
//! ViewModel primitives (cells/drawer/filters/search/sort/selection/
//! bulk_actions/settings). Best-effort line walker: this does not parse
//! the full grammar.

use std::collections::BTreeSet;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position,
};

pub fn completions_for_lzx(source: &str, position: Position) -> Vec<CompletionItem> {
    let ctx = detect_context(source, position.line as usize);
    match ctx {
        LzxContext::TopLevel => top_level_completions(),
        LzxContext::InViewList => view_list_completions(),
        LzxContext::InFilters => filter_completions(source),
        LzxContext::InSearchSegmented => search_segmented_completions(source),
        LzxContext::InSelection => selection_completions(),
        LzxContext::Other => vec![],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LzxContext {
    TopLevel,
    InViewList,
    InFilters,
    InSearchSegmented,
    InSelection,
    Other,
}

fn detect_context(source: &str, line: usize) -> LzxContext {
    if source.trim().is_empty() {
        return LzxContext::TopLevel;
    }

    let lines: Vec<_> = source.split('\n').collect();
    let Some(current_line) = lines.get(line).copied() else {
        return LzxContext::TopLevel;
    };
    let current_indent = leading_spaces(current_line);
    let current_trimmed = current_line.trim();

    if current_indent == 0 && current_trimmed.is_empty() {
        return LzxContext::TopLevel;
    }
    if current_indent == 0 && starts_lzx_container(current_trimmed) {
        return LzxContext::TopLevel;
    }
    if current_indent == 0 && !current_trimmed.is_empty() && !starts_known_root(current_trimmed) {
        return LzxContext::Other;
    }

    let mut enclosing = Vec::new();
    for (idx, candidate) in lines.iter().take(line + 1).enumerate() {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = leading_spaces(candidate);
        if idx == line && indent >= current_indent {
            continue;
        }
        while matches!(enclosing.last(), Some((prev_indent, _)) if *prev_indent >= indent) {
            enclosing.pop();
        }
        enclosing.push((indent, trimmed));
    }

    let in_view_list = enclosing
        .iter()
        .rev()
        .any(|(_, trimmed)| trimmed.starts_with("view list "));
    if !in_view_list {
        return if current_indent == 0 {
            LzxContext::TopLevel
        } else {
            LzxContext::Other
        };
    }

    for (_, trimmed) in enclosing.iter().rev() {
        if *trimmed == "filters" {
            return LzxContext::InFilters;
        }
        if *trimmed == "search segmented" {
            return LzxContext::InSearchSegmented;
        }
        if trimmed.starts_with("selection ") {
            return LzxContext::InSelection;
        }
        if trimmed.starts_with("view list ") {
            return LzxContext::InViewList;
        }
    }

    LzxContext::TopLevel
}

fn top_level_completions() -> Vec<CompletionItem> {
    vec![
        snippet(
            "view list <name>",
            "view list ${1:list_name}\n  source ${2:query.list}\n  columns [${3}]\n  $0",
            "Declare a list ViewModel.",
        ),
        snippet(
            "view detail <name>",
            "view detail ${1:detail_name}\n  source ${2:query.lookup}\n  cells\n    $0",
            "Declare a detail ViewModel.",
        ),
        snippet(
            "view create <name>",
            "view create ${1:create_name}\n  command ${2:command_name}\n  inputs\n    $0",
            "Declare a create ViewModel.",
        ),
    ]
}

fn view_list_completions() -> Vec<CompletionItem> {
    vec![
        snippet(
            "cells <field> @client.<slot>",
            "cells ${1:field} @client.${2:slot}",
            "Bind a list field to a client-rendered cell slot.",
        ),
        snippet(
            "columns [<a>, <b>]",
            "columns [${1}]",
            "Declare table columns for the list view.",
        ),
        snippet(
            "drawer <name>",
            "drawer ${1:detail}\n  source ${2:query.lookup}\n  cells\n    $0",
            "Declare an in-place drawer sub-view.",
        ),
        snippet("filters", "filters\n  $0", "Declare typed list filters."),
        snippet(
            "search <mode>",
            "search ${1|columns,segmented|}\n  $0",
            "Declare list search behavior.",
        ),
        snippet(
            "sort",
            "sort\n  default by ${1:created_at} desc\n  options ${2:[]}",
            "Declare list sort defaults and options.",
        ),
        snippet(
            "selection <mode>",
            "selection ${1|single,multi|}\n  $0",
            "Declare list selection state.",
        ),
        snippet(
            "settings ... persist <where>",
            "settings ${1:name}\n  default ${2}\n  persist ${3|local,workspace,none|}",
            "Declare persisted view settings.",
        ),
    ]
}

fn filter_completions(source: &str) -> Vec<CompletionItem> {
    let mut items = vec![
        snippet(
            "<name> Text",
            "${1:name} ${2:Text}",
            "Declare a Text filter.",
        ),
        snippet(
            "<name> Integer",
            "${1:name} ${2:Integer}",
            "Declare an Integer filter.",
        ),
        snippet(
            "<name> Boolean",
            "${1:name} ${2:Boolean}",
            "Declare a Boolean filter.",
        ),
        snippet(
            "<name> <type> multi url_sync",
            "${1:name} ${2:Type} multi url_sync",
            "Declare a multi-value URL-synced filter.",
        ),
    ];

    items.extend(extract_enum_names(source).into_iter().map(|name| {
        snippet(
            &format!("<name> {name}"),
            &format!("${{1:name}} {name}"),
            "Declare an enum-typed filter.",
        )
    }));
    items
}

fn search_segmented_completions(source: &str) -> Vec<CompletionItem> {
    let mut items: Vec<_> = extract_filter_names(source)
        .into_iter()
        .map(|name| {
            snippet(
                &format!("keyword <field> binds to filter.{name}"),
                &format!("keyword ${{1:{name}}} binds to filter.{name}"),
                "Bind a segmented-search keyword to a filter.",
            )
        })
        .collect();
    items.push(snippet(
        "free_text -> source.<input>",
        "free_text -> source.${1:input}",
        "Bind free text to a source query input.",
    ));
    items
}

fn selection_completions() -> Vec<CompletionItem> {
    vec![snippet(
        "bulk_actions [<command>, <command>]",
        "bulk_actions [${1:command}, ${2:command}]",
        "Declare commands that operate on the multi-selection.",
    )]
}

fn extract_enum_names(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split_whitespace();
            match (parts.next(), parts.next()) {
                (Some("enum"), Some(name)) => Some(clean_identifier(name).to_owned()),
                _ => None,
            }
        })
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn extract_filter_names(source: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    let mut in_filters = false;
    let mut filters_indent = 0;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = leading_spaces(line);
        if trimmed == "filters" {
            in_filters = true;
            filters_indent = indent;
            continue;
        }
        if in_filters && indent <= filters_indent {
            in_filters = false;
        }
        if in_filters {
            let Some(name) = trimmed
                .split(|ch: char| ch.is_whitespace() || ch == ':')
                .find(|part| !part.is_empty())
            else {
                continue;
            };
            let name = clean_identifier(name);
            if !name.is_empty() {
                names.insert(name.to_owned());
            }
        }
    }

    names.into_iter().collect()
}

fn snippet(label: &str, body: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_owned()),
        insert_text: Some(body.to_owned()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::String(detail.to_owned())),
        ..CompletionItem::default()
    }
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

fn starts_lzx_container(trimmed: &str) -> bool {
    trimmed.starts_with("surface ")
        || trimmed.starts_with("audience ")
        || trimmed.starts_with("uses feature ")
}

fn starts_known_root(trimmed: &str) -> bool {
    starts_lzx_container(trimmed) || trimmed.starts_with("view ") || trimmed.starts_with("enum ")
}

fn clean_identifier(value: &str) -> &str {
    value.trim_matches(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::completions_for_lzx;
    use tower_lsp::lsp_types::Position;

    fn labels(source: &str, line: u32, character: u32) -> Vec<String> {
        completions_for_lzx(source, Position { line, character })
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn top_level_offers_view_list_snippet() {
        let labels = labels("", 0, 0);

        assert!(labels.iter().any(|label| label.contains("view list")));
    }

    #[test]
    fn inside_view_list_offers_drawer_snippet() {
        let source = "view list mine\n  ";
        let labels = labels(source, 1, 2);

        assert!(labels.iter().any(|label| label.starts_with("drawer")));
    }

    #[test]
    fn inside_filters_offers_text_type() {
        let source = "view list mine\n  filters\n    ";
        let labels = labels(source, 2, 4);

        assert!(labels.iter().any(|label| label.contains("Text")));
        assert!(labels.iter().any(|label| label.contains("Integer")));
    }

    #[test]
    fn inside_filters_extracts_enum_from_source() {
        let source = "enum ItemType\n\nview list mine\n  filters\n    ";
        let labels = labels(source, 4, 4);

        assert!(labels.iter().any(|label| label.contains("ItemType")));
    }

    #[test]
    fn inside_search_segmented_extracts_filter_names() {
        let source = "\
view list mine
  filters
    category Text
    tags Text multi
  search segmented
    ";
        let labels = labels(source, 5, 4);

        assert!(labels.iter().any(|label| label.contains("filter.category")));
        assert!(labels.iter().any(|label| label.contains("filter.tags")));
    }

    #[test]
    fn other_context_returns_empty() {
        let labels = labels("not lzx\n  still junk", 1, 2);

        assert!(labels.is_empty());
    }
}
