// Package auth implements the runtime side of the Lazuli `auth` block.
// The language declares identity/password/sessions/MFA/OAuth contracts;
// this package owns the dispatchers and typed errors. Concrete provider
// integrations (Google OAuth, Twilio, WebAuthn) sit in `@runtime/...` or
// `@plugin/...` adapter packages and bind via `@adapter.*` resolution
// at boot.
//
// This file ships the password capability stubs: signatures + typed
// errors. Full implementation (DB queries, HTTP middleware) is owned
// by the runtime team — these stubs only commit to the contract shape
// the codegen will call.
package auth

import (
	"errors"

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
	ErrPasswordMismatch    = errors.New("auth: password mismatch")
	ErrPasswordRateLimited = errors.New("auth: password rate limited")
)

// HashPassword hashes a plaintext password per the contract's
// algorithm. Stub: full implementation (argon2id parameters,
// constant-time comparison) lands with the runtime team.
//
// Signature is committed; callers in generated code may import
// without waiting for the body.
func HashPassword(ctx *lazuli.Ctx, contract PasswordContract, plaintext string) (string, error) {
	_ = ctx
	_ = contract
	_ = plaintext
	// TODO(runtime): argon2id via golang.org/x/crypto/argon2, bcrypt
	// via golang.org/x/crypto/bcrypt. Honor m=64MiB, t=3, p=4 for
	// argon2id in v0.
	return "", errors.New("auth: HashPassword not yet implemented")
}

// VerifyPassword compares a plaintext password against a stored hash
// according to the contract's algorithm. Returns nil on match,
// ErrPasswordMismatch on miss, ErrPasswordRateLimited when the
// declarative rate limit has been exceeded.
func VerifyPassword(ctx *lazuli.Ctx, contract PasswordContract, plaintext, storedHash string) error {
	_ = ctx
	_ = contract
	_ = plaintext
	_ = storedHash
	return errors.New("auth: VerifyPassword not yet implemented")
}
