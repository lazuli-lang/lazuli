use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

const DEFAULT_TEMPLATE: &str = include_str!("../../../examples/crm.lzi");

#[derive(Debug, Parser)]
#[command(name = "lazuli")]
#[command(about = "Lazuli application metalinguage compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Parse {
        input: PathBuf,
    },
    Compile {
        input: PathBuf,
        #[arg(long, short)]
        out: PathBuf,
    },
    Init {
        path: PathBuf,
    },
    Lsp,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse { input } => parse_command(&input),
        Commands::Compile { input, out } => compile_command(&input, &out),
        Commands::Init { path } => init_command(&path),
        Commands::Lsp => lsp_command(),
    }
}

fn parse_command(input: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    println!("{}", serde_json::to_string_pretty(&app)?);
    Ok(())
}

fn compile_command(input: &Path, out: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    let plan = lazuli_planner::plan_initial_generation(&app);

    fs::create_dir_all(out)
        .with_context(|| format!("failed to create output directory {}", out.display()))?;

    for file in lazuli_codegen_go::generate(&app) {
        write_generated_file(out, &file.path, &file.contents)?;
    }

    for file in lazuli_codegen_ts::generate(&app) {
        write_generated_file(out, &file.path, &file.contents)?;
    }

    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

fn init_command(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
    }

    fs::write(path, DEFAULT_TEMPLATE)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("created {}", path.display());
    Ok(())
}

fn lsp_command() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to start Lazuli LSP runtime")?;
    runtime.block_on(lazuli_lsp::serve_stdio());
    Ok(())
}

fn compile_to_ir(input: &Path) -> Result<lazuli_ir::Application> {
    let source =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let document = lazuli_syntax::parse_document(&source).context("failed to parse .lzi file")?;
    lazuli_analyzer::lower_document(&document).context("failed to analyze .lzi file")
}

fn write_generated_file(root: &Path, relative: &str, contents: &str) -> Result<()> {
    let path = root.join(relative);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
