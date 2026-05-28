//! HANDLER-SIGNATURE-MISMATCH-001 — handler's Go function signature
//! disagrees with the codegen-emitted `Command[Input, Output]`
//! materialisation in `dist/go/<feature>/command.gen.go`.
//!
//! Sibling of [`super::handler_missing_001`]. That rule only stats the
//! file. THIS rule, once the file exists, opens both the handler Go
//! source and the codegen Go source, extracts the operative type idents
//! from each, and fires when they disagree.
//!
//! The runtime's [`ReturnsFromRegistry[I, O]`](
//! ../../../../runtime/go/lazuli/handler_registry.go) performs the same
//! comparison at dispatch time and returns a 500 `wrong signature`
//! error when they don't match. This rule promotes that runtime check
//! into a static check — the doctor's whole job per the runtime comment
//! at `handler_registry.go:67-72`.
//!
//! ## Detection
//!
//! For every `@fn.<name>` command-handler reference in the IR:
//!
//! 1. Locate the handler Go file via [`crate::handler_path::resolve`].
//!    Missing → skip (HANDLER-MISSING-001 owns that surface).
//! 2. Locate the codegen file at
//!    `dist/go/<feature>/command.gen.go`. Missing → skip silently
//!    (a sibling `@correctness.migration_out_of_sync`-shaped rule owns
//!    the "you didn't run lazuli generate go" path).
//! 3. Extract `func PascalCase(ctx *lazuli.Ctx, input <Type>) (<Out>, error)`
//!    from the handler. Unreadable → emit a
//!    [`Diff::HandlerSignatureUnreadable`] finding (conservative — the
//!    author opts out via `# doctor:allow` with a reason, leaving a
//!    paper trail).
//! 4. Extract `var <camel> = lazuli.Command[<I>, <O>]{ Name: "<f>.<c>", ... }`
//!    from the codegen, matching the `Name:` field. Not found → skip
//!    (cell A of `@correctness.migration_out_of_sync` covers IR↔codegen
//!    drift in the other direction).
//! 5. Normalise both sides by stripping a `<feature>gen.` package
//!    prefix and compare ident strings. Mismatch → finding.
//!
//! ## Severity
//!
//! Conservative `warning` here at the rule level; the doctor dispatcher
//! escalates to `error` in `strict`, `production`, and `tdd-iron-hand`
//! profiles via the existing severity-mapping machinery (mirrors
//! [`super::handler_missing_001`]). The dispatcher mapping is **not**
//! in this module's purview; it returns plain [`Finding`] values.
//!
//! ## Opt-out
//!
//! `# doctor:allow HANDLER-SIGNATURE-MISMATCH-001 — reason "..."` in
//! the `.lzi` file silences the finding for every site in that file.
//! Uses [`crate::allow_comment::file_contains_doctor_allow`].
//!
//! Reference: docs/proposals/doctor-handler-signature-mismatch.md.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

use crate::allow_comment::file_contains_doctor_allow;
use crate::handler_path;
use crate::handler_walker::{HandlerSite, HandlerSiteKind, iter_handler_sites};

/// Kind of signature drift surfaced by the rule. Drives the diagnostic
/// message and lets downstream consumers (IDE squiggle colour, JSON
/// projection) discriminate without re-parsing the message string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diff {
    /// Handler's input parameter type ident differs from the codegen
    /// `Command[<I>, _]` first type-parameter.
    InputMismatch { expected: String, found: String },
    /// Handler's first return type ident differs from the codegen
    /// `Command[_, <O>]` second type-parameter.
    OutputMismatch { expected: String, found: String },
    /// Both input AND output disagree. Emitted as one finding so the
    /// author sees the full drift in one diagnostic instead of two.
    Both {
        input_expected: String,
        input_found: String,
        output_expected: String,
        output_found: String,
    },
    /// Handler file exists but the rule's narrow byte-walker could not
    /// extract a recognisable `func PascalCase(ctx *lazuli.Ctx, input <T>) (<O>, error)`
    /// signature. Cause is usually a type alias the rule can't resolve
    /// or a novel signature shape — author opts out with
    /// `# doctor:allow` plus a reason rather than the rule silently
    /// passing what may be a real drift.
    HandlerSignatureUnreadable,
    /// Codegen `Command[...]` block was found but the handler file does
    /// NOT export the expected `PascalCase(handler_name)` symbol.
    /// Distinct from the `HANDLER-MISSING-001` surface (which gates on
    /// file presence): this variant fires when the file is on disk but
    /// the expected export was renamed away.
    MissingHandler,
}

