package auth

import (
	"errors"
	"net/url"
	"reflect"
	"strings"
	"testing"
)

func TestValidateOAuthPKCEVerifier(t *testing.T) {
	t.Parallel()

	valid43 := strings.Repeat("a", OAuthPKCEVerifierMinLength)
	valid128 := strings.Repeat("Z", OAuthPKCEVerifierMaxLength)

	tests := []struct {
		name string
		in   string
		want bool
	}{
		{name: "minimum length", in: valid43, want: true},
		{name: "maximum length", in: valid128, want: true},
		{name: "unreserved characters", in: "abcABC012-._~" + strings.Repeat("x", 30), want: true},
		{name: "empty", in: "", want: false},
		{name: "too short", in: strings.Repeat("a", OAuthPKCEVerifierMinLength-1), want: false},
		{name: "too long", in: strings.Repeat("a", OAuthPKCEVerifierMaxLength+1), want: false},
		{name: "space", in: strings.Repeat("a", 42) + " ", want: false},
		{name: "unicode", in: strings.Repeat("a", 42) + "é", want: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateOAuthPKCEVerifier(tt.in)
			if tt.want && err != nil {
				t.Fatalf("ValidateOAuthPKCEVerifier() error = %v", err)
			}
			if !tt.want && !errors.Is(err, ErrOAuthPKCEInvalid) {
				t.Fatalf("ValidateOAuthPKCEVerifier() error = %v, want ErrOAuthPKCEInvalid", err)
			}
		})
	}
}

func TestDeriveOAuthPKCES256ChallengeUsesRFCVector(t *testing.T) {
	t.Parallel()

	verifier := "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
	want := "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"

	got, err := DeriveOAuthPKCES256Challenge(" " + verifier + " ")
	if err != nil {
		t.Fatalf("DeriveOAuthPKCES256Challenge() error = %v", err)
	}
	if got != want {
		t.Fatalf("DeriveOAuthPKCES256Challenge() = %q, want %q", got, want)
	}
	if err := ValidateOAuthPKCEChallenge(got, OAuthPKCECodeChallengeMethodS256); err != nil {
		t.Fatalf("ValidateOAuthPKCEChallenge(derived) error = %v", err)
	}
}

func TestOAuthPKCEVerifierLengthForEntropy(t *testing.T) {
	t.Parallel()

	length, err := OAuthPKCEVerifierLengthForEntropy(OAuthPKCEVerifierMinEntropyBytes)
	if err != nil {
		t.Fatalf("OAuthPKCEVerifierLengthForEntropy(min) error = %v", err)
	}
	if length != OAuthPKCEVerifierMinLength {
		t.Fatalf("OAuthPKCEVerifierLengthForEntropy(min) = %d, want %d", length, OAuthPKCEVerifierMinLength)
	}

	length, err = OAuthPKCEVerifierLengthForEntropy(OAuthPKCEVerifierMaxEntropyBytes)
	if err != nil {
		t.Fatalf("OAuthPKCEVerifierLengthForEntropy(max) error = %v", err)
	}
	if length != OAuthPKCEVerifierMaxLength {
		t.Fatalf("OAuthPKCEVerifierLengthForEntropy(max) = %d, want %d", length, OAuthPKCEVerifierMaxLength)
	}

	for _, entropyBytes := range []int{OAuthPKCEVerifierMinEntropyBytes - 1, OAuthPKCEVerifierMaxEntropyBytes + 1} {
		if _, err := OAuthPKCEVerifierLengthForEntropy(entropyBytes); !errors.Is(err, ErrOAuthPKCEInvalid) {
			t.Fatalf("OAuthPKCEVerifierLengthForEntropy(%d) error = %v, want ErrOAuthPKCEInvalid", entropyBytes, err)
		}
	}
}

func TestValidateOAuthPKCEChallengeRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name      string
		challenge string
		method    string
		want      string
	}{
		{name: "missing challenge", method: "S256", want: "code_challenge is required"},
		{name: "unsupported method", challenge: strings.Repeat("a", 43), method: "plain", want: "must be S256"},
		{name: "wrong length", challenge: strings.Repeat("a", 44), method: "S256", want: "length must be 43"},
		{name: "invalid character", challenge: strings.Repeat("a", 42) + "=", method: "S256", want: "invalid character"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateOAuthPKCEChallenge(tt.challenge, tt.method)
			if !errors.Is(err, ErrOAuthPKCEInvalid) {
				t.Fatalf("ValidateOAuthPKCEChallenge() error = %v, want ErrOAuthPKCEInvalid", err)
			}
			if !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("ValidateOAuthPKCEChallenge() error = %q, want fragment %q", err, tt.want)
			}
		})
	}
}

