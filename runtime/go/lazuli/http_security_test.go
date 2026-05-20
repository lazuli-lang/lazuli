package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgconn"
)

func TestDevSessionGatedInProduction(t *testing.T) {
	t.Setenv("LAZULI_ENV", "production")
	req := httptest.NewRequest("GET", "/", nil)
	req.Header.Set("X-Lazuli-User-ID", "42")
	req.Header.Set("X-Lazuli-Org-ID", "1")
	ctx := &Ctx{Actor: ActorAnonymous}
	populateDevSession(req, ctx)
	if ctx.User != nil || ctx.Actor != ActorAnonymous {
		t.Fatalf("dev session escaped to production: %#v", ctx)
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
