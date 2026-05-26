//! Go templates baked into the auth_expired_session smoke harness.
//!
//! * `ACCOUNT_AUTH_TESTHOOK` — a tiny shim that exposes the generated
//!   `AccountAuthSessions` contract so the smoke main.go can drive the
//!   auth runtime without re-importing the generator output.
//! * `SMOKE_SERVER` — a custom main.go that signs up + logs in + walks
//!   the expiration path against the generated runtime.
//! * `MIGRATION_HELPER` — applies the emitted `migrations/*.sql` files
//!   and resets the `Session`/`user` tables so the test is idempotent.

#![cfg(feature = "smoke_e2e")]

pub const ACCOUNT_AUTH_TESTHOOK: &str = r#"package account

import "lazuli.dev/runtime/lazuli/auth"

func SmokeSessionsContract() auth.SessionsContract {
	return AccountAuthSessions
}
"#;

pub const SMOKE_SERVER: &str = r#"package main

import (
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"os"
	"time"

	"github.com/jackc/pgx/v5"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/auth"

	"github.com/lazuli-lang/example-marketplace-mini/generated/account"
)

type signupInput struct {
	Email    string `json:"email"`
	Password string `json:"password"`
	Name     string `json:"name"`
}

type loginInput struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

func main() {
	ctx := context.Background()
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo})))

	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		dbURL = "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable"
	}
	if err := lazuli.Boot(ctx, dbURL); err != nil {
		slog.Error("lazuli boot failed", "error", err)
		os.Exit(1)
	}
	if err := ensureAuthSessionTable(ctx); err != nil {
		slog.Error("auth session table setup failed", "error", err)
		os.Exit(1)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("POST /signup", signup)
	mux.HandleFunc("POST /login", login)
	mux.HandleFunc("GET /protected", protected)

	addr := os.Getenv("LAZULI_ADDR")
	if addr == "" {
		addr = ":8080"
	}
	if err := http.ListenAndServe(addr, mux); err != nil {
		slog.Error("lazuli http server exited", "error", err)
		os.Exit(1)
	}
}

func signup(w http.ResponseWriter, r *http.Request) {
	var input signupInput
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "bad_request"})
		return
	}
	_, err := lazuli.DB().Exec(r.Context(),
		`INSERT INTO "user" (email, name, role, password_hash) VALUES ($1, $2, 'buyer', $3)`,
		input.Email, input.Name, input.Password)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "signup_failed"})
		return
	}
	writeJSON(w, http.StatusCreated, map[string]string{"status": "created"})
}

func login(w http.ResponseWriter, r *http.Request) {
	var input loginInput
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "bad_request"})
		return
	}
	var userID lazuli.ID
	err := lazuli.DB().QueryRow(r.Context(), `SELECT id FROM "user" WHERE email = $1`, input.Email).Scan(&userID)
	if errors.Is(err, pgx.ErrNoRows) {
		writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.password_mismatch"})
		return
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "login_failed"})
		return
	}

	ctx := &lazuli.Ctx{Context: r.Context(), Now: time.Now()}
	token, expiresAt, err := auth.IssueSession(ctx, account.SmokeSessionsContract(), userID, nil)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "session_issue_failed"})
		return
	}
	auth.WriteSessionCookie(w, token, expiresAt, auth.SessionCookieOptions{})
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

func protected(w http.ResponseWriter, r *http.Request) {
	token, err := auth.ReadSessionCookie(r, auth.SessionCookieOptions{})
	if err != nil {
		writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.session_missing"})
		return
	}
	ctx := &lazuli.Ctx{Context: r.Context(), Now: time.Now()}
	if _, _, err := auth.ResolveSession(ctx, account.SmokeSessionsContract(), token); err != nil {
		if errors.Is(err, auth.ErrSessionExpired) {
			auth.ClearSessionCookie(w, auth.SessionCookieOptions{})
			writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.session_expired"})
			return
		}
		writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.session_unknown"})
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

func ensureAuthSessionTable(ctx context.Context) error {
	_, err := lazuli.DB().Exec(ctx, `CREATE TABLE IF NOT EXISTS "Session" (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT NOT NULL,
		token_hash TEXT NOT NULL UNIQUE,
		expires_at TIMESTAMPTZ NOT NULL
	)`)
	return err
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
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

	_, _ = conn.Exec(ctx, `CREATE TABLE IF NOT EXISTS "Session" (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT NOT NULL,
		token_hash TEXT NOT NULL UNIQUE,
		expires_at TIMESTAMPTZ NOT NULL
	)`)
	_, _ = conn.Exec(ctx, `DELETE FROM "Session"`)
	_, _ = conn.Exec(ctx, `DELETE FROM "user" WHERE email = $1`, "expired-session@example.com")
}
"#;
