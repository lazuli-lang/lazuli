use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

type SeedResult<T> = Result<T, Box<dyn Error>>;

pub fn run_seed(project_root: &Path, only: Option<&str>, force: bool) -> SeedResult<()> {
    let manifest = crate::lazurite_manifest::load(project_root)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "lazuli seed requires lazurite.toml",
        )
    })?;

    if std::env::var("LAZULI_ENV").as_deref() == Ok("production") && !force {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to seed in production env (use --force to override)",
        )
        .into());
    }

    let seed_dir = manifest
        .seeds
        .as_ref()
        .map(|seeds| seeds.dir.as_str())
        .unwrap_or("seeds");
    let dir = project_root.join(seed_dir);
    let files = discover_seed_files(&dir, only)?;

    for file in files {
        println!("seeding {}", file.display());
        run_seed_file(&file)?;
    }

    Ok(())
}

fn discover_seed_files(dir: &Path, only: Option<&str>) -> SeedResult<Vec<PathBuf>> {
    if !dir.exists() {
        eprintln!(
            "warning: seed dir {} does not exist; skipping",
            dir.display()
        );
        return Ok(Vec::new());
    }

    let mut files = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("sh" | "sql" | "go")
            )
        })
        .collect::<Vec<_>>();
    files.sort();

    if let Some(only) = only {
        files.retain(|path| path.file_name().and_then(|name| name.to_str()) == Some(only));
    }

    Ok(files)
}

fn run_seed_file(file: &Path) -> SeedResult<()> {
    let mut command = match file.extension().and_then(|extension| extension.to_str()) {
        Some("sh") => {
            let mut command = Command::new("bash");
            command.arg(file);
            command
        }
        Some("sql") => {
            let db_url = std::env::var("DATABASE_URL")?;
            let mut command = Command::new("psql");
            command.arg(db_url).arg("-f").arg(file);
            command
        }
        Some("go") => {
            let mut command = Command::new("go");
            command.arg("run").arg(file);
            command
        }
        _ => unreachable!("seed discovery only returns supported extensions"),
    };

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "seed {} failed with status {status}",
            file.display()
        ))
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_project(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-seed-test-{}-{name}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_manifest(root: &Path) {
        fs::write(
            root.join("lazurite.toml"),
            r#"
[project]
name = "seed-test"
module = "github.com/acme/seed-test"
schema = 1

[lazuli]
runtime = "0.1.0"

[seeds]
dir = "seeds"
auto = false
"#,
        )
        .unwrap();
    }

    #[test]
    fn seed_requires_manifest() {
        let root = temp_project("requires-manifest");
        let result = run_seed(&root, None, false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("lazurite.toml"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn seed_blocks_production_without_force() {
        let _guard = env_lock().lock().unwrap();
        let root = temp_project("blocks-production");
        write_manifest(&root);

        unsafe {
            std::env::set_var("LAZULI_ENV", "production");
        }
        let result = run_seed(&root, None, false);
        unsafe {
            std::env::remove_var("LAZULI_ENV");
        }

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("refusing to seed in production env")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn seed_force_bypasses_production_guard() {
        let _guard = env_lock().lock().unwrap();
        let root = temp_project("force-production");
        write_manifest(&root);

        unsafe {
            std::env::set_var("LAZULI_ENV", "production");
        }
        let result = run_seed(&root, None, true);
        unsafe {
            std::env::remove_var("LAZULI_ENV");
        }

        assert!(result.is_ok());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn seed_only_filters_to_single_file() {
        let root = temp_project("only-filter");
        let dir = root.join("seeds");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("001_alpha.sql"), "").unwrap();
        fs::write(dir.join("002_beta.sh"), "").unwrap();
        fs::write(dir.join("003_gamma.go"), "").unwrap();
        fs::write(dir.join("004_ignored.txt"), "").unwrap();

        let files = discover_seed_files(&dir, Some("002_beta.sh")).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().and_then(|name| name.to_str()),
            Some("002_beta.sh")
        );

        let _ = fs::remove_dir_all(root);
    }
}
