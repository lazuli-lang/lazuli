use std::path::PathBuf;
use std::process::Command;

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
}

#[test]
fn generate_playwright_help_lists_all_targets() {
    let output = Command::new(cli_bin())
        .args(["generate", "playwright", "--target=api-policy", "--help"])
        .output()
        .expect("run lazuli generate playwright --help");

    assert!(
        output.status.success(),
        "exit: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for target in [
        "api-policy",
        "lifecycle-gate",
        "scalar-fixtures-barrel",
        "all",
    ] {
        assert!(
            stdout.contains(target),
            "help text should mention {target}; stdout:\n{stdout}"
        );
    }
}
