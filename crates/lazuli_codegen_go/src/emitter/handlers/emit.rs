//! Stub-file body renderer — turn a `HandlerStub` into the Go source
//! that lands at `app/features/<feature>/handlers/<name>.go`.
//!
//! The output is intentionally user territory: a `// IMPLEMENT ME`
//! marker, an `errors.New(...)` placeholder, and a `func init()` that
//! self-registers with `lazuli.RegisterFn` so the generated
//! `lazuli.ReturnsFromRegistry[I, O]("<feature>.<name>")` callsite can
//! resolve the function without a back-import cycle.
//!
//! Boundary: this module owns the entire file body shape — package
//! header, imports, doc-comment block, function body, init().  Type
//! lookups go through `super::types`, identifier sanitisation through
//! `super::paths`. The walker (`super::collect`) populates the
//! `HandlerStub` that drives the template.

use super::HandlerStub;
use super::paths::{
    escape_comment, escape_string, exported_func_name, handlers_package_name, pascal_case,
};
use super::types::qualify_generated_stub_type;
use crate::emitter::casing;
use crate::emitter::patterns::PATTERN_EXTENSION_STUB;

// ---------------------------------------------------------------------------
// Smart stubs (spec 0025) — delegate-to-runtime starter bodies.
//
// When a stub's binding SITE maps to a known runtime symbol, codegen emits the
// DELEGATING runtime-call body instead of an empty `// IMPLEMENT ME`. The
// mechanism is a single parameterized table (mirrors 0024's REINVENTION_TABLE):
// O(1) growth, no per-case control flow in `emit_stub_contents`.
//
// Honesty limit (spec NL1): this only shapes what codegen writes on a fresh
// scaffold / regenerate of a not-yet-authored handler — `emit_handler_stubs`
// already skips any path that exists on disk, so an existing hand-written
// handler is never touched.
// ---------------------------------------------------------------------------

/// One auto-wire: a stub whose binding SITE ends with `site_suffix` delegates
/// to a known runtime symbol instead of emitting `// IMPLEMENT ME`.
///
/// `site_suffix` matches against `HandlerStub.site` with `.ends_with(...)`.
/// The site is `<feature>.auth.password.hash` (see
/// handlers/collect/feature_walks.rs:254), so the row keys on the stable
/// `.auth.password.hash` tail and is feature-agnostic.
pub(super) struct StubDelegation {
    /// Suffix of `stub.site` that triggers this row, e.g. ".auth.password.hash".
    pub(super) site_suffix: &'static str,
    /// Guard on the RESOLVED Go in/out types. The delegating body is only a
    /// starter that COMPILES if the stub's signature is shape-compatible with
    /// the runtime symbol's signature. A not-yet-tightened handler whose `@fn`
    /// has no extension contract resolves to `(any, any)` — passing `any` to a
    /// `plaintext string` param does NOT compile, so the guard returns false and
    /// the miss path emits the plain `// IMPLEMENT ME` stub instead. Once the
    /// author tightens the `@fn` to `Function[Text, Hashed(...)]`, regenerate
    /// re-emits the delegating body. Receives `(resolved_input, resolved_output)`
    /// after `qualify_generated_stub_type`.
    pub(super) applies: fn(&str, &str) -> bool,
    /// Renders the function body (the statements between the shared
    /// observability prologue and the closing `}`). Receives the resolved
    /// binding context so it can name the contract var + input + output type.
    pub(super) render_body: fn(&DelegationCtx) -> String,
    /// Extra imports this body needs beyond the stub's base set
    /// (context/errors + gen import). For the auth rows: the runtime auth pkg.
    pub(super) extra_imports: &'static [&'static str],
    /// Audit family label (parity with 0024's `family`), for the teach-doc
    /// table and any future diagnostic cross-ref.
    #[allow(dead_code)]
    pub(super) family: &'static str,
}

