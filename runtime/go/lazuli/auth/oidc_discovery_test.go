package auth

import (
	"errors"
	"reflect"
	"testing"
)

func TestNormalizeOIDCIssuerURL(t *testing.T) {
	tests := []struct {
		name    string
		raw     string
		want    string
		wantErr bool
	}{
		{
			name: "trims lowercases host and removes trailing slash",
			raw:  " https://LOGIN.EXAMPLE.com/tenant/ ",
			want: "https://login.example.com/tenant",
		},
		{
			name: "removes query and fragment",
			raw:  "https://idp.example.com/realms/acme?client=secret#keys",
			want: "https://idp.example.com/realms/acme",
		},
		{
			name:    "rejects http",
			raw:     "http://idp.example.com",
			wantErr: true,
		},
		{
			name:    "rejects credentials",
			raw:     "https://user:pass@idp.example.com",
			wantErr: true,
		},
		{
			name:    "rejects relative",
			raw:     "/issuer",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := NormalizeOIDCIssuerURL(tt.raw)
			if tt.wantErr {
				if !errors.Is(err, ErrOIDCDiscoveryInvalid) {
					t.Fatalf("NormalizeOIDCIssuerURL() error = %v, want ErrOIDCDiscoveryInvalid", err)
				}
				return
			}
			if err != nil {
				t.Fatalf("NormalizeOIDCIssuerURL() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("NormalizeOIDCIssuerURL() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestOIDCWellKnownURL(t *testing.T) {
	tests := []struct {
		issuer string
		want   string
	}{
		{
			issuer: "https://idp.example.com",
			want:   "https://idp.example.com/.well-known/openid-configuration",
		},
		{
			issuer: "https://idp.example.com/tenant/",
			want:   "https://idp.example.com/tenant/.well-known/openid-configuration",
		},
	}

	for _, tt := range tests {
		got, err := OIDCWellKnownURL(tt.issuer)
		if err != nil {
			t.Fatalf("OIDCWellKnownURL(%q) error = %v", tt.issuer, err)
		}
		if got != tt.want {
			t.Fatalf("OIDCWellKnownURL(%q) = %q, want %q", tt.issuer, got, tt.want)
		}
	}
}

func TestValidateOIDCDiscoveryDescriptor(t *testing.T) {
	valid := OIDCDiscoveryDescriptor{
		Issuer:                           "https://idp.example.com",
		AuthorizationEndpoint:            "https://idp.example.com/oauth2/auth",
		TokenEndpoint:                    "https://idp.example.com/oauth2/token",
		UserInfoEndpoint:                 "https://idp.example.com/oauth2/userinfo",
		JWKSURI:                          "https://idp.example.com/oauth2/keys?kid=current",
		ScopesSupported:                  []string{"openid", "email", "profile"},
		ResponseTypesSupported:           []string{"code"},
		SubjectTypesSupported:            []string{"public"},
		IDTokenSigningAlgValuesSupported: []string{"RS256"},
	}
	if err := ValidateOIDCDiscoveryDescriptor(valid); err != nil {
		t.Fatalf("ValidateOIDCDiscoveryDescriptor(valid) error = %v", err)
	}

	tests := []struct {
		name string
		edit func(*OIDCDiscoveryDescriptor)
	}{
		{
			name: "missing authorization endpoint",
			edit: func(desc *OIDCDiscoveryDescriptor) {
				desc.AuthorizationEndpoint = ""
			},
		},
		{
			name: "insecure token endpoint",
			edit: func(desc *OIDCDiscoveryDescriptor) {
				desc.TokenEndpoint = "http://idp.example.com/token"
			},
		},
		{
			name: "missing code response type",
			edit: func(desc *OIDCDiscoveryDescriptor) {
				desc.ResponseTypesSupported = []string{"id_token"}
			},
		},
		{
			name: "missing subject types",
			edit: func(desc *OIDCDiscoveryDescriptor) {
				desc.SubjectTypesSupported = nil
			},
		},
		{
			name: "missing signing algs",
			edit: func(desc *OIDCDiscoveryDescriptor) {
				desc.IDTokenSigningAlgValuesSupported = nil
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			desc := valid
			tt.edit(&desc)
			if err := ValidateOIDCDiscoveryDescriptor(desc); !errors.Is(err, ErrOIDCDiscoveryInvalid) {
				t.Fatalf("ValidateOIDCDiscoveryDescriptor() error = %v, want ErrOIDCDiscoveryInvalid", err)
			}
		})
	}
}

func TestRedactOIDCJWKSURI(t *testing.T) {
	got := RedactOIDCJWKSURI("https://user:pass@idp.example.com/keys?token=secret&kid=1#frag")
	want := "https://idp.example.com/keys?redacted"
	if got != want {
		t.Fatalf("RedactOIDCJWKSURI() = %q, want %q", got, want)
	}
	if got := RedactOIDCJWKSURI("not a url"); got != "<redacted>" {
		t.Fatalf("RedactOIDCJWKSURI(invalid) = %q, want <redacted>", got)
	}
}

func TestPlanOIDCScopes(t *testing.T) {
	plan, err := PlanOIDCScopes(
		[]string{" email ", "profile", "email"},
		[]string{"openid", "profile"},
	)
	if err != nil {
		t.Fatalf("PlanOIDCScopes() error = %v", err)
	}
	if want := []string{"openid", "email", "profile"}; !reflect.DeepEqual(plan.Requested, want) {
		t.Fatalf("PlanOIDCScopes() requested = %#v, want %#v", plan.Requested, want)
	}
	if want := []string{"email"}; !reflect.DeepEqual(plan.Unsupported, want) {
		t.Fatalf("PlanOIDCScopes() unsupported = %#v, want %#v", plan.Unsupported, want)
	}

	if _, err := PlanOIDCScopes([]string{"bad scope"}, nil); !errors.Is(err, ErrOIDCDiscoveryInvalid) {
		t.Fatalf("PlanOIDCScopes(invalid) error = %v, want ErrOIDCDiscoveryInvalid", err)
	}
}

func TestSafeOIDCDiscoverySummary(t *testing.T) {
	desc := OIDCDiscoveryDescriptor{
		Issuer:                           "https://IDP.example.com/tenant/",
		AuthorizationEndpoint:            "https://idp.example.com/auth",
		TokenEndpoint:                    "https://idp.example.com/token",
		UserInfoEndpoint:                 "https://idp.example.com/userinfo",
		JWKSURI:                          "https://idp.example.com/keys?token=secret",
		ScopesSupported:                  []string{" openid ", "email", "email"},
		ResponseTypesSupported:           []string{"code"},
		SubjectTypesSupported:            []string{"public"},
		IDTokenSigningAlgValuesSupported: []string{"RS256"},
	}

	summary, err := SafeOIDCDiscoverySummary(desc)
	if err != nil {
		t.Fatalf("SafeOIDCDiscoverySummary() error = %v", err)
	}
	if summary.Issuer != "https://idp.example.com/tenant" {
		t.Fatalf("summary issuer = %q", summary.Issuer)
	}
	if summary.WellKnownURL != "https://idp.example.com/tenant/.well-known/openid-configuration" {
		t.Fatalf("summary well-known url = %q", summary.WellKnownURL)
	}
	if summary.JWKSURI != "https://idp.example.com/keys?redacted" {
		t.Fatalf("summary jwks uri = %q", summary.JWKSURI)
	}
	if want := []string{"openid", "email"}; !reflect.DeepEqual(summary.Scopes, want) {
		t.Fatalf("summary scopes = %#v, want %#v", summary.Scopes, want)
	}
}
