package auth

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestSessionCookieHelpersReadWriteClear(t *testing.T) {
	opts := SessionCookieOptions{
		Name:     "sid",
		Path:     "/app",
		Domain:   "example.test",
		Secure:   true,
		SameSite: http.SameSiteLaxMode,
	}
	expiresAt := time.Now().Add(time.Hour)
	rec := httptest.NewRecorder()

	WriteSessionCookie(rec, "token", expiresAt, opts)

	cookies := rec.Result().Cookies()
	if len(cookies) != 1 {
		t.Fatalf("cookies len = %d, want 1", len(cookies))
	}
	cookie := cookies[0]
	if cookie.Name != "sid" {
		t.Fatalf("Name = %q, want sid", cookie.Name)
	}
	if cookie.Value != "token" {
		t.Fatalf("Value = %q, want token", cookie.Value)
	}
	if cookie.Path != "/app" {
		t.Fatalf("Path = %q, want /app", cookie.Path)
	}
	if cookie.Domain != "example.test" {
		t.Fatalf("Domain = %q, want example.test", cookie.Domain)
	}
	if !cookie.HttpOnly {
		t.Fatal("HttpOnly = false, want true")
	}
	if !cookie.Secure {
		t.Fatal("Secure = false, want true")
	}
	if cookie.SameSite != http.SameSiteLaxMode {
		t.Fatalf("SameSite = %v, want %v", cookie.SameSite, http.SameSiteLaxMode)
	}
	if cookie.MaxAge <= 0 {
		t.Fatalf("MaxAge = %d, want positive", cookie.MaxAge)
	}

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.AddCookie(cookie)
	got, err := ReadSessionCookie(req, opts)
	if err != nil {
		t.Fatalf("ReadSessionCookie returned error: %v", err)
	}
	if got != "token" {
		t.Fatalf("ReadSessionCookie = %q, want token", got)
	}

	clearRec := httptest.NewRecorder()
	ClearSessionCookie(clearRec, opts)
	header := clearRec.Result().Header.Get("Set-Cookie")
	for _, want := range []string{"sid=", "Path=/app", "Domain=example.test", "Max-Age=0", "HttpOnly", "SameSite=Lax"} {
		if !strings.Contains(header, want) {
			t.Fatalf("clear Set-Cookie = %q, want %q", header, want)
		}
	}
}

func TestSessionMiddlewareResolvesSessionIntoRequestContext(t *testing.T) {
	store := NewMemorySessionStore()
	token, _, err := store.Create(
		context.Background(),
		lazuli.ID(42),
		time.Hour,
		SessionAttrs{"provider": "password"},
	)
	if err != nil {
		t.Fatalf("Create: %v", err)
	}

	handler := SessionMiddleware(store, SessionMiddlewareOptions{})(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		session, ok := SessionFromContext(r.Context())
		if !ok {
			t.Fatal("SessionFromContext ok = false, want true")
		}
		if session.UserID != lazuli.ID(42) {
			t.Fatalf("UserID = %d, want 42", session.UserID)
		}
		if got := session.Attrs["provider"]; got != "password" {
			t.Fatalf("provider attr = %#v, want password", got)
		}
		if err := SessionErrorFromContext(r.Context()); err != nil {
			t.Fatalf("SessionErrorFromContext = %v, want nil", err)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.AddCookie(&http.Cookie{Name: CookieName, Value: token})
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
}

func TestSessionMiddlewareMissingCookiePassesThrough(t *testing.T) {
	store := &middlewareStore{}
	handler := SessionMiddleware(store, SessionMiddlewareOptions{})(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, ok := SessionFromContext(r.Context()); ok {
			t.Fatal("SessionFromContext ok = true, want false")
		}
		if err := SessionErrorFromContext(r.Context()); err != nil {
			t.Fatalf("SessionErrorFromContext = %v, want nil", err)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))

	if store.resolves != 0 {
		t.Fatalf("Resolve calls = %d, want 0", store.resolves)
	}
	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
}

func TestSessionMiddlewareClearsBadCookieAndAttachesError(t *testing.T) {
	store := &middlewareStore{
		resolveFunc: func(context.Context, string) (Session, error) {
			return Session{}, ErrSessionExpired
		},
	}
	handler := SessionMiddleware(store, SessionMiddlewareOptions{})(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, ok := SessionFromContext(r.Context()); ok {
			t.Fatal("SessionFromContext ok = true, want false")
		}
		if err := SessionErrorFromContext(r.Context()); !errors.Is(err, ErrSessionExpired) {
			t.Fatalf("SessionErrorFromContext = %v, want ErrSessionExpired", err)
		}
		w.WriteHeader(http.StatusOK)
	}))

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.AddCookie(&http.Cookie{Name: CookieName, Value: "stale-token"})
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	if store.resolves != 1 {
		t.Fatalf("Resolve calls = %d, want 1", store.resolves)
	}
	header := rec.Result().Header.Get("Set-Cookie")
	for _, want := range []string{CookieName + "=", "Path=/", "Max-Age=0", "HttpOnly"} {
		if !strings.Contains(header, want) {
			t.Fatalf("Set-Cookie = %q, want %q", header, want)
		}
	}
}

