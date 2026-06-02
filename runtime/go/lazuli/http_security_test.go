package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgconn"
)

// TestDevSessionAllowListGating is the SEC-DEVSESSION-FAILOPEN guard.
// Dev-session header impersonation must be ENABLED only for the explicit
// dev allow-list (`dev`/`local`) and DISABLED for every other value —
// including unset, empty, prod/staging/production, and typos. The old
// deny-list ("anything that isn't production") failed OPEN for unset/typo'd
// envs; this test pins the safe allow-list semantics.
func TestDevSessionAllowListGating(t *testing.T) {
	honored := []string{"dev", "local", "DEV", "Local", " dev "}
	ignored := []string{"", "prod", "production", "Production", "staging", "qa", "develop", "prodution"}

	for _, env := range honored {
		env := env
		t.Run("honored/"+env, func(t *testing.T) {
			t.Setenv("LAZULI_ENV", env)
			req := httptest.NewRequest("GET", "/", nil)
			req.Header.Set("X-Lazuli-User-ID", "42")
			req.Header.Set("X-Lazuli-Org-ID", "7")
			req.Header.Set("X-Lazuli-Roles", "ADMIN")
			ctx := &Ctx{Actor: ActorAnonymous}
			populateDevSession(req, ctx)
			if ctx.Actor != ActorUser || ctx.User == nil || ctx.User.ID != 42 || ctx.User.OrgID != 7 {
				t.Fatalf("dev-session not honored for LAZULI_ENV=%q: %#v / user=%+v", env, ctx, ctx.User)
			}
			if ctx.Tenant == nil || ctx.Tenant.OrgID != 7 {
				t.Fatalf("tenant not populated for LAZULI_ENV=%q: %+v", env, ctx.Tenant)
			}
		})
	}

	for _, env := range ignored {
		env := env
		t.Run("ignored/"+env, func(t *testing.T) {
			t.Setenv("LAZULI_ENV", env)
			req := httptest.NewRequest("GET", "/", nil)
			req.Header.Set("X-Lazuli-User-ID", "42")
			req.Header.Set("X-Lazuli-Org-ID", "1")
			req.Header.Set("X-Lazuli-Roles", "ADMIN")
			req.Header.Set("X-Lazuli-Actor", "system")
			ctx := &Ctx{Actor: ActorAnonymous}
			populateDevSession(req, ctx)
			if ctx.User != nil || ctx.Actor != ActorAnonymous || ctx.Tenant != nil {
				t.Fatalf("dev-session forged identity for non-dev LAZULI_ENV=%q: %#v / user=%+v", env, ctx, ctx.User)
			}
		})
	}
}

// TestDevSessionUnsetEnvFailsClosed is the headline regression: a real
// deploy that forgets to set LAZULI_ENV must NOT honor forged headers.
func TestDevSessionUnsetEnvFailsClosed(t *testing.T) {
	// Ensure LAZULI_ENV is genuinely unset (t.Setenv restores after).
	t.Setenv("LAZULI_ENV", "x")
	os.Unsetenv("LAZULI_ENV")
	req := httptest.NewRequest("GET", "/", nil)
	req.Header.Set("X-Lazuli-User-ID", "1")
	req.Header.Set("X-Lazuli-Org-ID", "1")
	req.Header.Set("X-Lazuli-Roles", "ADMIN")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateDevSession(req, ctx)
	if ctx.User != nil || ctx.Actor != ActorAnonymous {
		t.Fatalf("unset LAZULI_ENV honored forged headers (fail-open!): %#v", ctx)
	}
}

// TestDevSessionExplicitOptOut: within a dev env, LAZULI_DEV_SESSION=0
// force-disables the header path.
func TestDevSessionExplicitOptOut(t *testing.T) {
	t.Setenv("LAZULI_ENV", "dev")
	t.Setenv("LAZULI_DEV_SESSION", "0")
	req := httptest.NewRequest("GET", "/", nil)
	req.Header.Set("X-Lazuli-User-ID", "9")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateDevSession(req, ctx)
	if ctx.User != nil || ctx.Actor != ActorAnonymous {
		t.Fatalf("LAZULI_DEV_SESSION=0 did not disable dev-session: %#v", ctx)
	}
}

// TestDevSessionOptInCannotEscapeProd: LAZULI_DEV_SESSION truthy must NOT
// re-enable the header path outside a dev env.
func TestDevSessionOptInCannotEscapeProd(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	t.Setenv("LAZULI_DEV_SESSION", "1")
	req := httptest.NewRequest("GET", "/", nil)
	req.Header.Set("X-Lazuli-User-ID", "9")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateDevSession(req, ctx)
	if ctx.User != nil || ctx.Actor != ActorAnonymous {
		t.Fatalf("LAZULI_DEV_SESSION=1 re-enabled dev-session in production (fail-open!): %#v", ctx)
	}
}

func TestRequestBodyCapped(t *testing.T) {
	body := strings.NewReader(strings.Repeat("a", 2<<20))
	req := httptest.NewRequest("POST", "/", body)
	w := httptest.NewRecorder()

	_, err := readRequestBody(w, req)
	if err == nil {
		t.Fatal("readRequestBody accepted a body larger than the cap")
	}
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("readRequestBody error = %T, want *Error", err)
	}
	if le.Status != http.StatusRequestEntityTooLarge {
		t.Fatalf("status = %d, want %d", le.Status, http.StatusRequestEntityTooLarge)
	}
}

func TestErrorEnvelopeRedactsPgError(t *testing.T) {
	pgErr := &pgconn.PgError{
		Code:           "42P01",
		Message:        `relation "secret_accounts" does not exist`,
		ConstraintName: "secret_accounts_email_key",
		ColumnName:     "internal_email",
	}
	lazErr := classifyDBError("insert", pgErr)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/account.create", nil)
	writeError(rec, req, lazErr)

	body := rec.Body.String()
	for _, banned := range []string{"secret_accounts", "internal_email", "42P01", "does not exist"} {
		if strings.Contains(body, banned) {
			t.Fatalf("wire payload leaked pg error detail %q: %s", banned, body)
		}
	}
	payload := decodePayload(t, rec.Body.Bytes())
	if got := payload["code"]; got != CodeInternal {
		t.Fatalf("code = %q, want %q", got, CodeInternal)
	}

	rec = httptest.NewRecorder()
	writeError(rec, req, errors.New(`open C:\secret\stack.txt: permission denied`))
	body = rec.Body.String()
	if strings.Contains(body, "secret") || strings.Contains(body, "permission denied") {
		t.Fatalf("wire payload leaked unknown error detail: %s", body)
	}
	payload = decodePayload(t, rec.Body.Bytes())
	if got := payload["code"]; got != CodeInternal {
		t.Fatalf("unknown error code = %q, want %q", got, CodeInternal)
	}
}
