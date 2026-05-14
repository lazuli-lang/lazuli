package auth

import (
	"encoding/json"
	"errors"

	"github.com/golang-jwt/jwt/v5"
)

// Claims is the canonical JWT claims shape.
type Claims struct {
	Subject   string         `json:"sub,omitempty"`
	Issuer    string         `json:"iss,omitempty"`
	Audience  string         `json:"aud,omitempty"`
	ExpiresAt int64          `json:"exp,omitempty"`
	NotBefore int64          `json:"nbf,omitempty"`
	IssuedAt  int64          `json:"iat,omitempty"`
	Custom    map[string]any `json:"-"`
}

var ErrJWTInvalid, ErrJWTExpired, ErrJWTSignature = errors.New("auth: jwt invalid"), errors.New("auth: jwt expired"), errors.New("auth: jwt signature mismatch")

var registered = map[string]bool{"sub": true, "iss": true, "aud": true, "exp": true, "nbf": true, "iat": true}

// SignJWT returns a signed JWT with the given claims using HS256.
//
//	token, err := auth.SignJWT([]byte(secret), Claims{Subject: "user-123", ExpiresAt: time.Now().Add(time.Hour).Unix()})
func SignJWT(secret []byte, claims Claims) (string, error) {
	data, _ := json.Marshal(claims)
	mc := jwt.MapClaims{}
	_ = json.Unmarshal(data, &mc)
	for k, v := range claims.Custom {
		if !isRegistered(k) {
			mc[k] = v
		}
	}
	return jwt.NewWithClaims(jwt.SigningMethodHS256, mc).SignedString(secret)
}

// VerifyJWT parses + validates HS256 signature and `exp` claim.
//
//	claims, err := auth.VerifyJWT([]byte(secret), token)
//	if errors.Is(err, ErrJWTExpired) { ... }
func VerifyJWT(secret []byte, token string) (Claims, error) {
	mc := jwt.MapClaims{}
	_, err := jwt.ParseWithClaims(token, &mc, func(t *jwt.Token) (any, error) {
		if t.Method != jwt.SigningMethodHS256 {
			return nil, ErrJWTInvalid
		}
		return secret, nil
	}, jwt.WithValidMethods([]string{"HS256"}))
	if errors.Is(err, jwt.ErrTokenExpired) {
		return Claims{}, ErrJWTExpired
	}
	if errors.Is(err, jwt.ErrTokenSignatureInvalid) {
		return Claims{}, ErrJWTSignature
	}
	if err != nil {
		return Claims{}, ErrJWTInvalid
	}
	raw, _ := json.Marshal(mc)
	var c Claims
	if err := json.Unmarshal(raw, &c); err != nil {
		return Claims{}, ErrJWTInvalid
	}
	c.Custom = make(map[string]any)
	for k, v := range mc {
		if !isRegistered(k) {
			c.Custom[k] = v
		}
	}
	return c, nil
}

func isRegistered(k string) bool { return registered[k] }