/// One HANDLER-SIGNATURE-MISMATCH-001 finding — a handler's Go
/// signature does not match the codegen-emitted `Command[I, O]`
/// for the same operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the reference was authored in.
    pub path: PathBuf,
    /// Feature name.
    pub feature: String,
    /// Command name that holds the `@fn.X` reference.
    pub command: String,
    /// Handler symbol stem (snake_case).
    pub handler_name: String,
    /// Canonical handler `.go` path that was inspected.
    pub handler_path: PathBuf,
    /// Codegen `command.gen.go` path that was inspected.
    pub gen_path: PathBuf,
    /// Diff kind — drives the message and disambiguates input vs output.
    pub diff: Diff,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "HANDLER-SIGNATURE-MISMATCH-001";

    /// Render the diagnostic message naming the expected vs actual
    /// signature and pointing at the runtime line where the assertion
    /// would fail. The literal `handler_registry.go:89` anchor is
    /// load-bearing per the proposal §Diagnostic shape — agents
    /// reading the diagnostic should be able to jump to the runtime
    /// type-assertion site to verify the claim.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::handler_signature_mismatch_001::{Diff, Finding};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("account.lzi"),
    ///     feature: "account".into(),
    ///     command: "login_with_google".into(),
    ///     handler_name: "login_with_google".into(),
    ///     handler_path: PathBuf::from("features/account/handlers/login_with_google.go"),
    ///     gen_path: PathBuf::from("dist/go/account/command.gen.go"),
    ///     diff: Diff::OutputMismatch {
    ///         expected: "struct{}".into(),
    ///         found: "string".into(),
    ///     },
    /// };
    /// assert!(f.message().contains("handler_registry.go:89"));
    /// ```
    pub fn message(&self) -> String {
        let suffix = format!(
            " The signature mismatch will cause a 500 `wrong signature` \
             at dispatch time (runtime/go/lazuli/handler_registry.go:89). \
             Either edit {} to match, or change the command's input/returns \
             in the .lzi and re-run `lazuli generate go`. Opt out (with \
             reason) via `# doctor:allow {} — reason \"...\"` in the .lzi.",
            self.handler_path.display(),
            Self::CODE,
        );

        match &self.diff {
            Diff::InputMismatch { expected, found } => format!(
                "handler `{}` (command `{}.{}`) takes input `{}` but codegen at {} \
                 declared `lazuli.Command[{}, _]`.{}",
                self.handler_name,
                self.feature,
                self.command,
                found,
                self.gen_path.display(),
                expected,
                suffix,
            ),
            Diff::OutputMismatch { expected, found } => format!(
                "handler `{}` (command `{}.{}`) returns `({}, error)` but codegen at {} \
                 declared `lazuli.Command[_, {}]`.{}",
                self.handler_name,
                self.feature,
                self.command,
                found,
                self.gen_path.display(),
                expected,
                suffix,
            ),
            Diff::Both {
                input_expected,
                input_found,
                output_expected,
                output_found,
            } => format!(
                "handler `{}` (command `{}.{}`) has signature `(ctx, {}) ({}, error)` \
                 but codegen at {} declared `lazuli.Command[{}, {}]`.{}",
                self.handler_name,
                self.feature,
                self.command,
                input_found,
                output_found,
                self.gen_path.display(),
                input_expected,
                output_expected,
                suffix,
            ),
            Diff::HandlerSignatureUnreadable => format!(
                "handler `{}` (command `{}.{}`) at {} has a signature the rule could \
                 not parse (possible type alias or novel shape). Codegen at {} \
                 expects `lazuli.Command[I, O]` with specific idents — verify \
                 manually and opt out with `# doctor:allow {} — reason \"...\"` \
                 once confirmed.",
                self.handler_name,
                self.feature,
                self.command,
                self.handler_path.display(),
                self.gen_path.display(),
                Self::CODE,
            ),
            Diff::MissingHandler => format!(
                "handler `{}` (command `{}.{}`) referenced from .lzi but no \
                 exported `func {}` was found in {}. Codegen at {} expects this \
                 symbol.{}",
                self.handler_name,
                self.feature,
                self.command,
                pascal_case(&self.handler_name),
                self.handler_path.display(),
                self.gen_path.display(),
                suffix,
            ),
        }
    }
}

