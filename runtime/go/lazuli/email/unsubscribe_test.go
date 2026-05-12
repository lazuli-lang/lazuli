package email

import (
	"encoding/base64"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestUnsubscribeTokenRoundTrip(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	claims := UnsubscribeClaims{
		Subject:   "user-123",
		ListID:    "product-news",
		Purpose:   UnsubscribePurpose,
		ExpiresAt: now.Add(time.Hour).Unix(),
		IssuedAt:  now.Unix(),
	}

	token, err := SignUnsubscribeToken([]byte("secret"), claims)
	if err != nil {
		t.Fatalf("SignUnsubscribeToken: %v", err)
	}

	got, err := VerifyUnsubscribeToken([]byte("secret"), token, UnsubscribeScope{
		Purpose: UnsubscribePurpose,
		ListID:  "product-news",
		Now:     now,
	})
	if err != nil {
		t.Fatalf("VerifyUnsubscribeToken: %v", err)
	}
	if got != claims {
		t.Fatalf("VerifyUnsubscribeToken claims = %+v, want %+v", got, claims)
	}
}

func TestUnsubscribeTokenRejectsTamperedSignature(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	token, err := SignUnsubscribeToken([]byte("secret"), UnsubscribeClaims{
		Subject:   "user-123",
		ListID:    "product-news",
		Purpose:   UnsubscribePurpose,
		ExpiresAt: now.Add(time.Hour).Unix(),
	})
	if err != nil {
		t.Fatalf("SignUnsubscribeToken: %v", err)
	}

	parts := strings.Split(token, ".")
	signature, err := base64.RawURLEncoding.DecodeString(parts[2])
	if err != nil {
		t.Fatalf("DecodeString(signature): %v", err)
	}
	signature[0] ^= 0xff
	parts[2] = base64.RawURLEncoding.EncodeToString(signature)

	_, err = VerifyUnsubscribeToken([]byte("secret"), strings.Join(parts, "."), UnsubscribeScope{
		Purpose: UnsubscribePurpose,
		ListID:  "product-news",
		Now:     now,
	})
	if !errors.Is(err, ErrUnsubscribeTokenSignature) {
		t.Fatalf("VerifyUnsubscribeToken error = %v, want ErrUnsubscribeTokenSignature", err)
	}
}

func TestUnsubscribeTokenRejectsExpiredToken(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	token, err := SignUnsubscribeToken([]byte("secret"), UnsubscribeClaims{
		Subject:   "user-123",
		ListID:    "product-news",
		Purpose:   UnsubscribePurpose,
		ExpiresAt: now.Add(-time.Second).Unix(),
	})
	if err != nil {
		t.Fatalf("SignUnsubscribeToken: %v", err)
	}

	_, err = VerifyUnsubscribeToken([]byte("secret"), token, UnsubscribeScope{
		Purpose: UnsubscribePurpose,
		ListID:  "product-news",
		Now:     now,
	})
	if !errors.Is(err, ErrUnsubscribeTokenExpired) {
		t.Fatalf("VerifyUnsubscribeToken error = %v, want ErrUnsubscribeTokenExpired", err)
	}
}

func TestUnsubscribeTokenRejectsWrongScope(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	token, err := SignUnsubscribeToken([]byte("secret"), UnsubscribeClaims{
		Subject:   "user-123",
		ListID:    "product-news",
		Purpose:   UnsubscribePurpose,
		ExpiresAt: now.Add(time.Hour).Unix(),
	})
	if err != nil {
		t.Fatalf("SignUnsubscribeToken: %v", err)
	}

	tests := []struct {
		name  string
		scope UnsubscribeScope
	}{
		{
			name: "purpose",
			scope: UnsubscribeScope{
				Purpose: "preferences",
				ListID:  "product-news",
				Now:     now,
			},
		},
		{
			name: "list",
			scope: UnsubscribeScope{
				Purpose: UnsubscribePurpose,
				ListID:  "security-alerts",
				Now:     now,
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := VerifyUnsubscribeToken([]byte("secret"), token, tt.scope)
			if !errors.Is(err, ErrUnsubscribeTokenScope) {
				t.Fatalf("VerifyUnsubscribeToken error = %v, want ErrUnsubscribeTokenScope", err)
			}
		})
	}
}

func TestSignUnsubscribeTokenRejectsInvalidInput(t *testing.T) {
	t.Parallel()

	_, err := SignUnsubscribeToken([]byte("secret"), UnsubscribeClaims{
		Subject:   "user-123",
		ListID:    "product-news",
		Purpose:   UnsubscribePurpose,
		ExpiresAt: 0,
	})
	if !errors.Is(err, ErrUnsubscribeTokenInvalid) {
		t.Fatalf("SignUnsubscribeToken error = %v, want ErrUnsubscribeTokenInvalid", err)
	}
}

func TestBuildListUnsubscribeHeader(t *testing.T) {
	t.Parallel()

	got, err := BuildListUnsubscribeHeader(ListUnsubscribeOptions{
		URL:    "https://example.com/unsubscribe?token=abc",
		Mailto: "mailto:unsubscribe@example.com?subject=unsubscribe",
	})
	if err != nil {
		t.Fatalf("BuildListUnsubscribeHeader: %v", err)
	}

	want := "<https://example.com/unsubscribe?token=abc>, <mailto:unsubscribe@example.com?subject=unsubscribe>"
	if got != want {
		t.Fatalf("BuildListUnsubscribeHeader = %q, want %q", got, want)
	}
}

func TestBuildListUnsubscribeHeaderRejectsInvalidInput(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		opts ListUnsubscribeOptions
	}{
		{
			name: "empty",
		},
		{
			name: "unsupported url scheme",
			opts: ListUnsubscribeOptions{URL: "ftp://example.com/unsubscribe"},
		},
		{
			name: "missing mailto scheme",
			opts: ListUnsubscribeOptions{Mailto: "unsubscribe@example.com"},
		},
		{
			name: "header injection",
			opts: ListUnsubscribeOptions{URL: "https://example.com/unsubscribe\r\nBcc:attacker@example.com"},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := BuildListUnsubscribeHeader(tt.opts)
			if !errors.Is(err, ErrListUnsubscribeHeaderInvalid) {
				t.Fatalf("BuildListUnsubscribeHeader error = %v, want ErrListUnsubscribeHeaderInvalid", err)
			}
		})
	}
}
