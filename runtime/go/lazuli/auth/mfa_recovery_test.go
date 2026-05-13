package auth

import (
	"errors"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

func TestGenerateMfaRecoveryCodesHashesMetadata(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 18, 0, 0, 0, time.FixedZone("BRT", -3*60*60))
	codes, err := GenerateMfaRecoveryCodes(lazuli.ID(42), 8, now)
	if err != nil {
		t.Fatalf("GenerateMfaRecoveryCodes() error = %v", err)
	}
	if len(codes) != 8 {
		t.Fatalf("len(codes) = %d, want 8", len(codes))
	}

	seen := make(map[string]struct{}, len(codes))
	for _, recovery := range codes {
		assertMfaRecoveryCodeShape(t, recovery.Code)
		if _, ok := seen[recovery.Code]; ok {
			t.Fatalf("duplicate recovery code %q", recovery.Code)
		}
		seen[recovery.Code] = struct{}{}

		wantHash, err := HashMfaRecoveryCode(recovery.Code)
		if err != nil {
			t.Fatalf("HashMfaRecoveryCode() error = %v", err)
		}
		meta := recovery.Metadata
		if meta.IdentityID != lazuli.ID(42) {
			t.Fatalf("IdentityID = %d, want 42", meta.IdentityID)
		}
		if meta.CodeHash != wantHash {
			t.Fatalf("CodeHash = %q, want %q", meta.CodeHash, wantHash)
		}
		if meta.CodeHash == recovery.Code {
			t.Fatalf("CodeHash must not retain the raw recovery code")
		}
		if meta.ID != wantHash[:32] {
			t.Fatalf("ID = %q, want code hash prefix", meta.ID)
		}
		if !meta.CreatedAt.Equal(now.UTC()) {
			t.Fatalf("CreatedAt = %s, want %s", meta.CreatedAt, now.UTC())
		}
		if meta.Used() {
			t.Fatal("new recovery code is marked used")
		}
		if err := meta.Validate(); err != nil {
			t.Fatalf("Validate() error = %v", err)
		}
		if err := meta.Verify(recovery.Code); err != nil {
			t.Fatalf("Verify() error = %v", err)
		}

		unformattedLower := strings.ToLower(strings.ReplaceAll(recovery.Code, "-", ""))
		if err := VerifyMfaRecoveryCode(meta, unformattedLower); err != nil {
			t.Fatalf("VerifyMfaRecoveryCode(unformatted lower) error = %v", err)
		}
	}
}