/// Resolved binding context handed to `render_body`. Built in emit.rs from the
/// `HandlerStub` + casing helpers already in the crate.
pub(super) struct DelegationCtx<'a> {
    /// `pascal_case(stub.feature)` — matches the auth emitter's `feature_pascal`
    /// (emitter/auth/mod.rs:95), so the contract var name lines up exactly.
    pub(super) feature_pascal: &'a str,
    /// The gen-package import alias (`<feature>gen`, casing::gen_package_name)
    /// — the PasswordContract var lives in the generated feature package, so the
    /// body references it as `<feature>gen.<Feature>AuthPassword`.
    pub(super) gen_alias: &'a str,
    /// The stub's single input identifier in scope (the handler param is named
    /// `input`; for `password.hash` the plaintext IS `input` of Go type
    /// `string`).
    pub(super) input_ident: &'a str,
    /// Output Go type (`qualify_generated_stub_type` result) for the `var zero`
    /// fallback the body may still need.
    pub(super) output_type: &'a str,
}

/// Body for the flagship `.auth.password.hash` row.
///
/// Runtime: `auth.HashPassword(ctx *lazuli.Ctx, contract auth.PasswordContract,
/// plaintext string) (string, error)` [runtime/go/lazuli/auth/password.go:165].
/// The auth emitter already emits `var <Feature>AuthPassword =
/// auth.PasswordContract{...}` in the gen package (emitter/auth/contracts.rs:30).
///
/// Output-type bridge: the `@cap.Hashed` column lowers the `@fn` output to
/// `lazuli.HashedRef`, which is a `string` type alias (runtime/go/lazuli/types.go:70).
/// `auth.HashPassword` returns `string`, so the value is directly assignable to
/// the declared output type — no constructor wrap is needed and `return hashed,
/// nil` compiles. (If a pilot ever declares a bare `string` output the same
/// passthrough holds.)
/// Guard for the password-hash row. The runtime takes `plaintext string` and
/// returns `string`; `lazuli.HashedRef` is a `string` alias. So the delegating
/// body compiles iff the resolved input is `string` and the resolved output is a
/// string-shaped type (`lazuli.HashedRef` or bare `string`). Anything else
/// (notably the `(any, any)` fallback of an un-typed `@fn`) takes the miss path.
fn password_hash_applies(input_type: &str, output_type: &str) -> bool {
    input_type == "string" && matches!(output_type, "lazuli.HashedRef" | "string")
}

fn render_password_hash_body(ctx: &DelegationCtx) -> String {
    format!(
        "\thashed, err := auth.HashPassword(ctx, {gen_alias}.{feature_pascal}AuthPassword, {input_ident})\n\
         \tif err != nil {{\n\
         \t\tvar zero {output_type}\n\
         \t\treturn zero, err\n\
         \t}}\n\
         \t// auth.HashPassword returns the canonical PHC string; lazuli.HashedRef\n\
         \t// is a string alias, so the value is the @cap.Hashed column type as-is.\n\
         \treturn {output_type}(hashed), nil",
        gen_alias = ctx.gen_alias,
        feature_pascal = ctx.feature_pascal,
        input_ident = ctx.input_ident,
        output_type = ctx.output_type,
    )
}

/// The SITE→DELEGATION table. One row per proven auto-wire; growth is O(1).
///
/// Seeded: the password HASH row (the Pareto flagship per spec NL3 — a clean
/// 3-arg runtime fn + a stable contract var, output is a string alias).
///
/// NOT seeded (candidate rows — design proof that growth is one row each):
///   - ".auth.password.verify" -> auth.VerifyPassword. The verify `@fn` input
///     is a USER-DEFINED struct (e.g. `Function[PasswordVerifyInput, ...]` in
///     examples/user-auth.lzi:167), whose field names (plaintext / stored hash)
///     are not knowable from codegen, so the delegating body cannot name the
///     accessors. Ship it as a row only once the input shape is a stable,
///     codegen-known carrier (spec ADR verify note + Risks).
///   - ".auth.session.*"        -> auth.MintSessionToken / auth.HashSessionToken.
///   - ".auth.password.reset.*" -> auth.RequestPasswordReset / ConsumePasswordReset.
///   - ".auth.verify.*"         -> auth.IssueVerification / ConsumeVerification.
/// Each becomes one StubDelegation row when its runtime symbol + contract var
/// signature is confirmed stable.
const STUB_DELEGATION_TABLE: &[StubDelegation] = &[StubDelegation {
    site_suffix: ".auth.password.hash",
    applies: password_hash_applies,
    render_body: render_password_hash_body,
    extra_imports: &["lazuli.dev/runtime/lazuli/auth"],
    family: "auth.password-hash",
}];

