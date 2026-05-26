//! Go templates used by the auth_protected_endpoint smoke test.
//! `MIGRATION_HELPER` applies the emitted `migrations/*.sql` files
//! and resets the `User` table; `EXPIRED_COOKIE_HELPER` signs a
//! session cookie whose `exp` claim is already an hour in the past so
//! the test can drive the expired-cookie branch.

#![cfg(feature = "smoke_e2e")]

pub const EXPIRED_COOKIE_HELPER: &str = r#"package main

import (
	"fmt"
	"os"
	"time"

	"lazuli.dev/runtime/lazuli/auth"
)

func main() {
	signed, err := auth.SignCookieValue("key-v1", []byte(os.Getenv("COOKIE_SECRET")), os.Getenv("SESSION_TOKEN"), time.Now().Add(-time.Hour))
	if err != nil {
		panic(err)
	}
	fmt.Print(signed)
}
"#;

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

	_, _ = conn.Exec(ctx, `DELETE FROM "User" WHERE email = $1`, "test@example.com")
	_, _ = conn.Exec(ctx, `DELETE FROM "user" WHERE email = $1`, "test@example.com")
}
"#;
