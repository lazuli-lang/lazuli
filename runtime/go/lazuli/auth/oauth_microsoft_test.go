package auth

import (
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestNormalizeMicrosoftOAuthTenant(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name   string
		tenant string
		want   string
	}{
		{name: "default", tenant: " ", want: "common"},
		{name: "alias", tenant: " Organizations ", want: "organizations"},
		{name: "domain", tenant: "Example.OnMicrosoft.COM", want: "example.onmicrosoft.com"},
		{name: "guid", tenant: "72F988BF-86F1-41AF-91AB-2D7CD011DB47", want: "72f988bf-86f1-41af-91ab-2d7cd011db47"},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if got := NormalizeMicrosoftOAuthTenant(tt.tenant); got != tt.want {
				t.Fatalf("NormalizeMicrosoftOAuthTenant(%q) = %q, want %q", tt.tenant, got, tt.want)
			}
		})
	}
}

func TestValidateMicrosoftOAuthTenant(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		tenant  string
		wantErr bool
	}{
		{name: "default common", tenant: "", wantErr: false},
		{name: "organizations", tenant: "organizations", wantErr: false},
		{name: "domain", tenant: "contoso.onmicrosoft.com", wantErr: false},
		{name: "guid", tenant: "72f988bf-86f1-41af-91ab-2d7cd011db47", wantErr: false},
		{name: "path traversal", tenant: "common/oauth2", wantErr: true},
		{name: "space", tenant: "my tenant", wantErr: true},
		{name: "empty label", tenant: "contoso..com", wantErr: true},
		{name: "boundary", tenant: "-contoso", wantErr: true},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateMicrosoftOAuthTenant(tt.tenant)
			if tt.wantErr {
				if !errors.Is(err, ErrMicrosoftOAuthTenantInvalid) {
					t.Fatalf("expected ErrMicrosoftOAuthTenantInvalid, got %v", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("ValidateMicrosoftOAuthTenant(%q): %v", tt.tenant, err)
			}
		})
	}
}

func TestMicrosoftOAuthTenantEndpoints(t *testing.T) {
	t.Parallel()
	endpoints, err := MicrosoftOAuthTenantEndpoints(" Contoso.OnMicrosoft.COM ")
	if err != nil {
		t.Fatalf("MicrosoftOAuthTenantEndpoints: %v", err)
	}
	if endpoints.AuthorizeURL != "https://login.microsoftonline.com/contoso.onmicrosoft.com/oauth2/v2.0/authorize" {
		t.Fatalf("AuthorizeURL = %q", endpoints.AuthorizeURL)
	}
	if endpoints.TokenURL != "https://login.microsoftonline.com/contoso.onmicrosoft.com/oauth2/v2.0/token" {
		t.Fatalf("TokenURL = %q", endpoints.TokenURL)
	}
	if endpoints.ProfileURL != "https://graph.microsoft.com/v1.0/me" {
		t.Fatalf("ProfileURL = %q", endpoints.ProfileURL)
	}
}

func TestNormalizeMicrosoftOAuthScopes(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name   string
		scopes []string
		want   []string
	}{
		{name: "defaults", scopes: nil, want: []string{"openid", "profile", "email", "User.Read"}},
		{name: "dedupe", scopes: []string{" openid ", "", "email", "openid", "User.Read"}, want: []string{"openid", "email", "User.Read"}},
		{name: "empty becomes defaults", scopes: []string{" ", ""}, want: []string{"openid", "profile", "email", "User.Read"}},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if got := NormalizeMicrosoftOAuthScopes(tt.scopes); !reflect.DeepEqual(got, tt.want) {
				t.Fatalf("NormalizeMicrosoftOAuthScopes() = %#v, want %#v", got, tt.want)
			}
		})
	}
}

