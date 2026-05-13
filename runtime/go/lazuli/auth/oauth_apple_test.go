package auth

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

const testApplePrivateKey = `-----BEGIN PRIVATE KEY-----
dGVzdC1rZXk=
-----END PRIVATE KEY-----`

func TestAppleOAuthProviderDescriptorNormalize(t *testing.T) {
	t.Parallel()

	descriptor := AppleOAuthProviderDescriptor{
		TeamID:        " TEAM123 ",
		ClientID:      " com.example.web ",
		KeyID:         " KEY123 ",
		PrivateKeyPEM: " " + testApplePrivateKey + " ",
		CallbackURL:   " https://example.com/auth/apple/callback ",
		Scopes:        []string{" Email ", "name", "email", "", " NAME "},
	}
	normalized := descriptor.Normalize()

	if normalized.TeamID != "TEAM123" {
		t.Fatalf("TeamID = %q, want TEAM123", normalized.TeamID)
	}
	if normalized.ClientID != "com.example.web" {
		t.Fatalf("ClientID = %q, want com.example.web", normalized.ClientID)
	}
	if normalized.KeyID != "KEY123" {
		t.Fatalf("KeyID = %q, want KEY123", normalized.KeyID)
	}
	if normalized.PrivateKeyPEM != testApplePrivateKey {
		t.Fatalf("PrivateKeyPEM was not trimmed")
	}
	if normalized.CallbackURL != "https://example.com/auth/apple/callback" {
		t.Fatalf("CallbackURL = %q, want normalized URL", normalized.CallbackURL)
	}
	wantScopes := []string{"email", "name"}
	if !reflect.DeepEqual(normalized.Scopes, wantScopes) {
		t.Fatalf("Scopes = %#v, want %#v", normalized.Scopes, wantScopes)
	}
}

func TestPlanAppleOAuthProviderBuildsDeterministicPlan(t *testing.T) {
	t.Parallel()

	plan, err := PlanAppleOAuthProvider(AppleOAuthProviderDescriptor{
		TeamID:        " TEAM123 ",
		ClientID:      " com.example.web ",
		KeyID:         " KEY123 ",
		PrivateKeyPEM: testApplePrivateKey,
		CallbackURL:   "https://example.com/auth/apple/callback?next=/app",
		Scopes:        []string{"name", "email", "name"},
	})
	if err != nil {
		t.Fatalf("PlanAppleOAuthProvider() error = %v", err)
	}

	want := AppleOAuthProviderPlan{
		AuthorizeURL: AppleOAuthAuthorizeEndpoint,
		TokenURL:     AppleOAuthTokenEndpoint,
		TeamID:       "TEAM123",
		ClientID:     "com.example.web",
		KeyID:        "KEY123",
		CallbackURL:  "https://example.com/auth/apple/callback?next=/app",
		Scopes:       []string{"email", "name"},
	}
	if !reflect.DeepEqual(plan, want) {
		t.Fatalf("PlanAppleOAuthProvider() = %#v, want %#v", plan, want)
	}

	plan.Scopes[0] = "changed"
	again, err := PlanAppleOAuthProvider(AppleOAuthProviderDescriptor{
		TeamID:        "TEAM123",
		ClientID:      "com.example.web",
		KeyID:         "KEY123",
		PrivateKeyPEM: testApplePrivateKey,
		CallbackURL:   "https://example.com/auth/apple/callback",
		Scopes:        []string{"email"},
	})
	if err != nil {
		t.Fatalf("PlanAppleOAuthProvider(second) error = %v", err)
	}
	if again.Scopes[0] != "email" {
		t.Fatalf("plan scopes share mutable storage")
	}
}

