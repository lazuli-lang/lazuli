//! TypeScript type lowering for command/query slots and resource fields.
//!
//! Carved out of `ts/mod.rs` as part of Wave R6-5 (Rails-style refactor).
//! This module owns every IR → TS type resolver the `lazuli generate ts`
//! pipeline calls when shaping per-feature interfaces:
//!
//! - **Slot extractors** ([`command_sdk_slots`], [`command_input_slots`],
//!   [`query_args`]): collapse `Command::route` + `Command::input`
//!   variants and `Query::{List,Lookup,Sql}` keys/filters into a
//!   uniform [`TsSlot`] vector.
//! - **Resource lookups** ([`command_resource`], [`find_resource`],
//!   [`resource_ts_name`], [`resource_field_ts_name`],
//!   [`resource_field_ts_type`], [`is_resource_ref`]): the canonical
//!   resolvers used everywhere the SDK emitter needs to know whether a
//!   `UserDefined` ref collapses to `ID` (FK column) or carries the
//!   full row.
//! - **Query subject inference** ([`pick_query_resource_ts`],
//!   [`pluralize_snake`]): heuristic that maps `mine_properties` to
//!   `Property` (WAR-VOCAB-HOSTHOME-01 + Wave §A2 disambiguation).
//! - **Pure-read classification** ([`command_is_pure_read`]):
//!   `ir-returns-list-2026-05-22 §2.2` gate that lowers reads to
//!   `defineQuery` and writes to `defineCommand`.
//! - **Type formatters** ([`ts_type_for_type_ref`],
//!   [`ts_return_type_for_type_ref`], [`command_output_ts_type`]):
//!   produce the final TS-string for a `TypeRef` in field-position vs
//!   return-position.

use crate::casing::{pascal_case, to_snake_case};

use super::format::find_enum_decl;

#[derive(Clone)]
pub(crate) struct TsSlot {
    pub(crate) name: String,
    pub(crate) type_ref: lazuli_ir::TypeRef,
    pub(crate) required: bool,
    pub(crate) constraints: lazuli_ir::FieldConstraints,
}

pub(crate) fn command_sdk_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    let mut slots = Vec::new();
    for route in &command.route {
        slots.push(TsSlot {
            name: route.name.clone(),
            type_ref: route.type_ref.clone(),
            required: route.from.is_none(),
            constraints: lazuli_ir::FieldConstraints::default(),
        });
    }
    slots.extend(command_input_slots(feature, command, module));
    slots
}

