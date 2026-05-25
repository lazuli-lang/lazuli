//! `--expand=defaults` projection.
//!
//! Phase L Tier 4a unified the projection on the lifted
//! `Feature.defaults` block (canonical-indent code path), with a
//! legacy text-pattern walker for documents that don't yet lower
//! through `parse_feature_skeletons`. Both paths surface the same
//! `InspectDefault` shape so consumers see one projection.
//!
//! Additionally, two query-derived defaults stay text-driven because
//! they originate from CLI heuristics over query bodies, not from
//! feature-state IR:
//!
//! - `query_order` — the implicit `created_at desc` default for any
//!   `query.list` that omits an `order` clause.
//! - `query_filter_index` — index hints lifted from `<field> when
//!   params.<field>` / `<field> = params.<field>` filter shapes, with
//!   automatic relation-id unwrapping (e.g. `relation.id = params.relation_id`).
//!
//! `applies_to` for `policy` and `policy_for` retains the text walker
//! until job/webhook names are also lifted to the Tier 3 slice; the
//! typed `Defaults.policy` atom is already consumed, only the scope
//! enumeration stays text-derived.

use std::collections::BTreeSet;

use super::super::InspectDefault;
use super::super::Tier3FeatureSlice;
use super::super::expand::{is_identifier, leading_spaces, parse_ident_list};
use super::super::text_walkers::{
    collect_job_and_webhook_names, collect_named_top_blocks, collect_resource_names,
    direct_child_value, query_blocks, query_name,
};

pub(in crate::commands::inspect) fn inspect_defaults(
    lines: &[String],
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectDefault> {
    let mut defaults = match tier3 {
        Some(slice) => project_defaults_from_ir(slice, lines),
        None => inspect_defaults_legacy(lines),
    };

    for query in query_blocks(lines) {
        let header = query[0].trim_start();
        if !header.starts_with("query.list ") {
            continue;
        }
        if direct_child_value(query, "order ").is_some() {
            continue;
        }
        let name = query_name(header).unwrap_or("unknown");
        defaults.push(InspectDefault {
            name: "query_order".to_owned(),
            value: "created_at desc".to_owned(),
            origin: "language default",
            applies_to: vec![format!("query.{name}")],
        });
    }

    for generated in collect_query_filter_indexes(lines) {
        defaults.push(InspectDefault {
            name: "query_filter_index".to_owned(),
            value: generated.value,
            origin: "language default",
            applies_to: vec![
                format!("query.{}", generated.query),
                format!("filter.{}", generated.filter),
            ],
        });
    }

    defaults
}

fn project_defaults_from_ir(slice: &Tier3FeatureSlice, lines: &[String]) -> Vec<InspectDefault> {
    let mut out = Vec::new();

    if slice.defaults.timestamps {
        out.push(InspectDefault {
            name: "timestamps".to_owned(),
            value: "true".to_owned(),
            origin: "defaults",
            applies_to: slice.resource_names.clone(),
        });
    }

    if let Some(tenancy) = &slice.defaults.tenancy {
        let value = match tenancy {
            lazuli_ir::Tenancy::Org => "org".to_owned(),
            lazuli_ir::Tenancy::Team => "team".to_owned(),
            lazuli_ir::Tenancy::Custom(name) => name.clone(),
            lazuli_ir::Tenancy::None => "none".to_owned(),
        };
        out.push(InspectDefault {
            name: "tenancy".to_owned(),
            value,
            origin: "defaults",
            applies_to: slice.resource_names.clone(),
        });
    }

    // `policy` and `policy_for` retain text-derived `applies_to` until
    // jobs/webhooks have their names lifted to the slice. The IR
    // `Defaults.policy` carries the typed atom; the `applies_to`
    // projection mirrors the legacy text walker for now to keep the
    // projection JSON shape stable.
    let mut in_defaults = false;
    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }
        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("policy_for ") {
            if let Some((scopes, policy)) = value.split_once(':') {
                out.push(InspectDefault {
                    name: "policy_for".to_owned(),
                    value: policy.trim().to_owned(),
                    origin: "defaults",
                    applies_to: collect_policy_for_applies_to(lines, scopes),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            out.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: collect_job_and_webhook_names(lines),
            });
        }
    }

    out
}