func TestResolveSessionIntoCtxAppliesGenericIdentity(t *testing.T) {
	store := NewMemorySessionStore()
	token, _, err := store.Create(
		context.Background(),
		lazuli.ID(42),
		time.Hour,
		SessionAttrs{
			"org_id": lazuli.ID(7),
			"email":  "user@example.test",
			"roles":  "admin, ops",
		},
	)
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	ctx := &lazuli.Ctx{Context: context.Background(), Actor: lazuli.ActorAnonymous}

	session, err := ResolveSessionIntoCtx(ctx, store, token)
	if err != nil {
		t.Fatalf("ResolveSessionIntoCtx: %v", err)
	}

	if session.UserID != lazuli.ID(42) {
		t.Fatalf("session.UserID = %d, want 42", session.UserID)
	}
	if ctx.Actor != lazuli.ActorUser {
		t.Fatalf("ctx.Actor = %q, want %q", ctx.Actor, lazuli.ActorUser)
	}
	if ctx.User == nil || ctx.User.ID != lazuli.ID(42) || ctx.User.OrgID != lazuli.ID(7) {
		t.Fatalf("ctx.User = %#v, want id 42 org 7", ctx.User)
	}
	if ctx.User.Email != "user@example.test" {
		t.Fatalf("ctx.User.Email = %q, want user@example.test", ctx.User.Email)
	}
	if got := strings.Join(ctx.User.Roles, ","); got != "admin,ops" {
		t.Fatalf("ctx.User.Roles = %q, want admin,ops", got)
	}
	if ctx.Tenant == nil || ctx.Tenant.OrgID != lazuli.ID(7) {
		t.Fatalf("ctx.Tenant = %#v, want org 7", ctx.Tenant)
	}
	if _, ok := SessionFromContext(ctx.Context); !ok {
		t.Fatal("SessionFromContext(ctx.Context) ok = false, want true")
	}
}

type middlewareStore struct {
	resolves    int
	resolveFunc func(context.Context, string) (Session, error)
}

func (s *middlewareStore) Create(context.Context, lazuli.ID, time.Duration, SessionAttrs) (string, time.Time, error) {
	return "", time.Time{}, errors.New("unexpected Create call")
}

func (s *middlewareStore) Resolve(ctx context.Context, token string) (Session, error) {
	s.resolves++
	if s.resolveFunc == nil {
		return Session{}, errors.New("unexpected Resolve call")
	}
	return s.resolveFunc(ctx, token)
}

func (s *middlewareStore) Invalidate(context.Context, string) error {
	return errors.New("unexpected Invalidate call")
}

func (s *middlewareStore) CleanupExpired(context.Context) (int, error) {
	return 0, errors.New("unexpected CleanupExpired call")
}
