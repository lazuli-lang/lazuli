package auth

import (
	"errors"

	"lazuli.dev/runtime/lazuli"
)

// MfaMethod names a closed-catalog MFA method. v0 ships TOTP only.
type MfaMethod string

const (
	// MfaMethodTOTP is the Time-based One-Time Password method.
	MfaMethodTOTP MfaMethod = "totp"
)

// MfaContract is the lowered `auth mfa <method>` block.
type MfaContract struct {
	// Method selects the dispatcher (`totp` in v0).
	Method MfaMethod
	// EnrollFn is the `@fn.<name>` reference invoked by EnrollMFA to
	// generate the per-user enrolment payload (e.g. TOTP secret + QR).
	EnrollFn string
	// VerifyFn is the `@fn.<name>` or `@validator.<name>` reference
	// invoked by VerifyMFA to check a submitted code.
	VerifyFn string
	// AdapterRef is the optional `@adapter.<name>` reference that
	// supplies provider-specific config (TOTP issuer, digits, period).
	AdapterRef string
}

// MfaEnrolment is the typed return payload from EnrollMFA. The
// concrete shape is method-specific; this struct documents the
// canonical TOTP shape.
type MfaEnrolment struct {
	// Secret is the base32-encoded TOTP secret.
	Secret string
	// QR is the data-URI of the QR image authenticator apps can scan.
	QR string
}

// Typed errors returned by the MFA capability. Mapped to
// `expose client` HTTP status codes:
//
//	ErrMfaCodeInvalid       → 400 auth.mfa_code_invalid
//	ErrMfaNotEnrolled       → 409 auth.mfa_not_enrolled
//	ErrMfaMethodUnsupported → 500 auth.mfa_method_unsupported
var (
	ErrMfaCodeInvalid       = errors.New("auth: mfa code invalid")
	ErrMfaNotEnrolled       = errors.New("auth: mfa not enrolled")
	ErrMfaMethodUnsupported = errors.New("auth: mfa method unsupported")
)

// EnrollMFA dispatches to the registered EnrollFn and returns the
// method-specific enrolment payload.
func EnrollMFA(ctx *lazuli.Ctx, contract MfaContract, identityID lazuli.ID) (MfaEnrolment, error) {
	_ = ctx
	_ = contract
	_ = identityID
	// TODO(runtime): TOTP via github.com/pquerna/otp/totp.
	return MfaEnrolment{}, errors.New("auth: EnrollMFA not yet implemented")
}

// VerifyMFA dispatches to the registered VerifyFn for the contract's
// method and returns nil on success, ErrMfaCodeInvalid on miss,
// ErrMfaNotEnrolled when the user has no enrolment row,
// ErrMfaMethodUnsupported when the contract's method is not handled.
func VerifyMFA(ctx *lazuli.Ctx, contract MfaContract, identityID lazuli.ID, code string) error {
	_ = ctx
	_ = contract
	_ = identityID
	_ = code
	return errors.New("auth: VerifyMFA not yet implemented")
}