/// Look up a stub's site against the delegation table. `None` on the miss path
/// (the overwhelming majority) so `emit_stub_contents` falls through to today's
/// byte-identical `// IMPLEMENT ME` body. The site `.ends_with` match is the
/// first gate; the row's `applies` type guard is the second (enforced in
/// `emit_stub_contents` once the Go types are resolved).
pub(super) fn lookup_delegation(site: &str) -> Option<&'static StubDelegation> {
    STUB_DELEGATION_TABLE
        .iter()
        .find(|rule| site.ends_with(rule.site_suffix))
}

pub(super) fn emit_stub_contents(stub: &HandlerStub, module_name: &str) -> String {
    let fn_name = exported_func_name(&stub.name);
    let literal = format!("{}{}", stub.namespace.prefix(), stub.name);
    let escaped_literal = escape_comment(&literal);
    let escaped_site = escape_comment(&stub.site);
    let escaped_error = escape_string(&format!("{} not yet implemented", stub.name));
    let gen_pkg = casing::gen_package_name(&stub.feature);
    let handlers_pkg = handlers_package_name(&stub.feature);
    let qualified_handler = format!("{}.{}", stub.feature, stub.name);
    let gen_import_path = format!("{module_name}/{}", stub.feature);
    let (input_type, input_uses_gen) = qualify_generated_stub_type(&stub.input_type, &gen_pkg);
    let (output_type, output_uses_gen) = qualify_generated_stub_type(&stub.output_type, &gen_pkg);

    // Smart stubs (spec 0025): if the site maps to a runtime symbol AND the
    // resolved Go types are shape-compatible with that symbol, emit the
    // delegating body. Otherwise fall through to the byte-identical plain stub.
    if let Some(rule) = lookup_delegation(&stub.site) {
        if (rule.applies)(&input_type, &output_type) {
            return emit_delegating_stub(stub, module_name, rule, &input_type, &output_type);
        }
    }

    let gen_import = if input_uses_gen || output_uses_gen {
        format!("\t{gen_pkg} \"{gen_import_path}\"\n")
    } else {
        String::new()
    };

    format!(
        r#"// Code generated by lazuli as a starter stub. Edit and remove the
// `// IMPLEMENT ME` marker once you ship the real implementation.
// Lazuli will not overwrite this file on regenerate.

package {package}

import (
	"context"
	"errors"

{gen_import}
	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/observability"
)

// {fn_name} is the user-authored implementation of `{escaped_literal}`.
//   Site: {escaped_site}
// IMPLEMENT ME
//
// The handler self-registers with the runtime at package init() via
// lazuli.RegisterFn. Generated command literals reference it by name
// through lazuli.ReturnsFromRegistry[I, O]("{qualified_handler}"),
// which keeps the gen package free of import cycles into user code.
//
// When you tighten the input/output types, import the generated
// contract package as `{gen_alias}`:
//
//     import {gen_alias} "{gen_import_path}"
//
// and reference types as `{gen_alias}.<TypeName>`.
//lazuli:pattern {pattern_id} {pattern_version}
func {fn_name}(ctx *lazuli.Ctx, input {input_type}) ({output_type}, error) {{
	if ctx.Context == nil {{
		ctx.Context = context.Background()
	}}
	var endOp func()
	ctx.Context, endOp = observability.StartOp(ctx.Context)
	defer endOp()
	_ = ctx
	_ = input
	_ = context.Background
	// Inlined zero value — when two starter stubs land in the same
	// `<feature>handlers` package, a shared `func zero[T any]()` would
	// redeclare across files and break compile. The `var zero` /
	// `return zero` pair stays wire-thin and self-contained per stub.
	var zero {output_type}
	return zero, errors.New("{escaped_error}")
}}

//lazuli:pattern {pattern_id} {pattern_version}
func init() {{
	lazuli.RegisterFn("{qualified_handler}", {fn_name})
}}
"#,
        package = handlers_pkg,
        fn_name = fn_name,
        escaped_literal = escaped_literal,
        escaped_site = escaped_site,
        pattern_id = PATTERN_EXTENSION_STUB.0,
        pattern_version = PATTERN_EXTENSION_STUB.1,
        input_type = input_type,
        output_type = output_type,
        escaped_error = escaped_error,
        gen_alias = gen_pkg,
        gen_import_path = gen_import_path,
        qualified_handler = qualified_handler,
        gen_import = gen_import,
    )
}

