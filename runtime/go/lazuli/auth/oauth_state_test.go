package auth

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestValidateOAuthStateTokenAndRedaction(t *testing.T) {
	t.Parallel()

	valid := "abcdEFGH1234-_abcdEFGH1234-_"
	if err := ValidateOAuthStateToken(valid); err != nil {
		t.Fatalf("ValidateOAuthStateToken(valid) error = %v", err)
	}
	if got := RedactOAuthStateToken(valid); got != "abcd...34-_" {
		t.Fatalf("RedactOAuthStateToken() = %q, want stable prefix/suffix", got)
	}

	tests := []struct {
		name  string
		token string
	}{
		{name: "short", token: "too-short"},
		{name: "space", token: "abcdEFGH1234-_ with-space"},
		{name: "unicode", token: "abcdEFGH1234-_☃"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := ValidateOAuthStateToken(tt.token); !errors.Is(err, ErrOAuthStateInvalid) {
				t.Fatalf("ValidateOAuthStateToken() error = %v, want ErrOAuthStateInvalid", err)
			}
		})
	}
}

func TestValidateOAuthNonceAllowsEmptyAndRejectsMalformed(t *testing.T) {
	t.Parallel()

	if err := ValidateOAuthNonce(""); err != nil {
		t.Fatalf("ValidateOAuthNonce(empty) error = %v", err)
	}
	if err := ValidateOAuthNonce("nonce-value-123"); err != nil {
		t.Fatalf("ValidateOAuthNonce(valid) error = %v", err)
	}
	for _, nonce := range []string{"short", "nonce value with space", "nonce-value-☃"} {
		if err := ValidateOAuthNonce(nonce); !errors.Is(err, ErrOAuthNonceInvalid) {
			t.Fatalf("ValidateOAuthNonce(%q) error = %v, want ErrOAuthNonceInvalid", nonce, err)
		}
	}
}

func TestPlanOAuthReturnURLNormalizesAndAllowlists(t *testing.T) {
	t.Parallel()

	policy := OAuthReturnURLPolicy{
		DefaultURL:          "/dashboard?tab=home",
		AllowedOrigins:      []string{" HTTPS://Example.COM:443 ", "http://LOCALHOST:80"},
		AllowedPathPrefixes: []string{" /dashboard ", "/settings"},
	}

	plan, err := PlanOAuthReturnURL("", policy)
	if err != nil {
		t.Fatalf("PlanOAuthReturnURL(default) error = %v", err)
	}
	if !plan.Defaulted || plan.ReturnURL != "/dashboard?tab=home" || plan.MatchedRule != "/dashboard" {
		t.Fatalf("default plan = %#v, want dashboard default", plan)
	}
	if plan.RedactedURL != "/dashboard?tab=%5Bredacted%5D" {
		t.Fatalf("RedactedURL = %q, want query value redacted", plan.RedactedURL)
	}

	external, err := PlanOAuthReturnURL("https://EXAMPLE.com:443/welcome?code=secret", policy)
	if err != nil {
		t.Fatalf("PlanOAuthReturnURL(external) error = %v", err)
	}
	if external.ReturnURL != "https://example.com/welcome?code=secret" ||
		external.MatchedRule != "https://example.com" ||
		!external.AllowedExternal {
		t.Fatalf("external plan = %#v, want normalized origin match", external)
	}

	local, err := PlanOAuthReturnURL("http://localhost:80/callback", policy)
	if err != nil {
		t.Fatalf("PlanOAuthReturnURL(loopback) error = %v", err)
	}
	if local.ReturnURL != "http://localhost/callback" || local.MatchedRule != "http://localhost" {
		t.Fatalf("loopback plan = %#v, want default port stripped", local)
	}
}

func TestPlanOAuthReturnURLRejectsOpenRedirects(t *testing.T) {
	t.Parallel()

	policy := OAuthReturnURLPolicy{
		DefaultURL:          "/dashboard",
		AllowedOrigins:      []string{"https://example.com"},
		AllowedPathPrefixes: []string{"/dashboard"},
	}
	tests := []struct {
		name string
		raw  string
	}{
		{name: "protocol relative", raw: "//evil.example.com/callback"},
		{name: "offsite origin", raw: "https://evil.example.com/callback"},
		{name: "userinfo", raw: "https://user@example.com/callback"},
		{name: "fragment", raw: "/dashboard#token"},
		{name: "path prefix boundary", raw: "/dashboardish"},
		{name: "dot segment escape", raw: "/dashboard/../admin"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if _, err := PlanOAuthReturnURL(tt.raw, policy); !errors.Is(err, ErrOAuthReturnURLInvalid) {
				t.Fatalf("PlanOAuthReturnURL(%q) error = %v, want ErrOAuthReturnURLInvalid", tt.raw, err)
			}
		})
	}
}

