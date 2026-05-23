// Package captcha is the runtime-level Verifier seam for @lazuli/plugin-
// captcha. Adapters wrap Cloudflare Turnstile / hCaptcha /
// Google reCAPTCHA.
//
// The framework invokes Verify when a command annotated
// `requires_captcha` runs (IR slot defined in a follow-up cell).
// Until that IR slot exists, pilots manually call Verify in their
// handler code.
package captcha

import (
	"context"
	"errors"
)

// Verifier validates a captcha token against the vendor's API.
// Implementations MUST be safe for concurrent use. Verification
// should complete in < 1s under nominal conditions; the framework
// will cancel via ctx on slower responses.
type Verifier interface {
	Verify(ctx context.Context, token string, remoteIP string) (Verdict, error)
	Close() error
}

// Verdict carries the vendor's score + risk signals.
type Verdict struct {
	Passed bool
	// Score is the vendor's [0.0, 1.0] confidence (higher = more
	// likely human). 0.0 for vendors that don't report a score.
	Score float32
	// Reasons is the vendor's risk-flag list (e.g., from hCaptcha's
	// error_codes field). Free-form strings -- do NOT parse for logic.
	Reasons []string
}

var (
	ErrVerifierUnavailable = errors.New("lazuli/captcha: verifier unavailable")
	ErrTokenInvalid        = errors.New("lazuli/captcha: token invalid")
	ErrTokenExpired        = errors.New("lazuli/captcha: token expired")
)

// NoopVerifier accepts ALL tokens (Score=0.0, Passed=true). Default
// when no adapter binds. SECURITY: dev-only. Prod pilots MUST bind
// @lazuli/plugin-captcha.
type NoopVerifier struct{}

func (NoopVerifier) Verify(ctx context.Context, token string, remoteIP string) (Verdict, error) {
	return Verdict{Passed: true, Score: 0.0}, nil
}
func (NoopVerifier) Close() error { return nil }
