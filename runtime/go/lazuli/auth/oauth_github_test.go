package auth

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestDefaultGitHubOAuthDescriptorUsesCanonicalEndpointsAndScopes(t *testing.T) {
	t.Parallel()

	descriptor := DefaultGitHubOAuthDescriptor()
	if descriptor.AuthorizeURL != DefaultGitHubOAuthAuthorizeURL {
		t.Fatalf("AuthorizeURL = %q, want default", descriptor.AuthorizeURL)
	}
	if descriptor.TokenURL != DefaultGitHubOAuthTokenURL {
		t.Fatalf("TokenURL = %q, want default", descriptor.TokenURL)
	}
	if descriptor.UserEmailsURL != DefaultGitHubOAuthUserEmailsURL {
		t.Fatalf("UserEmailsURL = %q, want default", descriptor.UserEmailsURL)
	}
	if want := []string{"read:user", "user:email"}; !reflect.DeepEqual(descriptor.Scopes, want) {
		t.Fatalf("Scopes = %#v, want %#v", descriptor.Scopes, want)
	}

	descriptor.Scopes[0] = "mutated"
	if got := DefaultGitHubOAuthDescriptor().Scopes[0]; got != "read:user" {
		t.Fatalf("default scopes shared backing array, got %q", got)
	}
}

func TestNormalizeGitHubOAuthCallbackURL(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		raw  string
		want string
	}{
		{
			name: "https lowercases host and removes default port",
			raw:  " HTTPS://Example.COM:443/Auth/GitHub/Callback ",
			want: "https://example.com/Auth/GitHub/Callback",
		},
		{
			name: "http localhost is allowed",
			raw:  "http://LOCALHOST:80/auth/github/callback?tenant=acme",
			want: "http://localhost/auth/github/callback?tenant=acme",
		},
		{
			name: "empty path becomes slash",
			raw:  "https://example.com",
			want: "https://example.com/",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got, err := NormalizeGitHubOAuthCallbackURL(tt.raw)
			if err != nil {
				t.Fatalf("NormalizeGitHubOAuthCallbackURL() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeGitHubOAuthCallbackURL() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestNormalizeGitHubOAuthDescriptorAppliesDefaultsAndDoesNotMutate(t *testing.T) {
	t.Parallel()

	scopes := []string{" user:email ", "read:user", "user:email", ""}
	descriptor, err := NormalizeGitHubOAuthDescriptor(GitHubOAuthDescriptor{
		ClientID:     " client-id ",
		ClientSecret: " client-secret ",
		CallbackURL:  "https://EXAMPLE.com:443/auth/callback",
		Scopes:       scopes,
	})
	if err != nil {
		t.Fatalf("NormalizeGitHubOAuthDescriptor() error = %v", err)
	}
	if descriptor.ClientID != "client-id" || descriptor.ClientSecret != "client-secret" {
		t.Fatalf("credentials were not trimmed: %#v", descriptor)
	}
	if descriptor.CallbackURL != "https://example.com/auth/callback" {
		t.Fatalf("CallbackURL = %q, want normalized", descriptor.CallbackURL)
	}
	if want := []string{"user:email", "read:user"}; !reflect.DeepEqual(descriptor.Scopes, want) {
		t.Fatalf("Scopes = %#v, want %#v", descriptor.Scopes, want)
	}
	if descriptor.AuthorizeURL != DefaultGitHubOAuthAuthorizeURL ||
		descriptor.TokenURL != DefaultGitHubOAuthTokenURL ||
		descriptor.UserEmailsURL != DefaultGitHubOAuthUserEmailsURL {
		t.Fatalf("endpoints not defaulted: %#v", descriptor)
	}
	if scopes[0] != " user:email " {
		t.Fatalf("NormalizeGitHubOAuthDescriptor mutated input scopes: %#v", scopes)
	}
}

func TestPlanGitHubOAuthValidatesAndReturnsCopies(t *testing.T) {
	t.Parallel()

	plan, err := PlanGitHubOAuth(GitHubOAuthDescriptor{
		ClientID:     "client-id",
		ClientSecret: "client-secret",
		CallbackURL:  "https://example.com/auth/callback",
		Scopes:       []string{"user:email"},
	})
	if err != nil {
		t.Fatalf("PlanGitHubOAuth() error = %v", err)
	}
	if plan.Provider != "github" {
		t.Fatalf("Provider = %q, want github", plan.Provider)
	}
	if plan.AuthorizeURL != DefaultGitHubOAuthAuthorizeURL ||
		plan.TokenURL != DefaultGitHubOAuthTokenURL ||
		plan.UserEmailsURL != DefaultGitHubOAuthUserEmailsURL {
		t.Fatalf("unexpected default endpoints: %#v", plan)
	}

	summary := plan.SafeSummary()
	plan.Scopes[0] = "mutated"
	if summary.Scopes[0] != "user:email" {
		t.Fatalf("SafeSummary shared scope backing array: %#v", summary.Scopes)
	}
}

func TestValidateGitHubOAuthDescriptorRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   GitHubOAuthDescriptor
		want string
	}{
		{
			name: "missing required fields",
			in:   GitHubOAuthDescriptor{},
			want: "client_id is required",
		},
		{
			name: "non loopback http callback",
			in: GitHubOAuthDescriptor{
				ClientID:     "client-id",
				ClientSecret: "client-secret",
				CallbackURL:  "http://example.com/auth/callback",
			},
			want: "http is only allowed for loopback",
		},
		{
			name: "callback fragment",
			in: GitHubOAuthDescriptor{
				ClientID:     "client-id",
				ClientSecret: "client-secret",
				CallbackURL:  "https://example.com/auth/callback#token",
			},
			want: "must not include a fragment",
		},
		{
			name: "endpoint query",
			in: GitHubOAuthDescriptor{
				ClientID:     "client-id",
				ClientSecret: "client-secret",
				CallbackURL:  "https://example.com/auth/callback",
				TokenURL:     "https://github.com/login/oauth/access_token?client_secret=secret",
			},
			want: "token_url must not include query parameters",
		},
		{
			name: "scope whitespace",
			in: GitHubOAuthDescriptor{
				ClientID:     "client-id",
				ClientSecret: "client-secret",
				CallbackURL:  "https://example.com/auth/callback",
				Scopes:       []string{"user:email", "repo read"},
			},
			want: "must not contain whitespace",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateGitHubOAuthDescriptor(tt.in)
			if !errors.Is(err, ErrGitHubOAuthDescriptorInvalid) {
				t.Fatalf("ValidateGitHubOAuthDescriptor() error = %v, want ErrGitHubOAuthDescriptorInvalid", err)
			}
			if !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("ValidateGitHubOAuthDescriptor() error = %q, want fragment %q", err, tt.want)
			}
		})
	}
}

