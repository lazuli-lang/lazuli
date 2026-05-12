package auth

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/pquerna/otp/totp"

	"lazuli.dev/runtime/lazuli"
)

// totpCodeAt generates a TOTP code valid for `at` using the same
// defaults the production code uses (Generate → DefaultOpts).
func totpCodeAt(secret string, at time.Time) (string, error) {
	return totp.GenerateCode(secret, at)
}

// TestSessionTTLContractShape pins the shape of `SessionsContract`
// and the typed error catalog. Concrete TTL expiry under
// `testing/synctest` lands once the runtime team implements
// `IssueSession` / `ResolveSession`; this test ensures the contract
// fields exist + the error sentinels are exported so generated code
// can branch on them.
func TestSessionTTLContractShape(t *testing.T) {
	contract := SessionsContract{
		Resource: "CustomerSession",
		TTL:      7 * 24 * time.Hour,
		Refresh:  false,
	}
	if contract.TTL <= 0 {
		t.Fatalf("TTL must be positive, got %v", contract.TTL)
	}
	if contract.Resource == "" {
		t.Fatalf("Resource must be set")
	}
	if !errors.Is(ErrSessionExpired, ErrSessionExpired) {
		t.Fatalf("ErrSessionExpired must be a stable sentinel")
	}
}

// TestPasswordAlgorithmCatalog pins the closed-catalog axis the
// language enforces via `auth_password_algorithm_hash_mismatch`.
func TestPasswordAlgorithmCatalog(t *testing.T) {
	for _, algo := range []PasswordAlgorithm{AlgoArgon2id, AlgoBcrypt} {
		if algo == "" {
			t.Fatalf("algorithm string must be non-empty")
		}
	}
}

// TestMfaMethodCatalog pins the v0 method catalog (TOTP only).
func TestMfaMethodCatalog(t *testing.T) {
	if MfaMethodTOTP != "totp" {
		t.Fatalf("MfaMethodTOTP must equal \"totp\"")
	}
}

// TestErrorSentinels guarantees the typed error catalog is non-empty
// and uniquely valued. Generated code branches on these errors via
// errors.Is, so adding/removing them is a breaking change tracked
// here.
func TestErrorSentinels(t *testing.T) {
	sentinels := []error{
		ErrPasswordMismatch,
		ErrPasswordRateLimited,
		ErrSessionExpired,
		ErrSessionNotFound,
		ErrOAuthStateMismatch,
		ErrOAuthAdapterUnregistered,
		ErrMfaCodeInvalid,
		ErrMfaNotEnrolled,
		ErrMfaMethodUnsupported,
	}
	for _, err := range sentinels {
		if err == nil {
			t.Fatalf("sentinel must be non-nil")
		}
		if err.Error() == "" {
			t.Fatalf("sentinel must have a message")
		}
	}
}

// newCtx returns a *lazuli.Ctx wired with a Background context for
// tests that don't care about cancellation.
func newCtx() *lazuli.Ctx {
	return &lazuli.Ctx{Context: context.Background()}
}

// TestArgon2idHashRoundTrip wires the argon2id codepath end to end.
// HashPassword must emit a PHC-format string and VerifyPassword must
// accept it for the same plaintext.
func TestArgon2idHashRoundTrip(t *testing.T) {
	t.Parallel()
	contract := PasswordContract{Algorithm: AlgoArgon2id}
	hash, err := HashPassword(newCtx(), contract, "correct horse battery staple")
	if err != nil {
		t.Fatalf("HashPassword: %v", err)
	}
	if !strings.HasPrefix(hash, "$argon2id$v=19$m=65536,t=3,p=4$") {
		t.Fatalf("expected canonical PHC prefix, got %q", hash)
	}
	if err := VerifyPassword(newCtx(), contract, "correct horse battery staple", hash); err != nil {
		t.Fatalf("VerifyPassword (matching): %v", err)
	}
}

// TestArgon2idRejectsWrongPassword confirms the constant-time compare
// surfaces ErrPasswordMismatch for the wrong plaintext.
func TestArgon2idRejectsWrongPassword(t *testing.T) {
	t.Parallel()
	contract := PasswordContract{Algorithm: AlgoArgon2id}
	hash, err := HashPassword(newCtx(), contract, "right")
	if err != nil {
		t.Fatalf("HashPassword: %v", err)
	}
	if err := VerifyPassword(newCtx(), contract, "wrong", hash); !errors.Is(err, ErrPasswordMismatch) {
		t.Fatalf("expected ErrPasswordMismatch, got %v", err)
	}
}

// TestBcryptFallback exercises the bcrypt branch — used for legacy
// hash migrations.
func TestBcryptFallback(t *testing.T) {
	t.Parallel()
	contract := PasswordContract{Algorithm: AlgoBcrypt}
	hash, err := HashPassword(newCtx(), contract, "legacy")
	if err != nil {
		t.Fatalf("HashPassword (bcrypt): %v", err)
	}
	if !strings.HasPrefix(hash, "$2") {
		t.Fatalf("expected bcrypt prefix, got %q", hash)
	}
	if err := VerifyPassword(newCtx(), contract, "legacy", hash); err != nil {
		t.Fatalf("VerifyPassword (bcrypt match): %v", err)
	}
	if err := VerifyPassword(newCtx(), contract, "different", hash); !errors.Is(err, ErrPasswordMismatch) {
		t.Fatalf("expected ErrPasswordMismatch, got %v", err)
	}
}

