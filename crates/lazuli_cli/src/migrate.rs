use crate::lazurite_manifest::{MigrationStrategy, Migrations};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const MIGRATIONS_TABLE: &str = "lazuli_schema_migrations";

pub enum MigrateAction {
    Up { target: Option<String>, yes: bool },
    Down { steps: u32, yes: bool },
    Status,
}

#[derive(Debug, Clone)]
struct MigrationFile {
    version: String,
    up: Option<PathBuf>,
    down: Option<PathBuf>,
}

pub fn run_migrate(
    project_root: &Path,
    action: MigrateAction,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = crate::lazurite_manifest::load(project_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "lazuli migrate requires lazurite.toml — run `lazuli init --lazurite`",
        )
    })?;

    let migrations = manifest.migrations.unwrap_or_else(default_migrations);
    if matches!(migrations.strategy, MigrationStrategy::CheckOnly)
        && matches!(
            action,
            MigrateAction::Up { .. } | MigrateAction::Down { .. }
        )
    {
        return Err(io::Error::other(
            "lazuli migrate cannot apply or roll back migrations when [migrations].strategy = \"check-only\"",
        )
        .into());
    }

    let database_url = std::env::var("DATABASE_URL").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "lazuli migrate requires `DATABASE_URL` env var set",
        )
    })?;

    let generated_dir = project_root.join(&migrations.generated);
    let manual_dir = project_root.join(&migrations.manual);
    let files = discover_migrations(&generated_dir, &manual_dir)?;

    ensure_migrations_table(&database_url)?;

    match action {
        MigrateAction::Up { target, yes } => {
            migrate_up(&database_url, &files, target.as_deref(), yes)
        }
        MigrateAction::Down { steps, yes } => migrate_down(&database_url, &files, steps, yes),
        MigrateAction::Status => migrate_status(&database_url, &files),
    }
}

fn default_migrations() -> Migrations {
    Migrations {
        generated: "dist/go/migrations".to_owned(),
        manual: "migrations".to_owned(),
        strategy: MigrationStrategy::Auto,
    }
}

fn discover_migrations(
    generated_dir: &Path,
    manual_dir: &Path,
) -> Result<Vec<MigrationFile>, Box<dyn Error>> {
    let mut files = BTreeMap::<String, MigrationFile>::new();
    collect_migration_dir(generated_dir, &mut files)?;
    collect_migration_dir(manual_dir, &mut files)?;
    Ok(files.into_values().collect())
}

fn collect_migration_dir(
    dir: &Path,
    files: &mut BTreeMap<String, MigrationFile>,
) -> Result<(), Box<dyn Error>> {
    if !dir.exists() {
        return Ok(());
    }
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "migration path {} is not a directory",
            dir.display()
        ))
        .into());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_migration_dir(&path, files)?;
            continue;
        }

        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let Some((version, direction)) = split_migration_file_name(file_name) else {
            continue;
        };

        let migration = files
            .entry(version.to_owned())
            .or_insert_with(|| MigrationFile {
                version: version.to_owned(),
                up: None,
                down: None,
            });

        match direction {
            "up" => migration.up = Some(path),
            "down" => migration.down = Some(path),
            _ => {}
        }
    }

    Ok(())
}

fn split_migration_file_name(file_name: &str) -> Option<(&str, &str)> {
    let version = file_name
        .strip_suffix(".up.sql")
        .map(|version| (version, "up"))
        .or_else(|| {
            file_name
                .strip_suffix(".down.sql")
                .map(|version| (version, "down"))
        })?;
    Some(version)
}

fn migrate_up(
    database_url: &str,
    files: &[MigrationFile],
    target: Option<&str>,
    yes: bool,
) -> Result<(), Box<dyn Error>> {
    let applied = applied_versions(database_url)?;
    let pending = files
        .iter()
        .filter(|file| !applied.contains(&file.version))
        .filter(|file| target.is_none_or(|target| file.version.as_str() <= target))
        .filter(|file| file.up.is_some())
        .collect::<Vec<_>>();

    if pending.is_empty() {
        println!("all migrations up to date");
        return Ok(());
    }

    println!("migrations to apply:");
    for file in &pending {
        println!("  {}", file.up.as_ref().unwrap().display());
    }
    confirm("Apply these migrations?", yes)?;

    for file in pending {
        let up = file.up.as_ref().unwrap();
        println!("applying {}", up.display());
        run_psql_file(database_url, up)?;
        insert_applied_version(database_url, &file.version)?;
    }

    Ok(())
}

