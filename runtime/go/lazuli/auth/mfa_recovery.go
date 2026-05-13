package auth

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base32"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"time"
	"unicode"

	"lazuli.dev/runtime/lazuli"
)

const (
	// DefaultMfaRecoveryCodeCount is the number of one-time recovery codes
	// generated when callers pass count=0.
	DefaultMfaRecoveryCodeCount = 10

	mfaRecoveryCodeRandomBytes   = 10
	mfaRecoveryCodeEncodedLength = 16
	mfaRecoveryCodeGroupSize     = 4
	maxMfaRecoveryCodeCount      = 100
	maxMfaRecoveryCodeIDLength   = 128
)

var (
	ErrMfaRecoveryCodeInvalid = errors.New("auth: mfa recovery code invalid")
	ErrMfaRecoveryCodeUsed    = errors.New("auth: mfa recovery code used")

	mfaRecoveryCodeEncoding = base32.StdEncoding.WithPadding(base32.NoPadding)
)

// MfaRecoveryCode carries the user-visible code and the non-secret metadata
// adapters should persist. The raw Code should only be shown once.
type MfaRecoveryCode struct {
	Code     string
	Metadata MfaRecoveryCodeMetadata
}

// MfaRecoveryCodeMetadata is adapter-neutral state for a single one-time MFA
// recovery code. CodeHash is the SHA-256 hex digest of the normalized code;
// callers should never persist the raw code value.
type MfaRecoveryCodeMetadata struct {
	ID         string
	IdentityID lazuli.ID
	CodeHash   string
	CreatedAt  time.Time
	UsedAt     time.Time
}

// GenerateMfaRecoveryCode creates one cryptographically random recovery code
// and matching metadata for persistence.
func GenerateMfaRecoveryCode(identityID lazuli.ID, now time.Time) (MfaRecoveryCode, error) {
	code, err := randomMfaRecoveryCode()
	if err != nil {
		return MfaRecoveryCode{}, err
	}
	meta, err := NewMfaRecoveryCodeMetadata(identityID, code, now)
	if err != nil {
		return MfaRecoveryCode{}, err
	}
	return MfaRecoveryCode{Code: code, Metadata: meta}, nil
}

// GenerateMfaRecoveryCodes creates count one-time recovery codes. Passing
// count=0 uses DefaultMfaRecoveryCodeCount.
func GenerateMfaRecoveryCodes(identityID lazuli.ID, count int, now time.Time) ([]MfaRecoveryCode, error) {
	if identityID <= 0 {
		return nil, fmt.Errorf("%w: IdentityID is required", ErrMfaRecoveryCodeInvalid)
	}
	if count == 0 {
		count = DefaultMfaRecoveryCodeCount
	}
	if count < 0 || count > maxMfaRecoveryCodeCount {
		return nil, fmt.Errorf("%w: count must be between 0 and %d", ErrMfaRecoveryCodeInvalid, maxMfaRecoveryCodeCount)
	}

	codes := make([]MfaRecoveryCode, 0, count)
	seen := make(map[string]struct{}, count)
	for len(codes) < count {
		code, err := GenerateMfaRecoveryCode(identityID, now)
		if err != nil {
			return nil, err
		}
		if _, ok := seen[code.Code]; ok {
			continue
		}
		seen[code.Code] = struct{}{}
		codes = append(codes, code)
	}
	return codes, nil
}

// NewMfaRecoveryCodeMetadata hashes a raw recovery code into non-secret
// metadata for persistence. The raw code is not retained.
func NewMfaRecoveryCodeMetadata(identityID lazuli.ID, code string, now time.Time) (MfaRecoveryCodeMetadata, error) {
	if identityID <= 0 {
		return MfaRecoveryCodeMetadata{}, fmt.Errorf("%w: IdentityID is required", ErrMfaRecoveryCodeInvalid)
	}
	codeHash, err := HashMfaRecoveryCode(code)
	if err != nil {
		return MfaRecoveryCodeMetadata{}, err
	}
	now = normalizeMfaRecoveryCodeTime(now)
	return MfaRecoveryCodeMetadata{
		ID:         codeHash[:32],
		IdentityID: identityID,
		CodeHash:   codeHash,
		CreatedAt:  now,
	}, nil
}

// HashMfaRecoveryCode hashes a normalized MFA recovery code for persistence.
// Generated recovery codes are random one-time secrets, so they follow the
// existing token-secret pattern of SHA-256 hex storage.
func HashMfaRecoveryCode(code string) (string, error) {
	normalized, err := normalizeMfaRecoveryCode(code)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256([]byte(normalized))
	return hex.EncodeToString(sum[:]), nil
}

// VerifyMfaRecoveryCodeHash compares a submitted code with a stored SHA-256
// hex digest using constant-time comparison.
func VerifyMfaRecoveryCodeHash(code, storedHash string) error {
	codeHash, err := HashMfaRecoveryCode(code)
	if err != nil {
		return err
	}
	storedHash = strings.ToLower(strings.TrimSpace(storedHash))
	if !isSHA256Hex(storedHash) {
		return ErrMfaRecoveryCodeInvalid
	}
	if subtle.ConstantTimeCompare([]byte(codeHash), []byte(storedHash)) != 1 {
		return ErrMfaRecoveryCodeInvalid
	}
	return nil
}

// VerifyMfaRecoveryCode verifies a submitted code against unused persisted
// metadata. Used codes are rejected before hash comparison.
func VerifyMfaRecoveryCode(meta MfaRecoveryCodeMetadata, code string) error {
	if err := ValidateMfaRecoveryCodeMetadata(meta); err != nil {
		return err
	}
	if meta.Used() {
		return ErrMfaRecoveryCodeUsed
	}
	return VerifyMfaRecoveryCodeHash(code, meta.CodeHash)
}

