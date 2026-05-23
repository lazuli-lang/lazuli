package lazuli

import "testing"

func TestRegisterAdapterRejectsDuplicates(t *testing.T) {
	defer func() {
		if r := recover(); r == nil {
			t.Fatal("expected panic on duplicate registration")
		}
	}()
	// Use a unique ref to avoid colliding with real plugins.
	const ref = "@lazuli/plugin-test-duplicate-c3"
	RegisterAdapter(ref, struct{}{})
	RegisterAdapter(ref, struct{}{}) // should panic
}

func TestRegisterAdapterRejectsAfterLock(t *testing.T) {
	defer func() {
		// Unlock for subsequent tests.
		adapterMu.Lock()
		adapterBootLocked = false
		adapterMu.Unlock()
		if r := recover(); r == nil {
			t.Fatal("expected panic on post-lock registration")
		}
	}()
	LockAdapterRegistry()
	RegisterAdapter("@lazuli/plugin-test-after-lock-c3", struct{}{})
}