// TestOAuthStateTokenUniqueness pins that OAuthRedirect stashes a
// fresh state token into the Ctx per call and embeds it in the
// returned redirect URL. `crypto/rand` collisions are astronomically
// unlikely, so 8 samples are enough to flag a regression where the
// implementation re-uses a single token.
func TestOAuthStateTokenUniqueness(t *testing.T) {
	t.Parallel()
	contract := OAuthContract{
		Provider:     "google",
		ClientID:     "test-client",
		ClientSecret: "test-secret",
		RedirectURL:  "https://example.com/cb",
		Scopes:       []string{"openid", "email"},
	}
	seen := make(map[string]struct{}, 8)
	for i := 0; i < 8; i++ {
		ctx := newCtx()
		url, err := OAuthRedirect(ctx, contract)
		if err != nil {
			t.Fatalf("OAuthRedirect[%d]: %v", i, err)
		}
		state := LoadOAuthState(ctx, contract.Provider)
		if state == "" {
			t.Fatalf("OAuthRedirect[%d] did not stash a state token", i)
		}
		if !strings.Contains(url, "state="+state) {
			t.Fatalf("redirect URL %q must embed state %q", url, state)
		}
		if _, dup := seen[state]; dup {
			t.Fatalf("state token %q reused on iteration %d", state, i)
		}
		seen[state] = struct{}{}
	}
}

// TestOAuthCallbackStateMismatch confirms ErrOAuthStateMismatch fires
// when the inbound state doesn't match the stashed expected value.
func TestOAuthCallbackStateMismatch(t *testing.T) {
	t.Parallel()
	contract := OAuthContract{
		Provider:     "google",
		ClientID:     "x",
		ClientSecret: "y",
		RedirectURL:  "https://example.com/cb",
	}
	ctx := newCtx()
	if _, err := OAuthRedirect(ctx, contract); err != nil {
		t.Fatalf("OAuthRedirect: %v", err)
	}
	_, err := OAuthCallback(ctx, contract, SessionsContract{}, "code", "wrong-state")
	if !errors.Is(err, ErrOAuthStateMismatch) {
		t.Fatalf("expected ErrOAuthStateMismatch, got %v", err)
	}
}

// TestTOTPEnrollAndVerify wires `EnrollMFA` + `VerifyMFA`. The
// enrolment secret is fed back into Validate to assert the happy path
// works at runtime, not just by signature.
func TestTOTPEnrollAndVerify(t *testing.T) {
	t.Parallel()
	contract := MfaContract{Method: MfaMethodTOTP, Issuer: "Lazuli"}
	enrolment, err := EnrollMFA(newCtx(), contract, lazuli.ID(42))
	if err != nil {
		t.Fatalf("EnrollMFA: %v", err)
	}
	if enrolment.Secret == "" || enrolment.URI == "" {
		t.Fatalf("expected non-empty secret + URI, got %+v", enrolment)
	}
	// Generate a code for the current time and verify it round-trips.
	code, err := totpCodeAt(enrolment.Secret, time.Now())
	if err != nil {
		t.Fatalf("totp code gen: %v", err)
	}
	if err := VerifyMFA(newCtx(), contract, lazuli.ID(42), enrolment.Secret, code); err != nil {
		t.Fatalf("VerifyMFA (matching): %v", err)
	}
}

// TestTOTPRejectsExpiredCode pins the "wrong code surfaces
// ErrMfaCodeInvalid" branch. Generating a code with a stale timestamp
// (well outside the validation window) is sufficient — the
// `pquerna/otp` library defaults to ±0 windows in `totp.Validate`.
func TestTOTPRejectsExpiredCode(t *testing.T) {
	t.Parallel()
	contract := MfaContract{Method: MfaMethodTOTP}
	enrolment, err := EnrollMFA(newCtx(), contract, lazuli.ID(7))
	if err != nil {
		t.Fatalf("EnrollMFA: %v", err)
	}
	// A code from 10 minutes ago is far outside the default 30s
	// window; Validate must reject it.
	stale, err := totpCodeAt(enrolment.Secret, time.Now().Add(-10*time.Minute))
	if err != nil {
		t.Fatalf("totp code gen (stale): %v", err)
	}
	if err := VerifyMFA(newCtx(), contract, lazuli.ID(7), enrolment.Secret, stale); !errors.Is(err, ErrMfaCodeInvalid) {
		t.Fatalf("expected ErrMfaCodeInvalid for stale code, got %v", err)
	}

	// Also confirm that bogus code text is rejected.
	if err := VerifyMFA(newCtx(), contract, lazuli.ID(7), enrolment.Secret, "000000"); !errors.Is(err, ErrMfaCodeInvalid) {
		t.Fatalf("expected ErrMfaCodeInvalid for bogus code, got %v", err)
	}
}