/// Run the check across every `CommandHandler` site with namespace `fn`
/// in `feature`. `app_root` is the directory containing `features/`;
/// `dist_root` is the directory containing `go/<feature>/command.gen.go`.
///
/// Returns one finding per drifted site. Empty when:
/// - codegen file missing,
/// - handler file missing (delegated to HANDLER-MISSING-001),
/// - signatures match,
/// - `# doctor:allow HANDLER-SIGNATURE-MISMATCH-001` opt-out present.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::handler_signature_mismatch_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with @fn handler refs");
/// let _ = check(
///     &feature,
///     Path::new("account.lzi"),
///     Path::new("/app"),
///     Path::new("/app/dist"),
/// );
/// ```
pub fn check(
    feature: &Feature,
    lzi_path: &Path,
    app_root: &Path,
    dist_root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Opt-out short-circuit — same precedent as vocab rules.
    if file_contains_doctor_allow(lzi_path, Finding::CODE) {
        return findings;
    }

    let gen_path = dist_root
        .join("go")
        .join(&feature.name)
        .join("command.gen.go");
    let Ok(gen_source) = std::fs::read_to_string(&gen_path) else {
        // No codegen output yet — silent. Sibling rules cover the
        // "run lazuli generate go" surface.
        return findings;
    };

    for site in iter_handler_sites(feature) {
        if !is_command_fn_handler(&site) {
            continue;
        }
        let handler_path = handler_path::resolve(app_root, &feature.name, &site.handler_name);
        let Ok(handler_source) = std::fs::read_to_string(&handler_path) else {
            // HANDLER-MISSING-001 covers missing files; don't double-fire.
            continue;
        };

        let qualified_name = format!("{}.{}", feature.name, site.construct_name);
        let Some(gen_sig) = extract_command_signature(&gen_source, &qualified_name) else {
            // No matching Command[...] block — IR↔codegen drift in the
            // OTHER direction. Sibling rule covers this.
            continue;
        };

        let pascal = pascal_case(&site.handler_name);
        match extract_handler_signature(&handler_source, &pascal) {
            HandlerExtractResult::Found(handler_sig) => {
                if let Some(diff) = diff_signatures(&handler_sig, &gen_sig, &feature.name) {
                    findings.push(Finding {
                        path: lzi_path.to_path_buf(),
                        feature: feature.name.clone(),
                        command: site.construct_name.clone(),
                        handler_name: site.handler_name.clone(),
                        handler_path,
                        gen_path: gen_path.clone(),
                        diff,
                    });
                }
            }
            HandlerExtractResult::FunctionMissing => {
                // Function with the expected PascalCase name not found —
                // file exists, but the export drifted. v0.1 surfaces
                // this with the MissingHandler diff so the message is
                // unambiguous.
                findings.push(Finding {
                    path: lzi_path.to_path_buf(),
                    feature: feature.name.clone(),
                    command: site.construct_name.clone(),
                    handler_name: site.handler_name.clone(),
                    handler_path,
                    gen_path: gen_path.clone(),
                    diff: Diff::MissingHandler,
                });
            }
            HandlerExtractResult::Unreadable => {
                findings.push(Finding {
                    path: lzi_path.to_path_buf(),
                    feature: feature.name.clone(),
                    command: site.construct_name.clone(),
                    handler_name: site.handler_name.clone(),
                    handler_path,
                    gen_path: gen_path.clone(),
                    diff: Diff::HandlerSignatureUnreadable,
                });
            }
        }
    }

    findings
}

fn is_command_fn_handler(site: &HandlerSite) -> bool {
    matches!(site.kind, HandlerSiteKind::CommandHandler) && site.handler_namespace == "fn"
}

// ── signature extraction ────────────────────────────────────────────────────

/// Operative idents extracted from a handler signature.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HandlerSig {
    input: String,
    output: String,
}

/// Operative idents extracted from a codegen `Command[I, O]` block.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GenSig {
    input: String,
    output: String,
}

/// Result of attempting to extract a handler signature.
#[derive(Debug)]
enum HandlerExtractResult {
    /// Successfully parsed a `func Name(ctx *lazuli.Ctx, input T) (O, error)`
    /// shape.
    Found(HandlerSig),
    /// The expected exported function name was not present at all.
    FunctionMissing,
    /// Found a function with the right name but its signature shape did
    /// not match the canonical `(ctx, input) (output, error)` pattern.
    Unreadable,
}

