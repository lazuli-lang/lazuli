package auth

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"strings"
	"time"
)

const jwtAlgorithm = "HS256"

// Claims is the canonical JWT claims shape.
type Claims struct {
	Subject   string         `json:"sub,omitempty"`
	Issuer    string         `json:"iss,omitempty"`
	Audience  string         `json:"aud,omitempty"`
	ExpiresAt int64          `json:"exp,omitempty"` // unix seconds
	NotBefore int64          `json:"nbf,omitempty"`
	IssuedAt  int64          `json:"iat,omitempty"`
	Custom    map[string]any `json:"-"` // merged into top-level on sign
}

var (
	ErrJWTInvalid   = errors.New("auth: jwt invalid")
	ErrJWTExpired   = errors.New("auth: jwt expired")
	ErrJWTSignature = errors.New("auth: jwt signature mismatch")
)

// SignJWT returns a signed JWT with the given claims using HS256.
//
//	token, err := auth.SignJWT([]byte(secret), Claims{Subject: "user-123", ExpiresAt: time.Now().Add(time.Hour).Unix()})
func SignJWT(secret []byte, claims Claims) (string, error) {
	headerJSON, err := json.Marshal(map[string]string{
		"alg": jwtAlgorithm,
		"typ": "JWT",
	})
	if err != nil {
		return "", err
	}
	claimsJSON, err := marshalJWTClaims(claims)
	if err != nil {
		return "", err
	}

	encoding := base64.RawURLEncoding
	signingInput := encoding.EncodeToString(headerJSON) + "." + encoding.EncodeToString(claimsJSON)
	signature := signJWT(secret, signingInput)
	return signingInput + "." + encoding.EncodeToString(signature), nil
}

// VerifyJWT parses + validates HS256 signature and `exp` claim.
//
//	claims, err := auth.VerifyJWT([]byte(secret), token)
//	if errors.Is(err, ErrJWTExpired) { ... }
func VerifyJWT(secret []byte, token string) (Claims, error) {
	parts := strings.Split(token, ".")
	if len(parts) != 3 || parts[0] == "" || parts[1] == "" || parts[2] == "" {
		return Claims{}, ErrJWTInvalid
	}

	encoding := base64.RawURLEncoding
	headerJSON, err := encoding.DecodeString(parts[0])
	if err != nil {
		return Claims{}, ErrJWTInvalid
	}
	var header struct {
		Algorithm string `json:"alg"`
		Type      string `json:"typ"`
	}
	if err := json.Unmarshal(headerJSON, &header); err != nil {
		return Claims{}, ErrJWTInvalid
	}
	if header.Algorithm != jwtAlgorithm {
		return Claims{}, ErrJWTInvalid
	}

	claimsJSON, err := encoding.DecodeString(parts[1])
	if err != nil {
		return Claims{}, ErrJWTInvalid
	}
	signature, err := encoding.DecodeString(parts[2])
	if err != nil {
		return Claims{}, ErrJWTInvalid
	}
	signingInput := parts[0] + "." + parts[1]
	if !hmac.Equal(signature, signJWT(secret, signingInput)) {
		return Claims{}, ErrJWTSignature
	}

	claims, err := unmarshalJWTClaims(claimsJSON)
	if err != nil {
		return Claims{}, ErrJWTInvalid
	}
	if claims.ExpiresAt > 0 && claims.ExpiresAt <= time.Now().Unix() {
		return Claims{}, ErrJWTExpired
	}
	return claims, nil
}

func signJWT(secret []byte, signingInput string) []byte {
	mac := hmac.New(sha256.New, secret)
	_, _ = mac.Write([]byte(signingInput))
	return mac.Sum(nil)
}

func marshalJWTClaims(claims Claims) ([]byte, error) {
	payload := make(map[string]any, len(claims.Custom)+6)
	for key, value := range claims.Custom {
		if isRegisteredJWTClaim(key) {
			continue
		}
		payload[key] = value
	}
	if claims.Subject != "" {
		payload["sub"] = claims.Subject
	}
	if claims.Issuer != "" {
		payload["iss"] = claims.Issuer
	}
	if claims.Audience != "" {
		payload["aud"] = claims.Audience
	}
	if claims.ExpiresAt != 0 {
		payload["exp"] = claims.ExpiresAt
	}
	if claims.NotBefore != 0 {
		payload["nbf"] = claims.NotBefore
	}
	if claims.IssuedAt != 0 {
		payload["iat"] = claims.IssuedAt
	}
	return json.Marshal(payload)
}

func unmarshalJWTClaims(payload []byte) (Claims, error) {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(payload, &fields); err != nil {
		return Claims{}, err
	}

	var claims Claims
	custom := make(map[string]any, len(fields))
	for key, raw := range fields {
		switch key {
		case "sub":
			if err := json.Unmarshal(raw, &claims.Subject); err != nil {
				return Claims{}, err
			}
		case "iss":
			if err := json.Unmarshal(raw, &claims.Issuer); err != nil {
				return Claims{}, err
			}
		case "aud":
			if err := json.Unmarshal(raw, &claims.Audience); err != nil {
				return Claims{}, err
			}
		case "exp":
			if err := json.Unmarshal(raw, &claims.ExpiresAt); err != nil {
				return Claims{}, err
			}
		case "nbf":
			if err := json.Unmarshal(raw, &claims.NotBefore); err != nil {
				return Claims{}, err
			}
		case "iat":
			if err := json.Unmarshal(raw, &claims.IssuedAt); err != nil {
				return Claims{}, err
			}
		default:
			var value any
			if err := json.Unmarshal(raw, &value); err != nil {
				return Claims{}, err
			}
			custom[key] = value
		}
	}
	if len(custom) > 0 {
		claims.Custom = custom
	}
	return claims, nil
}

func isRegisteredJWTClaim(key string) bool {
	switch key {
	case "sub", "iss", "aud", "exp", "nbf", "iat":
		return true
	default:
		return false
	}
}