pub(crate) fn command_input_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match &command.input {
        lazuli_ir::CommandInput::Empty => Vec::new(),
        lazuli_ir::CommandInput::Typed(slots) => slots
            .iter()
            .map(|slot| TsSlot {
                name: slot.name.clone(),
                type_ref: slot.type_ref.clone(),
                required: slot.required,
                constraints: slot.constraints.clone(),
            })
            .collect(),
        lazuli_ir::CommandInput::Short(names) => {
            let resource = command_resource(feature, command, module);
            names
                .iter()
                .map(|name| {
                    let field = resource.and_then(|r| r.fields.iter().find(|f| f.name == *name));
                    TsSlot {
                        name: name.clone(),
                        type_ref: field
                            .map(|f| f.type_ref.clone())
                            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
                        required: field.map(|f| f.required).unwrap_or(true),
                        constraints: field.map(|f| f.constraints.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        }
    }
}

pub(crate) fn query_args(
    feature: &lazuli_ir::Feature,
    query: &lazuli_ir::Query,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match query {
        lazuli_ir::Query::List(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Sql(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Lookup(q) => {
            let mut slots: Vec<TsSlot> = q.params.iter().map(ts_slot_from_typed).collect();
            if slots.is_empty() {
                for key in &q.keys {
                    if let lazuli_ir::Expr::Path(path) = &key.equals {
                        if path.segments.first().is_some_and(|s| s == "input") {
                            if let Some(name) = path.segments.get(1) {
                                slots.push(query_input_slot(feature, module, name));
                            }
                        }
                    }
                }
            }
            if slots.is_empty() {
                collect_input_slots_from_filters(feature, module, &q.filters, &mut slots);
            }
            if slots.is_empty() {
                if let Some(name) = q.name.strip_prefix("by_") {
                    slots.push(query_input_slot(feature, module, name));
                }
            }
            slots
        }
    }
}

fn collect_input_slots_from_filters(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    filters: &[lazuli_ir::Filter],
    slots: &mut Vec<TsSlot>,
) {
    for filter in filters {
        collect_input_slots_from_predicate(feature, module, &filter.predicate, slots);
    }
}

fn collect_input_slots_from_predicate(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    predicate: &lazuli_ir::Predicate,
    slots: &mut Vec<TsSlot>,
) {
    match predicate {
        lazuli_ir::Predicate::Comparison { left, right, .. } => {
            collect_input_slot_from_expr(feature, module, left, slots);
            collect_input_slot_from_expr(feature, module, right, slots);
        }
        lazuli_ir::Predicate::Has {
            collection,
            element,
        } => {
            collect_input_slot_from_expr(feature, module, collection, slots);
            collect_input_slot_from_expr(feature, module, element, slots);
        }
        lazuli_ir::Predicate::And(predicates) | lazuli_ir::Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_input_slots_from_predicate(feature, module, predicate, slots);
            }
        }
    }
}

fn collect_input_slot_from_expr(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    expr: &lazuli_ir::Expr,
    slots: &mut Vec<TsSlot>,
) {
    let lazuli_ir::Expr::Path(path) = expr else {
        return;
    };
    if !path
        .segments
        .first()
        .is_some_and(|segment| segment == "input")
    {
        return;
    }
    let Some(name) = path.segments.get(1) else {
        return;
    };
    if slots.iter().any(|slot| slot.name == *name) {
        return;
    }
    slots.push(query_input_slot(feature, module, name));
}

fn query_input_slot(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    name: &str,
) -> TsSlot {
    let field = feature
        .resources
        .first()
        .and_then(|resource| resource.fields.iter().find(|field| field.name == name))
        .or_else(|| {
            module
                .features
                .iter()
                .flat_map(|feature| feature.resources.iter())
                .flat_map(|resource| resource.fields.iter())
                .find(|field| field.name == name)
        });
    TsSlot {
        name: name.to_owned(),
        type_ref: field
            .map(|field| field.type_ref.clone())
            .or_else(|| {
                name.eq_ignore_ascii_case("id")
                    .then_some(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id))
            })
            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
        required: true,
        constraints: field
            .map(|field| field.constraints.clone())
            .unwrap_or_default(),
    }
}

fn ts_slot_from_typed(slot: &lazuli_ir::TypedSlot) -> TsSlot {
    TsSlot {
        name: slot.name.clone(),
        type_ref: slot.type_ref.clone(),
        required: slot.required,
        constraints: slot.constraints.clone(),
    }
}

/// Pick the most likely resource for a `query.list` / `query.lookup` /
/// `query.sql` return type. Walks the feature's resources, returns the
/// one whose snake-cased name appears as a substring of the query
/// name (e.g. `my_host` → "host" → Host; `property_detail` → "property"
/// → Property). Returns None when no resource matches; caller falls
/// back to `feature.resources.first()`. Closes WAR-VOCAB-HOSTHOME-01.
///
/// Wave §A2 (mine_query disambiguation, 2026-05-23): now matches the
/// plural form of each resource's snake name as well so
/// `mine_properties` → "property" + "properties" → Property. Without
/// this, `mine_properties` fell through to `feature.resources.first()`
/// which in `catalog.lzi` happens to be `UploadedAsset` — emitting
/// the wrong TS return type. Hostpoint workaround was an explicit
/// `as unknown as Property[]` cast in HostHome.tsx.
pub(crate) fn pick_query_resource_ts(
    feature: &lazuli_ir::Feature,
    query_name: &str,
) -> Option<String> {
    let query_lc = query_name.to_ascii_lowercase();
    // Prefer the longest match (so "service_transaction" beats
    // "service" + "transaction" tie). Sort by length desc.
    let mut candidates: Vec<&lazuli_ir::Resource> = feature.resources.iter().collect();
    candidates.sort_by(|a, b| b.name.len().cmp(&a.name.len()));
    for resource in candidates {
        let snake = to_snake_case(&resource.name);
        if query_lc.contains(&snake) {
            return Some(pascal_case(&resource.name));
        }
        // Plural-aware match: a `query.list mine_properties` should
        // bind to the `Property` resource even though the snake form
        // is the singular `property`.
        let snake_plural = pluralize_snake(&snake);
        if !snake_plural.is_empty() && query_lc.contains(&snake_plural) {
            return Some(pascal_case(&resource.name));
        }
        // Also try a token-by-token match for compound names like
        // "ServiceTransaction" vs query "transaction_detail".
        let last_token = snake.rsplit('_').next().unwrap_or("");
        if !last_token.is_empty() && last_token.len() > 3 && query_lc.contains(last_token) {
            return Some(pascal_case(&resource.name));
        }
        // Plural-aware last-token match — same fix one level deeper.
        let last_token_plural = pluralize_snake(last_token);
        if !last_token_plural.is_empty()
            && last_token_plural.len() > 4
            && query_lc.contains(&last_token_plural)
        {
            return Some(pascal_case(&resource.name));
        }
    }
    None
}

/// Cheap English-only pluralizer for snake-case identifiers. Handles
/// the three patterns that actually appear in pilot vocabularies:
///   - ends in `y` preceded by consonant → drop `y`, append `ies`
///     (property → properties, story → stories).
///   - ends in `s`/`x`/`z` or `ch`/`sh`     → append `es`
///     (process → processes, box → boxes).
///   - otherwise                          → append `s` (host → hosts).
///
/// Returns empty when the input is empty. Not a general-purpose
/// pluralizer — does not handle irregular forms (man → men, child →
/// children). Pilots whose vocab uses those should declare an explicit
/// `returns <Resource>` on the query rather than rely on this
/// heuristic.
fn pluralize_snake(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }
    let len = word.len();
    let last = word.as_bytes()[len - 1];
    if last == b'y' && len >= 2 {
        let prev = word.as_bytes()[len - 2];
        let is_consonant = !matches!(prev, b'a' | b'e' | b'i' | b'o' | b'u');
        if is_consonant {
            let mut out = word[..len - 1].to_string();
            out.push_str("ies");
            return out;
        }
    }
    if word.ends_with('s')
        || word.ends_with('x')
        || word.ends_with('z')
        || word.ends_with("ch")
        || word.ends_with("sh")
    {
        return format!("{word}es");
    }
    format!("{word}s")
}

/// Wave 0 (ir-returns-list-2026-05-22 §2.2): a command is a *pure read*
/// when its sole declared effect is `Returns(_)`, carries no declared
/// side-effects (no event emits, no lifecycle triggers, no invalidations,
/// no external calls), is NOT synthesized from `@cap.File` (those are
/// upload-protocol commands with implicit side effects the analyzer
/// doesn't surface as `emits`/`triggers`), AND its name starts with a
/// read-verb prefix (`list_`, `get_`, `lookup_`, `search_`, `find_`,
/// `count_`).
///
/// Pure-read commands lower to `defineQuery<I, O>` on the TS side
/// (consumable via `useLazuliQuery`) so the React app gets cache +
/// refetch + suspense semantics for free, instead of `defineCommand`
/// (which forces `useLazuliCommand` and imperative call sites). The
/// wire payload is identical — only the client-side factory differs.
///
/// The name-prefix gate exists because pilots and the analyzer leave
/// the IR side-effect surface empty for many side-effecting commands —
/// e.g. `account.login` (mints a session but has no `emits` because the
/// session table is private), `request_profile_photo_upload` (mints a
/// presigned URL but has no `triggers`). Trusting only the IR's empty
/// side-effect set produced false positives (W0-5 surfaced this:
/// hostpoint app failed to typecheck because login + photo-upload
/// commands shipped as `defineQuery`, breaking existing
/// `useLazuliCommand` callsites). The name-prefix gate makes the
/// classification conservative — false negatives (a read that doesn't
/// follow the naming convention) ship as `defineCommand`, which still
/// works; false positives ship a wire mismatch, which doesn't.
pub(crate) fn command_is_pure_read(command: &lazuli_ir::Command) -> bool {
    if !matches!(command.effect, lazuli_ir::CommandEffect::Returns(_)) {
        return false;
    }
    if !command.emits.is_empty()
        || !command.triggers.is_empty()
        || !command.invalidates.is_empty()
        || !command.external_calls.is_empty()
    {
        return false;
    }
    // cap_file synth: Request/Confirm/Clear are upload-protocol writes;
    // only GetUrl is a pure read (mints a signed download URL, no
    // mutation). c-2 worker surfaced this nuance; integrated 2026-05-22.
    if command
        .synthesized_from_cap_file
        .as_ref()
        .is_some_and(|marker| marker.role != lazuli_ir::AutoPhotoCommandRole::GetUrl)
    {
        return false;
    }
    const READ_VERB_PREFIXES: &[&str] = &["list_", "get_", "lookup_", "search_", "find_", "count_"];
    READ_VERB_PREFIXES
        .iter()
        .any(|prefix| command.name.starts_with(prefix))
}

pub(crate) fn command_output_ts_type(
    _feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> String {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => resource_ts_name(&effect.resource, module),
        lazuli_ir::CommandEffect::Updates(effect) => resource_ts_name(&effect.resource, module),
        lazuli_ir::CommandEffect::Deletes(effect) => resource_ts_name(&effect.resource, module),
        // For `returns User` we want the full resource interface (User)
        // not the FK collapse to `ID`. `ts_type_for_type_ref` collapses
        // any `UserDefined(<Resource>)` to `ID` because that's correct
        // for resource-field positions (FK column). But the return
        // position carries the typed row — same fix as the Go side
        // (`types::go_return_type_for`).
        lazuli_ir::CommandEffect::Returns(effect) => {
            ts_return_type_for_type_ref(&effect.return_type, module)
        }
        // CommandEffect::None means the command has an `@fn.*` handler
        // with no declared return effect — the Go side returns `struct{}`
        // (empty object). TS surface mirrors that as `void`. Previously
        // this fell back to `feature.resources.first()`, which produced
        // wildly wrong types (e.g. every catalog command typed as
        // `UploadedAsset` — see WAR-VOCAB-HOSTPROPDETAIL-02).
        lazuli_ir::CommandEffect::None => "void".to_owned(),
    }
}

/// Variant of [`ts_type_for_type_ref`] that resolves resource refs to
/// their full interface name (`User`) instead of the FK collapse (`ID`).
/// Used by [`command_output_ts_type`] for `Returns` — the handler emits
/// the typed row, not the row id. Mirrors the Go side's
/// `go_return_type_for` / `command_output_type` split (see
/// `crates/lazuli_codegen_go/src/emitter/types.rs`).
fn ts_return_type_for_type_ref(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> String {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) if is_resource_ref(type_ref, module) => {
            // Skip the FK collapse — return the resource interface name.
            find_resource(module, name)
                .map(|r| pascal_case(&r.name))
                .unwrap_or_else(|| pascal_case(&name.name))
        }
        lazuli_ir::TypeRef::Many(inner) => {
            format!("{}[]", ts_return_type_for_type_ref(inner, module))
        }
        // Everything else (builtins, capabilities, enums, records,
        // unresolved) shares the same shape as field-position resolution.
        other => ts_type_for_type_ref(other, module),
    }
}

