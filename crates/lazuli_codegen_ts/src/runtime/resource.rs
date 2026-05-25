//! Resource interface emission — `export interface <Pascal> { … }`.
//!
//! Wire-transport JSON stays snake_case (the Go runtime contract); the
//! TS interface emits camelCase keys so idiomatic JS/TS callers don't
//! shout. The `LazuliClient.case-mapper.ts` translates both ways at the
//! HTTP boundary — see `runtime/ts/lazuli/src/case-mapper.ts`. Any field
//! shape change here must round-trip through that mapper.

use std::fmt::Write;

use lazuli_codegen_spec::RuntimeResource;

use super::header::write_section_banner;
use super::naming::{field_kind_ts, lower_camel, pascal_case};

pub(super) fn write_resource(s: &mut String, resource: &RuntimeResource) {
    let pascal = pascal_case(&resource.name);

    write_section_banner(s, &[format!("Resource: {pascal}")]);

    // Idiomatic JS/TS callers expect camelCase keys. Wire transport
    // remains snake_case (Go runtime contract); the LazuliClient
    // converts both ways at the boundary — see
    // `runtime/ts/lazuli/src/case-mapper.ts`.
    writeln!(s, "export interface {pascal} {{").ok();
    writeln!(s, "  id: ID;").ok();
    writeln!(s, "  orgId: ID;").ok();
    for field in &resource.fields {
        writeln!(
            s,
            "  {}: {};",
            lower_camel(&field.name),
            field_kind_ts(field.kind)
        )
        .ok();
    }
    writeln!(s, "  createdAt: Time;").ok();
    writeln!(s, "  updatedAt: Time;").ok();
    if resource.soft_delete {
        writeln!(s, "  deletedAt?: Time | null;").ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}
