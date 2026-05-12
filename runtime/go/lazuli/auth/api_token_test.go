package auth

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestAPITokenScopeMatching(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		grants   APITokenScopes
		required string
		want     bool
	}{
		{
			name:     "exact",
			grants:   APITokenScopes{"orders:read"},
			required: "orders:read",
			want:     true,
		},
		{
			name:     "resource wildcard",
			grants:   APITokenScopes{"orders:*"},
			required: "orders:write",
			want:     true,
		},
		{
			name:     "global wildcard",
			grants:   APITokenScopes{"*"},
			required: "admin:impersonate",
			want:     true,
		},
		{
			name:     "middle wildcard",
			grants:   APITokenScopes{"billing:*:read"},
			required: "billing:invoice:read",
			want:     true,
		},
		{
			name:     "mismatch",
			grants:   APITokenScopes{"orders:read"},
			required: "orders:write",
			want:     false,
		},
		{
			name:     "wildcard only applies to grant",
			grants:   APITokenScopes{"orders:read"},
			required: "orders:*",
			want:     false,
		},
		{
			name:     "invalid required scope",
			grants:   APITokenScopes{"*"},
			required: "orders read",
			want:     false,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if got := tt.grants.Has(tt.required); got != tt.want {
				t.Fatalf("Has(%q) = %v, want %v", tt.required, got, tt.want)
			}
		})
	}
}

func TestAPITokenStatus(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	tests := []struct {
		name string
		meta APITokenMetadata
		want APITokenStatus
	}{
		{
			name: "active",
			meta: APITokenMetadata{ExpiresAt: now.Add(time.Hour)},
			want: APITokenStatusActive,
		},
		{
			name: "zero expiry stays active",
			meta: APITokenMetadata{},
			want: APITokenStatusActive,
		},
		{
			name: "expired at boundary",
			meta: APITokenMetadata{ExpiresAt: now},
			want: APITokenStatusExpired,
		},
		{
			name: "rotated",
			meta: APITokenMetadata{ExpiresAt: now.Add(time.Hour), RotatedAt: now.Add(-time.Minute)},
			want: APITokenStatusRotated,
		},
		{
			name: "replacement marks rotated",
			meta: APITokenMetadata{ExpiresAt: now.Add(time.Hour), ReplacementTokenID: "tok_next"},
			want: APITokenStatusRotated,
		},
		{
			name: "revoked wins",
			meta: APITokenMetadata{
				ExpiresAt:  now.Add(-time.Hour),
				RotatedAt:  now.Add(-2 * time.Hour),
				RevokedAt:  now.Add(-time.Minute),
				Scopes:     APITokenScopes{"*"},
				CreatedAt:  now.Add(-24 * time.Hour),
				LastUsedAt: now.Add(-2 * time.Hour),
			},
			want: APITokenStatusRevoked,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if got := tt.meta.Status(now); got != tt.want {
				t.Fatalf("Status() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestValidateAPIToken(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	active := APITokenMetadata{
		ID:        "tok_123",
		Name:      "Deploy key",
		UserID:    lazuli.ID(42),
		Subject:   "user:42",
		Scopes:    APITokenScopes{"orders:*", "billing:invoice:read"},
		CreatedAt: now.Add(-time.Hour),
		ExpiresAt: now.Add(time.Hour),
	}

	if err := ValidateAPIToken(active, now, "orders:write", "billing:invoice:read"); err != nil {
		t.Fatalf("ValidateAPIToken(active) error = %v", err)
	}
	if err := active.Validate(now, "orders:read"); err != nil {
		t.Fatalf("APITokenMetadata.Validate(active) error = %v", err)
	}

	for _, tt := range []struct {
		name string
		meta APITokenMetadata
		want error
	}{
		{
			name: "expired",
			meta: APITokenMetadata{Scopes: APITokenScopes{"*"}, ExpiresAt: now.Add(-time.Second)},
			want: ErrAPITokenExpired,
		},
		{
			name: "rotated",
			meta: APITokenMetadata{Scopes: APITokenScopes{"*"}, ExpiresAt: now.Add(time.Hour), RotatedAt: now},
			want: ErrAPITokenRotated,
		},
		{
			name: "revoked",
			meta: APITokenMetadata{Scopes: APITokenScopes{"*"}, ExpiresAt: now.Add(time.Hour), RevokedAt: now},
			want: ErrAPITokenRevoked,
		},
		{
			name: "scope denied",
			meta: APITokenMetadata{Scopes: APITokenScopes{"orders:read"}, ExpiresAt: now.Add(time.Hour)},
			want: ErrAPITokenScopeDenied,
		},
	} {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := ValidateAPIToken(tt.meta, now, "orders:write"); !errors.Is(err, tt.want) {
				t.Fatalf("ValidateAPIToken() error = %v, want %v", err, tt.want)
			}
		})
	}

	if err := ValidateAPIToken(active, now, "orders read"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("ValidateAPIToken(invalid scope) error = %v, want ErrTokenInvalid", err)
	}
}

func TestAPITokenScopeNormalizationAndValidation(t *testing.T) {
	t.Parallel()

	scopes := NormalizeAPITokenScopes([]string{" orders:read ", "", "orders:read", "orders:write"})
	if len(scopes) != 2 {
		t.Fatalf("normalized scope count = %d, want 2: %#v", len(scopes), scopes)
	}
	if scopes[0] != "orders:read" || scopes[1] != "orders:write" {
		t.Fatalf("normalized scopes = %#v", scopes)
	}

	if err := ValidateAPITokenScopes(scopes); err != nil {
		t.Fatalf("ValidateAPITokenScopes(valid) error = %v", err)
	}
	if err := ValidateAPITokenScope("orders read"); !errors.Is(err, ErrTokenInvalid) {
		t.Fatalf("ValidateAPITokenScope(invalid) error = %v, want ErrTokenInvalid", err)
	}
}

func TestAPITokenMetadataClone(t *testing.T) {
	t.Parallel()

	meta := APITokenMetadata{
		Scopes: APITokenScopes{"orders:read"},
		Attrs: map[string]any{
			"label": "ci",
			"tags":  []string{"deploy"},
			"meta":  map[string]any{"tier": "prod"},
		},
	}

	cloned := meta.Clone()
	cloned.Scopes[0] = "orders:write"
	cloned.Attrs["label"] = "mutated"
	cloned.Attrs["tags"].([]string)[0] = "mutated"
	cloned.Attrs["meta"].(map[string]any)["tier"] = "mutated"

	if meta.Scopes[0] != "orders:read" {
		t.Fatalf("original scope mutated: %#v", meta.Scopes)
	}
	if meta.Attrs["label"] != "ci" {
		t.Fatalf("original label mutated: %#v", meta.Attrs)
	}
	if meta.Attrs["tags"].([]string)[0] != "deploy" {
		t.Fatalf("original tags mutated: %#v", meta.Attrs)
	}
	if meta.Attrs["meta"].(map[string]any)["tier"] != "prod" {
		t.Fatalf("original meta mutated: %#v", meta.Attrs)
	}
}
