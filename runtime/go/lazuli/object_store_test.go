package lazuli

import (
	"context"
	"errors"
	"io"
	"iter"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

// stubProvider implements `storage.Provider` so the facade resolver
// has something to type-assert against. The implementation is
// deliberately thin — the facade's job is the registry lookup +
// interface assertion, not the underlying mechanics.
type stubProvider struct{}

func (stubProvider) PutObject(_ context.Context, _, _, _ string, _ io.Reader) error {
	return nil
}

func (stubProvider) GetObject(_ context.Context, _, _ string) (io.ReadCloser, string, error) {
	return nil, "", nil
}

func (stubProvider) DeleteObject(_ context.Context, _, _ string) error { return nil }

func (stubProvider) ListPrefix(_ context.Context, _, _ string) iter.Seq2[storage.ObjectMeta, error] {
	return func(_ func(storage.ObjectMeta, error) bool) {}
}

func (stubProvider) PresignedURL(_ context.Context, _, _ string, _ time.Duration, _ string) (string, error) {
	return "stub://signed", nil
}

func TestObjectStoreResolvesRegisteredBinding(t *testing.T) {
	RegisterAppIntegration("object_store_facade_smoke", stubProvider{})
	got, err := ObjectStore("object_store_facade_smoke")
	if err != nil {
		t.Fatalf("ObjectStore: %v", err)
	}
	if got == nil {
		t.Fatal("ObjectStore: nil provider")
	}
	url, err := got.PresignedURL(context.Background(), "media", "k", time.Hour, "GET")
	if err != nil {
		t.Fatalf("PresignedURL: %v", err)
	}
	if url != "stub://signed" {
		t.Errorf("PresignedURL url: %q", url)
	}
}

func TestObjectStoreMissingBindingReturnsTypedError(t *testing.T) {
	_, err := ObjectStore("never_registered_object_store_test")
	if !errors.Is(err, ErrAppIntegrationMissing) {
		t.Errorf("ObjectStore: want ErrAppIntegrationMissing, got %v", err)
	}
}

// wrongTypeIntegration is intentionally not a `storage.Provider` — the
// facade resolver must surface a typed error when the registered
// adapter doesn't satisfy the contract.
type wrongTypeIntegration struct{}

func TestObjectStoreWrongTypeReturnsError(t *testing.T) {
	RegisterAppIntegration("wrong_type_test", wrongTypeIntegration{})
	_, err := ObjectStore("wrong_type_test")
	if err == nil {
		t.Fatal("ObjectStore: want type-assertion error, got nil")
	}
	// The error message should mention `storage.Provider` so the
	// operator can correct the binding (typically a missing plugin
	// import).
	if got := err.Error(); !contains(got, "storage.Provider") {
		t.Errorf("error mentions storage.Provider, got %q", got)
	}
}