func TestNormalizeOAuthReturnURLPolicyDeduplicates(t *testing.T) {
	t.Parallel()

	policy, err := NormalizeOAuthReturnURLPolicy(OAuthReturnURLPolicy{
		DefaultURL:          "/dashboard",
		AllowedOrigins:      []string{"https://example.com", "https://EXAMPLE.com:443", ""},
		AllowedPathPrefixes: []string{"/dashboard", "/dashboard", ""},
	})
	if err != nil {
		t.Fatalf("NormalizeOAuthReturnURLPolicy() error = %v", err)
	}
	if want := []string{"https://example.com"}; !reflect.DeepEqual(policy.AllowedOrigins, want) {
		t.Fatalf("AllowedOrigins = %#v, want %#v", policy.AllowedOrigins, want)
	}
	if want := []string{"/dashboard"}; !reflect.DeepEqual(policy.AllowedPathPrefixes, want) {
		t.Fatalf("AllowedPathPrefixes = %#v, want %#v", policy.AllowedPathPrefixes, want)
	}
}

func TestValidateOAuthReturnURLPolicyRequiresAllowedDefault(t *testing.T) {
	t.Parallel()

	err := ValidateOAuthReturnURLPolicy(OAuthReturnURLPolicy{
		DefaultURL:          "/admin",
		AllowedPathPrefixes: []string{"/dashboard"},
	})
	if !errors.Is(err, ErrOAuthReturnURLInvalid) || !strings.Contains(err.Error(), "default_url") {
		t.Fatalf("ValidateOAuthReturnURLPolicy() error = %v, want default_url allowlist error", err)
	}
}

func TestPlanOAuthStateMetadataAndSummary(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC)
	meta, returnPlan, err := PlanOAuthStateMetadata(
		"state-token-1234567890",
		"nonce-token-123",
		"/dashboard?next=secret",
		OAuthReturnURLPolicy{
			DefaultURL:          "/dashboard",
			AllowedPathPrefixes: []string{"/dashboard"},
		},
		now,
		2*time.Minute,
	)
	if err != nil {
		t.Fatalf("PlanOAuthStateMetadata() error = %v", err)
	}
	if meta.ReturnURL != "/dashboard?next=secret" || returnPlan.MatchedRule != "/dashboard" {
		t.Fatalf("metadata return URL = %#v, plan = %#v", meta, returnPlan)
	}
	if !meta.ExpiresAt.Equal(now.Add(2*time.Minute)) || meta.Expired(now.Add(time.Minute)) {
		t.Fatalf("ExpiresAt = %s, Expired(before) = %v", meta.ExpiresAt, meta.Expired(now.Add(time.Minute)))
	}
	if err := meta.Validate(now.Add(time.Minute)); err != nil {
		t.Fatalf("OAuthStateMetadata.Validate(before expiry) error = %v", err)
	}
	if err := meta.Validate(now.Add(2 * time.Minute)); !errors.Is(err, ErrOAuthStateInvalid) {
		t.Fatalf("OAuthStateMetadata.Validate(at expiry) error = %v, want ErrOAuthStateInvalid", err)
	}

	summary := meta.RedactedSummary(now.Add(3 * time.Minute))
	if summary.State == meta.State || summary.Nonce == meta.Nonce || strings.Contains(summary.ReturnURL, "secret") || !summary.Expired {
		t.Fatalf("RedactedSummary leaked sensitive data or expiry: %#v", summary)
	}
}

func TestValidateOAuthCallback(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 13, 12, 0, 0, 0, time.UTC)
	expected := OAuthStateMetadata{
		State:     "state-token-1234567890",
		Nonce:     "nonce-token-123",
		ReturnURL: "/dashboard",
		IssuedAt:  now,
		ExpiresAt: now.Add(time.Minute),
	}

	if err := ValidateOAuthCallback(OAuthCallbackInput{
		State: "state-token-1234567890",
		Nonce: "nonce-token-123",
		Code:  "code-123",
		Now:   now,
	}, expected); err != nil {
		t.Fatalf("ValidateOAuthCallback(valid) error = %v", err)
	}

	tests := []struct {
		name  string
		input OAuthCallbackInput
		want  error
	}{
		{
			name:  "state mismatch",
			input: OAuthCallbackInput{State: "different-state-123456", Nonce: "nonce-token-123", Code: "code-123", Now: now},
			want:  ErrOAuthStateMismatch,
		},
		{
			name:  "nonce mismatch",
			input: OAuthCallbackInput{State: "state-token-1234567890", Nonce: "different-123", Code: "code-123", Now: now},
			want:  ErrOAuthCallbackInvalid,
		},
		{
			name:  "missing code",
			input: OAuthCallbackInput{State: "state-token-1234567890", Nonce: "nonce-token-123", Now: now},
			want:  ErrOAuthCallbackInvalid,
		},
		{
			name:  "expired",
			input: OAuthCallbackInput{State: "state-token-1234567890", Nonce: "nonce-token-123", Code: "code-123", Now: now.Add(time.Minute)},
			want:  ErrOAuthStateInvalid,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := ValidateOAuthCallback(tt.input, expected); !errors.Is(err, tt.want) {
				t.Fatalf("ValidateOAuthCallback() error = %v, want %v", err, tt.want)
			}
		})
	}
}