/// Walk `source` (raw Go) looking for `func <pascal_name>(`. When found,
/// parse the parameter list and return-type list.
///
/// Recognises the canonical Lazuli command-handler shape:
///
/// ```text
/// func Name(ctx *lazuli.Ctx, input <Type>) (<Output>, error) {
/// ```
///
/// Anything else returns [`HandlerExtractResult::Unreadable`].
fn extract_handler_signature(source: &str, pascal_name: &str) -> HandlerExtractResult {
    let needle = format!("func {}(", pascal_name);
    let Some(start) = source.find(&needle) else {
        return HandlerExtractResult::FunctionMissing;
    };
    let after_open = start + needle.len();
    let bytes = source.as_bytes();

    // Walk to the matching `)` at depth 0 (depth-tracking handles
    // anonymous-struct field types like `input struct{ X int }` though
    // we don't expect them in canonical handlers).
    let params_end = match scan_to_matching(bytes, after_open, b'(', b')') {
        Some(p) => p,
        None => return HandlerExtractResult::Unreadable,
    };
    let params = &source[after_open..params_end];

    // After the closing `)` of the parameter list, the return type list
    // either starts with `(` (multi-return — our case, since we expect
    // `(Output, error)`) or with a single type ident. Canonical Lazuli
    // handlers always use the multi-return parenthesised form.
    let mut idx = params_end + 1;
    skip_whitespace(bytes, &mut idx);
    if idx >= bytes.len() || bytes[idx] != b'(' {
        return HandlerExtractResult::Unreadable;
    }
    let returns_open = idx + 1;
    let returns_end = match scan_to_matching(bytes, returns_open, b'(', b')') {
        Some(p) => p,
        None => return HandlerExtractResult::Unreadable,
    };
    let returns = &source[returns_open..returns_end];

    let Some(input_ident) = extract_input_ident(params) else {
        return HandlerExtractResult::Unreadable;
    };
    let Some(output_ident) = extract_output_ident(returns) else {
        return HandlerExtractResult::Unreadable;
    };

    HandlerExtractResult::Found(HandlerSig {
        input: input_ident,
        output: output_ident,
    })
}

/// Extract the input parameter's type from the handler param list.
///
/// Canonical shape: `ctx *lazuli.Ctx, input <Type>` — two parameters.
/// We split on the top-level comma (depth-aware so generic params or
/// anonymous-struct field commas don't fool us), then take the second
/// parameter's type (everything after the name token).
fn extract_input_ident(params: &str) -> Option<String> {
    let parts = split_top_level_commas(params);
    if parts.len() != 2 {
        return None;
    }
    // The first parameter must be `<name> *<...>Ctx` or `<name> *Ctx` —
    // we don't enforce the name, just verify the second param's shape.
    let second = parts[1].trim();
    // `name TYPE` — split on first ASCII whitespace.
    let (name, ty) = split_param_name_and_type(second)?;
    if name.is_empty() || ty.is_empty() {
        return None;
    }
    Some(canonical_ident(ty))
}

/// Extract the first return-type ident. Canonical shape:
/// `<Output>, error`. We split on the top-level comma and require the
/// second to be `error`. Returns the trimmed first part.
fn extract_output_ident(returns: &str) -> Option<String> {
    let parts = split_top_level_commas(returns);
    if parts.len() != 2 {
        return None;
    }
    if parts[1].trim() != "error" {
        return None;
    }
    let first = parts[0].trim();
    if first.is_empty() {
        return None;
    }
    Some(canonical_ident(first))
}

/// Split a Go parameter `name TYPE` into the two halves. Handles
/// `input accountgen.Foo` and `input  struct{}` (extra whitespace).
fn split_param_name_and_type(param: &str) -> Option<(&str, &str)> {
    let trimmed = param.trim();
    let mut split_at = None;
    for (i, ch) in trimmed.char_indices() {
        if ch.is_ascii_whitespace() {
            split_at = Some(i);
            break;
        }
    }
    let split_at = split_at?;
    let name = trimmed[..split_at].trim();
    let ty = trimmed[split_at..].trim();
    Some((name, ty))
}

// ── codegen extraction ──────────────────────────────────────────────────────

