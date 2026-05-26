//! Resource block emission for the runtime-form Go emitter. See
//! `super::emit_feature_go` for the orchestrator.

use std::fmt::Write;

use lazuli_codegen_spec::{RetentionAction, RuntimeResource, Tenancy};

use super::{
    build_db_json_rows, field_kind_go, lower_camel, pascal_case, write_aligned_struct_rows,
    write_section_banner,
};

pub(super) fn write_resource(s: &mut String, resource: &RuntimeResource) {
    let pascal = pascal_case(&resource.name);
    let var_name = format!("{}Resource", lower_camel(&resource.name));

    write_section_banner(
        s,
        &[
            format!("Resource: {pascal}"),
            format!("  resource {pascal}"),
        ],
    );

    // Struct definition. We assemble the rows first so column widths align
    // exactly the way `go fmt` would — there are no trailing-space surprises
    // that break a downstream byte-for-byte compare.
    writeln!(
        s,
        "// {pascal} is the row materialised from the {pascal} resource. Each field"
    )
    .ok();
    writeln!(
        s,
        "// carries `db` and `json` tags for pgx scan and HTTP encode respectively."
    )
    .ok();
    writeln!(s, "type {pascal} struct {{").ok();
    // Collect (Go field, Go type, db column, json tag suffix). The aligner
    // pads each tag's `db:"..."` portion to the width of the longest column
    // so the `json:` segment is column-aligned, mirroring `go fmt`.
    let mut tagged: Vec<(String, String, String, String)> = Vec::new();
    tagged.push((
        "ID".to_owned(),
        "lazuli.ID".to_owned(),
        "id".to_owned(),
        "id".to_owned(),
    ));
    if matches!(resource.tenancy, Tenancy::Org) {
        tagged.push((
            "OrgID".to_owned(),
            "lazuli.ID".to_owned(),
            "org_id".to_owned(),
            "org_id".to_owned(),
        ));
    }
    for field in &resource.fields {
        tagged.push((
            pascal_case(&field.name),
            field_kind_go(field.kind).to_owned(),
            field.name.clone(),
            field.name.clone(),
        ));
    }
    tagged.push((
        "CreatedAt".to_owned(),
        "lazuli.Time".to_owned(),
        "created_at".to_owned(),
        "created_at".to_owned(),
    ));
    tagged.push((
        "UpdatedAt".to_owned(),
        "lazuli.Time".to_owned(),
        "updated_at".to_owned(),
        "updated_at".to_owned(),
    ));
    if resource.soft_delete {
        tagged.push((
            "DeletedAt".to_owned(),
            "*lazuli.Time".to_owned(),
            "deleted_at".to_owned(),
            "deleted_at,omitempty".to_owned(),
        ));
    }
    let rows: Vec<(String, String, String)> = build_db_json_rows(&tagged);
    write_aligned_struct_rows(s, &rows);
    writeln!(s, "}}").ok();
    writeln!(s).ok();

    // Resource var.
    writeln!(s, "var {var_name} = lazuli.Resource[{pascal}]{{").ok();
    writeln!(s, "\tName:       \"{}\",", resource.name).ok();
    writeln!(s, "\tFeature:    \"{}\",", resource.name).ok();
    let tenancy = match resource.tenancy {
        Tenancy::Org => "lazuli.TenancyOrg",
        Tenancy::None => "lazuli.TenancyNone",
    };
    writeln!(s, "\tTenancy:    {tenancy},").ok();
    if resource.soft_delete {
        writeln!(s, "\tSoftDelete: true,").ok();
    }
    if let Some(retention) = &resource.retention {
        writeln!(s, "\tRetention: &lazuli.RetentionSpec{{").ok();
        writeln!(s, "\t\tWindow: lazuli.Duration(\"{}\"),", retention.window).ok();
        let action = match retention.action {
            RetentionAction::Anonymize => "lazuli.Anonymize",
            RetentionAction::Purge => "lazuli.Purge",
        };
        writeln!(s, "\t\tThen:   {action},").ok();
        writeln!(s, "\t}},").ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}
