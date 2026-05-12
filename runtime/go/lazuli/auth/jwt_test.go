package auth

import (
	"encoding/base64"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestJWTRoundTrip(t *testing.T) {
	t.Parallel()
	now := time.Now()
	token, err := SignJWT([]byte("secret"), Claims{
		Subject:   "user-123",
		Issuer:    "lazuli",
		Audience:  "web",
		ExpiresAt: now.Add(time.Hour).Unix(),
		NotBefore: now.Unix(),
		IssuedAt:  now.Unix(),
		Custom: map[string]any{
			"role":   "admin",
			"active": true,
		},
	})
	if err != nil {
		t.Fatalf("SignJWT: %v", err)
	}

	claims, err := VerifyJWT([]byte("secret"), token)
	if err != nil {
		t.Fatalf("VerifyJWT: %v", err)
	}
	if claims.Subject != "user-123" {
		t.Fatalf("expected subject user-123, got %q", claims.Subject)
	}
	if claims.Issuer != "lazuli" || claims.Audience != "web" {
		t.Fatalf("expected issuer/audience to round-trip, got %+v", claims)
	}
	if claims.ExpiresAt == 0 || claims.NotBefore == 0 || claims.IssuedAt == 0 {
		t.Fatalf("expected time claims to round-trip, got %+v", claims)
	}
	if claims.Custom["role"] != "admin" || claims.Custom["active"] != true {
		t.Fatalf("expected custom claims to round-trip, got %+v", claims.Custom)
	}
}

func TestJWTExpiredToken(t *testing.T) {
	t.Parallel()
	token, err := SignJWT([]byte("secret"), Claims{
		Subject:   "user-123",
		ExpiresAt: time.Now().Add(-time.Second).Unix(),
	})
	if err != nil {
		t.Fatalf("SignJWT: %v", err)
	}

	_, err = VerifyJWT([]byte("secret"), token)
	if !errors.Is(err, ErrJWTExpired) {
		t.Fatalf("expected ErrJWTExpired, got %v", err)
	}
}

func TestJWTTamperedSignature(t *testing.T) {
	t.Parallel()
	token, err := SignJWT([]byte("secret"), Claims{
		Subject:   "user-123",
		ExpiresAt: time.Now().Add(time.Hour).Unix(),
	})
	if err != nil {
		t.Fatalf("SignJWT: %v", err)
	}
	parts := strings.Split(token, ".")
	signature, err := base64.RawURLEncoding.DecodeString(parts[2])
	if err != nil {
		t.Fatalf("DecodeString(signature): %v", err)
	}
	signature[0] ^= 0xff
	parts[2] = base64.RawURLEncoding.EncodeToString(signature)

	_, err = VerifyJWT([]byte("secret"), strings.Join(parts, "."))
	if !errors.Is(err, ErrJWTSignature) {
		t.Fatalf("expected ErrJWTSignature, got %v", err)
	}
}

func TestJWTMalformedToken(t *testing.T) {
	t.Parallel()
	for _, token := range []string{
		"",
		"one.two",
		"one.two.three.four",
		"one.two.three",
	} {
		_, err := VerifyJWT([]byte("secret"), token)
		if !errors.Is(err, ErrJWTInvalid) {
			t.Fatalf("expected ErrJWTInvalid for %q, got %v", token, err)
		}
	}
}
