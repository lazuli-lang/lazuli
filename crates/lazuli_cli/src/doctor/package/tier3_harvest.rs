//! Tier-3 fact harvest for one lowered feature.
//!
//! Walks the lifted `Feature` IR plus its raw source to materialize
//! a `Tier3FeatureFacts` row that pairs every command / query / api /
//! job / webhook / notification / tenant-migration / event-group /
//! aggregate / report / cache / translation slot with its authoring
//! line number. The line lookups happen here (via
//! `collect_construct_lines`, `collect_query_lines`,
//! `collect_event_group_lines`) so downstream diagnostics anchor at
//! the right authoring site.
//!
//! Also harvests resource/field rename facts (`previous_names`) and
//! the set of every resource/field name in the feature — both
//! consumed by the migrations bucket diagnostics. The function is
//! a no-op when the feature has zero of every Tier-3 body slot.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    DoctorFile, FieldPreviousFact, ResourcePreviousFact, Tier3FeatureFacts, collect_construct_lines,
    collect_event_group_lines, collect_query_lines, collect_text_pattern_api_names,
    find_keyword_line, line_col_for_offset, tenancy_axis_for,
};
use super::state::LoadAccumulator;

pub(super) fn harvest_tier3_facts(
    file: &DoctorFile,
    acc: &mut LoadAccumulator,
    skeleton: &lazuli_syntax::FeatureSkeleton,
    feature: &lazuli_ir::Feature,
    header_line: usize,
) {
    let mut resource_previous_names: Vec<ResourcePreviousFact> = Vec::new();
    let mut field_previous_names: Vec<FieldPreviousFact> = Vec::new();
    let mut all_resource_names_in_feature: BTreeSet<String> = BTreeSet::new();
    let mut all_field_names_in_feature: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if !feature.resources.is_empty() {
        let resource_header_lines = collect_construct_lines(
            &file.source,
            "resource ",
            feature.resources.iter().map(|r| r.name.as_str()).collect(),
        );
        for res in &feature.resources {
            all_resource_names_in_feature.insert(res.name.clone());
            let field_set = all_field_names_in_feature
                .entry(res.name.clone())
                .or_default();
            for fld in &res.fields {
                field_set.insert(fld.name.clone());
            }
            let res_line = resource_header_lines
                .get(&res.name)
                .copied()
                .unwrap_or(header_line);
            if !res.previous_names.is_empty() {
                resource_previous_names.push(ResourcePreviousFact {
                    current_name: res.name.clone(),
                    previous_names: res.previous_names.clone(),
                    line: res_line,
                });
            }
            for field in &res.fields {
                if !field.previous_names.is_empty() {
                    field_previous_names.push(FieldPreviousFact {
                        resource_name: res.name.clone(),
                        current_name: field.name.clone(),
                        previous_names: field.previous_names.clone(),
                        line: res_line,
                    });
                }
            }
        }
    }

    // OpenAPI/Cache/i18n bucket cycles — commands/queries/apis/translation
    // are always harvested (no gate); the buckets' diagnostics walk over
    // `Tier3FeatureFacts` unconditionally. Doctor skips features with
    // empty slots in the diagnostic bodies themselves.
    let has_text_pattern_api = file
        .source
        .lines()
        .any(|line| line.trim_start().starts_with("api "));
    if !feature.jobs.is_empty()
        || !feature.webhooks.is_empty()
        || !feature.notifications.is_empty()
        || !feature.event_groups.is_empty()
        || !feature.tenant_migrations.is_empty()
        || !resource_previous_names.is_empty()
        || !field_previous_names.is_empty()
        || !feature.commands.is_empty()
        || !feature.queries.is_empty()
        || !feature.apis.is_empty()
        || !feature.records.is_empty()
        || !feature.enums.is_empty()
        || !feature.reports.is_empty()
        || !feature.resources.is_empty()
        || feature.translation.is_some()
        || has_text_pattern_api
    {
        let job_lines = collect_construct_lines(
            &file.source,
            "job ",
            feature.jobs.iter().map(|j| j.name.as_str()).collect(),
        );
        let webhook_lines = collect_construct_lines(
            &file.source,
            "webhook ",
            feature.webhooks.iter().map(|w| w.name.as_str()).collect(),
        );
        let notification_lines = collect_construct_lines(
            &file.source,
            "notification ",
            feature
                .notifications
                .iter()
                .map(|n| n.name.as_str())
                .collect(),
        );
        let tenant_migration_lines = collect_construct_lines(
            &file.source,
            "tenant_migration ",
            feature
                .tenant_migrations
                .iter()
                .map(|t| t.name.as_str())
                .collect(),
        );
        let event_group_lines = collect_event_group_lines(
            &file.source,
            feature
                .event_groups
                .iter()
                .map(|g| g.pattern.as_str())
                .collect(),
        );
        let command_lines = collect_construct_lines(
            &file.source,
            "command ",
            feature.commands.iter().map(|c| c.name.as_str()).collect(),
        );
        let query_lines = collect_query_lines(&file.source, &feature.queries);
        let api_names_text_pattern = collect_text_pattern_api_names(&file.source);
        let api_lines = collect_construct_lines(
            &file.source,
            "api ",
            feature.apis.iter().map(|a| a.name.as_str()).collect(),
        );
        let report_lines = collect_construct_lines(
            &file.source,
            "report ",
            feature.reports.iter().map(|r| r.name.as_str()).collect(),
        );
        let cache_lines = collect_construct_lines(
            &file.source,
            "cache ",
            feature.caches.iter().map(|c| c.name.as_str()).collect(),
        );
        let translation_line = feature
            .translation
            .as_ref()
            .map(|_| find_keyword_line(&file.source, "translation").unwrap_or(header_line))
            .unwrap_or(header_line);
        // CL.C.4 — line lookup for aggregates so domain diagnostics
        // anchor at the `aggregate <Name>` header.
        let mut aggregate_lines: BTreeMap<String, usize> = BTreeMap::new();
        for agg in &feature.aggregates {
            let agg_line = agg
                .span_ref
                .as_ref()
                .map(|s| line_col_for_offset(&file.source, s.start).0)
                .unwrap_or(header_line);
            aggregate_lines.insert(agg.name.clone(), agg_line);
        }
        acc.tier3_facts.push(Tier3FeatureFacts {
            feature: feature.name.clone(),
            path: file.path.clone(),
            feature_line: header_line,
            tenancy_axis: tenancy_axis_for(feature),
            defaults_policy: feature.defaults.policy.clone(),
            defaults_timestamps: feature.defaults.timestamps,
            jobs: feature.jobs.clone(),
            webhooks: feature.webhooks.clone(),
            notifications: feature.notifications.clone(),
            event_groups: feature.event_groups.clone(),
            tenant_migrations: feature.tenant_migrations.clone(),
            resource_previous_names,
            field_previous_names,
            all_resource_names_in_feature,
            all_field_names_in_feature,
            job_lines,
            webhook_lines,
            notification_lines,
            tenant_migration_lines,
            event_group_lines,
            commands: feature.commands.clone(),
            command_lines,
            queries: feature.queries.clone(),
            query_lines,
            caches: feature.caches.clone(),
            cache_lines,
            api_names_text_pattern,
            apis: feature.apis.clone(),
            api_lines,
            agents: feature.agents.clone(),
            translation: feature.translation.clone(),
            translation_line,
            records: feature.records.clone(),
            enums: feature.enums.clone(),
            events: feature.events.clone(),
            policies_declared: feature.policies.span_ref.is_some(),
            policies: feature.policies.clone(),
            extensions: feature.extensions.clone(),
            reports: feature.reports.clone(),
            report_lines,
            resources: feature.resources.clone(),
            report_decls: skeleton.reports.clone(),
            aggregates: feature.aggregates.clone(),
            aggregate_lines,
            errors: feature.errors.clone(),
            uses: feature.uses.clone(),
            channels: feature.channels.clone(),
        });
    }
}
