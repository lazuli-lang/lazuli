// Package auth implements the runtime side of the Lazuli `auth` block.
// The language declares identity/password/sessions/MFA/OAuth contracts;
// this package owns the dispatchers and typed errors. Concrete provider
// integrations (Google OAuth, Twilio, WebAuthn) sit in `@runtime/...` or
// `@lazuli/plugin-...` adapter packages and bind via `@adapter.*` resolution
// at boot.
package auth

import (
	"context"
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"fmt"
	"net/http"
	"strings"

	"golang.org/x/crypto/argon2"
	"golang.org/x/crypto/bcrypt"

	"lazuli.dev/runtime/lazuli"
)

// PasswordAlgorithm names a closed-catalog hash algorithm. The string
// values match the `auth.password.algorithm` axis in the DSL and the
// `algorithm:` argument of `@cap.Hashed(...)`.
type PasswordAlgorithm string

const (
	// AlgoArgon2id is the v0 recommended password hash algorithm.
	AlgoArgon2id PasswordAlgorithm = "argon2id"
	// AlgoBcrypt is supported for legacy migration only.
	AlgoBcrypt PasswordAlgorithm = "bcrypt"
)

// Argon2id PHC parameters per OWASP guidance (m=64 MiB, t=3, p=4,
// keyLen=32, saltLen=16).
const (
	argon2idMemoryKiB = 64 * 1024
	argon2idTime      = 3
	argon2idThreads   = 4
	argon2idKeyLen    = 32
	argon2idSaltLen   = 16
)

const defaultArgon2Concurrency = 4

var argon2Semaphore = make(chan struct{}, defaultArgon2Concurrency)

// Argon2Params carries the PHC parameters used for one argon2id
// computation.
type Argon2Params struct {
	Salt    []byte
	Time    uint32
	Memory  uint32
	Threads uint8
	KeyLen  uint32
}

// FieldRef names a typed `resource.field` reference. Stub mirroring
// what the language IR `FieldRef` will eventually expose to runtime
// code; the language team will replace this with a shared declaration
// once the IR JSON crossing surfaces it.
type FieldRef struct {
	Resource string
	Field    string
}

// PasswordContract is the lowered `auth password` block. The codegen
// emits a `var PasswordContract = auth.PasswordContract{...}` per
// feature.
type PasswordContract struct {
	// Identity is the resource.field used as the canonical login id
	// (e.g. `Customer.email`).
	Identity FieldRef
	// Algorithm pins the hash algorithm; must match the session
	// resource's `@cap.Hashed(algorithm:…)` axis (doctor checks this
	// at compile time via `auth_password_algorithm_hash_mismatch`).
	Algorithm PasswordAlgorithm
	// HashFn is the `@fn.<name>` reference for hashing on signup /
	// password reset.
	HashFn string
	// VerifyFn is the `@fn.<name>` reference used by `Login`.
	VerifyFn string
	// RateLimit is the declarative throttle string parsed by the
	// adapter (e.g. `"5 per 10 minutes"`).
	RateLimit string
}

// Typed errors returned by the password capability. Mapped to
// `expose client` HTTP status codes:
//
//	ErrPasswordMismatch    → 401 auth.password_mismatch
//	ErrPasswordRateLimited → 429 auth.rate_limited
var (
	ErrPasswordMismatch        = errors.New("auth: password mismatch")
	ErrPasswordRateLimited     = errors.New("auth: password rate limited")
	ErrPasswordHashMalformed   = errors.New("auth: password hash malformed")
	ErrPasswordAlgoUnsupported = errors.New("auth: password algorithm unsupported")
	// ErrArgon2Saturated is returned when the argon2 semaphore is full
	// and the request couldn't acquire a slot before the context deadline.
	// Maps to HTTP 503 at the transport layer so legitimate clients can
	// retry instead of the server OOMing.
	ErrArgon2Saturated = errors.New("lazuli/auth: argon2 saturated; retry later")
)

// HashWithArgon2 acquires a semaphore slot before calling argon2.IDKey
// to cap total RAM. SECURITY (SEC-H10): without this cap, 100 concurrent
// /login requests at 64 MiB × 4 threads exhaust 25.6 GiB RAM. Slot
// count = 4 by default; tune via SetArgon2Concurrency for hardware
// with more memory.
func HashWithArgon2(ctx context.Context, password []byte, params Argon2Params) ([]byte, error) {
	release, err := acquireArgon2Slot(ctx)
	if err != nil {
		return nil, err
	}
	defer release()
	return argon2.IDKey(password, params.Salt, params.Time, params.Memory, params.Threads, params.KeyLen), nil
}

func acquireArgon2Slot(ctx context.Context) (func(), error) {
	if ctx == nil {
		ctx = context.Background()
	}
	sem := argon2Semaphore
	select {
	case sem <- struct{}{}:
		return func() { <-sem }, nil
	case <-ctx.Done():
		return nil, newArgon2SaturatedError()
	}
}

