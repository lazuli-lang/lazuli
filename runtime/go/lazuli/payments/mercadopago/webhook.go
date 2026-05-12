// Package mercadopago contains Mercado Pago-specific payment webhook helpers.
package mercadopago

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"lazuli.dev/runtime/lazuli/webhooks"
)

const (
	// HeaderSignature is the Mercado Pago webhook signature header.
	HeaderSignature = "X-Signature"
	// HeaderRequestID is included in the signed Mercado Pago manifest.
	HeaderRequestID = "X-Request-Id"

	// QueryDataID is the signed resource id query parameter.
	QueryDataID = "data.id"
	// QueryType is the resource kind query parameter commonly sent with data.id.
	QueryType = "type"
	// QueryTopic is accepted as a legacy/alternate resource kind parameter.
	QueryTopic = "topic"
)

var (
	// ErrRequestMissing means a nil request was passed to a request helper.
	ErrRequestMissing = errors.New("mercadopago: request missing")
	// ErrSecretMissing means the configured Mercado Pago webhook secret is empty.
	ErrSecretMissing = errors.New("mercadopago: webhook secret missing")
	// ErrSignatureMissing means x-signature was not present.
	ErrSignatureMissing = errors.New("mercadopago: signature header missing")
	// ErrSignatureMalformed means x-signature could not be parsed.
	ErrSignatureMalformed = errors.New("mercadopago: signature header malformed")
	// ErrSignatureInvalid means x-signature did not match the expected HMAC.
	ErrSignatureInvalid = errors.New("mercadopago: signature invalid")
	// ErrIdempotencyKeyMissing means the request did not carry data.id.
	ErrIdempotencyKeyMissing = errors.New("mercadopago: idempotency key missing")
)

// Signature is the parsed x-signature header.
type Signature struct {
	Timestamp string
	V1        string
}

// WebhookIdentity contains the stable Mercado Pago identifiers carried on a
// webhook request.
type WebhookIdentity struct {
	DataID    string
	Type      string
	RequestID string
}

// ParseSignatureHeader extracts the timestamp and v1 HMAC from x-signature.
func ParseSignatureHeader(header string) (Signature, error) {
	if strings.TrimSpace(header) == "" {
		return Signature{}, ErrSignatureMissing
	}

	var signature Signature
	parsedAny := false
	for _, part := range strings.Split(header, ",") {
		key, value, ok := strings.Cut(part, "=")
		if !ok {
			continue
		}
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key == "" || value == "" {
			continue
		}
		parsedAny = true
		switch key {
		case "ts":
			signature.Timestamp = value
		case "v1":
			signature.V1 = value
		}
	}

	if !parsedAny {
		return Signature{}, ErrSignatureMalformed
	}
	if signature.V1 == "" {
		return Signature{}, ErrSignatureMalformed
	}
	return signature, nil
}

// Manifest builds the Mercado Pago signed template. Mercado Pago documents
// that absent values must be omitted from the template.
func Manifest(dataID, requestID, timestamp string) string {
	var builder strings.Builder
	appendManifestPart(&builder, "id", dataID)
	appendManifestPart(&builder, "request-id", requestID)
	appendManifestPart(&builder, "ts", timestamp)
	return builder.String()
}

// Sign returns the lowercase hexadecimal HMAC-SHA256 for the Mercado Pago
// manifest fields.
func Sign(secret []byte, dataID, requestID, timestamp string) (string, error) {
	sum, err := sign(secret, dataID, requestID, timestamp)
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(sum), nil
}

// VerifySignature validates a Mercado Pago x-signature value. Signature
// failures wrap webhooks.ErrWebhookHmacInvalid so generic webhook callers can
// classify them with errors.Is.
func VerifySignature(secret []byte, dataID, requestID, signatureHeader string) error {
	signature, err := ParseSignatureHeader(signatureHeader)
	if err != nil {
		return signatureError(err)
	}

	expected, err := sign(secret, dataID, requestID, signature.Timestamp)
	if err != nil {
		return err
	}
	actual, err := hex.DecodeString(signature.V1)
	if err != nil {
		return signatureError(ErrSignatureMalformed)
	}
	if !hmac.Equal(expected, actual) {
		return signatureError(ErrSignatureInvalid)
	}
	return nil
}

// VerifyRequest validates the request using x-signature, x-request-id, and
// data.id from the request URL.
func VerifyRequest(r *http.Request, secret []byte) error {
	identity, err := IdentityFromRequest(r)
	if err != nil {
		return err
	}
	return VerifySignature(secret, identity.DataID, identity.RequestID, r.Header.Get(HeaderSignature))
}

// IdentityFromRequest extracts the Mercado Pago identifiers from headers and
// query parameters.
func IdentityFromRequest(r *http.Request) (WebhookIdentity, error) {
	if r == nil {
		return WebhookIdentity{}, ErrRequestMissing
	}

	var dataID string
	var kind string
	if r.URL != nil {
		values := r.URL.Query()
		dataID = values.Get(QueryDataID)
		kind = strings.TrimSpace(values.Get(QueryType))
		if kind == "" {
			kind = strings.TrimSpace(values.Get(QueryTopic))
		}
	}
	return WebhookIdentity{
		DataID:    dataID,
		Type:      kind,
		RequestID: r.Header.Get(HeaderRequestID),
	}, nil
}

// IdempotencyKeyFromRequest returns a stable dedupe key based on Mercado Pago's
// resource kind and data.id query parameters.
func IdempotencyKeyFromRequest(r *http.Request) (string, error) {
	identity, err := IdentityFromRequest(r)
	if err != nil {
		return "", err
	}
	return IdempotencyKey(identity)
}

// IdempotencyKey returns the stable key used to claim a webhook envelope. When
// Type is present it scopes same-valued resource ids across Mercado Pago topics.
func IdempotencyKey(identity WebhookIdentity) (string, error) {
	dataID := strings.TrimSpace(identity.DataID)
	if dataID == "" {
		return "", ErrIdempotencyKeyMissing
	}

	kind := strings.TrimSpace(identity.Type)
	if kind == "" {
		return dataID, nil
	}
	return kind + ":" + dataID, nil
}

// TimestampFromRequest returns the timestamp embedded in x-signature using the
// same parser as the generic webhook replay guard.
func TimestampFromRequest(r *http.Request) (time.Time, error) {
	if r == nil {
		return time.Time{}, ErrRequestMissing
	}
	signature, err := ParseSignatureHeader(r.Header.Get(HeaderSignature))
	if err != nil {
		return time.Time{}, err
	}
	return webhooks.ParseWebhookTimestamp(signature.Timestamp)
}

func appendManifestPart(builder *strings.Builder, key, value string) {
	if strings.TrimSpace(value) == "" {
		return
	}
	builder.WriteString(key)
	builder.WriteByte(':')
	builder.WriteString(value)
	builder.WriteByte(';')
}

func sign(secret []byte, dataID, requestID, timestamp string) ([]byte, error) {
	if len(secret) == 0 {
		return nil, ErrSecretMissing
	}
	mac := hmac.New(sha256.New, secret)
	mac.Write([]byte(Manifest(dataID, requestID, timestamp)))
	return mac.Sum(nil), nil
}

func signatureError(err error) error {
	return fmt.Errorf("%w: %w", webhooks.ErrWebhookHmacInvalid, err)
}
