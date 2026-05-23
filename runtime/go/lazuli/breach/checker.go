// Package breach is the runtime-level Checker seam for
// @lazuli/plugin-breach-watch. Adapters query Have-I-Been-Pwned-style
// services to reject signup/password-change with credentials
// known to be leaked.
//
// The framework calls Check from the auth/password flows when a
// new password is being set. Implementations MUST use k-anonymity
// (or similar privacy-preserving query) so the actual password
// never leaves the process.
package breach

import (
	"context"
	"errors"
)

// Checker queries a breach database for credential leaks.
// Implementations MUST NOT transmit the raw password - use HIBP's
// range API (k-anonymity: send only the first 5 chars of the
// SHA-1 hash; vendor returns all matching suffix hashes; caller
// checks locally).
type Checker interface {
	// PasswordBreached returns true when the password appears in
	// known breach corpora. Returns count of times seen (HIBP
	// shape) - 0 means clean.
	PasswordBreached(ctx context.Context, password string) (count int, err error)

	Close() error
}

var (
	ErrCheckerUnavailable = errors.New("lazuli/breach: checker unavailable")
	ErrCheckerTimeout     = errors.New("lazuli/breach: checker timed out")
)

// NoopChecker treats every password as clean. Default when no
// adapter binds. Pilots SHOULD bind @lazuli/plugin-breach-watch on
// signup/password-change flows.
type NoopChecker struct{}

func (NoopChecker) PasswordBreached(ctx context.Context, password string) (int, error) {
	return 0, nil
}
func (NoopChecker) Close() error { return nil }
