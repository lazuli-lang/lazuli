//! Go migration helper template used by the auth_wrong_password smoke
//! test. Loaded as `smoke_migrations/main.go` into the temp output dir
//! and invoked via `go run ./smoke_migrations` to apply the emitted
//! `migrations/*.sql` plus cleanup of any pre-existing rows for the
//! smoke email passed via `LAZULI_SMOKE_EMAIL`.

#![cfg(feature = "smoke_e2e")]

pub const MIGRATION_HELPER: &str = r#"package main

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

func main() {
	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		panic("LAZULI_DB is required")
	}
	migrationsDir := os.Getenv("LAZULI_MIGRATIONS")
	if migrationsDir == "" {
		panic("LAZULI_MIGRATIONS is required")
	}
	smokeEmail := os.Getenv("LAZULI_SMOKE_EMAIL")
	if smokeEmail == "" {
		panic("LAZULI_SMOKE_EMAIL is required")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		panic(fmt.Sprintf("connect postgres: %v", err))
	}
	defer conn.Close(context.Background())

	entries, err := os.ReadDir(migrationsDir)
	if err != nil {
		panic(fmt.Sprintf("read migrations: %v", err))
	}

	files := make([]string, 0, len(entries))
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".sql") || strings.HasSuffix(name, ".down.sql") {
			continue
		}
		files = append(files, filepath.Join(migrationsDir, name))
	}
	sort.Strings(files)

	for _, file := range files {
		sql, err := os.ReadFile(file)
		if err != nil {
			panic(fmt.Sprintf("read %s: %v", file, err))
		}
		if _, err := conn.Exec(ctx, string(sql)); err != nil {
			panic(fmt.Sprintf("apply %s: %v", file, err))
		}
	}

	_, _ = conn.Exec(ctx, `DELETE FROM audit_log WHERE command_name IN ('auth.login', 'account.login')`)
	_, _ = conn.Exec(ctx, `DELETE FROM "Session"`)
	_, _ = conn.Exec(ctx, `DELETE FROM "session"`)
	_, _ = conn.Exec(ctx, `DELETE FROM "User" WHERE email = $1`, smokeEmail)
	_, _ = conn.Exec(ctx, `DELETE FROM "user" WHERE email = $1`, smokeEmail)
}
"#;
