//! `lazuli_app.gen.go` emission. Walks `module.app: Option<AppManifest>`
//! and lowers each sub-block the Lazuli Go lib already models into a
//! per-contract `var` declaration:
//!
//!   - `app.locale`     → `i18n.LocaleContract`
//!   - `app.logging`    → `observability.LoggingContract`
//!   - `app.tracing`    → `observability.TracingContract`
//!   - `app.encryption` → `[]encryption.Binding` + `init()` register loop
//!   - `app.cors`       → `lazuli.AppCors` + `init()` middleware register
//!
//! Returns `None` when no `AppManifest` is declared AND no sub-block
//! would render — keeps the listing signal-rich.

use lazuli_ir::{AppCors, AppLocale, AppLogging, AppTracing, Module};

use super::super::imports::ImportSet;
use super::super::patterns::{PATTERN_CORS_REGISTER, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::encryption::emit_encryption_bindings;
use super::helpers::{
    emit_aligned_struct_value_rows, format_f64, log_format_const, log_level_const,
    parse_duration_to_seconds, redact_strategy_const,
};

/// Emit `lazuli_app.gen.go` lowering every observable sub-block of
/// `module.app` into typed Go contract values. Returns `None` when
/// the manifest declares no observable surface so the orchestrator can
/// skip the file (keeps the output listing signal-rich).
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_lazuli_app_gen(&module, "app.lzi");
/// // None when the module has no `app` manifest.
/// ```
pub fn emit_lazuli_app_gen(module: &Module, source_label: &str) -> Option<String> {
    let manifest = module.app.as_ref()?;

    // Pre-walk: which sub-blocks will we emit? Used both to decide
    // whether to bail out entirely (no observable surface → no file)
    // and to populate the per-block import set.
    let emit_locale = manifest.locale.is_some();
    let emit_logging = manifest.logging.is_some();
    let emit_tracing = manifest.tracing.is_some();
    let emit_name = !manifest.name.trim().is_empty();
    let emit_cors_todo = manifest.cors.is_some();
    let emit_routes_todo = true; // `Module.app.routes` lifts from `ExperienceModule`.
    // Encryption bucket cycle — emit `var EncryptionBindings = ...`
    // when the capsule declares one or more `encryption.key @key.<scope>`
    // bindings. Each binding wires to the runtime registry via an
    // `init()` block calling `encryption.Register(...)`. See
    // `docs/proposals/encryption-vocab.md` §Codegen.
    let emit_encryption = !manifest.encryption_bindings.is_empty();

    if !emit_locale
        && !emit_logging
        && !emit_tracing
        && !emit_name
        && !emit_cors_todo
        && !emit_encryption
    {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    if emit_locale {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }
    if emit_logging || emit_tracing {
        imports.add("lazuli.dev/runtime/lazuli/observability");
    }
    if emit_encryption {
        imports.add("lazuli.dev/runtime/lazuli/encryption");
    }
    if emit_cors_todo {
        // `lazuli.AppCors` + `lazuli.SetCorsContract` live in the runtime root.
        imports.add("lazuli.dev/runtime/lazuli");
    }

    p.banner(source_label, "main");
    if !imports.is_empty() {
        imports.emit(&mut p);
        p.blank();
    }

    // Blank-line BETWEEN blocks, never after the last one — `gofmt`
    // strips trailing newlines from composite files and we want
    // byte-equivalent output without invoking the formatter
    // (proposal §11 invariant). `first_block` toggles after the first
    // emission lands.
    let mut first_block = true;
    let maybe_blank = |p: &mut GoPrinter, first_block: &mut bool| {
        if *first_block {
            *first_block = false;
        } else {
            p.blank();
        }
    };

    // `AppName` constant lands first so reading the file top-down
    // surfaces the app identity before the per-bucket contracts.
    if emit_name {
        maybe_blank(&mut p, &mut first_block);
        p.line("// AppName is the lowered `app <Name>` identifier from app.lzi.");
        p.line(&format!("const AppName = {:?}", manifest.name));
    }

    if let Some(locale) = manifest.locale.as_ref() {
        maybe_blank(&mut p, &mut first_block);
        emit_locale_contract(&mut p, locale);
    }
    if let Some(logging) = manifest.logging.as_ref() {
        maybe_blank(&mut p, &mut first_block);
        emit_logging_contract(&mut p, logging);
    }
    if let Some(tracing) = manifest.tracing.as_ref() {
        maybe_blank(&mut p, &mut first_block);
        emit_tracing_contract(&mut p, tracing);
    }
    if emit_encryption {
        maybe_blank(&mut p, &mut first_block);
        emit_encryption_bindings(&mut p, &manifest.encryption_bindings);
    }
    if emit_cors_todo {
        maybe_blank(&mut p, &mut first_block);
        emit_cors_contract(
            &mut p,
            manifest.cors.as_ref().expect("cors guarded"),
            manifest.locale.is_some(),
        );
    }
    if emit_routes_todo {
        maybe_blank(&mut p, &mut first_block);
        // Top-level `app.routes` declarations live on `ExperienceModule`,
        // not the backend `Module`. When the experience layer threads
        // routes back into the backend module (Phase L Tier 4 follow-up
        // continuation) this block upgrades to a real
        // `var Routes = []lazuli.AppRoute{...}`.
        p.line("// TODO(runtime): emit `var Routes = []lazuli.AppRoute{...}` once");
        p.line("// `Module.app.routes` lifts from `ExperienceModule`. The shape");
        p.line("// lives in docs/proposals/codegen-lazuli-go.md §3.13.1.");
    }

    Some(p.finish())
}

fn emit_locale_contract(p: &mut GoPrinter, locale: &AppLocale) {
    p.line("// LocaleContract is the lowered `app.locale` block from app.lzi.");
    p.line("// Codegen surfaces the typed catalog so the runtime negotiation");
    p.line("// middleware can resolve a request locale without re-parsing the IR.");
    p.line("var LocaleContract = i18n.LocaleContract{");
    p.indent();
    p.line(&format!("Default: {:?},", locale.default));
    if !locale.supported.is_empty() {
        p.line("Supported: []string{");
        p.indent();
        for tag in &locale.supported {
            p.line(&format!("{:?},", tag));
        }
        p.dedent();
        p.line("},");
    }
    if !locale.fallbacks.is_empty() {
        p.line("Fallbacks: []i18n.Fallback{");
        p.indent();
        for fb in &locale.fallbacks {
            p.line(&format!("{{From: {:?}, To: {:?}}},", fb.from, fb.to));
        }
        p.dedent();
        p.line("},");
    }
    p.dedent();
    p.line("}");
}

fn emit_logging_contract(p: &mut GoPrinter, logging: &AppLogging) {
    p.line("// LoggingContract is the lowered `app.logging` block from app.lzi.");
    p.line("// The runtime materialises the slog handler stack from this contract;");
    p.line("// adapter selection lives in registry.capabilities.");
    p.line("var LoggingContract = observability.LoggingContract{");
    p.indent();
    // Gather rows first so we can column-align `Key:` widths the way
    // `gofmt` would. Mirrors `resource.rs`'s kv-row padding approach.
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(level) = logging.level.as_deref() {
        rows.push((
            "Level:".to_owned(),
            format!("observability.{},", log_level_const(level)),
        ));
    }
    if let Some(format) = logging.format.as_deref() {
        rows.push((
            "Format:".to_owned(),
            format!("observability.{},", log_format_const(format)),
        ));
    }
    if let Some(redact) = logging.redact.as_deref() {
        rows.push((
            "Redact:".to_owned(),
            format!("observability.{},", redact_strategy_const(redact)),
        ));
    }
    if let Some(rate) = logging.sample_rate {
        rows.push(("SampleRate:".to_owned(), format!("{},", format_f64(rate))));
    }
    emit_aligned_struct_value_rows(p, &rows);
    p.dedent();
    p.line("}");
}

fn emit_tracing_contract(p: &mut GoPrinter, tracing: &AppTracing) {
    p.line("// TracingContract is the lowered `app.tracing` block from app.lzi.");
    p.line("// The runtime resolves the exporter adapter at boot; `Propagate` and");
    p.line("// `SampleRate` drive the head-sampling decision.");
    p.line("var TracingContract = observability.TracingContract{");
    p.indent();
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(propagate) = tracing.propagate {
        rows.push(("Propagate:".to_owned(), format!("{},", propagate)));
    }
    if let Some(rate) = tracing.sample_rate {
        rows.push(("SampleRate:".to_owned(), format!("{},", format_f64(rate))));
    }
    if let Some(exporter) = tracing.exporter.as_deref() {
        // Adapter refs (`@adapter.otlp`) are emitted verbatim — the
        // runtime resolves them at boot via `RegisterAdapter`.
        rows.push(("Exporter:".to_owned(), format!("{:?},", exporter)));
    }
    emit_aligned_struct_value_rows(p, &rows);
    p.dedent();
    p.line("}");
}

/// Emit the lowered `app.cors` block as a `lazuli.AppCors` value
/// + `init()` that registers it with the runtime middleware via
/// `lazuli.SetCorsContract`. The runtime CORS middleware (wire of
/// `rs/cors`) consumes the registered contract at request time.
///
/// `app.cors`:
///   allow_origins production "https://app.example.com"
///   allow_origins local      "http://localhost:5173"
///   allow_credentials true
///   max_age "1h"
///
/// Emits as:
///   var CorsContract = lazuli.AppCors{
///       AllowOrigins: map[string][]string{
///           "production": {"https://app.example.com"},
///           "local":      {"http://localhost:5173"},
///       },
///       AllowCredentials: true,
///       MaxAge:           3600,
///   }
///   func init() { lazuli.SetCorsContract(&CorsContract) }
fn emit_cors_contract(p: &mut GoPrinter, cors: &AppCors, has_locale: bool) {
    p.line("// CorsContract is the lowered `app.cors` block from app.lzi.");
    p.line("// Origins are keyed by environment (matches `app.environments`).");
    p.line("// The runtime middleware resolves the active set against");
    p.line("// `LAZULI_ENV` at request time, wires `github.com/rs/cors` for");
    p.line("// preflight + Access-Control-* headers.");
    p.line("var CorsContract = lazuli.AppCors{");
    p.indent();
    if cors.allow_origins.is_empty() {
        p.line("AllowOrigins: map[string][]string{},");
    } else {
        p.line("AllowOrigins: map[string][]string{");
        p.indent();
        // Merge any duplicate environment entries (DSL allows multiple
        // `allow_origins <env>` lines per env).
        let mut by_env: std::collections::BTreeMap<&str, Vec<&str>> =
            std::collections::BTreeMap::new();
        for rule in &cors.allow_origins {
            let entry = by_env.entry(rule.environment.as_str()).or_default();
            for o in &rule.origins {
                entry.push(o.as_str());
            }
        }
        for (env, origins) in by_env {
            let origin_list = origins
                .into_iter()
                .map(|o| format!("{:?}", o))
                .collect::<Vec<_>>()
                .join(", ");
            p.line(&format!("{:?}: {{{}}},", env, origin_list));
        }
        p.dedent();
        p.line("},");
    }
    if cors.allow_credentials {
        p.line("AllowCredentials: true,");
    }
    if let Some(max_age) = cors.max_age.as_deref() {
        if let Some(seconds) = parse_duration_to_seconds(max_age) {
            p.line(&format!("MaxAge: {},", seconds));
        }
    }
    p.dedent();
    p.line("}");
    p.blank();
    emit_pattern_header(p, PATTERN_CORS_REGISTER);
    p.line("func init() {");
    p.indent();
    p.line("lazuli.SetCorsContract(&CorsContract)");
    if has_locale {
        // IR Error-Vocab — register the lowered locale contract so the
        // HTTP error boundary can negotiate `Accept-Language` against
        // the supported set + default. Without this call the resolver
        // falls back to `en-US` and the proposal's pt-BR floor never
        // reaches the wire on a default-Locale install (proposal §2.E).
        p.line("lazuli.RegisterAppLocaleContract(LocaleContract)")
    }
    p.dedent();
    p.line("}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::Module;

    fn empty_module() -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: Vec::new(),
        }
    }

    #[test]
    fn module_without_app_manifest_emits_nothing() {
        assert!(emit_lazuli_app_gen(&empty_module(), "app.lzi").is_none());
    }
}
