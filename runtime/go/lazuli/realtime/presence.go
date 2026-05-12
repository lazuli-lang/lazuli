package realtime

import (
	"context"
	"errors"
	"sort"
	"sync"
	"time"
)

var (
	// ErrPresenceRoomRequired is returned when a presence operation has no room.
	ErrPresenceRoomRequired = errors.New("realtime: presence room required")
	// ErrPresenceUserRequired is returned when a presence operation has no user.
	ErrPresenceUserRequired = errors.New("realtime: presence user required")
	// ErrPresenceTTLRequired is returned when a join or heartbeat TTL is not positive.
	ErrPresenceTTLRequired = errors.New("realtime: presence ttl must be positive")
	// ErrPresenceNotFound is returned when a heartbeat targets no active member.
	ErrPresenceNotFound = errors.New("realtime: presence member not found")
)

// PresenceMember is an active realtime presence record for one user in one room.
type PresenceMember struct {
	// Room is the provider-neutral realtime room name.
	Room string
	// UserID is the application user identity as understood by the caller.
	UserID string
	// JoinedAt is when the current presence session started.
	JoinedAt time.Time
	// LastSeenAt is the last join or heartbeat time.
	LastSeenAt time.Time
	// ExpiresAt is when this member is considered absent unless heartbeated.
	ExpiresAt time.Time
}

// PresenceStore tracks active realtime members independently of any pubsub implementation.
//
// Implementations must be safe for concurrent use.
type PresenceStore interface {
	// Join marks userID present in room for ttl.
	Join(ctx context.Context, room string, userID string, ttl time.Duration) (PresenceMember, error)
	// Heartbeat renews an active member for ttl.
	Heartbeat(ctx context.Context, room string, userID string, ttl time.Duration) (PresenceMember, error)
	// Leave removes userID from room if present.
	Leave(ctx context.Context, room string, userID string) error
	// ListByRoom returns unexpired members in room.
	ListByRoom(ctx context.Context, room string) ([]PresenceMember, error)
	// PruneExpired removes expired members and returns the number deleted.
	PruneExpired(ctx context.Context) (int, error)
}

// MemoryPresenceStore is an in-process PresenceStore for tests and local runtimes.
//
// The zero value is ready to use. It stores only room membership state and has
// no dependency on a realtime transport or pubsub adapter.
type MemoryPresenceStore struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu    sync.Mutex
	rooms map[string]map[string]memoryPresenceMember
}

type memoryPresenceMember struct {
	joinedAt   time.Time
	lastSeenAt time.Time
	expiresAt  time.Time
}

var _ PresenceStore = (*MemoryPresenceStore)(nil)

// NewMemoryPresenceStore returns an empty in-process presence store.
func NewMemoryPresenceStore() *MemoryPresenceStore {
	return &MemoryPresenceStore{
		rooms: make(map[string]map[string]memoryPresenceMember),
	}
}