fn migrate_down(
    database_url: &str,
    files: &[MigrationFile],
    steps: u32,
    yes: bool,
) -> Result<(), Box<dyn Error>> {
    if steps == 0 {
        println!("no migrations rolled back");
        return Ok(());
    }

    let by_version = files
        .iter()
        .map(|file| (file.version.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    let applied = applied_versions_desc(database_url, steps)?;
    if applied.is_empty() {
        println!("no applied migrations to roll back");
        return Ok(());
    }

    let rollback = applied
        .iter()
        .map(|version| {
            let file = by_version.get(version.as_str()).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("applied migration `{version}` has no local migration file"),
                )
            })?;
            let down = file.down.as_ref().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("migration `{version}` has no .down.sql file"),
                )
            })?;
            Ok((version, down))
        })
        .collect::<Result<Vec<_>, io::Error>>()?;

    println!("migrations to roll back:");
    for (_, down) in &rollback {
        println!("  {}", down.display());
    }
    confirm("Roll back these migrations?", yes)?;

    for (version, down) in rollback {
        println!("rolling back {}", down.display());
        run_psql_file(database_url, down)?;
        delete_applied_version(database_url, version)?;
    }

    Ok(())
}

fn migrate_status(database_url: &str, files: &[MigrationFile]) -> Result<(), Box<dyn Error>> {
    let applied = applied_versions(database_url)?;
    let current = applied
        .iter()
        .next_back()
        .map(String::as_str)
        .unwrap_or("none");
    let pending = files
        .iter()
        .filter(|file| file.up.is_some() && !applied.contains(&file.version))
        .collect::<Vec<_>>();

    println!("current version: {current}");
    if pending.is_empty() {
        println!("pending migrations: 0");
    } else {
        println!("pending migrations: {}", pending.len());
        for file in pending {
            println!("  {}", file.up.as_ref().unwrap().display());
        }
    }

    Ok(())
}

fn confirm(prompt: &str, yes: bool) -> Result<(), Box<dyn Error>> {
    if yes {
        return Ok(());
    }

    print!("{prompt} [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if matches!(input.trim(), "y" | "Y" | "yes" | "YES") {
        Ok(())
    } else {
        Err(io::Error::other("migration cancelled").into())
    }
}

fn ensure_migrations_table(database_url: &str) -> Result<(), Box<dyn Error>> {
    run_psql_sql(
        database_url,
        &format!(
            "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (version TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())"
        ),
    )
}

fn applied_versions(database_url: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let output = run_psql_query(
        database_url,
        &format!("SELECT version FROM {MIGRATIONS_TABLE} ORDER BY version"),
    )?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn applied_versions_desc(database_url: &str, steps: u32) -> Result<Vec<String>, Box<dyn Error>> {
    let output = run_psql_query(
        database_url,
        &format!("SELECT version FROM {MIGRATIONS_TABLE} ORDER BY version DESC LIMIT {steps}"),
    )?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn insert_applied_version(database_url: &str, version: &str) -> Result<(), Box<dyn Error>> {
    run_psql_sql(
        database_url,
        &format!(
            "INSERT INTO {MIGRATIONS_TABLE} (version) VALUES ('{}') ON CONFLICT (version) DO NOTHING",
            sql_literal(version)
        ),
    )
}

fn delete_applied_version(database_url: &str, version: &str) -> Result<(), Box<dyn Error>> {
    run_psql_sql(
        database_url,
        &format!(
            "DELETE FROM {MIGRATIONS_TABLE} WHERE version = '{}'",
            sql_literal(version)
        ),
    )
}

fn run_psql_file(database_url: &str, path: &Path) -> Result<(), Box<dyn Error>> {
    run_psql(database_url, &["-f".to_owned(), path.display().to_string()]).map(|_| ())
}

fn run_psql_sql(database_url: &str, sql: &str) -> Result<(), Box<dyn Error>> {
    run_psql(database_url, &["-c".to_owned(), sql.to_owned()]).map(|_| ())
}

fn run_psql_query(database_url: &str, sql: &str) -> Result<String, Box<dyn Error>> {
    run_psql(
        database_url,
        &[
            "-A".to_owned(),
            "-t".to_owned(),
            "-c".to_owned(),
            sql.to_owned(),
        ],
    )
}

fn run_psql(database_url: &str, args: &[String]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("psql")
        .arg(database_url)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .args(args)
        .output()
        .map_err(|err| io::Error::new(err.kind(), format!("failed to run psql: {err}")))?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "psql failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn migrate_requires_manifest_returns_error() {
        let root = temp_project("missing-manifest");
        fs::create_dir_all(&root).unwrap();

        let err = run_migrate(&root, MigrateAction::Status).unwrap_err();
        assert!(
            err.to_string()
                .contains("lazuli migrate requires lazurite.toml")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migrate_check_only_blocks_up() {
        let root = temp_project("check-only");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("lazurite.toml"),
            r#"
[project]
name = "myapp"
module = "github.com/acme/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[migrations]
generated = "dist/go/migrations"
manual = "migrations"
strategy = "check-only"
"#,
        )
        .unwrap();

        let err = run_migrate(
            &root,
            MigrateAction::Up {
                target: None,
                yes: true,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("check-only"));

        let _ = fs::remove_dir_all(root);
    }

    fn temp_project(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lazuli-migrate-{name}-{}-{suffix}",
            std::process::id()
        ))
    }
}
