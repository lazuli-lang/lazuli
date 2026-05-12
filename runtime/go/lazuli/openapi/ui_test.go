package openapi

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestUIHandlerServesMinimalHTML(t *testing.T) {
	handler := UIHandler(UIConfig{
		SpecURL:      "/openapi.yaml",
		Title:        "Service API",
		CacheControl: "no-store",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if got := rec.Header().Get("Content-Type"); got != ContentTypeHTML {
		t.Fatalf("Content-Type = %q, want %q", got, ContentTypeHTML)
	}
	if got := rec.Header().Get("Content-Security-Policy"); got != defaultUIContentSecurityPolicy {
		t.Fatalf("Content-Security-Policy = %q, want %q", got, defaultUIContentSecurityPolicy)
	}
	if got := rec.Header().Get("X-Content-Type-Options"); got != "nosniff" {
		t.Fatalf("X-Content-Type-Options = %q, want nosniff", got)
	}
	if got := rec.Header().Get("Cache-Control"); got != "no-store" {
		t.Fatalf("Cache-Control = %q, want no-store", got)
	}

	body := rec.Body.String()
	for _, want := range []string{
		"<title>Service API</title>",
		"<h1>Service API</h1>",
		`<meta name="openapi-spec-url" content="/openapi.yaml">`,
		`<a href="/openapi.yaml" rel="noopener noreferrer">Open the OpenAPI document</a>`,
	} {
		if !strings.Contains(body, want) {
			t.Fatalf("body missing %q:\n%s", want, body)
		}
	}
	for _, disallowed := range []string{"<script", "<style", " style="} {
		if strings.Contains(strings.ToLower(body), disallowed) {
			t.Fatalf("body contains %q:\n%s", disallowed, body)
		}
	}
}

func TestUIHandlerServesAbsoluteSpecURL(t *testing.T) {
	handler := UIHandler(UIConfig{
		SpecURL: "https://api.example.test/openapi.json?version=1",
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if body := rec.Body.String(); !strings.Contains(body, `href="https://api.example.test/openapi.json?version=1"`) {
		t.Fatalf("body missing absolute spec URL:\n%s", body)
	}
}

func TestUIHandlerUsesDefaultTitle(t *testing.T) {
	handler := UIHandler(UIConfig{SpecURL: "/openapi.yaml"})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusOK)
	}
	if body := rec.Body.String(); !strings.Contains(body, "<title>OpenAPI</title>") {
		t.Fatalf("body missing default title:\n%s", body)
	}
}

func TestUIHandlerEscapesTitle(t *testing.T) {
	handler := UIHandler(UIConfig{
		SpecURL: "/openapi.yaml",
		Title:   `<script>alert("x")</script>`,
	})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	body := rec.Body.String()
	if strings.Contains(body, "<script>") {
		t.Fatalf("body contains unescaped script title:\n%s", body)
	}
	if !strings.Contains(body, `&lt;script&gt;alert(&#34;x&#34;)&lt;/script&gt;`) {
		t.Fatalf("body missing escaped title:\n%s", body)
	}
}

func TestUIHandlerHEADDoesNotWriteBody(t *testing.T) {
	handler := UIHandler(UIConfig{SpecURL: "/openapi.yaml"})

	getRec := httptest.NewRecorder()
	handler.ServeHTTP(getRec, httptest.NewRequest(http.MethodGet, "/docs", nil))

	headRec := httptest.NewRecorder()
	handler.ServeHTTP(headRec, httptest.NewRequest(http.MethodHead, "/docs", nil))

	if headRec.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", headRec.Code, http.StatusOK)
	}
	if headRec.Body.Len() != 0 {
		t.Fatalf("body length = %d, want 0", headRec.Body.Len())
	}
	if got, want := headRec.Header().Get("Content-Length"), getRec.Header().Get("Content-Length"); got != want {
		t.Fatalf("Content-Length = %q, want %q", got, want)
	}
}

func TestUIHandlerRejectsUnsupportedMethods(t *testing.T) {
	handler := UIHandler(UIConfig{SpecURL: "/openapi.yaml"})

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/docs", nil))

	if rec.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusMethodNotAllowed)
	}
	if got := rec.Header().Get("Allow"); got != "GET, HEAD" {
		t.Fatalf("Allow = %q, want GET, HEAD", got)
	}
}

func TestUIHandlerPanicsForInvalidSpecURL(t *testing.T) {
	tests := []struct {
		name    string
		specURL string
	}{
		{
			name:    "empty",
			specURL: "",
		},
		{
			name:    "relative",
			specURL: "openapi.yaml",
		},
		{
			name:    "relative traversal",
			specURL: "../openapi.yaml",
		},
		{
			name:    "absolute traversal",
			specURL: "/../openapi.yaml",
		},
		{
			name:    "encoded traversal",
			specURL: "/%2e%2e/openapi.yaml",
		},
		{
			name:    "backslash",
			specURL: "/openapi\\yaml",
		},
		{
			name:    "scheme relative",
			specURL: "//example.test/openapi.yaml",
		},
		{
			name:    "unsafe scheme",
			specURL: "javascript:alert(1)",
		},
		{
			name:    "credentials",
			specURL: "https://user:pass@example.test/openapi.yaml",
		},
		{
			name:    "fragment",
			specURL: "/openapi.yaml#v1",
		},
		{
			name:    "root path",
			specURL: "/",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			defer func() {
				if recover() == nil {
					t.Fatal("UIHandler did not panic")
				}
			}()

			_ = UIHandler(UIConfig{SpecURL: tt.specURL})
		})
	}
}
