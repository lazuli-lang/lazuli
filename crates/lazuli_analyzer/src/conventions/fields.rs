//! `conventions [crud]` §5.7 field-categorisation + input projection.
//!
//! The synth pass walks a resource's `fields` and projects them into
//! the canonical `Create` / `Update` input shapes (§5.2 / §5.3). The
//! groups are:
//!
//! * **Tenant** — `org` and `user` (when `user: User required unique`).
//!   Never appear in input; filled by the runtime from `ctx`.
//! * **Auto** — `id`, `created_at`, `updated_at`, and any lifecycle
//!   discriminator. Never appear in input; filled by the codegen /
//!   DB defaults.
//! * **Required** — author-declared `required` fields that aren't
//!   Tenant/Auto. Land in `create.input` as required slots; degraded
//!   to optional in `update.input` for partial-update semantics.
//! * **Optional** — remaining fields. Optional in both shapes.
//!
//! `input_to_command_input` and `input_field_assignments` are the two
//! small helpers shared by every CRUD command builder so the codegen
//! has a populated `Bindings` body to emit.

use lazuli_ir as ir;

/// §5.7 field categorisation result. Each field on a resource lands in
/// exactly one group; `create_input_fields` and `update_input_fields`
/// project the Required + Optional groups into the canonical input lists
/// per §5.2 / §5.3.
pub(crate) struct CategorisedFields<'a> {
    required: Vec<&'a ir::Field>,
    optional: Vec<&'a ir::Field>,
}

impl<'a> CategorisedFields<'a> {
    /// §5.2 — `create.input` shape: required fields stay required,
    /// optional fields stay optional. Order matches resource field order.
    pub(super) fn create_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, true));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }

    /// §5.3 — `update.input` shape: every non-immutable field becomes
    /// optional regardless of its required-on-resource flag. Field-by-
    /// field optional update — fields omitted from input are not touched.
    pub(super) fn update_input_fields(&self) -> Vec<(&'a ir::Field, bool)> {
        let mut out: Vec<(&'a ir::Field, bool)> = Vec::new();
        for field in &self.required {
            out.push((field, false));
        }
        for field in &self.optional {
            out.push((field, false));
        }
        out
    }
}

/// §5.7 — split a resource's fields into Tenant / Auto / Required /
/// Optional groups. Only Required + Optional are returned (Tenant and
/// Auto have no presence in the synth input lists).
pub(crate) fn categorize_fields(resource: &ir::Resource) -> CategorisedFields<'_> {
    // Detect the `user: User required unique` shape per §5.7.
    let has_user_unique = resource.fields.iter().any(|f| {
        f.name == "user"
            && f.required
            && f.unique
            && matches!(&f.type_ref, ir::TypeRef::UserDefined(q) if q.name == "User")
    });

    // Discriminator field is in the Auto group when lifecycle is set.
    let lifecycle_discriminator: Option<&str> = resource
        .lifecycle
        .as_ref()
        .map(|lc| lc.discriminator_field.as_str());

    let mut required: Vec<&ir::Field> = Vec::new();
    let mut optional: Vec<&ir::Field> = Vec::new();

    for field in &resource.fields {
        // Tenant group.
        let is_tenant = field.name == "org" || (has_user_unique && field.name == "user");
        // Auto group.
        let is_auto = matches!(field.name.as_str(), "id" | "created_at" | "updated_at")
            || lifecycle_discriminator.is_some_and(|d| d == field.name);

        if is_tenant || is_auto {
            continue;
        }

        if field.required {
            required.push(field);
        } else {
            optional.push(field);
        }
    }

    CategorisedFields { required, optional }
}

/// Project a `(field, required)` list into a typed `CommandInput`.
pub(crate) fn input_to_command_input(fields: &[(&ir::Field, bool)]) -> ir::CommandInput {
    if fields.is_empty() {
        return ir::CommandInput::Empty;
    }
    let slots: Vec<ir::TypedSlot> = fields
        .iter()
        .map(|(f, required)| ir::TypedSlot {
            name: f.name.clone(),
            type_ref: f.type_ref.clone(),
            required: *required,
            constraints: f.constraints.clone(),
            validate_skip: false,
        })
        .collect();
    ir::CommandInput::Typed(slots)
}

/// Build one `<field> = input.<field>` assignment per input slot.
/// Used by both `build_create_command` and `build_update_command` so the
/// codegen has a populated `Bindings` body to emit. The emitter inspects
/// `TypedSlot.required` on the command's input to pick between
/// `FromInput` (required slot) and `FromInputOptional` (optional slot,
/// skip-on-nil at runtime).
pub(crate) fn input_field_assignments(input_fields: &[(&ir::Field, bool)]) -> Vec<ir::Assignment> {
    input_fields
        .iter()
        .map(|(f, _required)| ir::Assignment {
            field: f.name.clone(),
            value: ir::Expr::Path(ir::Path::from_segments([
                "input".to_owned(),
                f.name.clone(),
            ])),
        })
        .collect()
}
