package secret_test

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"testing"

	"lazuli.dev/runtime/lazuli/secret"
)

func TestMemoryStoreResolveReturnsDefensiveCopies(t *testing.T) {
	store := secret.NewMemoryStore()
	ref := secret.Ref("payments.api_key")
	value := []byte("secret-value")

	if err := store.Put(context.Background(), ref, value); err != nil {
		t.Fatalf("Put() error = %v", err)
	}
	value[0] = 'S'

	got, err := store.Resolve(context.Background(), ref)
	if err != nil {
		t.Fatalf("Resolve() error = %v", err)
	}
	if string(got) != "secret-value" {
		t.Fatalf("Resolve() = %q, want secret-value", got)
	}

	got[0] = 'S'
	got, err = store.Resolve(context.Background(), ref)
	if err != nil {
		t.Fatalf("Resolve() after mutation error = %v", err)
	}
	if string(got) != "secret-value" {
		t.Fatalf("Resolve() after mutation = %q, want secret-value", got)
	}
}

func TestMemoryStoreVersionLabelsAreDistinct(t *testing.T) {
	store := secret.NewMemoryStore()
	current := secret.Ref("oauth.client_secret").WithVersion("current")
	previous := secret.Ref("oauth.client_secret").WithVersion("previous")

	mustPut(t, store, current, "current-secret")
	mustPut(t, store, previous, "previous-secret")

	assertSecret(t, store, current, "current-secret")
	assertSecret(t, store, previous, "previous-secret")

	if _, err := store.Resolve(context.Background(), secret.Ref("oauth.client_secret")); !errors.Is(err, secret.ErrNotFound) {
		t.Fatalf("Resolve() unversioned error = %v, want ErrNotFound", err)
	}
}

func TestMemoryStoreDelete(t *testing.T) {
	store := secret.NewMemoryStore()
	ref := secret.Ref("webhook.hmac")
	mustPut(t, store, ref, "secret")

	if err := store.Delete(context.Background(), ref); err != nil {
		t.Fatalf("Delete() error = %v", err)
	}
	if _, err := store.Resolve(context.Background(), ref); !errors.Is(err, secret.ErrNotFound) {
		t.Fatalf("Resolve() after Delete error = %v, want ErrNotFound", err)
	}
}

func TestMemoryStoreZeroValueIsUsable(t *testing.T) {
	var store secret.MemoryStore
	ref := secret.Ref("local.secret")

	if err := store.Put(context.Background(), ref, []byte("value")); err != nil {
		t.Fatalf("zero-value Put() error = %v", err)
	}
	assertSecret(t, &store, ref, "value")
}

func TestEnvStoreResolve(t *testing.T) {
	t.Setenv("LAZULI_TEST_SECRET", "env-secret")

	got, err := secret.ResolveEnv(context.Background(), secret.Env("env.LAZULI_TEST_SECRET"))
	if err != nil {
		t.Fatalf("ResolveEnv() error = %v", err)
	}
	if string(got) != "env-secret" {
		t.Fatalf("ResolveEnv() = %q, want env-secret", got)
	}
}

func TestEnvStoreUsesInjectedLookup(t *testing.T) {
	store := secret.EnvStore{
		LookupEnv: func(name string) (string, bool) {
			if name != "API_TOKEN" {
				t.Fatalf("LookupEnv name = %q, want API_TOKEN", name)
			}
			return "token", true
		},
	}

	got, err := store.Resolve(context.Background(), secret.Env("env.API_TOKEN").WithVersion("current"))
	if err != nil {
		t.Fatalf("Resolve() error = %v", err)
	}
	if string(got) != "token" {
		t.Fatalf("Resolve() = %q, want token", got)
	}
}

func TestStoresReturnContextAndReferenceErrors(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	store := secret.NewMemoryStore()
	if err := store.Put(ctx, secret.Ref("api.key"), []byte("value")); !errors.Is(err, context.Canceled) {
		t.Fatalf("Put() canceled error = %v, want context.Canceled", err)
	}
	if _, err := store.Resolve(ctx, secret.Ref("api.key")); !errors.Is(err, context.Canceled) {
		t.Fatalf("Resolve() canceled error = %v, want context.Canceled", err)
	}
	if _, err := secret.ResolveEnv(ctx, secret.Env("API_KEY")); !errors.Is(err, context.Canceled) {
		t.Fatalf("ResolveEnv() canceled error = %v, want context.Canceled", err)
	}

	if err := store.Put(context.Background(), secret.SecretRef{}, []byte("value")); !errors.Is(err, secret.ErrInvalidRef) {
		t.Fatalf("Put() invalid ref error = %v, want ErrInvalidRef", err)
	}
	if _, err := secret.ResolveEnv(context.Background(), secret.SecretRef{}); !errors.Is(err, secret.ErrInvalidRef) {
		t.Fatalf("ResolveEnv() invalid ref error = %v, want ErrInvalidRef", err)
	}
}

func TestMemoryStoreConcurrentAccess(t *testing.T) {
	store := secret.NewMemoryStore()
	ctx := context.Background()

	var wg sync.WaitGroup
	for i := range 32 {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			label := secret.VersionLabel("v" + strconv.Itoa(i))
			ref := secret.Ref("shared.secret").WithVersion(label)
			want := "payload-" + strconv.Itoa(i)
			for range 100 {
				if err := store.Put(ctx, ref, []byte(want)); err != nil {
					t.Errorf("Put() error = %v", err)
					return
				}
				got, err := store.Resolve(ctx, ref)
				if err != nil {
					t.Errorf("Resolve() error = %v", err)
					return
				}
				if string(got) != want {
					t.Errorf("Resolve() = %q, want %q", got, want)
					return
				}
			}
		}(i)
	}
	wg.Wait()

	for i := range 32 {
		ref := secret.Ref("shared.secret").WithVersion(secret.VersionLabel("v" + strconv.Itoa(i)))
		assertSecret(t, store, ref, "payload-"+strconv.Itoa(i))
	}
}

func mustPut(t *testing.T, store *secret.MemoryStore, ref secret.SecretRef, value string) {
	t.Helper()
	if err := store.Put(context.Background(), ref, []byte(value)); err != nil {
		t.Fatalf("Put(%+v) error = %v", ref, err)
	}
}

func assertSecret(t *testing.T, store secret.Store, ref secret.SecretRef, want string) {
	t.Helper()

	got, err := store.Resolve(context.Background(), ref)
	if err != nil {
		t.Fatalf("Resolve(%+v) error = %v", ref, err)
	}
	if string(got) != want {
		t.Fatalf("Resolve(%+v) = %q, want %q", ref, got, want)
	}
}