func TestMfaRecoveryCodeHashAndVerify(t *testing.T) {
	t.Parallel()

	hash, err := HashMfaRecoveryCode("abcd-efgh-ijkl-mnop")
	if err != nil {
		t.Fatalf("HashMfaRecoveryCode() error = %v", err)
	}
	sameHash, err := HashMfaRecoveryCode("ABCD EFGH IJKL MNOP")
	if err != nil {
		t.Fatalf("HashMfaRecoveryCode(spaced) error = %v", err)
	}
	if hash != sameHash {
		t.Fatalf("normalized hashes differ: %q != %q", hash, sameHash)
	}
	if len(hash) != 64 {
		t.Fatalf("hash len = %d, want 64", len(hash))
	}

	if err := VerifyMfaRecoveryCodeHash("ABCD-EFGH-IJKL-MNOP", hash); err != nil {
		t.Fatalf("VerifyMfaRecoveryCodeHash(match) error = %v", err)
	}
	if err := VerifyMfaRecoveryCodeHash("ABCD-EFGH-IJKL-MNOQ", hash); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("VerifyMfaRecoveryCodeHash(miss) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
	if err := VerifyMfaRecoveryCodeHash("ABCD-EFGH-IJKL-MNOP", "not-a-sha256-hash"); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("VerifyMfaRecoveryCodeHash(bad hash) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
	if _, err := HashMfaRecoveryCode("short"); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("HashMfaRecoveryCode(short) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
}

func TestUseMfaRecoveryCodeMarksOneTimeUse(t *testing.T) {
	t.Parallel()

	createdAt := time.Date(2026, 5, 12, 19, 0, 0, 0, time.UTC)
	usedAt := createdAt.Add(5 * time.Minute)
	meta, err := NewMfaRecoveryCodeMetadata(lazuli.ID(7), "AAAA-BBBB-CCCC-DDDD", createdAt)
	if err != nil {
		t.Fatalf("NewMfaRecoveryCodeMetadata() error = %v", err)
	}

	used, err := UseMfaRecoveryCode(meta, "aaaa-bbbb-cccc-dddd", usedAt)
	if err != nil {
		t.Fatalf("UseMfaRecoveryCode() error = %v", err)
	}
	if !used.Used() || !used.IsUsed() {
		t.Fatal("used metadata is not marked used")
	}
	if !used.UsedAt.Equal(usedAt) {
		t.Fatalf("UsedAt = %s, want %s", used.UsedAt, usedAt)
	}
	if err := VerifyMfaRecoveryCode(used, "AAAA-BBBB-CCCC-DDDD"); !errors.Is(err, ErrMfaRecoveryCodeUsed) {
		t.Fatalf("VerifyMfaRecoveryCode(used) error = %v, want ErrMfaRecoveryCodeUsed", err)
	}
	if _, err := used.Use("AAAA-BBBB-CCCC-DDDD", usedAt.Add(time.Minute)); !errors.Is(err, ErrMfaRecoveryCodeUsed) {
		t.Fatalf("Use(used) error = %v, want ErrMfaRecoveryCodeUsed", err)
	}
}

func TestValidateMfaRecoveryCodeMetadataRejectsInvalidState(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 20, 0, 0, 0, time.UTC)
	valid, err := NewMfaRecoveryCodeMetadata(lazuli.ID(9), "ZZZZ-YYYY-XXXX-WWWW", now)
	if err != nil {
		t.Fatalf("NewMfaRecoveryCodeMetadata() error = %v", err)
	}

	bad := valid
	bad.IdentityID = 0
	if err := bad.Validate(); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("Validate(missing identity) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
	bad = valid
	bad.CodeHash = "raw-code"
	if err := bad.Validate(); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("Validate(raw hash) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
	bad = valid
	bad.UsedAt = now.Add(-time.Minute)
	if err := bad.Validate(); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("Validate(used before created) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
}

func TestGenerateMfaRecoveryCodesDefaultsAndRejectsInvalidCount(t *testing.T) {
	t.Parallel()

	codes, err := GenerateMfaRecoveryCodes(lazuli.ID(1), 0, time.Time{})
	if err != nil {
		t.Fatalf("GenerateMfaRecoveryCodes(default) error = %v", err)
	}
	if len(codes) != DefaultMfaRecoveryCodeCount {
		t.Fatalf("len(codes) = %d, want %d", len(codes), DefaultMfaRecoveryCodeCount)
	}
	if _, err := GenerateMfaRecoveryCodes(lazuli.ID(1), -1, time.Time{}); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("GenerateMfaRecoveryCodes(negative) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
	if _, err := GenerateMfaRecoveryCodes(lazuli.ID(0), 1, time.Time{}); !errors.Is(err, ErrMfaRecoveryCodeInvalid) {
		t.Fatalf("GenerateMfaRecoveryCodes(no identity) error = %v, want ErrMfaRecoveryCodeInvalid", err)
	}
}

func assertMfaRecoveryCodeShape(t *testing.T, code string) {
	t.Helper()

	parts := strings.Split(code, "-")
	if len(parts) != 4 {
		t.Fatalf("code %q has %d groups, want 4", code, len(parts))
	}
	for _, part := range parts {
		if len(part) != 4 {
			t.Fatalf("code %q group %q len = %d, want 4", code, part, len(part))
		}
		for _, r := range part {
			if !validMfaRecoveryCodeRune(r) {
				t.Fatalf("code %q contains invalid rune %q", code, r)
			}
		}
	}
}
