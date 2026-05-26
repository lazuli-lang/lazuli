//! Go templates baked into the auth_rate_limit smoke harness.
//!
//! * `ACCOUNT_AUTH_EXPORT_GO` — a tiny shim that re-exports the
//!   generated `AccountAuthPassword` contract so the smoke main.go can
//!   reach it from outside the `account` package.
//! * `MAIN_GO` — a small HTTP server that wires signup/login through
//!   the lazuli auth runtime plus an in-process IP bucket so the smoke
//!   can exercise the configured `rate_limit` clause without Postgres.

#![cfg(feature = "smoke_e2e")]

pub const ACCOUNT_AUTH_EXPORT_GO: &str = r#"package account

import "lazuli.dev/runtime/lazuli/auth"

func AuthPasswordContract() auth.PasswordContract {
	return AccountAuthPassword
}
"#;

pub const MAIN_GO: &str = r#"package main

import (
	"encoding/json"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/auth"

	"lazuli/auth-rate-limit-smoke/account"
)

type credentials struct {
	Email    string `json:"email"`
	Password string `json:"password"`
	Name     string `json:"name"`
}

type userRecord struct {
	Email        string `json:"email"`
	Name         string `json:"name"`
	PasswordHash string `json:"-"`
}

var (
	usersMu sync.Mutex
	users   = map[string]userRecord{}
	limitMu sync.Mutex
	buckets = map[string]*failureBucket{}
)

type failureBucket struct {
	windowStart time.Time
	count       int
}

func main() {
	addr := os.Getenv("LAZULI_ADDR")
	if addr == "" {
		addr = "127.0.0.1:8080"
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("POST /auth/signup", signup)
	mux.HandleFunc("POST /auth/login", login)

	slog.Info("auth-rate-limit smoke listening", "addr", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {
		slog.Error("server exited", "error", err)
		os.Exit(1)
	}
}

func signup(w http.ResponseWriter, r *http.Request) {
	var input credentials
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeProblem(w, http.StatusBadRequest, "bad_request")
		return
	}
	if input.Email == "" || input.Password == "" {
		writeProblem(w, http.StatusBadRequest, "bad_request")
		return
	}

	contract := account.AuthPasswordContract()
	hash, err := auth.HashPassword(&lazuli.Ctx{Context: r.Context(), Now: time.Now()}, contract, input.Password)
	if err != nil {
		writeProblem(w, http.StatusInternalServerError, "internal")
		return
	}

	usersMu.Lock()
	defer usersMu.Unlock()
	users[input.Email] = userRecord{Email: input.Email, Name: input.Name, PasswordHash: hash}
	writeJSON(w, http.StatusCreated, map[string]string{"email": input.Email})
}

func login(w http.ResponseWriter, r *http.Request) {
	var input credentials
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeProblem(w, http.StatusBadRequest, "bad_request")
		return
	}

	key := requestIP(r)
	spec, err := lazuli.ParseRateLimit(lazuli.RateLimit(account.AuthPasswordContract().RateLimit))
	if err != nil {
		writeProblem(w, http.StatusInternalServerError, "internal")
		return
	}
	if retryAfter, limited := reserveFailureAttempt(key, spec); limited {
		w.Header().Set("Retry-After", retryAfter)
		writeProblem(w, http.StatusTooManyRequests, "rate_limited")
		return
	}

	usersMu.Lock()
	user, ok := users[input.Email]
	usersMu.Unlock()
	if !ok {
		writeProblem(w, http.StatusUnauthorized, "auth.password_mismatch")
		return
	}
	err = auth.VerifyPassword(&lazuli.Ctx{Context: r.Context(), Now: time.Now()}, account.AuthPasswordContract(), input.Password, user.PasswordHash)
	if errors.Is(err, auth.ErrPasswordMismatch) {
		writeProblem(w, http.StatusUnauthorized, "auth.password_mismatch")
		return
	}
	if err != nil {
		writeProblem(w, http.StatusInternalServerError, "internal")
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"email": user.Email})
}

func reserveFailureAttempt(key string, spec lazuli.RateLimitSpec) (string, bool) {
	now := time.Now()
	window := time.Duration(float64(spec.Burst)/spec.PerSecond) * time.Second
	if window <= 0 {
		window = time.Second
	}

	limitMu.Lock()
	defer limitMu.Unlock()
	bucket := buckets[key]
	if bucket == nil || now.Sub(bucket.windowStart) >= window {
		bucket = &failureBucket{windowStart: now}
		buckets[key] = bucket
	}
	if bucket.count >= spec.Burst {
		remaining := window - now.Sub(bucket.windowStart)
		if remaining < time.Second {
			remaining = time.Second
		}
		return strconv.Itoa(int((remaining + time.Second - 1) / time.Second)), true
	}
	bucket.count++
	return "", false
}

func requestIP(r *http.Request) string {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err == nil && host != "" {
		return host
	}
	return strings.TrimSpace(r.RemoteAddr)
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}

func writeProblem(w http.ResponseWriter, status int, code string) {
	writeJSON(w, status, map[string]any{
		"status": status,
		"code":   code,
	})
}
"#;
