//! Command-output and query-subject inference.
//!
//! Hosts the heuristic that maps `mine_properties` → `Property`
//! (`pick_query_resource_ts`, `pluralize_snake`) plus the
//! return-position type formatters (`command_output_ts_type`,
//! `ts_return_type_for_type_ref`).
//!
//! Lifted out of the `types` god-file in the rails-style R9 split.

use crate::casing::{pascal_case, to_snake_case};

use super::resources::{find_resource, is_resource_ref, resource_ts_name, ts_type_for_type_ref};

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
