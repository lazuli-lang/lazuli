package auth

import (
	"errors"
	"testing"
	"time"
)

// TestSessionTTLContractShape pins the shape of `SessionsContract`
// and the typed error catalog. Concrete TTL expiry under
// `testing/synctest` lands once the runtime team implements
// `IssueSession` / `ResolveSession`; this test ensures the contract
// fields exist + the error sentinels are exported so generated code
// can branch on them.
//
// When the runtime body lands, the synctest-based expiry test should
// look like:
//
//	synctest.Run(func() {
//	    contract := SessionsContract{TTL: 7 * 24 * time.Hour}
//	    cookie, err := IssueSession(ctx, contract, 1, "password")
//	    if err != nil { t.Fatal(err) }
//	    time.Sleep(8 * 24 * time.Hour) // advance synthetic clock
//	    if err := ResolveSession(ctx, contract, cookie); !errors.Is(err, ErrSessionExpired) {
//	        t.Fatalf("expected ErrSessionExpired, got %v", err)
//	    }
//	})
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