fn command_resource<'a>(
    feature: &'a lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &'a lazuli_ir::Module,
) -> Option<&'a lazuli_ir::Resource> {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Updates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Deletes(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => {
            feature.resources.first()
        }
    }
}

pub(crate) fn find_resource<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::Resource> {
    module
        .features
        .iter()
        .filter(|feature| name.feature.as_ref().is_none_or(|n| n == &feature.name))
        .flat_map(|feature| feature.resources.iter())
        .find(|resource| resource.name.eq_ignore_ascii_case(&name.name))
}

fn resource_ts_name(name: &lazuli_ir::QualifiedName, module: &lazuli_ir::Module) -> String {
    find_resource(module, name)
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| pascal_case(&name.name))
}

pub(crate) fn resource_field_ts_name(
    field: &lazuli_ir::Field,
    module: &lazuli_ir::Module,
) -> String {
    if is_resource_ref(&field.type_ref, module) && !field.name.ends_with("_id") {
        format!("{}_id", field.name)
    } else {
        field.name.clone()
    }
}

pub(crate) fn resource_field_ts_type(
    field: &lazuli_ir::Field,
    module: &lazuli_ir::Module,
) -> String {
    if is_resource_ref(&field.type_ref, module) {
        "ID".to_owned()
    } else {
        ts_type_for_type_ref(&field.type_ref, module)
    }
}

