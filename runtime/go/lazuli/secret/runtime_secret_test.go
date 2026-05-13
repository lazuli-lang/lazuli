package secret_test

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/secret"
)

func TestRuntimeSecretFormattingAndMarshalAreRedacted(t *testing.T) {
	handle := secret.NewRuntimeSecret(secret.Ref("payments.api_key").WithVersion("v1"), func(context.Context, secret.SecretRef) ([]byte, error) {
		return []byte("super-secret-value"), nil
	})

	text, err := handle.MarshalText()
	if err != nil {
		t.Fatalf("MarshalText() error = %v", err)
	}
	payload, err := json.Marshal(struct {
		Secret secret.RuntimeSecret `json:"secret"`
	}{
		Secret: handle,
	})
	if err != nil {
		t.Fatalf("json.Marshal() error = %v", err)
	}
	if string(payload) != `{"secret":"[REDACTED]"}` {
		t.Fatalf("json.Marshal() = %s, want redacted secret", payload)
	}

	outputs := map[string]string{
		"String":      handle.String(),
		"Sprint":      fmt.Sprint(handle),
		"Sprintf+v":   fmt.Sprintf("%+v", handle),
		"Sprintf#v":   fmt.Sprintf("%#v", handle),
		"MarshalText": string(text),
		"MarshalJSON": string(payload),
	}
	for name, out := range outputs {
		if !strings.Contains(out, secret.RuntimeSecretMask) {
			t.Fatalf("%s = %q, want redaction mask", name, out)
		}
		for _, leaked := range []string{"super-secret-value", "payments.api_key", "v1"} {
			if strings.Contains(out, leaked) {
				t.Fatalf("%s leaked %q in %q", name, leaked, out)
			}
		}
	}
}

func TestRuntimeSecretRevealNormalizesRefAndReturnsDefensiveCopies(t *testing.T) {
	backing := []byte("runtime-secret")
	calls := 0
	handle := secret.NewRuntimeSecret(secret.Env(" env.API_TOKEN ").WithVersion(" active "), func(ctx context.Context, ref secret.SecretRef) ([]byte, error) {
		calls++
		if ref.Name != "API_TOKEN" {
			t.Fatalf("reveal ref.Name = %q, want API_TOKEN", ref.Name)
		}
		if ref.Version != "active" {
			t.Fatalf("reveal ref.Version = %q, want active", ref.Version)
		}
		return backing, nil
	})

	ref := handle.Ref()
	if ref.Name != "API_TOKEN" || ref.Version != "active" {
		t.Fatalf("Ref() = %+v, want normalized API_TOKEN@active", ref)
	}

	got, err := handle.Reveal(context.Background())
	if err != nil {
		t.Fatalf("Reveal() error = %v", err)
	}
	if string(got) != "runtime-secret" {
		t.Fatalf("Reveal() = %q, want runtime-secret", got)
	}
	got[0] = 'R'
	if string(backing) != "runtime-secret" {
		t.Fatalf("Reveal() returned callback buffer without defensive copy: %q", backing)
	}

	gotString, err := handle.RevealString(context.Background())
	if err != nil {
		t.Fatalf("RevealString() error = %v", err)
	}
	if gotString != "runtime-secret" {
		t.Fatalf("RevealString() = %q, want runtime-secret", gotString)
	}
	if calls != 2 {
		t.Fatalf("reveal calls = %d, want 2", calls)
	}
}

func TestRuntimeSecretFromStoreRevealsStoredSecret(t *testing.T) {
	store := secret.NewMemoryStore()
	ref := secret.Ref("db.password").WithVersion("active")
	if err := store.Put(context.Background(), ref, []byte("store-secret")); err != nil {
		t.Fatalf("Put() error = %v", err)
	}

	handle := secret.RuntimeSecretFromStore(secret.SecretRef{Name: " db.password ", Version: " active "}, store)
	got, err := handle.RevealString(context.Background())
	if err != nil {
		t.Fatalf("RevealString() error = %v", err)
	}
	if got != "store-secret" {
		t.Fatalf("RevealString() = %q, want store-secret", got)
	}
}

func TestRuntimeSecretValidationAndRevealErrors(t *testing.T) {
	handle := secret.RuntimeSecretRef(secret.Ref("api.key"))
	if err := secret.ValidateRuntimeSecret(handle); err != nil {
		t.Fatalf("ValidateRuntimeSecret() error = %v", err)
	}
	if _, err := handle.Reveal(context.Background()); !errors.Is(err, secret.ErrRuntimeSecretRevealUnavailable) {
		t.Fatalf("Reveal() without callback error = %v, want ErrRuntimeSecretRevealUnavailable", err)
	}

	invalid := secret.RuntimeSecretRef(secret.SecretRef{Version: "v1"})
	if err := invalid.Validate(); !errors.Is(err, secret.ErrInvalidRef) {
		t.Fatalf("Validate() invalid error = %v, want ErrInvalidRef", err)
	}
	if _, err := invalid.Reveal(context.Background()); !errors.Is(err, secret.ErrInvalidRef) {
		t.Fatalf("Reveal() invalid error = %v, want ErrInvalidRef", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	calls := 0
	canceled := secret.NewRuntimeSecret(secret.Ref("api.key"), func(context.Context, secret.SecretRef) ([]byte, error) {
		calls++
		return []byte("secret"), nil
	})
	if _, err := canceled.Reveal(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("Reveal() canceled error = %v, want context.Canceled", err)
	}
	if calls != 0 {
		t.Fatalf("canceled Reveal() invoked callback %d times, want 0", calls)
	}
}
