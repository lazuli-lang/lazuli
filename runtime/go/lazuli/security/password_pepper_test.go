package security

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"testing"

	"lazuli.dev/runtime/lazuli/secret"
)

func TestResolvePasswordPepperEnv(t *testing.T) {
	t.Setenv("LAZULI_PASSWORD_PEPPER", "env-pepper-secret")

	pepper, err := ResolvePasswordPepperEnv(context.Background(), "pepper-v1", "env.LAZULI_PASSWORD_PEPPER")
	if err != nil {
		t.Fatalf("ResolvePasswordPepperEnv() error = %v", err)
	}
	if pepper.ID != "pepper-v1" {
		t.Fatalf("pepper ID = %q, want pepper-v1", pepper.ID)
	}

	got, err := ApplyPasswordPepperString("password", pepper)
	if err != nil {
		t.Fatalf("ApplyPasswordPepperString() error = %v", err)
	}
	want := expectedPepperedString("password", []byte("env-pepper-secret"))
	if got != want {
		t.Fatalf("peppered password = %q, want %q", got, want)
	}
}

func TestResolvePasswordPepperRingFromSecretStore(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	store := secret.NewMemoryStore()
	current := PasswordPepperSecret("pepper-v2", secret.Ref("auth.password_pepper"))
	previous := PasswordPepperSecret("pepper-v1", secret.Ref("auth.password_pepper"))

	mustPutSecret(t, store, current.Ref, "new-secret")
	mustPutSecret(t, store, previous.Ref, "old-secret")

	ring, err := ResolvePasswordPepperRing(ctx, store, current, previous)
	if err != nil {
		t.Fatalf("ResolvePasswordPepperRing() error = %v", err)
	}
	if ring.Current.ID != "pepper-v2" {
		t.Fatalf("current pepper ID = %q, want pepper-v2", ring.Current.ID)
	}
	if len(ring.Previous) != 1 || ring.Previous[0].ID != "pepper-v1" {
		t.Fatalf("previous peppers = %+v, want pepper-v1", ring.Previous)
	}

	currentPeppered, err := ApplyPasswordPepperString("password", ring.Current)
	if err != nil {
		t.Fatalf("Apply current pepper error = %v", err)
	}
	if currentPeppered != expectedPepperedString("password", []byte("new-secret")) {
		t.Fatalf("current peppered password = %q, want new-secret HMAC", currentPeppered)
	}

	previousPeppered, err := ApplyPasswordPepperString("password", ring.Previous[0])
	if err != nil {
		t.Fatalf("Apply previous pepper error = %v", err)
	}
	if previousPeppered != expectedPepperedString("password", []byte("old-secret")) {
		t.Fatalf("previous peppered password = %q, want old-secret HMAC", previousPeppered)
	}
}

func TestPasswordPepperSecretReturnsVersionedSource(t *testing.T) {
	t.Parallel()
	source := PasswordPepperSecret("pepper-v3", secret.Ref("auth.password_pepper"))

	if source.ID != "pepper-v3" {
		t.Fatalf("source ID = %q, want pepper-v3", source.ID)
	}
	if source.Ref.Version != secret.VersionLabel("pepper-v3") {
		t.Fatalf("source version = %q, want pepper-v3", source.Ref.Version)
	}
}

func TestApplyPasswordPepperUsesHMACSHA256(t *testing.T) {
	t.Parallel()
	pepper := mustPasswordPepper(t, "pepper-v1", "pepper-secret")
	password := []byte("correct horse battery staple")

	got, err := ApplyPasswordPepper(password, pepper)
	if err != nil {
		t.Fatalf("ApplyPasswordPepper() error = %v", err)
	}
	if len(got) != sha256.Size {
		t.Fatalf("peppered length = %d, want %d", len(got), sha256.Size)
	}
	if bytes.Equal(got, password) {
		t.Fatalf("peppered password must not equal plaintext")
	}

	wantMAC := hmac.New(sha256.New, []byte("pepper-secret"))
	wantMAC.Write(password)
	if !bytes.Equal(got, wantMAC.Sum(nil)) {
		t.Fatalf("peppered password does not match expected HMAC-SHA256")
	}

	gotString, err := ApplyPasswordPepperString(string(password), pepper)
	if err != nil {
		t.Fatalf("ApplyPasswordPepperString() error = %v", err)
	}
	if gotString != base64.RawStdEncoding.EncodeToString(got) {
		t.Fatalf("string peppered password = %q, want base64 raw std encoding", gotString)
	}
}

func TestVerifyPasswordStringWithPepperRotationMatchesCurrent(t *testing.T) {
	t.Parallel()
	current := mustPasswordPepper(t, "pepper-v2", "new-secret")
	previous := mustPasswordPepper(t, "pepper-v1", "old-secret")
	ring := mustPasswordPepperRing(t, current, previous)
	stored := expectedPepperedString("password", []byte("new-secret"))

	match, err := VerifyPasswordStringWithPepperRotation("password", ring, func(peppered string) (bool, error) {
		return peppered == stored, nil
	})
	if err != nil {
		t.Fatalf("VerifyPasswordStringWithPepperRotation() error = %v", err)
	}
	if match.PepperID != "pepper-v2" {
		t.Fatalf("matched pepper ID = %q, want pepper-v2", match.PepperID)
	}
	if match.NeedsRotation {
		t.Fatalf("NeedsRotation = true, want false")
	}
}

