package auth

import (
	"encoding/base64"
	"errors"
	"strings"
	"testing"
	"time"
)

func TestSignedCookieRoundTrip(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	value := SignedCookieValue{
		KeyID:     "key-v2",
		Value:     "session-token-123",
		ExpiresAt: now.Add(time.Hour),
		IssuedAt:  now,
	}

	signed, err := SignSignedCookie([]byte("new-secret"), value)
	if err != nil {
		t.Fatalf("SignSignedCookie: %v", err)
	}
	if strings.Contains(signed, "=") {
		t.Fatalf("signed cookie contains base64 padding: %q", signed)
	}

	parts := strings.Split(signed, ".")
	if len(parts) != 4 {
		t.Fatalf("signed cookie parts = %d, want 4", len(parts))
	}
	keyID, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		t.Fatalf("DecodeString(key id): %v", err)
	}
	if string(keyID) != "key-v2" {
		t.Fatalf("encoded key id = %q, want key-v2", keyID)
	}

	got, err := VerifySignedCookie(SignedCookieKeys{
		"key-v1": []byte("old-secret"),
		"key-v2": []byte("new-secret"),
	}, signed, now)
	if err != nil {
		t.Fatalf("VerifySignedCookie: %v", err)
	}
	if got.KeyID != value.KeyID || got.Value != value.Value {
		t.Fatalf("VerifySignedCookie = %+v, want key/value from %+v", got, value)
	}
	if !got.ExpiresAt.Equal(value.ExpiresAt) {
		t.Fatalf("ExpiresAt = %s, want %s", got.ExpiresAt, value.ExpiresAt)
	}
	if !got.IssuedAt.Equal(value.IssuedAt) {
		t.Fatalf("IssuedAt = %s, want %s", got.IssuedAt, value.IssuedAt)
	}
}

func TestSignedCookieVerifiesOldKeyAfterRotation(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	signed, err := SignCookieValue("key-v1", []byte("old-secret"), "session-token-123", now.Add(time.Hour))
	if err != nil {
		t.Fatalf("SignCookieValue: %v", err)
	}

	got, err := VerifyCookieValue(SignedCookieKeys{
		"key-v1": []byte("old-secret"),
		"key-v2": []byte("new-secret"),
	}, signed, now)
	if err != nil {
		t.Fatalf("VerifyCookieValue: %v", err)
	}
	if got.KeyID != "key-v1" || got.Value != "session-token-123" {
		t.Fatalf("VerifyCookieValue = %+v, want old key session", got)
	}
}

func TestSignedCookieRejectsTampering(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	signed, err := SignCookieValue("key-v1", []byte("secret"), "session-token-123", now.Add(time.Hour))
	if err != nil {
		t.Fatalf("SignCookieValue: %v", err)
	}

	tests := []struct {
		name   string
		mutate func([]string)
	}{
		{
			name: "payload",
			mutate: func(parts []string) {
				payload, err := base64.RawURLEncoding.DecodeString(parts[2])
				if err != nil {
					t.Fatalf("DecodeString(payload): %v", err)
				}
				payload[0] ^= 0xff
				parts[2] = base64.RawURLEncoding.EncodeToString(payload)
			},
		},
		{
			name: "signature",
			mutate: func(parts []string) {
				signature, err := base64.RawURLEncoding.DecodeString(parts[3])
				if err != nil {
					t.Fatalf("DecodeString(signature): %v", err)
				}
				signature[0] ^= 0xff
				parts[3] = base64.RawURLEncoding.EncodeToString(signature)
			},
		},
		{
			name: "key id",
			mutate: func(parts []string) {
				parts[1] = base64.RawURLEncoding.EncodeToString([]byte("key-v2"))
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			parts := strings.Split(signed, ".")
			tt.mutate(parts)

			_, err := VerifySignedCookie(SignedCookieKeys{
				"key-v1": []byte("secret"),
				"key-v2": []byte("other-secret"),
			}, strings.Join(parts, "."), now)
			if !errors.Is(err, ErrSignedCookieSignature) {
				t.Fatalf("VerifySignedCookie error = %v, want ErrSignedCookieSignature", err)
			}
		})
	}
}

func TestSignedCookieRejectsExpired(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	signed, err := SignCookieValue("key-v1", []byte("secret"), "session-token-123", now.Add(-time.Second))
	if err != nil {
		t.Fatalf("SignCookieValue: %v", err)
	}

	_, err = VerifySignedCookie(SignedCookieKeys{"key-v1": []byte("secret")}, signed, now)
	if !errors.Is(err, ErrSignedCookieExpired) {
		t.Fatalf("VerifySignedCookie error = %v, want ErrSignedCookieExpired", err)
	}
}

func TestSignedCookieRejectsUnknownKey(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	signed, err := SignCookieValue("key-v1", []byte("secret"), "session-token-123", now.Add(time.Hour))
	if err != nil {
		t.Fatalf("SignCookieValue: %v", err)
	}

	_, err = VerifySignedCookie(SignedCookieKeys{"key-v2": []byte("secret")}, signed, now)
	if !errors.Is(err, ErrSignedCookieKeyNotFound) {
		t.Fatalf("VerifySignedCookie error = %v, want ErrSignedCookieKeyNotFound", err)
	}
}

func TestSignedCookieRejectsInvalidInput(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0).UTC()
	tests := []struct {
		name  string
		value SignedCookieValue
	}{
		{
			name: "missing key id",
			value: SignedCookieValue{
				Value:     "session-token-123",
				ExpiresAt: now.Add(time.Hour),
			},
		},
		{
			name: "missing expiry",
			value: SignedCookieValue{
				KeyID: "key-v1",
				Value: "session-token-123",
			},
		},
		{
			name: "key id whitespace",
			value: SignedCookieValue{
				KeyID:     " key-v1 ",
				Value:     "session-token-123",
				ExpiresAt: now.Add(time.Hour),
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := SignSignedCookie([]byte("secret"), tt.value)
			if !errors.Is(err, ErrSignedCookieInvalid) {
				t.Fatalf("SignSignedCookie error = %v, want ErrSignedCookieInvalid", err)
			}
		})
	}

	_, err := SignCookieValue("key-v1", nil, "session-token-123", now.Add(time.Hour))
	if !errors.Is(err, ErrSignedCookieInvalid) {
		t.Fatalf("SignCookieValue error = %v, want ErrSignedCookieInvalid", err)
	}
}

func TestSignedCookieRejectsMalformedValues(t *testing.T) {
	t.Parallel()

	tests := []string{
		"",
		"one.two.three",
		"sc1.key.payload.signature.extra",
		"sc2.key.payload.signature",
		"sc1.%%%.payload.signature",
	}
	for _, signed := range tests {
		_, err := VerifySignedCookie(SignedCookieKeys{"key-v1": []byte("secret")}, signed, time.Unix(1_700_000_000, 0))
		if !errors.Is(err, ErrSignedCookieInvalid) {
			t.Fatalf("VerifySignedCookie(%q) error = %v, want ErrSignedCookieInvalid", signed, err)
		}
	}
}