func TestValidateMicrosoftOAuthCallbackURL(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		rawURL  string
		wantErr bool
	}{
		{name: "https", rawURL: "https://example.com/auth/microsoft/callback", wantErr: false},
		{name: "localhost http", rawURL: "http://localhost:3000/auth/callback", wantErr: false},
		{name: "loopback http", rawURL: "http://127.0.0.1:3000/auth/callback", wantErr: false},
		{name: "empty", rawURL: " ", wantErr: true},
		{name: "relative", rawURL: "/auth/callback", wantErr: true},
		{name: "remote http", rawURL: "http://example.com/auth/callback", wantErr: true},
		{name: "fragment", rawURL: "https://example.com/auth/callback#token", wantErr: true},
		{name: "userinfo", rawURL: "https://user@example.com/auth/callback", wantErr: true},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateMicrosoftOAuthCallbackURL(tt.rawURL)
			if tt.wantErr {
				if !errors.Is(err, ErrMicrosoftOAuthCallbackInvalid) {
					t.Fatalf("expected ErrMicrosoftOAuthCallbackInvalid, got %v", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("ValidateMicrosoftOAuthCallbackURL(%q): %v", tt.rawURL, err)
			}
		})
	}
}

func TestPlanMicrosoftOAuthDescriptor(t *testing.T) {
	t.Parallel()
	plan, err := PlanMicrosoftOAuthDescriptor(MicrosoftOAuthDescriptor{
		Tenant:       " Organizations ",
		ClientID:     " client-id ",
		ClientSecret: " client-secret ",
		RedirectURL:  " https://example.com/auth/callback ",
		Scopes:       []string{" openid ", "email", "openid"},
	})
	if err != nil {
		t.Fatalf("PlanMicrosoftOAuthDescriptor: %v", err)
	}
	if plan.Provider != MicrosoftOAuthProvider {
		t.Fatalf("Provider = %q", plan.Provider)
	}
	if plan.Tenant != "organizations" {
		t.Fatalf("Tenant = %q", plan.Tenant)
	}
	if plan.ClientID != "client-id" {
		t.Fatalf("ClientID = %q", plan.ClientID)
	}
	if plan.RedirectURL != "https://example.com/auth/callback" {
		t.Fatalf("RedirectURL = %q", plan.RedirectURL)
	}
	if want := []string{"openid", "email"}; !reflect.DeepEqual(plan.Scopes, want) {
		t.Fatalf("Scopes = %#v, want %#v", plan.Scopes, want)
	}
	if !strings.Contains(plan.Endpoints.AuthorizeURL, "/organizations/oauth2/v2.0/authorize") {
		t.Fatalf("AuthorizeURL = %q", plan.Endpoints.AuthorizeURL)
	}
}

func TestValidateMicrosoftOAuthDescriptorJoinsErrors(t *testing.T) {
	t.Parallel()
	err := ValidateMicrosoftOAuthDescriptor(MicrosoftOAuthDescriptor{
		Tenant:      "bad/tenant",
		RedirectURL: "http://example.com/callback",
		Scopes:      []string{"bad scope"},
	})
	if !errors.Is(err, ErrMicrosoftOAuthTenantInvalid) {
		t.Fatalf("expected tenant error, got %v", err)
	}
	if !errors.Is(err, ErrMicrosoftOAuthDescriptorInvalid) {
		t.Fatalf("expected descriptor error, got %v", err)
	}
	if !errors.Is(err, ErrMicrosoftOAuthCallbackInvalid) {
		t.Fatalf("expected callback error, got %v", err)
	}
	if !errors.Is(err, ErrMicrosoftOAuthScopeInvalid) {
		t.Fatalf("expected scope error, got %v", err)
	}
}

func TestMicrosoftOAuthRedactedSummary(t *testing.T) {
	t.Parallel()
	summary, err := MicrosoftOAuthRedactedSummary(MicrosoftOAuthDescriptor{
		Tenant:       "common",
		ClientID:     "client-id",
		ClientSecret: "client-secret",
		RedirectURL:  "https://example.com/auth/callback?code=secret&state=state-value",
		Scopes:       []string{"openid", "profile"},
	})
	if err != nil {
		t.Fatalf("MicrosoftOAuthRedactedSummary: %v", err)
	}
	if summary.ClientSecret != "[redacted]" {
		t.Fatalf("ClientSecret = %q", summary.ClientSecret)
	}
	if summary.RedirectURL != "https://example.com/auth/callback?code=%5Bredacted%5D&state=%5Bredacted%5D" {
		t.Fatalf("RedirectURL = %q", summary.RedirectURL)
	}
	if summary.Endpoints.TokenURL != "https://login.microsoftonline.com/common/oauth2/v2.0/token" {
		t.Fatalf("TokenURL = %q", summary.Endpoints.TokenURL)
	}
}