// UseMfaRecoveryCode verifies a submitted code and returns metadata with
// UsedAt populated. Adapters must persist this update atomically so two
// concurrent submissions cannot consume the same code twice.
func UseMfaRecoveryCode(meta MfaRecoveryCodeMetadata, code string, now time.Time) (MfaRecoveryCodeMetadata, error) {
	if err := VerifyMfaRecoveryCode(meta, code); err != nil {
		return MfaRecoveryCodeMetadata{}, err
	}
	next := meta
	next.UsedAt = normalizeMfaRecoveryCodeTime(now)
	if err := ValidateMfaRecoveryCodeMetadata(next); err != nil {
		return MfaRecoveryCodeMetadata{}, err
	}
	return next, nil
}

// Validate reports whether metadata is complete and internally consistent.
func (m MfaRecoveryCodeMetadata) Validate() error {
	return ValidateMfaRecoveryCodeMetadata(m)
}

// Used reports whether the recovery code has already been consumed.
func (m MfaRecoveryCodeMetadata) Used() bool {
	return !m.UsedAt.IsZero()
}

// IsUsed is an alias for Used.
func (m MfaRecoveryCodeMetadata) IsUsed() bool {
	return m.Used()
}

// Verify checks a submitted code against this unused metadata.
func (m MfaRecoveryCodeMetadata) Verify(code string) error {
	return VerifyMfaRecoveryCode(m, code)
}

// Use verifies a submitted code and returns metadata with UsedAt populated.
func (m MfaRecoveryCodeMetadata) Use(code string, now time.Time) (MfaRecoveryCodeMetadata, error) {
	return UseMfaRecoveryCode(m, code, now)
}

// ValidateMfaRecoveryCodeMetadata reports malformed recovery code metadata
// before an adapter persists or consumes it.
func ValidateMfaRecoveryCodeMetadata(m MfaRecoveryCodeMetadata) error {
	var errs []error
	if err := validateMfaRecoveryCodeIdentifier("recovery code ID", m.ID); err != nil {
		errs = append(errs, fmt.Errorf("%w: %v", ErrMfaRecoveryCodeInvalid, err))
	}
	if m.IdentityID <= 0 {
		errs = append(errs, fmt.Errorf("%w: IdentityID is required", ErrMfaRecoveryCodeInvalid))
	}
	if codeHash := strings.TrimSpace(m.CodeHash); !isSHA256Hex(codeHash) {
		errs = append(errs, fmt.Errorf("%w: code hash must be SHA-256 hex", ErrMfaRecoveryCodeInvalid))
	}
	if m.CreatedAt.IsZero() {
		errs = append(errs, fmt.Errorf("%w: CreatedAt is required", ErrMfaRecoveryCodeInvalid))
	}
	if !m.UsedAt.IsZero() && !m.CreatedAt.IsZero() && m.UsedAt.Before(m.CreatedAt) {
		errs = append(errs, fmt.Errorf("%w: UsedAt must not be before CreatedAt", ErrMfaRecoveryCodeInvalid))
	}
	return errors.Join(errs...)
}

func randomMfaRecoveryCode() (string, error) {
	buf := make([]byte, mfaRecoveryCodeRandomBytes)
	if _, err := rand.Read(buf); err != nil {
		return "", err
	}
	return formatMfaRecoveryCode(mfaRecoveryCodeEncoding.EncodeToString(buf)), nil
}

func normalizeMfaRecoveryCode(code string) (string, error) {
	var b strings.Builder
	b.Grow(mfaRecoveryCodeEncodedLength)
	for _, r := range strings.TrimSpace(code) {
		switch {
		case r == '-' || unicode.IsSpace(r):
			continue
		case r >= 'a' && r <= 'z':
			r -= 'a' - 'A'
		}
		if !validMfaRecoveryCodeRune(r) {
			return "", ErrMfaRecoveryCodeInvalid
		}
		b.WriteRune(r)
	}
	normalized := b.String()
	if len(normalized) != mfaRecoveryCodeEncodedLength {
		return "", ErrMfaRecoveryCodeInvalid
	}
	return normalized, nil
}

func validMfaRecoveryCodeRune(r rune) bool {
	return (r >= 'A' && r <= 'Z') || (r >= '2' && r <= '7')
}

func formatMfaRecoveryCode(encoded string) string {
	var b strings.Builder
	b.Grow(len(encoded) + (len(encoded)-1)/mfaRecoveryCodeGroupSize)
	for i := 0; i < len(encoded); i++ {
		if i > 0 && i%mfaRecoveryCodeGroupSize == 0 {
			b.WriteByte('-')
		}
		b.WriteByte(encoded[i])
	}
	return b.String()
}

func normalizeMfaRecoveryCodeTime(t time.Time) time.Time {
	if t.IsZero() {
		return time.Now().UTC()
	}
	return t.UTC()
}

func validateMfaRecoveryCodeIdentifier(kind, value string) error {
	value = strings.TrimSpace(value)
	if value == "" {
		return fmt.Errorf("%s is required", kind)
	}
	if len(value) > maxMfaRecoveryCodeIDLength {
		return fmt.Errorf("%s exceeds %d bytes", kind, maxMfaRecoveryCodeIDLength)
	}
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			return fmt.Errorf("%s contains a control character", kind)
		}
	}
	return nil
}