// Join marks userID present in room for ttl.
func (s *MemoryPresenceStore) Join(
	ctx context.Context,
	room string,
	userID string,
	ttl time.Duration,
) (PresenceMember, error) {
	if err := validatePresenceWrite(ctx, room, userID, ttl); err != nil {
		return PresenceMember{}, err
	}

	now := s.now()
	member := memoryPresenceMember{
		joinedAt:   now,
		lastSeenAt: now,
		expiresAt:  now.Add(ttl),
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	roomMembers := s.rooms[room]
	if roomMembers == nil {
		roomMembers = make(map[string]memoryPresenceMember)
		s.rooms[room] = roomMembers
	}
	if current, ok := roomMembers[userID]; ok && now.Before(current.expiresAt) {
		member.joinedAt = current.joinedAt
	}
	roomMembers[userID] = member
	return presenceSnapshot(room, userID, member), nil
}

// Heartbeat renews an active member for ttl.
func (s *MemoryPresenceStore) Heartbeat(
	ctx context.Context,
	room string,
	userID string,
	ttl time.Duration,
) (PresenceMember, error) {
	if err := validatePresenceWrite(ctx, room, userID, ttl); err != nil {
		return PresenceMember{}, err
	}

	now := s.now()

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	roomMembers := s.rooms[room]
	if roomMembers == nil {
		return PresenceMember{}, ErrPresenceNotFound
	}
	current, ok := roomMembers[userID]
	if !ok {
		return PresenceMember{}, ErrPresenceNotFound
	}
	if !now.Before(current.expiresAt) {
		delete(roomMembers, userID)
		if len(roomMembers) == 0 {
			delete(s.rooms, room)
		}
		return PresenceMember{}, ErrPresenceNotFound
	}

	current.lastSeenAt = now
	current.expiresAt = now.Add(ttl)
	roomMembers[userID] = current
	return presenceSnapshot(room, userID, current), nil
}

// Leave removes userID from room if present.
func (s *MemoryPresenceStore) Leave(ctx context.Context, room string, userID string) error {
	if err := validatePresenceMember(ctx, room, userID); err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	roomMembers := s.rooms[room]
	delete(roomMembers, userID)
	if len(roomMembers) == 0 {
		delete(s.rooms, room)
	}
	return nil
}

// ListByRoom returns unexpired members in room sorted by user ID.
func (s *MemoryPresenceStore) ListByRoom(ctx context.Context, room string) ([]PresenceMember, error) {
	if err := presenceContextErr(ctx); err != nil {
		return nil, err
	}
	if room == "" {
		return nil, ErrPresenceRoomRequired
	}

	now := s.now()

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	s.pruneExpiredLocked(now)

	roomMembers := s.rooms[room]
	members := make([]PresenceMember, 0, len(roomMembers))
	for userID, member := range roomMembers {
		members = append(members, presenceSnapshot(room, userID, member))
	}
	sort.Slice(members, func(i, j int) bool {
		return members[i].UserID < members[j].UserID
	})
	return members, nil
}

// PruneExpired removes expired members and returns the number deleted.
func (s *MemoryPresenceStore) PruneExpired(ctx context.Context) (int, error) {
	if err := presenceContextErr(ctx); err != nil {
		return 0, err
	}

	now := s.now()

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	return s.pruneExpiredLocked(now), nil
}

func (s *MemoryPresenceStore) ensureLocked() {
	if s.rooms == nil {
		s.rooms = make(map[string]map[string]memoryPresenceMember)
	}
}

func (s *MemoryPresenceStore) now() time.Time {
	if s != nil && s.Clock != nil {
		return s.Clock().UTC()
	}
	return time.Now().UTC()
}

func (s *MemoryPresenceStore) pruneExpiredLocked(now time.Time) int {
	var deleted int
	for room, roomMembers := range s.rooms {
		for userID, member := range roomMembers {
			if !now.Before(member.expiresAt) {
				delete(roomMembers, userID)
				deleted++
			}
		}
		if len(roomMembers) == 0 {
			delete(s.rooms, room)
		}
	}
	return deleted
}

func validatePresenceWrite(ctx context.Context, room string, userID string, ttl time.Duration) error {
	if err := validatePresenceMember(ctx, room, userID); err != nil {
		return err
	}
	if ttl <= 0 {
		return ErrPresenceTTLRequired
	}
	return nil
}

func validatePresenceMember(ctx context.Context, room string, userID string) error {
	if err := presenceContextErr(ctx); err != nil {
		return err
	}
	if room == "" {
		return ErrPresenceRoomRequired
	}
	if userID == "" {
		return ErrPresenceUserRequired
	}
	return nil
}

func presenceContextErr(ctx context.Context) error {
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

func presenceSnapshot(room string, userID string, member memoryPresenceMember) PresenceMember {
	return PresenceMember{
		Room:       room,
		UserID:     userID,
		JoinedAt:   member.joinedAt,
		LastSeenAt: member.lastSeenAt,
		ExpiresAt:  member.expiresAt,
	}
}