/// Walk codegen Go source looking for a `lazuli.Command[I, O]{ ... Name: "<qualified>" ... }`
/// block. Returns the two type-parameter idents.
///
/// Strategy: find every `lazuli.Command[` occurrence; for each, parse
/// the bracketed type-parameter list, then scan forward until the
/// matching `{` block and look for `Name: "<qualified>"`. Stop on
/// first match.
fn extract_command_signature(source: &str, qualified_name: &str) -> Option<GenSig> {
    let bytes = source.as_bytes();
    let needle = "lazuli.Command[";
    let mut search_from = 0usize;
    let target_name_literal = format!("\"{}\"", qualified_name);

    while let Some(rel) = source[search_from..].find(needle) {
        let start = search_from + rel;
        let bracket_open = start + needle.len();
        // Walk to matching ']' at depth 0.
        let bracket_end = match scan_to_matching(bytes, bracket_open, b'[', b']') {
            Some(p) => p,
            None => {
                search_from = bracket_open;
                continue;
            }
        };
        let type_params = &source[bracket_open..bracket_end];
        let parts = split_top_level_commas(type_params);
        if parts.len() != 2 {
            search_from = bracket_end + 1;
            continue;
        }
        let input = canonical_ident(parts[0].trim());
        let output = canonical_ident(parts[1].trim());

        // Now find the `{` immediately after the `]` (Go allows
        // whitespace between them, e.g. `Command[I, O] {`).
        let mut idx = bracket_end + 1;
        skip_whitespace(bytes, &mut idx);
        if idx >= bytes.len() || bytes[idx] != b'{' {
            search_from = bracket_end + 1;
            continue;
        }
        let brace_open = idx + 1;
        let brace_end = match scan_to_matching(bytes, brace_open, b'{', b'}') {
            Some(p) => p,
            None => {
                search_from = brace_open;
                continue;
            }
        };
        let body = &source[brace_open..brace_end];

        // Look for Name: "<qualified>" anywhere in the body. Conservative
        // match — the codegen always emits `Name:` with this exact shape.
        if body.contains(&target_name_literal) {
            return Some(GenSig { input, output });
        }
        search_from = brace_end + 1;
    }
    None
}

// ── ident normalisation ─────────────────────────────────────────────────────

/// Compute the diff between handler and gen signatures, normalising
/// idents against the `<feature>gen.` package prefix.
fn diff_signatures(handler: &HandlerSig, gen_sig: &GenSig, feature: &str) -> Option<Diff> {
    let prefix = format!("{}gen.", feature);
    let h_in = strip_prefix(&handler.input, &prefix);
    let h_out = strip_prefix(&handler.output, &prefix);
    let g_in = strip_prefix(&gen_sig.input, &prefix);
    let g_out = strip_prefix(&gen_sig.output, &prefix);

    let input_match = idents_match(h_in, g_in);
    let output_match = idents_match(h_out, g_out);

    match (input_match, output_match) {
        (true, true) => None,
        (false, true) => Some(Diff::InputMismatch {
            expected: g_in.to_owned(),
            found: h_in.to_owned(),
        }),
        (true, false) => Some(Diff::OutputMismatch {
            expected: g_out.to_owned(),
            found: h_out.to_owned(),
        }),
        (false, false) => Some(Diff::Both {
            input_expected: g_in.to_owned(),
            input_found: h_in.to_owned(),
            output_expected: g_out.to_owned(),
            output_found: h_out.to_owned(),
        }),
    }
}

/// Strip the `<feature>gen.` package prefix when present so the rule
/// can compare `accountgen.LoginInput` vs `LoginInput` cleanly.
fn strip_prefix<'a>(ident: &'a str, prefix: &str) -> &'a str {
    ident.strip_prefix(prefix).unwrap_or(ident)
}

/// Compare two idents after collapsing internal whitespace. Mainly
/// handles `struct {}` vs `struct{}` and similar trivial whitespace
/// drift; non-trivial divergence (extra fields inside an anonymous
/// struct) still differs, which is what we want.
fn idents_match(a: &str, b: &str) -> bool {
    collapse_ws(a) == collapse_ws(b)
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if !ch.is_whitespace() {
            out.push(ch);
        }
    }
    out
}

/// Convert a snake_case handler name to PascalCase (mirrors codegen).
/// `verify_password_v2` → `VerifyPasswordV2`.
fn pascal_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper_next = true;
    for ch in snake.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Trim and lightly normalise an ident. Whitespace collapsing happens
/// at compare-time via [`collapse_ws`].
fn canonical_ident(raw: &str) -> String {
    raw.trim().to_owned()
}

// ── byte-walker helpers ─────────────────────────────────────────────────────

