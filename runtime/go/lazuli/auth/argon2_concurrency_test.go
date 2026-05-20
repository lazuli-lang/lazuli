package auth

import (
	"context"
	"errors"
	"net/http"
	"sync"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
)

// TestArgon2ConcurrencyCap saturates the semaphore with N held slots
// and asserts the (N+1)th hash returns ErrArgon2Saturated when its
// context deadline expires before a slot frees.
func TestArgon2ConcurrencyCap(t *testing.T) {
	SetArgon2Concurrency(2)
	t.Cleanup(func() { SetArgon2Concurrency(defaultArgon2Concurrency) })

	started := make(chan struct{}, 2)
	releaseSlots := make(chan struct{})
	errs := make(chan error, 2)
	var wg sync.WaitGroup
	for i := 0; i < 2; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			release, err := acquireArgon2Slot(context.Background())
			if err != nil {
				errs <- err
				return
			}
			started <- struct{}{}
			<-releaseSlots
			release()
		}()
	}
	<-started
	<-started

	params := Argon2Params{
		Salt:    []byte("teststeststests1"),
		Time:    1,
		Memory:  4 * 1024,
		Threads: 1,
		KeyLen:  32,
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
	defer cancel()
	_, err := HashWithArgon2(ctx, []byte("pw"), params)
	if !errors.Is(err, ErrArgon2Saturated) {
		t.Fatalf("expected ErrArgon2Saturated; got %v", err)
	}
	var lazErr *lazuli.Error
	if !errors.As(err, &lazErr) || lazErr.Status != http.StatusServiceUnavailable {
		t.Fatalf("expected Lazuli 503 error envelope; got %#v", err)
	}

	close(releaseSlots)
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatalf("slot holder failed: %v", err)
		}
	}
}