func TestNormalizeOAuthPKCEAuthorizationDescriptor(t *testing.T) {
	t.Parallel()

	extra := url.Values{"prompt": {" consent "}}
	descriptor, err := NormalizeOAuthPKCEAuthorizationDescriptor(OAuthPKCEAuthorizationDescriptor{
		AuthorizationURL: " HTTPS://IdP.Example:443/oauth/authorize ",
		ClientID:         " client-id ",
		RedirectURL:      "http://LOCALHOST:80/callback?tenant=acme",
		Scopes:           []string{" profile ", "email", "profile", ""},
		State:            " state ",
		CodeVerifier:     " " + strings.Repeat("v", 43) + " ",
		ExtraParams:      extra,
	})
	if err != nil {
		t.Fatalf("NormalizeOAuthPKCEAuthorizationDescriptor() error = %v", err)
	}
	if descriptor.AuthorizationURL != "https://idp.example/oauth/authorize" {
		t.Fatalf("AuthorizationURL = %q, want normalized", descriptor.AuthorizationURL)
	}
	if descriptor.RedirectURL != "http://localhost/callback?tenant=acme" {
		t.Fatalf("RedirectURL = %q, want normalized", descriptor.RedirectURL)
	}
	if descriptor.ClientID != "client-id" || descriptor.State != "state" || descriptor.CodeVerifier != strings.Repeat("v", 43) {
		t.Fatalf("scalar fields were not trimmed: %#v", descriptor)
	}
	if want := []string{"profile", "email"}; !reflect.DeepEqual(descriptor.Scopes, want) {
		t.Fatalf("Scopes = %#v, want %#v", descriptor.Scopes, want)
	}
	extra.Set("prompt", "mutated")
	if descriptor.ExtraParams.Get("prompt") != "consent" {
		t.Fatalf("ExtraParams shared backing map: %#v", descriptor.ExtraParams)
	}
}

func TestPlanOAuthPKCEAuthorizationBuildsParamsAndRequest(t *testing.T) {
	t.Parallel()

	verifier := "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
	plan, err := PlanOAuthPKCEAuthorization(OAuthPKCEAuthorizationDescriptor{
		AuthorizationURL: "https://idp.example/oauth/authorize?audience=runtime",
		ClientID:         "client-id",
		RedirectURL:      "https://app.example/oauth/callback?tenant=acme",
		Scopes:           []string{"openid", "profile"},
		State:            "state-token",
		CodeVerifier:     verifier,
		ExtraParams:      url.Values{"prompt": {"consent"}},
	})
	if err != nil {
		t.Fatalf("PlanOAuthPKCEAuthorization() error = %v", err)
	}
	if plan.CodeVerifier != verifier {
		t.Fatalf("CodeVerifier = %q, want original verifier", plan.CodeVerifier)
	}
	if plan.CodeChallenge != "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM" {
		t.Fatalf("CodeChallenge = %q, want RFC vector", plan.CodeChallenge)
	}
	if plan.CodeChallengeMethod != OAuthPKCECodeChallengeMethodS256 {
		t.Fatalf("CodeChallengeMethod = %q, want S256", plan.CodeChallengeMethod)
	}
	for key, want := range map[string]string{
		"response_type":         "code",
		"client_id":             "client-id",
		"redirect_uri":          "https://app.example/oauth/callback?tenant=acme",
		"scope":                 "openid profile",
		"state":                 "state-token",
		"code_challenge":        plan.CodeChallenge,
		"code_challenge_method": "S256",
		"prompt":                "consent",
	} {
		if got := plan.Params.Get(key); got != want {
			t.Fatalf("Params[%q] = %q, want %q", key, got, want)
		}
	}

	requestURL, err := url.Parse(plan.AuthorizationRequest)
	if err != nil {
		t.Fatalf("AuthorizationRequest parse error: %v", err)
	}
	if requestURL.Query().Get("audience") != "runtime" || requestURL.Query().Get("code_challenge") != plan.CodeChallenge {
		t.Fatalf("AuthorizationRequest query missing expected params: %q", plan.AuthorizationRequest)
	}
}