/// Walk `bytes` from `start` forward, tracking `open`/`close` bracket
/// depth (starting at 1 because the caller has already consumed the
/// initial open). Returns the index of the matching close-bracket when
/// the depth reaches zero, or `None` when the input runs out.
///
/// Aware of Go string literals (double-quoted, backtick raw) and
/// line comments (`//`) and block comments (`/* */`) so we don't
/// trip on brackets inside them.
fn scan_to_matching(bytes: &[u8], start: usize, open: u8, close: u8) -> Option<usize> {
    let mut idx = start;
    let mut depth: i64 = 1;
    while idx < bytes.len() {
        let b = bytes[idx];
        // Skip Go string literals.
        if b == b'"' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'"' {
                if bytes[idx] == b'\\' && idx + 1 < bytes.len() {
                    idx += 2;
                    continue;
                }
                idx += 1;
            }
            idx += 1;
            continue;
        }
        if b == b'`' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'`' {
                idx += 1;
            }
            idx += 1;
            continue;
        }
        if b == b'/' && idx + 1 < bytes.len() {
            if bytes[idx + 1] == b'/' {
                // Line comment — skip to newline.
                idx += 2;
                while idx < bytes.len() && bytes[idx] != b'\n' {
                    idx += 1;
                }
                continue;
            }
            if bytes[idx + 1] == b'*' {
                // Block comment — skip to `*/`.
                idx += 2;
                while idx + 1 < bytes.len() && !(bytes[idx] == b'*' && bytes[idx + 1] == b'/') {
                    idx += 1;
                }
                idx += 2;
                continue;
            }
        }
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
        idx += 1;
    }
    None
}

/// Advance `*idx` past ASCII whitespace and Go line/block comments.
fn skip_whitespace(bytes: &[u8], idx: &mut usize) {
    while *idx < bytes.len() {
        let b = bytes[*idx];
        if b.is_ascii_whitespace() {
            *idx += 1;
            continue;
        }
        if b == b'/' && *idx + 1 < bytes.len() {
            if bytes[*idx + 1] == b'/' {
                *idx += 2;
                while *idx < bytes.len() && bytes[*idx] != b'\n' {
                    *idx += 1;
                }
                continue;
            }
            if bytes[*idx + 1] == b'*' {
                *idx += 2;
                while *idx + 1 < bytes.len() && !(bytes[*idx] == b'*' && bytes[*idx + 1] == b'/') {
                    *idx += 1;
                }
                *idx += 2;
                continue;
            }
        }
        break;
    }
}