func TestAppleOAuthProviderDescriptorValidateRejectsInvalidShape(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		descriptor AppleOAuthProviderDescriptor
		want       error
	}{
		{
			name: "missing metadata",
			descriptor: AppleOAuthProviderDescriptor{
				CallbackURL: "https://example.com/callback",
			},
			want: ErrAppleOAuthDescriptorInvalid,
		},
		{
			name: "missing callback",
			descriptor: validAppleOAuthProviderDescriptor(func(d *AppleOAuthProviderDescriptor) {
				d.CallbackURL = ""
			}),
			want: ErrAppleOAuthCallbackURLInvalid,
		},
		{
			name: "invalid private key",
			descriptor: validAppleOAuthProviderDescriptor(func(d *AppleOAuthProviderDescriptor) {
				d.PrivateKeyPEM = "not pem"
			}),
			want: ErrAppleOAuthDescriptorInvalid,
		},
		{
			name: "invalid scope",
			descriptor: validAppleOAuthProviderDescriptor(func(d *AppleOAuthProviderDescriptor) {
				d.Scopes = []string{"profile"}
			}),
			want: ErrAppleOAuthScopeInvalid,
		},
		{
			name: "http callback",
			descriptor: validAppleOAuthProviderDescriptor(func(d *AppleOAuthProviderDescriptor) {
				d.CallbackURL = "http://example.com/callback"
			}),
			want: ErrAppleOAuthCallbackURLInvalid,
		},
		{
			name: "callback with userinfo",
			descriptor: validAppleOAuthProviderDescriptor(func(d *AppleOAuthProviderDescriptor) {
				d.CallbackURL = "https://user:pass@example.com/callback"
			}),
			want: ErrAppleOAuthCallbackURLInvalid,
		},
		{
			name: "callback with fragment",
			descriptor: validAppleOAuthProviderDescriptor(func(d *AppleOAuthProviderDescriptor) {
				d.CallbackURL = "https://example.com/callback#token"
			}),
			want: ErrAppleOAuthCallbackURLInvalid,
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			err := tc.descriptor.Validate()
			if !errors.Is(err, tc.want) {
				t.Fatalf("Validate() error = %v, want %v", err, tc.want)
			}
		})
	}
}

func TestValidateAppleOAuthScopes(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		scopes  []string
		wantErr bool
	}{
		{name: "empty", scopes: nil},
		{name: "supported", scopes: []string{" email ", "NAME"}},
		{name: "duplicate supported", scopes: []string{"name", "name"}},
		{name: "unsupported", scopes: []string{"openid"}, wantErr: true},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateAppleOAuthScopes(tc.scopes)
			if tc.wantErr && !errors.Is(err, ErrAppleOAuthScopeInvalid) {
				t.Fatalf("ValidateAppleOAuthScopes() error = %v, want ErrAppleOAuthScopeInvalid", err)
			}
			if !tc.wantErr && err != nil {
				t.Fatalf("ValidateAppleOAuthScopes() error = %v", err)
			}
		})
	}
}

func TestAppleOAuthSafeSummaryRedactsSecretsAndURL(t *testing.T) {
	t.Parallel()

	descriptor := AppleOAuthProviderDescriptor{
		TeamID:        "TEAM123456",
		ClientID:      "com.example.service",
		KeyID:         "KEY123456",
		PrivateKeyPEM: testApplePrivateKey,
		CallbackURL:   "https://example.com/auth/apple/callback?code=secret&state=secret#fragment",
		Scopes:        []string{"name", "email"},
	}
	summary := descriptor.SafeSummary()

	if summary.AuthorizeURL != AppleOAuthAuthorizeEndpoint || summary.TokenURL != AppleOAuthTokenEndpoint {
		t.Fatalf("summary endpoints = %q %q", summary.AuthorizeURL, summary.TokenURL)
	}
	if !summary.PrivateKeyConfigured {
		t.Fatalf("PrivateKeyConfigured = false, want true")
	}
	for _, leaked := range []string{"TEAM123456", "com.example.service", "KEY123456", "test-key", "code=secret", "state=secret", "#fragment"} {
		if strings.Contains(summary.TeamID, leaked) ||
			strings.Contains(summary.ClientID, leaked) ||
			strings.Contains(summary.KeyID, leaked) ||
			strings.Contains(summary.CallbackURL, leaked) {
			t.Fatalf("summary leaked %q: %#v", leaked, summary)
		}
	}
	if summary.CallbackURL != "https://example.com/auth/apple/callback" {
		t.Fatalf("CallbackURL = %q, want query and fragment removed", summary.CallbackURL)
	}

	summary.Scopes[0] = "changed"
	if descriptor.SafeSummary().Scopes[0] != "email" {
		t.Fatalf("summary scopes share mutable storage")
	}
}

func validAppleOAuthProviderDescriptor(mutators ...func(*AppleOAuthProviderDescriptor)) AppleOAuthProviderDescriptor {
	descriptor := AppleOAuthProviderDescriptor{
		TeamID:        "TEAM123",
		ClientID:      "com.example.web",
		KeyID:         "KEY123",
		PrivateKeyPEM: testApplePrivateKey,
		CallbackURL:   "https://example.com/auth/apple/callback",
		Scopes:        []string{"email", "name"},
	}
	for _, mutate := range mutators {
		mutate(&descriptor)
	}
	return descriptor
}