func TestValidateOAuthPKCEAuthorizationDescriptorRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	validVerifier := strings.Repeat("v", 43)
	tests := []struct {
		name string
		in   OAuthPKCEAuthorizationDescriptor
		want string
	}{
		{
			name: "missing required fields",
			in:   OAuthPKCEAuthorizationDescriptor{},
			want: "authorization_url is required",
		},
		{
			name: "non https authorization",
			in: OAuthPKCEAuthorizationDescriptor{
				AuthorizationURL: "http://idp.example/oauth/authorize",
				ClientID:         "client-id",
				RedirectURL:      "https://app.example/callback",
				State:            "state",
				CodeVerifier:     validVerifier,
			},
			want: "authorization_url http is only allowed for loopback hosts",
		},
		{
			name: "non loopback http redirect",
			in: OAuthPKCEAuthorizationDescriptor{
				AuthorizationURL: "https://idp.example/oauth/authorize",
				ClientID:         "client-id",
				RedirectURL:      "http://app.example/callback",
				State:            "state",
				CodeVerifier:     validVerifier,
			},
			want: "redirect_url http is only allowed for loopback hosts",
		},
		{
			name: "reserved extra param",
			in: OAuthPKCEAuthorizationDescriptor{
				AuthorizationURL: "https://idp.example/oauth/authorize",
				ClientID:         "client-id",
				RedirectURL:      "https://app.example/callback",
				State:            "state",
				CodeVerifier:     validVerifier,
				ExtraParams:      url.Values{"client_id": {"override"}},
			},
			want: "reserved",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			err := ValidateOAuthPKCEAuthorizationDescriptor(tt.in)
			if !errors.Is(err, ErrOAuthPKCEInvalid) {
				t.Fatalf("ValidateOAuthPKCEAuthorizationDescriptor() error = %v, want ErrOAuthPKCEInvalid", err)
			}
			if !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("ValidateOAuthPKCEAuthorizationDescriptor() error = %q, want fragment %q", err, tt.want)
			}
		})
	}
}

func TestSafeOAuthPKCEDiagnosticsRedactsSecretsAndURLQuery(t *testing.T) {
	t.Parallel()

	plan, err := PlanOAuthPKCEAuthorization(OAuthPKCEAuthorizationDescriptor{
		AuthorizationURL: "https://idp.example/oauth/authorize?client_secret=secret",
		ClientID:         "client-id",
		RedirectURL:      "https://app.example/oauth/callback?code=secret-code&state=secret-state",
		Scopes:           []string{"openid", "email"},
		State:            "state-token",
		CodeVerifier:     strings.Repeat("v", 43),
		ExtraParams:      url.Values{"prompt": {"consent"}, "login_hint": {"user@example.com"}},
	})
	if err != nil {
		t.Fatalf("PlanOAuthPKCEAuthorization() error = %v", err)
	}

	diagnostics := SafeOAuthPKCEDiagnostics(plan)
	if diagnostics.AuthorizationURL != "https://idp.example/oauth/authorize" {
		t.Fatalf("AuthorizationURL = %q, want redacted query", diagnostics.AuthorizationURL)
	}
	if diagnostics.RedirectURL != "https://app.example/oauth/callback" {
		t.Fatalf("RedirectURL = %q, want redacted query", diagnostics.RedirectURL)
	}
	if diagnostics.CodeChallengeLength != 43 || diagnostics.CodeChallengeMethod != "S256" {
		t.Fatalf("challenge diagnostics = length %d method %q, want 43 S256", diagnostics.CodeChallengeLength, diagnostics.CodeChallengeMethod)
	}
	if !diagnostics.StateConfigured || !diagnostics.CodeVerifierConfigured {
		t.Fatalf("presence flags = state %v verifier %v, want both true", diagnostics.StateConfigured, diagnostics.CodeVerifierConfigured)
	}
	if want := []string{"login_hint", "prompt"}; !reflect.DeepEqual(diagnostics.ExtraParamKeys, want) {
		t.Fatalf("ExtraParamKeys = %#v, want %#v", diagnostics.ExtraParamKeys, want)
	}
	for _, value := range []string{diagnostics.AuthorizationURL, diagnostics.RedirectURL} {
		if strings.Contains(value, "secret") || strings.Contains(value, "?") {
			t.Fatalf("diagnostics leaked sensitive URL data: %#v", diagnostics)
		}
	}
}