func newArgon2SaturatedError() error {
	const code = "auth.argon2_saturated"
	return &lazuli.Error{
		Status:  http.StatusServiceUnavailable,
		Code:    code,
		Message: "argon2 saturated; retry later",
		Base: lazuli.ErrorBase{
			Code:    code,
			Surface: lazuli.SurfaceLibInternal,
			Status:  http.StatusServiceUnavailable,
			Message: "argon2 saturated; retry later",
			Cause:   ErrArgon2Saturated,
		},
	}
}

// SetArgon2Concurrency replaces the semaphore size. Call from init
// or at config-load time; safe-after-boot only when no inflight
// hashing is in progress.
func SetArgon2Concurrency(n int) {
	if n < 1 {
		n = 1
	}
	argon2Semaphore = make(chan struct{}, n)
}

// HashPassword hashes a plaintext password per the contract's
// algorithm. Argon2id emits a PHC string format
// `$argon2id$v=19$m=65536,t=3,p=4$<salt>$<hash>`; bcrypt emits the
// native bcrypt string format.
func HashPassword(ctx *lazuli.Ctx, contract PasswordContract, plaintext string) (string, error) {
	switch contract.Algorithm {
	case AlgoArgon2id, "":
		salt := make([]byte, argon2idSaltLen)
		if _, err := rand.Read(salt); err != nil {
			return "", err
		}
		params := Argon2Params{
			Salt:    salt,
			Time:    argon2idTime,
			Memory:  argon2idMemoryKiB,
			Threads: argon2idThreads,
			KeyLen:  argon2idKeyLen,
		}
		hash, err := HashWithArgon2(ctxOrBackground(ctx), []byte(plaintext), params)
		if err != nil {
			return "", err
		}
		return encodeArgon2idPHC(salt, hash), nil
	case AlgoBcrypt:
		hash, err := bcrypt.GenerateFromPassword([]byte(plaintext), bcrypt.DefaultCost)
		if err != nil {
			return "", err
		}
		return string(hash), nil
	default:
		return "", ErrPasswordAlgoUnsupported
	}
}

// VerifyPassword compares a plaintext password against a stored hash
// according to the contract's algorithm. Returns nil on match,
// ErrPasswordMismatch on miss, ErrPasswordRateLimited when the
// declarative rate limit has been exceeded.
func VerifyPassword(ctx *lazuli.Ctx, contract PasswordContract, plaintext, storedHash string) error {
	switch contract.Algorithm {
	case AlgoArgon2id, "":
		salt, want, err := decodeArgon2idPHC(storedHash)
		if err != nil {
			return err
		}
		params := Argon2Params{
			Salt:    salt,
			Time:    argon2idTime,
			Memory:  argon2idMemoryKiB,
			Threads: argon2idThreads,
			KeyLen:  uint32(len(want)),
		}
		got, err := HashWithArgon2(ctxOrBackground(ctx), []byte(plaintext), params)
		if err != nil {
			return err
		}
		if subtle.ConstantTimeCompare(got, want) != 1 {
			return ErrPasswordMismatch
		}
		return nil
	case AlgoBcrypt:
		if err := bcrypt.CompareHashAndPassword([]byte(storedHash), []byte(plaintext)); err != nil {
			if errors.Is(err, bcrypt.ErrMismatchedHashAndPassword) {
				return ErrPasswordMismatch
			}
			return err
		}
		return nil
	default:
		return ErrPasswordAlgoUnsupported
	}
}

// encodeArgon2idPHC renders salt + hash in the canonical
// `$argon2id$v=19$m=...,t=...,p=...$<salt>$<hash>` PHC format.
func encodeArgon2idPHC(salt, hash []byte) string {
	b64 := base64.RawStdEncoding
	return fmt.Sprintf("$argon2id$v=%d$m=%d,t=%d,p=%d$%s$%s",
		argon2.Version,
		argon2idMemoryKiB, argon2idTime, argon2idThreads,
		b64.EncodeToString(salt),
		b64.EncodeToString(hash),
	)
}

// decodeArgon2idPHC parses a PHC-format argon2id hash and returns
// the salt + raw hash bytes.
func decodeArgon2idPHC(encoded string) (salt, hash []byte, err error) {
	parts := strings.Split(encoded, "$")
	// Format: ["", "argon2id", "v=19", "m=...,t=...,p=...", "<salt>", "<hash>"]
	if len(parts) != 6 || parts[1] != "argon2id" {
		return nil, nil, ErrPasswordHashMalformed
	}
	b64 := base64.RawStdEncoding
	salt, err = b64.DecodeString(parts[4])
	if err != nil {
		return nil, nil, ErrPasswordHashMalformed
	}
	hash, err = b64.DecodeString(parts[5])
	if err != nil {
		return nil, nil, ErrPasswordHashMalformed
	}
	return salt, hash, nil
}
