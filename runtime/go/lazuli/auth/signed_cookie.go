package auth

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"
	"unicode"
)

const signedCookieVersion = "sc1"

var (
	// ErrSignedCookieInvalid is returned for malformed signed cookie values or
	// invalid signing inputs.
	ErrSignedCookieInvalid = errors.New("auth: signed cookie invalid")
	// ErrSignedCookieExpired is returned when the signed cookie expiry is at or
	// before the verification time.
	ErrSignedCookieExpired = errors.New("auth: signed cookie expired")
	// ErrSignedCookieSignature is returned when a signed cookie HMAC does not match.
	ErrSignedCookieSignature = errors.New("auth: signed cookie signature mismatch")
	// ErrSignedCookieKeyNotFound is returned when verification cannot resolve
	// the key id embedded in the signed value.
	ErrSignedCookieKeyNotFound = errors.New("auth: signed cookie key not found")
)

// SignedCookieValue is the metadata carried by a signed cookie value.
//
// KeyID identifies the HMAC key used to sign the cookie. ExpiresAt is required
// and is encoded as Unix seconds so the compact value stays stable across
// transports.
type SignedCookieValue struct {
	KeyID     string
	Value     string
	ExpiresAt time.Time
	IssuedAt  time.Time
}

// SignedCookieKeys maps key ids to HMAC secrets. Keep old keys in this map
// until all cookies signed with them have expired.
type SignedCookieKeys map[string][]byte

type signedCookiePayload struct {
	Value     string `json:"value"`
	ExpiresAt int64  `json:"exp"`
	IssuedAt  int64  `json:"iat,omitempty"`
}

// SignSignedCookie returns a compact HMAC-SHA256 signed cookie value.
func SignSignedCookie(secret []byte, value SignedCookieValue) (string, error) {
	if len(secret) == 0 {
		return "", signedCookieInvalidf("secret is required")
	}
	normalized, err := normalizeSignedCookieValue(value)
	if err != nil {
		return "", err
	}

	payload := signedCookiePayload{
		Value:     normalized.Value,
		ExpiresAt: normalized.ExpiresAt.Unix(),
	}
	if !normalized.IssuedAt.IsZero() {
		payload.IssuedAt = normalized.IssuedAt.Unix()
	}

	payloadJSON, err := json.Marshal(payload)
	if err != nil {
		return "", err
	}

	encoding := base64.RawURLEncoding
	encodedKeyID := encoding.EncodeToString([]byte(normalized.KeyID))
	encodedPayload := encoding.EncodeToString(payloadJSON)
	signingInput := signedCookieVersion + "." + encodedKeyID + "." + encodedPayload
	signature := signSignedCookie(secret, signingInput)
	return signingInput + "." + encoding.EncodeToString(signature), nil
}

// SignCookieValue signs a plain cookie value with required key id and expiry
// metadata.
func SignCookieValue(keyID string, secret []byte, value string, expiresAt time.Time) (string, error) {
	return SignSignedCookie(secret, SignedCookieValue{
		KeyID:     keyID,
		Value:     value,
		ExpiresAt: expiresAt,
	})
}

// VerifySignedCookie validates the HMAC-SHA256 signature, decodes the cookie
// metadata, and enforces expiry. When now is zero, time.Now is used.
func VerifySignedCookie(keys map[string][]byte, signedValue string, now time.Time) (SignedCookieValue, error) {
	parts := strings.Split(signedValue, ".")
	if len(parts) != 4 ||
		parts[0] != signedCookieVersion ||
		parts[1] == "" ||
		parts[2] == "" ||
		parts[3] == "" {
		return SignedCookieValue{}, ErrSignedCookieInvalid
	}

	encoding := base64.RawURLEncoding
	keyIDBytes, err := encoding.DecodeString(parts[1])
	if err != nil {
		return SignedCookieValue{}, ErrSignedCookieInvalid
	}
	keyID := string(keyIDBytes)
	if err := validateSignedCookieKeyID(keyID); err != nil {
		return SignedCookieValue{}, err
	}

	secret, ok := keys[keyID]
	if !ok {
		return SignedCookieValue{}, fmt.Errorf("%w: %s", ErrSignedCookieKeyNotFound, keyID)
	}
	if len(secret) == 0 {
		return SignedCookieValue{}, signedCookieInvalidf("secret for key %q is required", keyID)
	}

	signature, err := encoding.DecodeString(parts[3])
	if err != nil {
		return SignedCookieValue{}, ErrSignedCookieInvalid
	}
	signingInput := parts[0] + "." + parts[1] + "." + parts[2]
	if !hmac.Equal(signature, signSignedCookie(secret, signingInput)) {
		return SignedCookieValue{}, ErrSignedCookieSignature
	}

	payloadJSON, err := encoding.DecodeString(parts[2])
	if err != nil {
		return SignedCookieValue{}, ErrSignedCookieInvalid
	}
	var payload signedCookiePayload
	if err := json.Unmarshal(payloadJSON, &payload); err != nil {
		return SignedCookieValue{}, ErrSignedCookieInvalid
	}
	if payload.ExpiresAt <= 0 {
		return SignedCookieValue{}, signedCookieInvalidf("exp is required")
	}

	value := SignedCookieValue{
		KeyID:     keyID,
		Value:     payload.Value,
		ExpiresAt: time.Unix(payload.ExpiresAt, 0).UTC(),
	}
	if payload.IssuedAt > 0 {
		value.IssuedAt = time.Unix(payload.IssuedAt, 0).UTC()
	}

	if now.IsZero() {
		now = time.Now()
	}
	if !value.ExpiresAt.After(now.UTC()) {
		return SignedCookieValue{}, ErrSignedCookieExpired
	}
	return value, nil
}

// VerifyCookieValue is an alias for VerifySignedCookie.
func VerifyCookieValue(keys map[string][]byte, signedValue string, now time.Time) (SignedCookieValue, error) {
	return VerifySignedCookie(keys, signedValue, now)
}

func signSignedCookie(secret []byte, signingInput string) []byte {
	mac := hmac.New(sha256.New, secret)
	_, _ = mac.Write([]byte(signingInput))
	return mac.Sum(nil)
}

func normalizeSignedCookieValue(value SignedCookieValue) (SignedCookieValue, error) {
	if err := validateSignedCookieKeyID(value.KeyID); err != nil {
		return SignedCookieValue{}, err
	}
	if value.ExpiresAt.IsZero() {
		return SignedCookieValue{}, signedCookieInvalidf("ExpiresAt is required")
	}
	if value.ExpiresAt.Unix() <= 0 {
		return SignedCookieValue{}, signedCookieInvalidf("ExpiresAt must be after Unix epoch")
	}

	value.ExpiresAt = time.Unix(value.ExpiresAt.Unix(), 0).UTC()
	if !value.IssuedAt.IsZero() {
		if value.IssuedAt.Unix() <= 0 {
			return SignedCookieValue{}, signedCookieInvalidf("IssuedAt must be after Unix epoch")
		}
		value.IssuedAt = time.Unix(value.IssuedAt.Unix(), 0).UTC()
	}
	return value, nil
}

func validateSignedCookieKeyID(keyID string) error {
	if strings.TrimSpace(keyID) == "" {
		return signedCookieInvalidf("key id is required")
	}
	if strings.TrimSpace(keyID) != keyID {
		return signedCookieInvalidf("key id has surrounding whitespace")
	}
	for _, r := range keyID {
		if unicode.IsControl(r) {
			return signedCookieInvalidf("key id contains control characters")
		}
	}
	return nil
}

func signedCookieInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrSignedCookieInvalid}, args...)...)
}
