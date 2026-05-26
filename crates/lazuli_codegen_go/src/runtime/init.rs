//! `init()` block emission for the runtime-form Go emitter. Wires the
//! feature's resources, commands, and queries into `lazuli.Register(...)`.

use std::fmt::Write;

use lazuli_codegen_spec::{QueryKind, RuntimeFeature};

use super::{command_var_name, lower_camel, pascal_case};

pub(super) fn write_init(s: &mut String, feature: &RuntimeFeature) {
    let resource = feature.resources.first();
    let resource_pascal = resource.map(|r| pascal_case(&r.name)).unwrap_or_default();
    let resource_name = resource.map(|r| r.name.as_str()).unwrap_or("");

    writeln!(s, "func init() {{").ok();
    writeln!(s, "\tlazuli.Register(").ok();
    for r in &feature.resources {
        writeln!(s, "\t\t&{}Resource,", lower_camel(&r.name)).ok();
    }
    for c in &feature.commands {
        writeln!(
            s,
            "\t\t&{},",
            command_var_name(&c.short_name, &resource_pascal, resource_name)
        )
        .ok();
    }
    for q in &feature.queries {
        let var = match q.kind {
            QueryKind::List => format!("list{}s", resource_pascal),
            QueryKind::Lookup => format!(
                "{}{}",
                lower_camel(resource_name),
                pascal_case(&q.short_name)
            ),
        };
        writeln!(s, "\t\t&{var},").ok();
    }
    writeln!(s, "\t)").ok();
    writeln!(s, "}}").ok();
}
