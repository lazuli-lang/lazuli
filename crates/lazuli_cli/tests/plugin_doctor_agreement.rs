//! 0020 — the doctor↔generate-path AGREEMENT drift guard.
//!
//! This is the tripwire the ADR names: on the SAME plugin-using fixture
//! run from the SAME features subdir, the doctor's plugin-semantic
//! resolution set and the generate path's residual-`@semantic.*` set
//! must AGREE. Either both resolve the alias (zero `SEMANTIC-PLUGIN-001`
//! ∧ zero generate residual), OR both flag the identical alias. They can
//! never disagree — that disagreement was the Seam-4 bug (doctor-green ≠
//! codegen-green) 0020 closes by making both surfaces resolve through the
//! ONE re-homed `find_project_root` + `build_alias_map`.
//!
//! This test lives in `lazuli_cli` (not the doctor engine crate) because
//! it needs BOTH the generate-path module loader
//! (`lazuli_cli::module_loader`) AND the doctor engine
//! (`lazuli_doctor_run`), and the engine crate must not depend on the
//! CLI.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_cli::module_loader::build_module_from_path;
use lazuli_doctor_run::{
    DoctorDiagnostic, FileLocalInjector, ResolvedDoctorConfig, run_package,
};
use lazuli_ir::{CommandInput, Module, TypeRef};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// The generate path's residual: every distinct `@semantic.*` alias still
/// carried as `UserDefined` after `build_module_from_path` resolution.
fn generate_residual(module: &Module) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut visit = |tr: &TypeRef| {
        let mut t = tr;
        loop {
            match t {
                TypeRef::UserDefined(q) if q.name.starts_with("@semantic.") => {
                    out.insert(q.name.clone());
                    break;
                }
                TypeRef::Many(inner) => t = inner,
                _ => break,
            }
        }
    };
    for feature in &module.features {
        for resource in &feature.resources {
            for field in &resource.fields {
                visit(&field.type_ref);
            }
        }
        for record in &feature.records {
            for field in &record.fields {
                visit(&field.type_ref);
            }
        }
        for command in &feature.commands {
            if let CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    visit(&slot.type_ref);
                }
            }
        }
    }
    out
}

/// The doctor's unresolved set: the `@semantic.<X>` aliases doctor flags
/// with `SEMANTIC-PLUGIN-001` (extracted from the message head token).
fn doctor_unresolved_set(diagnostics: &[DoctorDiagnostic]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for d in diagnostics {
        if d.code != "SEMANTIC-PLUGIN-001" {
            continue;
        }
        // The field-anchored message embeds the alias in backticks:
        // "unknown plugin semantic type `@semantic.Bar`. ...".
        if let Some(start) = d.message.find("@semantic.") {
            let tail = &d.message[start..];
            let alias: String = tail
                .chars()
                .take_while(|c| {
                    c.is_alphanumeric() || *c == '@' || *c == '.' || *c == '_'
                })
                .collect();
            out.insert(alias);
        }
    }
    out
}

fn run_doctor(input: &Path) -> Vec<DoctorDiagnostic> {
    let config = ResolvedDoctorConfig::default();
    let file_local: &FileLocalInjector =
        &|_p: &Path, _s: &str| Vec::<DoctorDiagnostic>::new();
    run_package(input, &config, file_local, Vec::new())
        .expect("doctor run_package")
        .diagnostics()
}

/// AGREEMENT (resolving case): on the `plugin_resolution` fixture run from
/// the `app/` features subdir, doctor's SEMANTIC-PLUGIN-001 alias set is
/// EMPTY ∧ the generate path's residual `@semantic.*` set is EMPTY — both
/// resolve `@semantic.Foo` via the SHARED upward walk. The headline
/// drift-guard.
#[test]
fn plugin_semantic_doctor_and_generate_agree() {
    let subdir = fixtures_dir().join("plugin_resolution").join("app");

    // Generate path.
    let module = build_module_from_path(&subdir).expect("generate-path module builds");
    let residual = generate_residual(&module);

    // Doctor path.
    let diagnostics = run_doctor(&subdir);
    let doctor_unresolved = doctor_unresolved_set(&diagnostics);

    assert!(
        residual.is_empty(),
        "generate path left unresolved @semantic.*: {residual:?}"
    );
    assert!(
        doctor_unresolved.is_empty(),
        "doctor flagged @semantic.* that generate resolved (DRIFT): {doctor_unresolved:?}"
    );
    // The mechanical invariant: the two sets are identical.
    assert_eq!(
        doctor_unresolved, residual,
        "doctor and generate must agree on the unresolved @semantic.* set"
    );
}

/// AGREEMENT (unresolved case): a fixture with `[plugins]` present but a
/// `@semantic.Bar` that no plugin provides — doctor flags `@semantic.Bar`
/// ∧ the generate path leaves it a residual. Both flag the SAME alias.
#[test]
fn doctor_flags_unresolved_alias_generate_also_fails() {
    let tmp = std::env::temp_dir().join(format!(
        "lazuli-0020-cli-agree-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&tmp);
    // Plugin provides Foo only.
    fs::create_dir_all(tmp.join("plugins/mini")).unwrap();
    fs::write(
        tmp.join("plugins/mini/manifest.toml"),
        r#"[plugin]
name = "mini"
namespace = "@lazuli/plugin-mini"

[[semantic_types]]
name = "Foo"
alias = "@semantic.Foo"
carrier_type = "String"
validator = "ValidateFoo"
"#,
    )
    .unwrap();
    fs::write(
        tmp.join("Lazurite.toml"),
        r#"[project]
name = "agree-unresolved"
module = "example.test/agree-unresolved"
schema = 1

[lazurite]
app_dir = "app"

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-mini" = { path = "plugins/mini" }
"#,
    )
    .unwrap();
    fs::create_dir_all(tmp.join("app/features/widget")).unwrap();
    fs::write(tmp.join("app/app.lzi"), "app AgreeUnresolved\n").unwrap();
    // References @semantic.Bar — NO plugin provides it.
    fs::write(
        tmp.join("app/features/widget/widget.lzi"),
        "feature widget\n  domain\n    resource Widget\n      bar: @semantic.Bar required\n",
    )
    .unwrap();

    let subdir = tmp.join("app");

    // Generate path: build_module_from_path BAILS loud (residual scan)
    // because [plugins] is present and @semantic.Bar is unresolved. The
    // loud error names the alias — that IS the residual signal.
    let gen_result = build_module_from_path(&subdir);

    // Doctor path.
    let diagnostics = run_doctor(&subdir);
    let doctor_unresolved = doctor_unresolved_set(&diagnostics);

    let _ = fs::remove_dir_all(&tmp);

    // Doctor must flag @semantic.Bar.
    assert!(
        doctor_unresolved.contains("@semantic.Bar"),
        "doctor must flag @semantic.Bar; got: {doctor_unresolved:?}"
    );
    // Doctor must NOT flag @semantic.Foo (the plugin provides it).
    assert!(
        !doctor_unresolved.contains("@semantic.Foo"),
        "doctor must resolve @semantic.Foo; got: {doctor_unresolved:?}"
    );
    // Generate must ALSO fail on the SAME alias (loud residual bail).
    let gen_err = gen_result.expect_err("generate must bail loud on @semantic.Bar");
    let msg = format!("{gen_err:#}");
    assert!(
        msg.contains("@semantic.Bar"),
        "generate's loud error must name @semantic.Bar (same alias doctor flagged); got: {msg}"
    );
    assert!(
        !msg.contains("@semantic.Foo"),
        "generate must not complain about the resolved @semantic.Foo; got: {msg}"
    );
}
