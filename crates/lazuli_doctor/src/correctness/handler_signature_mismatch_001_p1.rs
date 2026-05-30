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
