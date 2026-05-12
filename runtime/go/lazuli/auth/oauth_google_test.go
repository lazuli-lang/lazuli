package auth

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"

	"golang.org/x/oauth2"
)

func TestGoogleOAuthRedirectURLUsesOnlineAccess(t *testing.T) {
	t.Parallel()
	contract := OAuthContract{
		Provider:     "google",
		ClientID:     "client-id",
		ClientSecret: "client-secret",
		RedirectURL:  "https://example.com/auth/callback",
		Scopes:       []string{"openid", "email", "profile"},
	}
	redirect, err := GoogleOAuthRedirectURL(contract, "state-123")
	if err != nil {
		t.Fatalf("GoogleOAuthRedirectURL: %v", err)
	}
	u, err := url.Parse(redirect)
	if err != nil {
		t.Fatalf("parse redirect URL: %v", err)
	}
	q := u.Query()
	if q.Get("state") != "state-123" {
		t.Fatalf("expected state to round-trip, got %q", q.Get("state"))
	}
	if q.Get("access_type") != "online" {
		t.Fatalf("expected online access, got %q", q.Get("access_type"))
	}
	if q.Get("client_id") != contract.ClientID {
		t.Fatalf("expected client_id %q, got %q", contract.ClientID, q.Get("client_id"))
	}
	if q.Get("redirect_uri") != contract.RedirectURL {
		t.Fatalf("expected redirect_uri %q, got %q", contract.RedirectURL, q.Get("redirect_uri"))
	}
	for _, scope := range contract.Scopes {
		if !strings.Contains(q.Get("scope"), scope) {
			t.Fatalf("expected scope %q in %q", scope, q.Get("scope"))
		}
	}
}

func TestGoogleOAuthCallbackFetchesUserInfo(t *testing.T) {
	t.Parallel()
	var sawTokenExchange bool
	var sawUserInfo bool
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/token":
			sawTokenExchange = true
			if r.Method != http.MethodPost {
				t.Fatalf("token exchange method = %s, want POST", r.Method)
			}
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse token exchange form: %v", err)
			}
			if r.Form.Get("code") != "auth-code" {
				t.Fatalf("exchange code = %q, want auth-code", r.Form.Get("code"))
			}
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"access_token": "access-token",
				"token_type":   "Bearer",
				"expires_in":   3600,
			})
		case "/v1/userinfo":
			sawUserInfo = true
			if got := r.Header.Get("Authorization"); got != "Bearer access-token" {
				t.Fatalf("Authorization = %q, want bearer token", got)
			}
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(GoogleUserInfo{
				Sub:           "google-sub",
				Email:         "user@example.com",
				EmailVerified: true,
				Name:          "Test User",
				Picture:       "https://example.com/avatar.png",
			})
		default:
			t.Fatalf("unexpected path %s", r.URL.Path)
		}
	}))
	defer server.Close()

	target, err := url.Parse(server.URL)
	if err != nil {
		t.Fatalf("parse test server URL: %v", err)
	}
	ctx := context.WithValue(context.Background(), oauth2.HTTPClient, &http.Client{
		Transport: rewriteGoogleTransport{target: target},
	})
	contract := OAuthContract{
		Provider:     "google",
		ClientID:     "client-id",
		ClientSecret: "client-secret",
		RedirectURL:  "https://example.com/auth/callback",
		Scopes:       []string{"openid", "email", "profile"},
	}
	info, token, err := GoogleOAuthCallback(ctx, contract, "auth-code", "state-123", "state-123")
	if err != nil {
		t.Fatalf("GoogleOAuthCallback: %v", err)
	}
	if !sawTokenExchange {
		t.Fatalf("expected token exchange request")
	}
	if !sawUserInfo {
		t.Fatalf("expected userinfo request")
	}
	if token.AccessToken != "access-token" {
		t.Fatalf("access token = %q, want access-token", token.AccessToken)
	}
	if info.Sub != "google-sub" || info.Email != "user@example.com" || !info.EmailVerified {
		t.Fatalf("unexpected userinfo: %+v", info)
	}
}

func TestGoogleOAuthCallbackStateMismatch(t *testing.T) {
	t.Parallel()
	_, _, err := GoogleOAuthCallback(
		context.Background(),
		OAuthContract{Provider: "google"},
		"auth-code",
		"state-123",
		"different-state",
	)
	if !errors.Is(err, ErrOAuthStateMismatch) {
		t.Fatalf("expected ErrOAuthStateMismatch, got %v", err)
	}
}

type rewriteGoogleTransport struct {
	target *url.URL
	base   http.RoundTripper
}

func (t rewriteGoogleTransport) RoundTrip(req *http.Request) (*http.Response, error) {
	base := t.base
	if base == nil {
		base = http.DefaultTransport
	}
	rewritten := req.Clone(req.Context())
	rewritten.URL.Scheme = t.target.Scheme
	rewritten.URL.Host = t.target.Host
	rewritten.Host = req.URL.Host
	return base.RoundTrip(rewritten)
}