fn inspect_defaults_legacy(lines: &[String]) -> Vec<InspectDefault> {
    let resources = collect_resource_names(lines);
    let mut defaults = Vec::new();
    let mut in_defaults = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }

        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        if trimmed == "timestamps" {
            defaults.push(InspectDefault {
                name: "timestamps".to_owned(),
                value: "true".to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("tenancy ") {
            defaults.push(InspectDefault {
                name: "tenancy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("policy_for ") {
            if let Some((scopes, policy)) = value.split_once(':') {
                defaults.push(InspectDefault {
                    name: "policy_for".to_owned(),
                    value: policy.trim().to_owned(),
                    origin: "defaults",
                    applies_to: collect_policy_for_applies_to(lines, scopes),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            defaults.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: collect_job_and_webhook_names(lines),
            });
        }
    }

    defaults
}

struct GeneratedFilterIndex {
    query: String,
    filter: String,
    value: String,
}

fn collect_query_filter_indexes(lines: &[String]) -> Vec<GeneratedFilterIndex> {
    let tenancy_axis = single_tenancy_axis(lines);
    let mut seen = BTreeSet::new();
    let mut indexes = Vec::new();

    for query in query_blocks(lines) {
        let header = query[0].trim_start();
        if !header.starts_with("query.list ") || query_has_scope_override(query) {
            continue;
        }
        let name = query_name(header).unwrap_or("unknown");

        for field in query_filter_index_fields(query) {
            let value = tenancy_axis
                .as_ref()
                .map(|tenant| format!("{tenant}, {field}"))
                .unwrap_or_else(|| field.clone());

            if seen.insert(value.clone()) {
                indexes.push(GeneratedFilterIndex {
                    query: name.to_owned(),
                    filter: field,
                    value,
                });
            }
        }
    }

    indexes
}

fn single_tenancy_axis(lines: &[String]) -> Option<String> {
    let axes: BTreeSet<String> = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let axis = trimmed.strip_prefix("tenancy ")?.trim();
            (!axis.is_empty() && axis != "none").then(|| axis.to_owned())
        })
        .collect();

    (axes.len() == 1).then(|| axes.into_iter().next()).flatten()
}

fn query_has_scope_override(query: &[String]) -> bool {
    query
        .iter()
        .any(|line| line.trim_start() == "scope override")
}

fn query_filter_index_fields(query: &[String]) -> Vec<String> {
    let mut fields = Vec::new();
    let mut in_filters = false;

    for line in query.iter().skip(1) {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            continue;
        }

        if leading == 6 {
            in_filters = trimmed == "filters";
            continue;
        }

        if in_filters
            && leading == 8
            && let Some(field) = filter_index_field(trimmed)
        {
            fields.push(field);
        }
    }

    fields
}

fn filter_index_field(filter: &str) -> Option<String> {
    if filter.contains(" has ")
        || filter.contains(" != ")
        || filter.contains(" = nil")
        || filter.contains(" != nil")
    {
        return None;
    }

    if let Some((field, param)) = filter.split_once(" when ") {
        let field = field.trim();
        let param = param.trim().strip_prefix("params.")?;
        if is_identifier(field) && field == param {
            return Some(field.to_owned());
        }
        return None;
    }

    if let Some((left, right)) = filter.split_once(" = ") {
        let left = left.trim();
        let param = right.trim().strip_prefix("params.")?;

        if is_identifier(left) && left == param {
            return Some(left.to_owned());
        }

        if let Some(relation) = left.strip_suffix(".id")
            && is_identifier(relation)
            && param == format!("{relation}_id")
        {
            return Some(relation.to_owned());
        }
    }

    None
}

fn collect_policy_for_applies_to(lines: &[String], scopes: &str) -> Vec<String> {
    let mut applies_to = Vec::new();

    for scope in parse_ident_list(scopes) {
        match scope.as_str() {
            "jobs" => applies_to.extend(collect_named_top_blocks(lines, "job ")),
            "webhooks" => applies_to.extend(collect_named_top_blocks(lines, "webhook ")),
            _ => {}
        }
    }

    applies_to
}
