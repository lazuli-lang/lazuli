//! Identifier-case helpers shared across the runtime TS emitter.
//!
//! Owns three primitives — `pascal_case`, `lower_camel`, `field_kind_ts` —
//! plus the `is_acronym` table that keeps `id` / `url` / `api` etc. all-caps
//! across both casings. The runtime emitter, query naming heuristics, and
//! lifecycle action maps all consume these; the case helpers also have to
//! agree with `runtime/ts/lazuli/src/case-mapper.ts` so the wire boundary
//! round-trips snake_case ↔ camelCase exactly.
//!
//! `lower_camel_export` is the public re-export entry point: the CLI zod
//! emitter consumes it via `lazuli_codegen_ts::lower_camel_export` so SDK
//! and zod surfaces share a single casing source of truth.

use lazuli_codegen_spec::FieldKind;

/// Convert a wire field name (the Go struct's `json:"…"` tag) into the
/// camelCase TypeScript interface key. This is a faithful PORT of the
/// runtime [`snakeToCamelKey`] in `runtime/ts/lazuli/src/case-mapper.ts`:
/// the generated TS key MUST equal whatever the runtime mapper produces
/// from the wire field, otherwise the SDK declares a key the payload
/// never carries and the value reads back `undefined`.
///
/// Contract: `ts_key(field) == snakeToCamelKey(go_json_tag(field))`.
/// The Go emitter writes the DSL field name VERBATIM as the json tag
/// (`crates/lazuli_codegen_go/.../struct_emit.rs`), and the inbound
/// case-mapper only collapses `_`/`-` separators — it never lower-cases
/// or acronym-uppercases. So this function must do exactly the same:
///
/// - split on `_` / `-` only;
/// - upper-case the first letter of each NON-leading segment;
/// - leave every other character (including the entire leading segment)
///   untouched, preserving existing case.
///
/// That makes already-camelCase / PascalCase / acronym names pass
/// through unchanged (`registrationStep` → `registrationStep`,
/// `HTMLBody` → `HTMLBody`, `URL` → `URL`) while snake/kebab names fold
/// to camelCase (`org_id` → `orgId`, `created_at` → `createdAt`).
///
/// Re-exported via `lazuli_codegen_ts::lower_camel_export` for the CLI
/// SDK/zod emitters (single source of truth for SDK/zod casing).
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_ts::lower_camel_export;
/// assert_eq!(lower_camel_export("org_id"), "orgId");
/// assert_eq!(lower_camel_export("created_at"), "createdAt");
/// assert_eq!(lower_camel_export("registrationStep"), "registrationStep");
/// ```
pub fn lower_camel_export(s: &str) -> String {
    lower_camel(s)
}

