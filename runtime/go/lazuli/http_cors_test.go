package lazuli

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestCORSMiddlewareAllowsExactOrigin(t *testing.T) {
	handler := CORSMiddleware(AppCors{
		Allow:       []string{"https://app.example.com"},
		Credentials: true,
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusAccepted)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(headerOrigin, "https://app.example.com")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	header := rec.Result().Header
	if got := header.Get(headerAccessControlAllowOrigin); got != "https://app.example.com" {
		t.Fatalf("%s = %q, want allowed origin", headerAccessControlAllowOrigin, got)
	}
	if got := header.Get(headerAccessControlAllowCredentials); got != "true" {
		t.Fatalf("%s = %q, want true", headerAccessControlAllowCredentials, got)
	}
	if got := header.Get("Vary"); got != "Origin" {
		t.Fatalf("Vary = %q, want Origin", got)
	}
}

func TestCORSMiddlewareSkipsDisallowedSimpleOrigin(t *testing.T) {
	called := false
	handler := CORSMiddleware(AppCors{
		Allow: []string{"https://app.example.com"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		called = true
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(headerOrigin, "https://evil.example.com")

	handler.ServeHTTP(rec, req)

	if !called {
		t.Fatal("next handler was not called")
	}
	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowOrigin); got != "" {
		t.Fatalf("%s = %q, want empty", headerAccessControlAllowOrigin, got)
	}
	if got := rec.Result().Header.Get("Vary"); got != "Origin" {
		t.Fatalf("Vary = %q, want Origin", got)
	}
}

func TestCORSMiddlewareHandlesPreflight(t *testing.T) {
	called := false
	handler := CORSMiddleware(AppCors{
		Allow:       []string{"https://app.example.com"},
		Credentials: true,
		MaxAge:      3600,
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodOptions, "/", nil)
	req.Header.Set(headerOrigin, "https://app.example.com")
	req.Header.Set(headerAccessControlRequestMethod, http.MethodPut)
	req.Header.Set(headerAccessControlRequestHeaders, "x-api-key, content-type")

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("next handler was called for preflight")
	}
	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	header := rec.Result().Header
	if got := header.Get(headerAccessControlAllowOrigin); got != "https://app.example.com" {
		t.Fatalf("%s = %q, want allowed origin", headerAccessControlAllowOrigin, got)
	}
	if got := header.Get(headerAccessControlAllowCredentials); got != "true" {
		t.Fatalf("%s = %q, want true", headerAccessControlAllowCredentials, got)
	}
	if got := header.Get(headerAccessControlAllowMethods); got != corsAllowMethods {
		t.Fatalf("%s = %q, want %q", headerAccessControlAllowMethods, got, corsAllowMethods)
	}
	if got := header.Get(headerAccessControlAllowHeaders); got != "X-Api-Key, Content-Type" {
		t.Fatalf("%s = %q, want canonical request headers", headerAccessControlAllowHeaders, got)
	}
	if got := header.Get(headerAccessControlMaxAge); got != "3600" {
		t.Fatalf("%s = %q, want 3600", headerAccessControlMaxAge, got)
	}
	if got := header.Get("Vary"); got != "Origin, Access-Control-Request-Method, Access-Control-Request-Headers" {
		t.Fatalf("Vary = %q, want CORS preflight vary headers", got)
	}
}

func TestCORSMiddlewareRejectsDisallowedPreflightOrigin(t *testing.T) {
	called := false
	handler := CORSMiddleware(AppCors{
		Allow: []string{"https://app.example.com"},
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodOptions, "/", nil)
	req.Header.Set(headerOrigin, "https://evil.example.com")
	req.Header.Set(headerAccessControlRequestMethod, http.MethodDelete)

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("next handler was called for rejected preflight")
	}
	if rec.Code != http.StatusForbidden {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusForbidden)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowOrigin); got != "" {
		t.Fatalf("%s = %q, want empty", headerAccessControlAllowOrigin, got)
	}
}

func TestCORSMiddlewareRejectsUnsupportedPreflightMethod(t *testing.T) {
	called := false
	handler := CORSMiddleware(AppCors{
		Allow: []string{"https://app.example.com"},
	})(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		called = true
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodOptions, "/", nil)
	req.Header.Set(headerOrigin, "https://app.example.com")
	req.Header.Set(headerAccessControlRequestMethod, http.MethodTrace)

	handler.ServeHTTP(rec, req)

	if called {
		t.Fatal("next handler was called for rejected preflight")
	}
	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowMethods); got != "" {
		t.Fatalf("%s = %q, want empty", headerAccessControlAllowMethods, got)
	}
}

func TestCORSMiddlewareAllowsWildcardWithoutCredentials(t *testing.T) {
	handler := CORSMiddleware(AppCors{
		Allow: []string{"*"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusAccepted)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(headerOrigin, "https://any.example.com")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowOrigin); got != "*" {
		t.Fatalf("%s = %q, want *", headerAccessControlAllowOrigin, got)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowCredentials); got != "" {
		t.Fatalf("%s = %q, want empty", headerAccessControlAllowCredentials, got)
	}
}

func TestCORSMiddlewareRejectsWildcardWithCredentials(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("CORSMiddleware did not panic")
		}
	}()

	_ = CORSMiddleware(AppCors{
		Allow:       []string{"*"},
		Credentials: true,
	})
}

func TestCORSMiddlewareAllowsWildcardSubdomain(t *testing.T) {
	handler := CORSMiddleware(AppCors{
		Allow: []string{"https://*.example.com"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(headerOrigin, "https://api.example.com")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowOrigin); got != "https://api.example.com" {
		t.Fatalf("%s = %q, want wildcard subdomain origin", headerAccessControlAllowOrigin, got)
	}
}

func TestCORSMiddlewareWildcardSubdomainDoesNotMatchApex(t *testing.T) {
	handler := CORSMiddleware(AppCors{
		Allow: []string{"https://*.example.com"},
	})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(headerOrigin, "https://example.com")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowOrigin); got != "" {
		t.Fatalf("%s = %q, want empty", headerAccessControlAllowOrigin, got)
	}
}

func TestCORSMiddlewareEmptyAllowIsNoop(t *testing.T) {
	handler := CORSMiddleware(AppCors{})(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusAccepted)
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/", nil)
	req.Header.Set(headerOrigin, "https://app.example.com")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if got := rec.Result().Header.Get("Vary"); got != "" {
		t.Fatalf("Vary = %q, want empty", got)
	}
	if got := rec.Result().Header.Get(headerAccessControlAllowOrigin); got != "" {
		t.Fatalf("%s = %q, want empty", headerAccessControlAllowOrigin, got)
	}
}