/// Split `s` on commas at bracket-depth 0. Bracket-aware so generic
/// type parameters like `Foo[A, B]` are not torn apart at the inner
/// comma. Tracks `()`, `[]`, `{}`.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut depth_brace = 0i32;
    let mut start = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        // Skip string literals — inline so we don't return a tuple.
        if b == b'"' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'"' {
                if bytes[idx] == b'\\' && idx + 1 < bytes.len() {
                    idx += 2;
                    continue;
                }
                idx += 1;
            }
            idx += 1;
            continue;
        }
        if b == b'`' {
            idx += 1;
            while idx < bytes.len() && bytes[idx] != b'`' {
                idx += 1;
            }
            idx += 1;
            continue;
        }
        match b {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'[' => depth_brack += 1,
            b']' => depth_brack -= 1,
            b'{' => depth_brace += 1,
            b'}' => depth_brace -= 1,
            b',' if depth_paren == 0 && depth_brack == 0 && depth_brace == 0 => {
                out.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    if start <= s.len() {
        out.push(&s[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, HandlerRef,
        Policies, PolicyRef, ReturnsEffect, TypeRef,
    };
    use std::fs;
    use tempfile::TempDir;

    fn mk_cmd_with_handler(name: &str, handler_name: &str) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
            }),
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: Some(HandlerRef {
                namespace: "fn".into(),
                name: handler_name.to_owned(),
                span_ref: None,
            }),
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(name: &str, commands: Vec<Command>) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    /// Write a handler `.go` and a `command.gen.go` under the canonical
    /// `<app_root>/features/<feature>/handlers/` and
    /// `<dist_root>/go/<feature>/` paths.
    fn lay_out_files(
        app_root: &Path,
        dist_root: &Path,
        feature: &str,
        handler_name: &str,
        handler_source: &str,
        gen_source: &str,
    ) {
        let handler_dir = handler_path::handlers_dir(app_root, feature);
        fs::create_dir_all(&handler_dir).unwrap();
        fs::write(
            handler_dir.join(format!("{handler_name}.go")),
            handler_source,
        )
        .unwrap();

        let gen_dir = dist_root.join("go").join(feature);
        fs::create_dir_all(&gen_dir).unwrap();
        fs::write(gen_dir.join("command.gen.go"), gen_source).unwrap();
    }

    fn write_lzi(lzi_path: &Path, source: &str) {
        if let Some(parent) = lzi_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(lzi_path, source).unwrap();
    }

    const MATCHING_HANDLER: &str = r#"package accounthandlers

import (
    "lazuli.dev/runtime/lazuli"
    accountgen "github.com/example/account"
)

func Login(ctx *lazuli.Ctx, input accountgen.LoginInput) (string, error) {
    return "token", nil
}
"#;

    const MATCHING_GEN: &str = r#"package accountgen

import "lazuli.dev/runtime/lazuli"

var login = lazuli.Command[LoginInput, string]{
    Name: "account.login",
    Effect: lazuli.ReturnsFromRegistry[LoginInput, string]("account.login"),
}
"#;

    #[test]
    fn happy_path_matching_signatures_silent() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            MATCHING_HANDLER,
            MATCHING_GEN,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(
            findings.is_empty(),
            "expected no findings, got {:?}",
            findings
        );
        assert_eq!(Finding::CODE, "HANDLER-SIGNATURE-MISMATCH-001");
    }

    #[test]
    fn output_mismatch_string_vs_struct_fires() {
        // The hostpoint Google-OAuth bug verbatim: handler returns
        // (string, error) but codegen emitted Command[..., struct{}].
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
import "lazuli.dev/runtime/lazuli"
func LoginWithGoogle(ctx *lazuli.Ctx, input accountgen.LoginResultWithGoogleInput) (string, error) {
    return "token", nil
}
"#;
        let gen_src = r#"package accountgen
var loginResultWithGoogle = lazuli.Command[LoginResultWithGoogleInput, struct{}]{
    Name: "account.login_with_google",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login_with_google",
            handler_src,
            gen_src,
        );

        let feature = mk_feature(
            "account",
            vec![mk_cmd_with_handler(
                "login_with_google",
                "login_with_google",
            )],
        );
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1, "got: {:?}", findings);
        match &findings[0].diff {
            Diff::OutputMismatch { expected, found } => {
                assert_eq!(expected, "struct{}");
                assert_eq!(found, "string");
            }
            other => panic!("expected OutputMismatch, got {:?}", other),
        }
        let msg = findings[0].message();
        assert!(msg.contains("handler_registry.go:89"), "msg = {msg}");
        assert!(msg.contains("login_with_google"), "msg = {msg}");
    }

    #[test]
    fn input_mismatch_fires() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
