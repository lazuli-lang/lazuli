// Package webhooks — HMAC verifier + adapter dispatch.
//
// Phase L Tier 3 / row 33 stubs.
package webhooks

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
)

// VerifyHmacSignature compares the signature carried on `header` (per
// the verify spec) against `hmac(secret, body)`. Algorithm is closed:
// today only `sha256` is supported; future schemes gate on adapter
// pilot evidence.
//
// Returns ErrWebhookHmacInvalid on mismatch or unsupported algorithm.
func VerifyHmacSignature(spec VerifySpec, secret []byte, body []byte, header string) error {
	if spec.Scheme != VerifyHmac {
		return ErrWebhookHmacInvalid
	}
	switch spec.Algorithm {
	case "sha256":
		mac := hmac.New(sha256.New, secret)
		mac.Write(body)
		expected := hex.EncodeToString(mac.Sum(nil))
		if hmac.Equal([]byte(expected), []byte(header)) {
			return nil
		}
		return ErrWebhookHmacInvalid
	default:
		return ErrWebhookHmacInvalid
	}
}