/// Emit a delegating starter stub (spec 0025): identical file scaffold to the
/// plain stub — same "will not overwrite" header, same package, same
/// `//lazuli:pattern extension_stub` markers on both the fn and `init()`, same
/// `func init()` + `RegisterFn`, same observability prologue — only the body
/// and the marker comment line differ. The stub stays user territory: the
/// author edits the delegating body only for *custom* behavior.
///
/// The gen import is forced on because the runtime contract var lives in the
/// generated feature package (the body references `<feature>gen.<Feature>...`).
fn emit_delegating_stub(
    stub: &HandlerStub,
    module_name: &str,
    rule: &StubDelegation,
    input_type: &str,
    output_type: &str,
) -> String {
    let fn_name = exported_func_name(&stub.name);
    let literal = format!("{}{}", stub.namespace.prefix(), stub.name);
    let escaped_literal = escape_comment(&literal);
    let escaped_site = escape_comment(&stub.site);
    let gen_pkg = casing::gen_package_name(&stub.feature);
    let feature_pascal = pascal_case(&stub.feature);
    let handlers_pkg = handlers_package_name(&stub.feature);
    let qualified_handler = format!("{}.{}", stub.feature, stub.name);
    let gen_import_path = format!("{module_name}/{}", stub.feature);

    // The gen import is always present for delegating bodies (the contract var
    // lives in the gen package), regardless of whether the IN/OUT types alone
    // would have pulled it in.
    let gen_import = format!("\t{gen_pkg} \"{gen_import_path}\"\n");

    // Extra runtime imports this row's body needs (e.g. the auth pkg). Sorted +
    // one per line, slotted into the same import block as the plain stub.
    let extra_import_lines: String = {
        let mut imports: Vec<&str> = rule.extra_imports.to_vec();
        imports.sort_unstable();
        imports
            .into_iter()
            .map(|path| format!("\t\"{path}\"\n"))
            .collect()
    };

    let ctx = DelegationCtx {
        feature_pascal: &feature_pascal,
        gen_alias: &gen_pkg,
        input_ident: "input",
        output_type,
    };
    let body = (rule.render_body)(&ctx);

    format!(
        r#"// Code generated by lazuli as a starter stub. It delegates to the Lazuli
// runtime — edit it only if you need custom behavior.
// Lazuli will not overwrite this file on regenerate.

package {package}

import (
	"context"

{gen_import}{extra_import_lines}	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/observability"
)

// {fn_name} is the user-authored implementation of `{escaped_literal}`.
//   Site: {escaped_site}
// Delegates to the Lazuli runtime — edit if you need custom behavior.
//
// The handler self-registers with the runtime at package init() via
// lazuli.RegisterFn. Generated command literals reference it by name
// through lazuli.ReturnsFromRegistry[I, O]("{qualified_handler}"),
// which keeps the gen package free of import cycles into user code.
//
// The generated contract package is imported as `{gen_alias}`; reference
// generated types and contract vars as `{gen_alias}.<Name>`.
//lazuli:pattern {pattern_id} {pattern_version}
func {fn_name}(ctx *lazuli.Ctx, input {input_type}) ({output_type}, error) {{
	if ctx.Context == nil {{
		ctx.Context = context.Background()
	}}
	var endOp func()
	ctx.Context, endOp = observability.StartOp(ctx.Context)
	defer endOp()
	_ = context.Background
{body}
}}

//lazuli:pattern {pattern_id} {pattern_version}
func init() {{
	lazuli.RegisterFn("{qualified_handler}", {fn_name})
}}
"#,
        package = handlers_pkg,
        fn_name = fn_name,
        escaped_literal = escaped_literal,
        escaped_site = escaped_site,
        pattern_id = PATTERN_EXTENSION_STUB.0,
        pattern_version = PATTERN_EXTENSION_STUB.1,
        input_type = input_type,
        output_type = output_type,
        gen_alias = gen_pkg,
        qualified_handler = qualified_handler,
        gen_import = gen_import,
        extra_import_lines = extra_import_lines,
        body = body,
    )
}