pub(super) fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(['_', '-']) {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

pub(super) fn lower_camel(s: &str) -> String {
    // Faithful port of `snakeToCamelKey` (case-mapper.ts): the only
    // transform applied to the verbatim Go json tag is collapsing
    // `_`/`-` separators by upper-casing the following letter. Existing
    // case is otherwise preserved EXACTLY — we must NOT `to_lowercase`
    // the leading segment (that would turn the already-camelCase
    // `registrationStep` into `registrationstep`, a key the wire never
    // carries → silent `undefined`) and must NOT acronym-uppercase
    // (the wire field is whatever the DSL author wrote, and the runtime
    // mapper does no such thing).
    let mut out = String::with_capacity(s.len());
    let mut first = true;
    for word in s.split(['_', '-']) {
        if word.is_empty() {
            continue;
        }
        if first {
            // Leading segment: verbatim, preserving its existing case.
            out.push_str(word);
            first = false;
            continue;
        }
        // Non-leading segment: upper-case only its first character; the
        // remainder keeps its original case (so `HTTP_body` → `HTTPBody`,
        // `created_at` → `createdAt`).
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            for u in c.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

/// camelCase key for surfaces whose Go json tag is emitted LOWER-CASED
/// rather than verbatim — namely command **inputs** and query **args**.
/// The Go runtime emitters write `json:"{field_name.to_ascii_lowercase()}"`
/// for those structs (see `lazuli_codegen_go/src/runtime/command.rs` and
/// `query.rs`), so the SDK key must mirror that: lower-case the whole
/// name first, *then* fold snake/kebab separators to camelCase. This
/// keeps `ts_key == snakeToCamelKey(go_json_tag)` on these surfaces too.
///
/// `Name` → `name`, `full_name` → `fullName`, `Email` → `email`.
pub(super) fn lower_camel_wire_lowered(s: &str) -> String {
    lower_camel(&s.to_ascii_lowercase())
}

/// Port of `snakeToCamelKey` from `runtime/ts/lazuli/src/case-mapper.ts`,
/// used only by the casing-invariant tests as the authoritative
/// definition of what the runtime client produces from a wire field.
/// Kept in lock-step with that TS source: collapse interior `_`
/// separators (not leading/trailing) by upper-casing the next char.
#[cfg(test)]
pub(super) fn snake_to_camel_wire(key: &str) -> String {
    let bytes: Vec<char> = key.chars().collect();
    let mut out = String::with_capacity(key.len());
    let mut upper = false;
    for (i, &ch) in bytes.iter().enumerate() {
        if ch == '_' && i > 0 && i < bytes.len() - 1 {
            upper = true;
            continue;
        }
        if upper {
            for u in ch.to_uppercase() {
                out.push(u);
            }
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}

pub(super) fn field_kind_ts(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text | FieldKind::Email => "string",
        FieldKind::Integer => "ID",
        FieldKind::Boolean => "boolean",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_camel_export_handles_snake_case_identifiers() {
        assert_eq!(lower_camel_export("org_id"), "orgId");
        assert_eq!(lower_camel_export("created_at"), "createdAt");
    }

    #[test]
    fn lower_camel_preserves_already_camel_case() {
        // W1-4 CODEGEN-CASING-DRIFT regression: the live `registrationStep`
        // bug. An already-camelCase DSL field name must pass through
        // UNCHANGED — the old impl `to_ascii_lowercase()`'d the whole
        // leading segment, yielding `registrationstep`, a key the wire
        // payload never carries, so `result.registrationStep` read
        // `undefined` on the client.
        assert_eq!(lower_camel_export("registrationStep"), "registrationStep");
        assert_eq!(lower_camel_export("apiKey"), "apiKey");
    }

    #[test]
    fn lower_camel_preserves_pascal_and_acronyms() {
        // PascalCase / acronym-bearing names also pass through verbatim,
        // because the Go json tag is emitted verbatim and the runtime
        // case-mapper is a no-op on names without `_`/`-`.
        assert_eq!(lower_camel_export("HTMLBody"), "HTMLBody");
        assert_eq!(lower_camel_export("URL"), "URL");
        assert_eq!(lower_camel_export("OrgId"), "OrgId");
    }

    /// THE core invariant for W1-4: the TypeScript interface key the SDK
    /// emitter writes (`lower_camel`) MUST equal what the runtime client
    /// derives from the wire field (`snakeToCamelKey` over the Go json
    /// tag). The Go emitter writes the DSL field name VERBATIM as the
    /// json tag, so `go_json_tag(field) == field_name`. Hence the
    /// end-to-end equality the client depends on is:
    ///
    ///   ts_key(field) == snake_to_camel_wire(go_json_tag(field))
    ///
    /// If this ever fails, the generated SDK declares a key the wire
    /// payload never carries → the value silently reads `undefined`.
    #[test]
    fn ts_key_equals_wire_round_trip_for_every_field_shape() {
        // Representative field-name catalog spanning every casing shape
        // the DSL admits. Each entry is the verbatim DSL field name (==
        // the Go json tag the emitter writes).
        let fields = [
            // snake_case
            "tenant_id",
            "org_id",
            "created_at",
            "registration_step",
            "provider_payment_id",
            // already camelCase (the live bug)
            "registrationStep",
            "apiKey",
            // PascalCase
            "OrgId",
            "HTMLBody",
            // bare acronym
            "URL",
            // single lowercase word
            "token",
            "role",
        ];
        // NB: field names are snake_case / camelCase identifiers — the
        // lexer forbids `-`, so kebab is not a wire-field shape and is
        // intentionally excluded (the runtime `snakeToCamelKey` collapses
        // only `_`, not `-`, so a hyphenated tag could never round-trip).
        for field in fields {
            let ts_key = lower_camel(field);
            // `go_json_tag(field)` is the verbatim DSL name; the client
            // reads it back through the runtime case-mapper.
            let go_json_tag = field;
            let client_sees = snake_to_camel_wire(go_json_tag);
            assert_eq!(
                ts_key, client_sees,
                "casing drift for field `{field}`: SDK declares key `{ts_key}` \
                 but the client reads `{client_sees}` off the wire (json tag \
                 `{go_json_tag}`) — value would be silently dropped",
            );
        }
    }

    #[test]
    fn pascal_case_uppercases_known_acronyms() {
        assert_eq!(pascal_case("api_url"), "APIURL");
        assert_eq!(pascal_case("user_id"), "UserID");
    }

    #[test]
    fn field_kind_ts_maps_known_kinds() {
        assert_eq!(field_kind_ts(FieldKind::Text), "string");
        assert_eq!(field_kind_ts(FieldKind::Integer), "ID");
        assert_eq!(field_kind_ts(FieldKind::Boolean), "boolean");
    }
}
