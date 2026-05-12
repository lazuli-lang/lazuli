package email

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"time"
)

var errPreviewStoreMissing = errors.New("email: preview store is nil")

// PreviewMessage is the email payload captured by preview stores.
//
// ID and CreatedAt are preview metadata added by the store when omitted.
type PreviewMessage struct {
	ID        string
	From      string
	To        string
	Subject   string
	HTMLBody  string
	TextBody  string
	CreatedAt time.Time
}

// PreviewStore stores captured email messages for local development previews.
type PreviewStore interface {
	// Save stores a message and returns the stored copy. Stores may add preview
	// metadata such as ID and CreatedAt when the caller leaves them empty.
	Save(ctx context.Context, message PreviewMessage) (PreviewMessage, error)
	// List returns messages in store-defined order.
	List(ctx context.Context) ([]PreviewMessage, error)
	// Get returns the message for id and whether it was found.
	Get(ctx context.Context, id string) (PreviewMessage, bool, error)
	// Delete removes the message for id and reports whether it existed.
	Delete(ctx context.Context, id string) (bool, error)
}

// MemoryPreviewStore is an in-memory PreviewStore safe for concurrent use.
//
// The zero value is ready to use. List returns messages in insertion order.
type MemoryPreviewStore struct {
	mu       sync.RWMutex
	next     uint64
	messages []PreviewMessage
	index    map[string]int
	now      func() time.Time
}

// NewMemoryPreviewStore returns an empty in-memory preview store.
func NewMemoryPreviewStore() *MemoryPreviewStore {
	return &MemoryPreviewStore{
		index: make(map[string]int),
	}
}

// Save stores message and returns the stored copy. If message.ID is empty,
// Save assigns a store-local ID. If message.CreatedAt is zero, Save assigns the
// current time. Saving a message with an existing ID replaces that message in
// place while preserving list order.
func (s *MemoryPreviewStore) Save(ctx context.Context, message PreviewMessage) (PreviewMessage, error) {
	if err := previewContextErr(ctx); err != nil {
		return PreviewMessage{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensureLocked()

	if message.ID == "" {
		message.ID = s.nextIDLocked()
	}
	if message.CreatedAt.IsZero() {
		message.CreatedAt = s.nowLocked()
	}

	if idx, ok := s.index[message.ID]; ok {
		s.messages[idx] = message
		return message, nil
	}

	s.index[message.ID] = len(s.messages)
	s.messages = append(s.messages, message)
	return message, nil
}

// List returns a snapshot of captured messages in insertion order.
func (s *MemoryPreviewStore) List(ctx context.Context) ([]PreviewMessage, error) {
	if err := previewContextErr(ctx); err != nil {
		return nil, err
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	out := make([]PreviewMessage, len(s.messages))
	copy(out, s.messages)
	return out, nil
}

// Get returns the captured message for id and whether it was found.
func (s *MemoryPreviewStore) Get(ctx context.Context, id string) (PreviewMessage, bool, error) {
	if err := previewContextErr(ctx); err != nil {
		return PreviewMessage{}, false, err
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	idx, ok := s.index[id]
	if !ok {
		return PreviewMessage{}, false, nil
	}
	return s.messages[idx], true, nil
}

// Delete removes the captured message for id and reports whether it existed.
func (s *MemoryPreviewStore) Delete(ctx context.Context, id string) (bool, error) {
	if err := previewContextErr(ctx); err != nil {
		return false, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	idx, ok := s.index[id]
	if !ok {
		return false, nil
	}

	copy(s.messages[idx:], s.messages[idx+1:])
	s.messages[len(s.messages)-1] = PreviewMessage{}
	s.messages = s.messages[:len(s.messages)-1]
	delete(s.index, id)
	for i := idx; i < len(s.messages); i++ {
		s.index[s.messages[i].ID] = i
	}
	return true, nil
}

func (s *MemoryPreviewStore) ensureLocked() {
	if s.index == nil {
		s.index = make(map[string]int)
	}
}

func (s *MemoryPreviewStore) nextIDLocked() string {
	for {
		s.next++
		id := "preview-" + strconv.FormatUint(s.next, 10)
		if _, exists := s.index[id]; !exists {
			return id
		}
	}
}

func (s *MemoryPreviewStore) nowLocked() time.Time {
	if s.now != nil {
		return s.now().UTC()
	}
	return time.Now().UTC()
}

func previewContextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
		return nil
	}
}

// SandboxDispatcher captures email messages in a PreviewStore instead of
// sending them to an email provider.
type SandboxDispatcher struct {
	// From is copied onto each captured message when set.
	From string
	// Store receives captured messages.
	Store PreviewStore
}

// NewSandboxDispatcher returns a dispatcher backed by store. When store is
// nil, it creates a MemoryPreviewStore.
func NewSandboxDispatcher(store PreviewStore) *SandboxDispatcher {
	if store == nil {
		store = NewMemoryPreviewStore()
	}
	return &SandboxDispatcher{Store: store}
}

// Send captures a single email message in the dispatcher's PreviewStore.
func (d *SandboxDispatcher) Send(ctx context.Context, to, subject, htmlBody, textBody string) error {
	if d == nil || d.Store == nil {
		return errPreviewStoreMissing
	}
	_, err := d.Store.Save(ctx, PreviewMessage{
		From:     d.From,
		To:       to,
		Subject:  subject,
		HTMLBody: htmlBody,
		TextBody: textBody,
	})
	return err
}
