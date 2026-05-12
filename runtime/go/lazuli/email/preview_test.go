package email

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"testing"
)

func TestSandboxDispatcherCapturesMessages(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryPreviewStore()
	dispatcher := &SandboxDispatcher{
		From:  "Dev <dev@example.com>",
		Store: store,
	}

	if err := dispatcher.Send(ctx, "user@example.com", "Welcome", "<p>Hello</p>", "Hello"); err != nil {
		t.Fatalf("Send: %v", err)
	}

	messages, err := store.List(ctx)
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(messages) != 1 {
		t.Fatalf("len(messages) = %d, want 1", len(messages))
	}

	got := messages[0]
	if got.ID == "" {
		t.Fatalf("ID is empty")
	}
	if got.CreatedAt.IsZero() {
		t.Fatalf("CreatedAt is zero")
	}
	if got.From != "Dev <dev@example.com>" ||
		got.To != "user@example.com" ||
		got.Subject != "Welcome" ||
		got.HTMLBody != "<p>Hello</p>" ||
		got.TextBody != "Hello" {
		t.Fatalf("captured message = %+v", got)
	}

	byID, ok, err := store.Get(ctx, got.ID)
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if !ok {
		t.Fatalf("Get(%q) not found", got.ID)
	}
	if byID != got {
		t.Fatalf("Get(%q) = %+v, want %+v", got.ID, byID, got)
	}
}

func TestMemoryPreviewStoreDelete(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryPreviewStore()
	first, err := store.Save(ctx, Message{To: "first@example.com", Subject: "First"})
	if err != nil {
		t.Fatalf("Save first: %v", err)
	}
	second, err := store.Save(ctx, Message{To: "second@example.com", Subject: "Second"})
	if err != nil {
		t.Fatalf("Save second: %v", err)
	}

	deleted, err := store.Delete(ctx, first.ID)
	if err != nil {
		t.Fatalf("Delete: %v", err)
	}
	if !deleted {
		t.Fatalf("Delete(%q) = false, want true", first.ID)
	}

	if _, ok, err := store.Get(ctx, first.ID); err != nil {
		t.Fatalf("Get deleted: %v", err)
	} else if ok {
		t.Fatalf("Get(%q) found deleted message", first.ID)
	}

	messages, err := store.List(ctx)
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(messages) != 1 || messages[0].ID != second.ID {
		t.Fatalf("messages = %+v, want only second message", messages)
	}

	deleted, err = store.Delete(ctx, "missing")
	if err != nil {
		t.Fatalf("Delete missing: %v", err)
	}
	if deleted {
		t.Fatalf("Delete missing = true, want false")
	}
}

func TestMemoryPreviewStoreZeroValue(t *testing.T) {
	ctx := context.Background()
	var store MemoryPreviewStore

	saved, err := store.Save(ctx, Message{To: "user@example.com", Subject: "Zero"})
	if err != nil {
		t.Fatalf("Save: %v", err)
	}
	if saved.ID == "" {
		t.Fatalf("ID is empty")
	}

	messages, err := store.List(ctx)
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(messages) != 1 || messages[0].ID != saved.ID {
		t.Fatalf("messages = %+v, want saved message", messages)
	}
}

func TestSandboxDispatcherHonorsCanceledContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	store := NewMemoryPreviewStore()
	dispatcher := &SandboxDispatcher{Store: store}

	err := dispatcher.Send(ctx, "user@example.com", "Canceled", "<p>body</p>", "body")
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Send error = %v, want context.Canceled", err)
	}

	messages, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(messages) != 0 {
		t.Fatalf("len(messages) = %d, want 0", len(messages))
	}
}

func TestMemoryPreviewStoreConcurrentAccess(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryPreviewStore()
	dispatcher := &SandboxDispatcher{Store: store}
	seed, err := store.Save(ctx, Message{To: "seed@example.com", Subject: "Seed"})
	if err != nil {
		t.Fatalf("Save seed: %v", err)
	}

	const workers = 64
	errs := make(chan error, workers*4)
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		i := i
		wg.Add(1)
		go func() {
			defer wg.Done()

			errs <- dispatcher.Send(
				ctx,
				fmt.Sprintf("user-%d@example.com", i),
				fmt.Sprintf("Subject %d", i),
				"<p>body</p>",
				"body",
			)
			if _, ok, err := store.Get(ctx, seed.ID); err != nil {
				errs <- err
			} else if !ok {
				errs <- fmt.Errorf("seed message not found")
			} else {
				errs <- nil
			}
			_, err := store.List(ctx)
			errs <- err
			_, err = store.Delete(ctx, fmt.Sprintf("missing-%d", i))
			errs <- err
		}()
	}
	wg.Wait()
	close(errs)

	for err := range errs {
		if err != nil {
			t.Fatalf("concurrent operation: %v", err)
		}
	}

	messages, err := store.List(ctx)
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(messages) != workers+1 {
		t.Fatalf("len(messages) = %d, want %d", len(messages), workers+1)
	}

	seen := make(map[string]bool, len(messages))
	for _, message := range messages {
		if message.ID == "" {
			t.Fatalf("empty ID in message %+v", message)
		}
		if seen[message.ID] {
			t.Fatalf("duplicate ID %q", message.ID)
		}
		seen[message.ID] = true
	}
}
