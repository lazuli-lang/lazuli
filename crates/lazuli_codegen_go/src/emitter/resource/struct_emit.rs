//! Struct + `Resource[T]` value emission for resources and records.
//!
//! `emit_resource` walks one `Resource`: implicit ID column, implicit
//! tenancy column, user-declared fields (with derived columns surfaced
//! as standalone comments), per-Money currency pair columns, implicit
//! timestamps, soft-delete sentinel, then the `lazuli.Resource[T]`
//! value literal with tenancy / soft-delete / timestamps / retention /
//! encryption metadata.
//!
//! `emit_record` is the lighter variant — typed struct only, no
//! resource value, no identity, no tenancy axis.
//!
//! Boundary: `write_section_banner` and the parent module's helpers
//! (`pascal_case`, `lower_camel`) live one level up so encryption
//! emission can share them.

use std::fmt::Write;

use lazuli_ir::{
    BuiltinType, ComputedDateBase, ComputedDateOffset, Feature, Record, Resource, Tenancy, TypeRef,
};

use crate::emitter::casing::{lower_camel, pascal_case};
use crate::emitter::printer::GoPrinter;
use crate::emitter::types::{self, TypeCtx};

use super::attributes::{
    db_col_for, effective_tenancy, field_validate_tag, is_secret_capability,
    retention_action_const, tenancy_const, uses_timestamps,
};
use super::encryption::{
    EncryptedFieldRef, emit_resource_value_encryption_fields, encrypted_fields,
};
use super::sanitize::{
    SanitizedFieldRef, emit_resource_value_sanitize_fields, sanitized_fields,
};
use super::write_section_banner;

include!("struct_emit_p1.rs");
include!("struct_emit_p2.rs");
