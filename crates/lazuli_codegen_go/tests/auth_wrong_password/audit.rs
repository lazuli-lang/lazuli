//! Audit-log helper used by the auth_wrong_password smoke test. Writes
//! a tiny Go program to the temp output dir, runs it via `go run`, and
//! parses the single tab-separated row produced by the helper.

#![cfg(feature = "smoke_e2e")]

use std::fs;
use std::path::Path;
use std::process::Command;

pub struct AuditRow {
    pub actor_kind: String,
    pub command_name: String,
    pub result_status: String,
}

pub fn latest_login_audit_row(out_dir: &Path, db_url: &str) -> Result<Option<AuditRow>, String> {
    let helper_dir = out_dir.join("smoke_audit_query");
    fs::create_dir_all(&helper_dir)
        .map_err(|err| format!("creating {}: {err}", helper_dir.display()))?;
    fs::write(helper_dir.join("main.go"), AUDIT_QUERY_HELPER)
        .map_err(|err| format!("writing audit query helper: {err}"))?;

    let query = Command::new("go")
        .current_dir(out_dir)
        .env("GOFLAGS", "-mod=mod")
        .env("LAZULI_DB", db_url)
        .args(["run", "./smoke_audit_query"])
        .output()
        .map_err(|err| format!("running audit query helper: {err}"))?;
    if !query.status.success() {
        return Err(format!(
            "audit query helper failed with status {}\nstdout:\n{}\nstderr:\n{}",
            query.status,
            String::from_utf8_lossy(&query.stdout),
            String::from_utf8_lossy(&query.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&query.stdout);
    let line = stdout.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let mut parts = line.split('\t');
    let actor_kind = parts.next().unwrap_or_default().to_owned();
    let command_name = parts.next().unwrap_or_default().to_owned();
    let result_status = parts.next().unwrap_or_default().to_owned();
    if actor_kind.is_empty() || command_name.is_empty() || result_status.is_empty() {
        return Err(format!("malformed audit query output: {line:?}"));
    }
    Ok(Some(AuditRow {
        actor_kind,
        command_name,
        result_status,
    }))
}

const AUDIT_QUERY_HELPER: &str = r#"package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/jackc/pgx/v5"
)

func main() {
	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		panic("LAZULI_DB is required")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		panic(fmt.Sprintf("connect postgres: %v", err))
	}
	defer conn.Close(context.Background())

	var actorKind, commandName, resultStatus string
	err = conn.QueryRow(ctx, `
		SELECT actor_kind, command_name, result_status
		FROM audit_log
		WHERE command_name = 'auth.login'
		ORDER BY happened_at DESC, id DESC
		LIMIT 1
	`).Scan(&actorKind, &commandName, &resultStatus)
	if err == pgx.ErrNoRows {
		return
	}
	if err != nil {
		panic(fmt.Sprintf("query audit_log: %v", err))
	}
	fmt.Printf("%s\t%s\t%s\n", actorKind, commandName, resultStatus)
}
"#;