pub(crate) fn is_resource_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> bool {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) => module
            .features
            .iter()
            .flat_map(|feature| feature.resources.iter())
            .any(|resource| resource.name.eq_ignore_ascii_case(&name.name)),
        _ => false,
    }
}

pub(crate) fn ts_type_for_type_ref(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> String {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(builtin) => match builtin {
            lazuli_ir::BuiltinType::Id => "ID".to_owned(),
            lazuli_ir::BuiltinType::Text
            | lazuli_ir::BuiltinType::SemanticEmail
            | lazuli_ir::BuiltinType::SemanticPhone
            | lazuli_ir::BuiltinType::SemanticUrl
            | lazuli_ir::BuiltinType::SemanticUuid
            | lazuli_ir::BuiltinType::SemanticCurrency
            | lazuli_ir::BuiltinType::CapSecret => "string".to_owned(),
            // B3 — plugin-contributed `@semantic.<Name>` projects to
            // the brand alias name (e.g. `BrazilianCPF`). The SDK
            // emitter (`emit_feature_sdk_ts`) writes the
            // `export type <Name> = string;` line at file head so
            // every consuming interface picks up the alias.
            lazuli_ir::BuiltinType::SemanticPluginType { name, .. } => pascal_case(name),
            lazuli_ir::BuiltinType::Boolean => "boolean".to_owned(),
            lazuli_ir::BuiltinType::Integer | lazuli_ir::BuiltinType::Decimal => {
                "number".to_owned()
            }
            // Per `semantic-types-money-brazilian.md` v0.3 — Money is
            // the rich struct on the TS side too. `Money` interface
            // lives in `@lazuli/runtime`; downstream consumers get the
            // shape via the typed import.
            lazuli_ir::BuiltinType::SemanticMoney { .. } => "Money".to_owned(),
            lazuli_ir::BuiltinType::Date | lazuli_ir::BuiltinType::DateTime => "Time".to_owned(),
            lazuli_ir::BuiltinType::Json
            | lazuli_ir::BuiltinType::SemanticGeoPoint
            | lazuli_ir::BuiltinType::CapFile => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Capability(capability) => match capability {
            lazuli_ir::CapabilityRef::Hashed(_)
            | lazuli_ir::CapabilityRef::Encrypted(_)
            | lazuli_ir::CapabilityRef::E2ee(_)
            | lazuli_ir::CapabilityRef::Token(_)
            | lazuli_ir::CapabilityRef::PII(_) => "string".to_owned(),
            lazuli_ir::CapabilityRef::File(_) => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Many(inner) => format!("{}[]", ts_type_for_type_ref(inner, module)),
        lazuli_ir::TypeRef::EnumRef(name) => find_enum_decl(module, name)
            .map(|enum_decl| pascal_case(&enum_decl.name))
            .unwrap_or_else(|| "unknown".to_owned()),
        lazuli_ir::TypeRef::UserDefined(name) => {
            if is_resource_ref(type_ref, module) {
                "ID".to_owned()
            } else if let Some(enum_decl) = find_enum_decl(module, name) {
                // Enum referenced via UserDefined path. The parser
                // sometimes tags an enum field as UserDefined when the
                // analyzer hasn't promoted it to EnumRef (review bug #3,
                // 2026-05-15: `tier: CustomerTier = free` and
                // `source: CustomerSource = manual` both flowed as
                // UserDefined-with-no-record-match and lowered to
                // `unknown` — even though `CustomerTier`/`CustomerSource`
                // are declared above the resource block).
                pascal_case(&enum_decl.name)
            } else {
                module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(&name.name))
                    .map(|record| pascal_case(&record.name))
                    .unwrap_or_else(|| "unknown".to_owned())
            }
        }
        lazuli_ir::TypeRef::Unresolved(raw) => {
            if raw.starts_with("@cap.Hashed")
                || raw.starts_with("@cap.Encrypted")
                || raw.starts_with("@cap.Token")
                || raw == "@semantic.Email"
            {
                return "string".to_owned();
            }
            // Bare PascalCase fallback: the analyzer occasionally leaves
            // a `Unresolved("Foo")` even when `Foo` is a declared enum /
            // record / resource somewhere in the module (review bug #3,
            // 2026-05-15: `tier: CustomerTier = manual` flowed as
            // `Unresolved("CustomerTier")` and lowered to `unknown`
            // even though `CustomerTier` is declared three lines above).
            // Recover by walking the module's catalogs here so the TS
            // SDK preserves typing instead of falling to opaque
            // `unknown` whenever the analyzer's resolve pass misses an
            // edge case.
            if !raw.starts_with('@') {
                let synthetic = lazuli_ir::QualifiedName {
                    feature: None,
                    name: raw.clone(),
                };
                if let Some(enum_decl) = find_enum_decl(module, &synthetic) {
                    return pascal_case(&enum_decl.name);
                }
                if let Some(record) = module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(raw))
                {
                    return pascal_case(&record.name);
                }
                if module
                    .features
                    .iter()
                    .flat_map(|feature| feature.resources.iter())
                    .any(|resource| resource.name.eq_ignore_ascii_case(raw))
                {
                    return "ID".to_owned();
                }
            }
            "unknown".to_owned()
        }
    }
}