import "lazuli.dev/runtime/lazuli"
func Login(ctx *lazuli.Ctx, input OtherInput) (string, error) {
    return "x", nil
}
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[ExpectedInput, string]{
    Name: "account.login",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        match &findings[0].diff {
            Diff::InputMismatch { expected, found } => {
                assert_eq!(expected, "ExpectedInput");
                assert_eq!(found, "OtherInput");
            }
            other => panic!("expected InputMismatch, got {:?}", other),
        }
    }

    #[test]
    fn both_sides_drift_fires_with_both_variant() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input WrongInput) (WrongOutput, error) {
    return WrongOutput{}, nil
}
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[RightInput, RightOutput]{
    Name: "account.login",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        match &findings[0].diff {
            Diff::Both {
                input_expected,
                input_found,
                output_expected,
                output_found,
            } => {
                assert_eq!(input_expected, "RightInput");
                assert_eq!(input_found, "WrongInput");
                assert_eq!(output_expected, "RightOutput");
                assert_eq!(output_found, "WrongOutput");
            }
            other => panic!("expected Both, got {:?}", other),
        }
    }

    #[test]
    fn package_prefix_stripped_before_compare() {
        // accountgen.LoginInput on the handler side vs bare LoginInput
        // on the codegen side — must NOT fire after normalisation.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input accountgen.LoginInput) (string, error) { return "", nil }
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty(), "got: {:?}", findings);
    }

    #[test]
    fn handler_unreadable_emits_specific_finding() {
        // Function exists but signature shape is not the canonical
        // `(ctx, input) (output, error)` — should emit
        // HandlerSignatureUnreadable, NOT panic. Covers the
        // type-alias risk from the proposal §False-positive cases.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        // Three return values — not the canonical shape.
        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input LoginInput) (string, int, error) { return "", 0, nil }
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].diff, Diff::HandlerSignatureUnreadable));
        assert!(findings[0].message().contains("could not parse"));
    }

    #[test]
    fn malformed_handler_does_not_panic() {
        // Truncated Go — handler `func Login(` with no matching `)`.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = "package accounthandlers\nfunc Login(ctx *lazuli.Ctx, input";
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        // Should produce a single Unreadable finding — no panic.
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].diff, Diff::HandlerSignatureUnreadable));
    }

    #[test]
    fn missing_codegen_file_silent() {
        // Handler exists. Codegen file does NOT. Sibling rule covers
        // this; we must short-circuit cleanly.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_dir = handler_path::handlers_dir(&app_root, "account");
        fs::create_dir_all(&handler_dir).unwrap();
        fs::write(handler_dir.join("login.go"), MATCHING_HANDLER).unwrap();
        // No gen file written.

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn missing_handler_file_silent_delegates_to_handler_missing_001() {
        // Codegen present, handler `.go` absent — HANDLER-MISSING-001
        // owns that finding; this rule MUST stay silent to avoid
        // double-fire.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let gen_dir = dist_root.join("go").join("account");
        fs::create_dir_all(&gen_dir).unwrap();
        fs::write(gen_dir.join("command.gen.go"), MATCHING_GEN).unwrap();

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn allow_comment_silences_finding() {
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(
            &lzi_path,
            "feature account\n# doctor:allow HANDLER-SIGNATURE-MISMATCH-001 — reason \"alias\"\n",
        );

        // Drift on disk — would fire without the opt-out.
        let handler_src = r#"package accounthandlers
func Login(ctx *lazuli.Ctx, input WrongInput) (string, error) { return "", nil }
"#;
        let gen_src = r#"package accountgen
var login = lazuli.Command[LoginInput, string]{ Name: "account.login" }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn gen_file_with_no_matching_command_silent() {
        // Codegen file exists but contains no Command[...] block whose
        // Name: matches `account.login`. Sibling rule
        // (@correctness.migration_out_of_sync) owns that surface;
        // this rule MUST stay silent.
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        // gen declares a DIFFERENT command name — login_with_google,
        // not login.
        let gen_src = r#"package accountgen
var loginWithGoogle = lazuli.Command[OtherInput, OtherOutput]{
    Name: "account.login_with_google",
}
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            MATCHING_HANDLER,
            gen_src,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert!(findings.is_empty());
    }

    #[test]
    fn function_export_renamed_emits_missing_handler() {
        // Handler file exists but author renamed `Login` to
        // `DoLogin` — the rule should surface MissingHandler so the
        // diagnostic is specific (not a confusing "signature drift" msg).
        let tmp = TempDir::new().unwrap();
        let app_root = tmp.path().join("app");
        let dist_root = tmp.path().join("dist");
        let lzi_path = tmp.path().join("features/account/account.lzi");
        write_lzi(&lzi_path, "feature account\n");

        let handler_src = r#"package accounthandlers
func DoLogin(ctx *lazuli.Ctx, input LoginInput) (string, error) { return "", nil }
"#;
        lay_out_files(
            &app_root,
            &dist_root,
            "account",
            "login",
            handler_src,
            MATCHING_GEN,
        );

        let feature = mk_feature("account", vec![mk_cmd_with_handler("login", "login")]);
        let findings = check(&feature, &lzi_path, &app_root, &dist_root);
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].diff, Diff::MissingHandler));
    }

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case("login_with_google"), "LoginWithGoogle");
        assert_eq!(pascal_case("login"), "Login");
        assert_eq!(pascal_case("verify_password_v2"), "VerifyPasswordV2");
    }

    #[test]
    fn extract_command_signature_finds_block() {
        let src = r#"package accountgen
var foo = lazuli.Command[Input1, Output1]{ Name: "account.foo" }
var bar = lazuli.Command[Input2, Output2]{
    Name: "account.bar",
}
"#;
        let sig = extract_command_signature(src, "account.bar").unwrap();
        assert_eq!(sig.input, "Input2");
        assert_eq!(sig.output, "Output2");
    }

    #[test]
    fn extract_handler_signature_strips_package_prefix() {
        let src = r#"package accounthandlers
func LoginWithGoogle(ctx *lazuli.Ctx, input accountgen.LoginResultWithGoogleInput) (struct{}, error) {
    return struct{}{}, nil
}
"#;
        let result = extract_handler_signature(src, "LoginWithGoogle");
        match result {
            HandlerExtractResult::Found(sig) => {
                assert_eq!(sig.input, "accountgen.LoginResultWithGoogleInput");
                assert_eq!(sig.output, "struct{}");
            }
            other => panic!("expected Found, got {:?}", other),
        }
    }
}