func TestVerifyPasswordStringWithPepperRotationMatchesPrevious(t *testing.T) {
	t.Parallel()
	current := mustPasswordPepper(t, "pepper-v2", "new-secret")
	previous := mustPasswordPepper(t, "pepper-v1", "old-secret")
	ring := mustPasswordPepperRing(t, current, previous)
	stored := expectedPepperedString("password", []byte("old-secret"))
	var attempts []string

	match, err := VerifyPasswordStringWithPepperRotation("password", ring, func(peppered string) (bool, error) {
		attempts = append(attempts, peppered)
		return peppered == stored, nil
	})
	if err != nil {
		t.Fatalf("VerifyPasswordStringWithPepperRotation() error = %v", err)
	}
	if match.PepperID != "pepper-v1" {
		t.Fatalf("matched pepper ID = %q, want pepper-v1", match.PepperID)
	}
	if !match.NeedsRotation {
		t.Fatalf("NeedsRotation = false, want true")
	}
	if len(attempts) != 2 {
		t.Fatalf("attempts = %d, want current then previous", len(attempts))
	}
	if attempts[0] != expectedPepperedString("password", []byte("new-secret")) {
		t.Fatalf("first attempt did not use current pepper")
	}
}

func TestVerifyPasswordWithPepperRotationPropagatesVerifierError(t *testing.T) {
	t.Parallel()
	ring := mustPasswordPepperRing(t, mustPasswordPepper(t, "pepper-v1", "secret"))
	wantErr := errors.New("hash malformed")

	_, err := VerifyPasswordWithPepperRotation([]byte("password"), ring, func([]byte) (bool, error) {
		return false, wantErr
	})
	if !errors.Is(err, wantErr) {
		t.Fatalf("VerifyPasswordWithPepperRotation() error = %v, want %v", err, wantErr)
	}
}

func TestVerifyPasswordWithPepperRotationReturnsMismatch(t *testing.T) {
	t.Parallel()
	ring := mustPasswordPepperRing(t, mustPasswordPepper(t, "pepper-v1", "secret"))

	_, err := VerifyPasswordWithPepperRotation([]byte("password"), ring, func([]byte) (bool, error) {
		return false, nil
	})
	if !errors.Is(err, ErrPasswordPepperMismatch) {
		t.Fatalf("VerifyPasswordWithPepperRotation() error = %v, want ErrPasswordPepperMismatch", err)
	}
}

func TestPasswordPepperValidation(t *testing.T) {
	t.Parallel()
	if _, err := NewPasswordPepper("", []byte("secret")); !errors.Is(err, ErrInvalidPasswordPepper) {
		t.Fatalf("NewPasswordPepper() empty id error = %v, want ErrInvalidPasswordPepper", err)
	}
	if _, err := NewPasswordPepper("pepper-v1", nil); !errors.Is(err, ErrInvalidPasswordPepper) {
		t.Fatalf("NewPasswordPepper() empty secret error = %v, want ErrInvalidPasswordPepper", err)
	}

	current := mustPasswordPepper(t, "pepper-v1", "secret")
	duplicate := mustPasswordPepper(t, "pepper-v1", "other-secret")
	if _, err := NewPasswordPepperRing(current, duplicate); !errors.Is(err, ErrInvalidPasswordPepper) {
		t.Fatalf("NewPasswordPepperRing() duplicate error = %v, want ErrInvalidPasswordPepper", err)
	}

	if _, err := ResolvePasswordPepper(context.Background(), nil, PasswordPepperSource{}); !errors.Is(err, ErrPasswordPepperStoreRequired) {
		t.Fatalf("ResolvePasswordPepper() nil store error = %v, want ErrPasswordPepperStoreRequired", err)
	}
	if _, err := VerifyPasswordWithPepperRotation([]byte("password"), mustPasswordPepperRing(t, current), nil); !errors.Is(err, ErrPasswordPepperVerifierRequired) {
		t.Fatalf("VerifyPasswordWithPepperRotation() nil verifier error = %v, want ErrPasswordPepperVerifierRequired", err)
	}
}

func TestPasswordPepperSecretReturnsDefensiveCopy(t *testing.T) {
	t.Parallel()
	pepper := mustPasswordPepper(t, "pepper-v1", "secret")
	copy := pepper.Secret()
	copy[0] = 'S'

	got, err := ApplyPasswordPepperString("password", pepper)
	if err != nil {
		t.Fatalf("ApplyPasswordPepperString() error = %v", err)
	}
	want := expectedPepperedString("password", []byte("secret"))
	if got != want {
		t.Fatalf("peppered password = %q, want original secret HMAC", got)
	}
}

func expectedPepperedString(password string, key []byte) string {
	mac := hmac.New(sha256.New, key)
	mac.Write([]byte(password))
	return base64.RawStdEncoding.EncodeToString(mac.Sum(nil))
}

func mustPasswordPepper(t *testing.T, id, value string) PasswordPepper {
	t.Helper()
	pepper, err := NewPasswordPepper(id, []byte(value))
	if err != nil {
		t.Fatalf("NewPasswordPepper() error = %v", err)
	}
	return pepper
}

func mustPasswordPepperRing(t *testing.T, current PasswordPepper, previous ...PasswordPepper) PasswordPepperRing {
	t.Helper()
	ring, err := NewPasswordPepperRing(current, previous...)
	if err != nil {
		t.Fatalf("NewPasswordPepperRing() error = %v", err)
	}
	return ring
}

func mustPutSecret(t *testing.T, store *secret.MemoryStore, ref secret.SecretRef, value string) {
	t.Helper()
	if err := store.Put(context.Background(), ref, []byte(value)); err != nil {
		t.Fatalf("Put(%+v) error = %v", ref, err)
	}
}