func TestSafeGitHubOAuthSummaryRedactsSecretsAndURLQuery(t *testing.T) {
	t.Parallel()

	plan := GitHubOAuthPlan{
		Provider:      "github",
		ClientID:      "client-id",
		ClientSecret:  "client-secret",
		CallbackURL:   "https://example.com/auth/callback?code=secret-code&state=secret-state",
		Scopes:        []string{"user:email"},
		AuthorizeURL:  "https://github.com/login/oauth/authorize?client_secret=secret",
		TokenURL:      "https://github.com/login/oauth/access_token",
		UserEmailsURL: "https://api.github.com/user/emails",
	}

	summary := SafeGitHubOAuthSummary(plan)
	if !summary.ClientSecretConfigured {
		t.Fatalf("ClientSecretConfigured = false, want true")
	}
	for name, value := range map[string]string{
		"CallbackURL":  summary.CallbackURL,
		"AuthorizeURL": summary.AuthorizeURL,
		"TokenURL":     summary.TokenURL,
	} {
		if strings.Contains(value, "secret") || strings.Contains(value, "?") {
			t.Fatalf("%s leaked sensitive URL data: %q", name, value)
		}
	}
	if summary.CallbackURL != "https://example.com/auth/callback" {
		t.Fatalf("CallbackURL = %q, want query redacted", summary.CallbackURL)
	}
	if summary.Scopes[0] != "user:email" {
		t.Fatalf("Scopes = %#v, want copied plan scopes", summary.Scopes)
	}
}
