package storage

import (
	"errors"
	"fmt"
	"sort"
	"sync"
	"time"
)

// ResumableChunkRange is a half-open byte range recorded for a
// resumable upload session. Start is inclusive and End is exclusive.
type ResumableChunkRange struct {
	Start int64
	End   int64
}

// Length returns the number of bytes covered by the range.
func (r ResumableChunkRange) Length() int64 {
	return r.End - r.Start
}

// ResumableUploadState is the lifecycle state of a resumable upload
// session tracked by MemoryResumableTracker.
type ResumableUploadState int

const (
	// ResumableUploadOpen accepts additional chunk ranges.
	ResumableUploadOpen ResumableUploadState = iota
	// ResumableUploadCompleted has a gap-free range set covering
	// the session's declared size.
	ResumableUploadCompleted
	// ResumableUploadAborted was cancelled and refuses more writes.
	ResumableUploadAborted
)

// String renders the upload state as a stable lowercase token.
func (s ResumableUploadState) String() string {
	switch s {
	case ResumableUploadOpen:
		return "open"
	case ResumableUploadCompleted:
		return "completed"
	case ResumableUploadAborted:
		return "aborted"
	default:
		return "unknown"
	}
}

// ResumableCreateOptions configures a new resumable upload session.
type ResumableCreateOptions struct {
	// ID is optional. When empty, MemoryResumableTracker mints a
	// process-local deterministic ID.
	ID string

	// Key is the eventual object key the caller plans to write when
	// the session completes. The tracker records it but never writes
	// to object storage.
	Key Key

	// Metadata is the upload metadata captured at session creation.
	Metadata Metadata

	// Size is the expected final object size in bytes. When zero and
	// Metadata.Size is positive, Metadata.Size is used.
	Size int64

	// TTL overrides the tracker's default session TTL. Zero uses the
	// default; if both are zero, the session does not expire.
	TTL time.Duration
}

// ResumableUploadSession is a snapshot of tracked upload state.
type ResumableUploadSession struct {
	ID          string
	Key         Key
	Metadata    Metadata
	Size        int64
	State       ResumableUploadState
	Ranges      []ResumableChunkRange
	CreatedAt   time.Time
	UpdatedAt   time.Time
	ExpiresAt   time.Time
	CompletedAt time.Time
	AbortedAt   time.Time
}

// UploadedBytes returns the contiguous byte count recorded from
// offset zero. Later out-of-order ranges after the first gap are not
// counted until the gap is filled.
func (s ResumableUploadSession) UploadedBytes() int64 {
	ranges := cloneRanges(s.Ranges)
	sortRanges(ranges)
	return contiguousEnd(ranges)
}

// MissingRanges returns the gaps between zero and the session's
// expected size. A completed session returns an empty slice.
func (s ResumableUploadSession) MissingRanges() []ResumableChunkRange {
	ranges := cloneRanges(s.Ranges)
	sortRanges(ranges)

	var gaps []ResumableChunkRange
	next := int64(0)
	for _, r := range ranges {
		if r.Start > next {
			gaps = append(gaps, ResumableChunkRange{Start: next, End: r.Start})
		}
		if r.End > next {
			next = r.End
		}
	}
	if next < s.Size {
		gaps = append(gaps, ResumableChunkRange{Start: next, End: s.Size})
	}
	return gaps
}

// MemoryResumableTracker tracks resumable upload sessions in memory.
// It is intended for local development and tests; production object
// storage writes remain the responsibility of provider adapters.
type MemoryResumableTracker struct {
	// Clock returns the current time. Defaults to time.Now.
	Clock func() time.Time

	// DefaultTTL is applied to sessions whose create options omit TTL.
	// A zero value means sessions do not expire by default.
	DefaultTTL time.Duration

	mu       sync.Mutex
	next     uint64
	sessions map[string]ResumableUploadSession
}

// NewMemoryResumableTracker returns an empty in-memory tracker.
func NewMemoryResumableTracker(defaultTTL time.Duration) *MemoryResumableTracker {
	return &MemoryResumableTracker{
		DefaultTTL: defaultTTL,
		sessions:   make(map[string]ResumableUploadSession),
	}
}

// Create starts a new resumable upload session.
func (t *MemoryResumableTracker) Create(opts ResumableCreateOptions) (ResumableUploadSession, error) {
	if opts.Size < 0 || opts.Metadata.Size < 0 || opts.TTL < 0 {
		return ResumableUploadSession{}, ErrResumableSessionInvalid
	}
	size := opts.Size
	if size == 0 && opts.Metadata.Size > 0 {
		size = opts.Metadata.Size
	}
	if opts.Size > 0 && opts.Metadata.Size > 0 && opts.Size != opts.Metadata.Size {
		return ResumableUploadSession{}, ErrResumableSessionInvalid
	}

	metadata := opts.Metadata
	if metadata.Size == 0 {
		metadata.Size = size
	}

	t.mu.Lock()
	defer t.mu.Unlock()
	t.ensureSessionsLocked()

	id := opts.ID
	if id == "" {
		id = t.nextIDLocked()
	} else if _, ok := t.sessions[id]; ok {
		return ResumableUploadSession{}, ErrResumableSessionExists
	}

	now := t.now()
	ttl := opts.TTL
	if ttl == 0 {
		ttl = t.DefaultTTL
	}
	if ttl < 0 {
		return ResumableUploadSession{}, ErrResumableSessionInvalid
	}

	session := ResumableUploadSession{
		ID:        id,
		Key:       opts.Key,
		Metadata:  metadata,
		Size:      size,
		State:     ResumableUploadOpen,
		CreatedAt: now,
		UpdatedAt: now,
	}
	if ttl > 0 {
		session.ExpiresAt = now.Add(ttl)
	}
	t.sessions[id] = session
	return cloneSession(session), nil
}

// Get returns a snapshot of a tracked session.
func (t *MemoryResumableTracker) Get(id string) (ResumableUploadSession, error) {
	t.mu.Lock()
	defer t.mu.Unlock()

	session, err := t.sessionLocked(id)
	if err != nil {
		return ResumableUploadSession{}, err
	}
	return cloneSession(session), nil
}

// AppendRange records the next contiguous range for an open session.
// If range.Start is after the current contiguous upload offset, it
// returns ErrResumableRangeGap.
func (t *MemoryResumableTracker) AppendRange(id string, r ResumableChunkRange) (ResumableUploadSession, error) {
	t.mu.Lock()
	defer t.mu.Unlock()

	session, err := t.mutableSessionLocked(id)
	if err != nil {
		return ResumableUploadSession{}, err
	}
	if err := validateRange(r, session.Size); err != nil {
		return ResumableUploadSession{}, err
	}
	next := contiguousEnd(session.Ranges)
	if r.Start > next {
		return ResumableUploadSession{}, ErrResumableRangeGap
	}
	if r.Start < next {
		return ResumableUploadSession{}, ErrResumableRangeOverlap
	}
	return t.recordRangeLocked(session, r)
}

// RecordRange records a non-overlapping range for an open session.
// Ranges may arrive out of order; gaps are reported by Complete or
// MissingRanges.
func (t *MemoryResumableTracker) RecordRange(id string, r ResumableChunkRange) (ResumableUploadSession, error) {
	t.mu.Lock()
	defer t.mu.Unlock()

	session, err := t.mutableSessionLocked(id)
	if err != nil {
		return ResumableUploadSession{}, err
	}
	return t.recordRangeLocked(session, r)
}

// Complete marks a session complete when its ranges exactly cover
// [0, Size). It returns ErrResumableRangeGap when bytes are missing.
func (t *MemoryResumableTracker) Complete(id string) (ResumableUploadSession, error) {
	t.mu.Lock()
	defer t.mu.Unlock()

	session, err := t.mutableSessionLocked(id)
	if err != nil {
		return ResumableUploadSession{}, err
	}
	if len(session.MissingRanges()) > 0 {
		return ResumableUploadSession{}, ErrResumableRangeGap
	}

	now := t.now()
	session.State = ResumableUploadCompleted
	session.UpdatedAt = now
	session.CompletedAt = now
	t.sessions[id] = session
	return cloneSession(session), nil
}

// Abort marks an open session aborted. Aborted sessions refuse new
// ranges and cannot be completed.
func (t *MemoryResumableTracker) Abort(id string) (ResumableUploadSession, error) {
	t.mu.Lock()
	defer t.mu.Unlock()

	session, err := t.mutableSessionLocked(id)
	if err != nil {
		return ResumableUploadSession{}, err
	}

	now := t.now()
	session.State = ResumableUploadAborted
	session.UpdatedAt = now
	session.AbortedAt = now
	t.sessions[id] = session
	return cloneSession(session), nil
}

// CleanupExpired removes expired sessions and returns the number of
// entries deleted.
func (t *MemoryResumableTracker) CleanupExpired() int {
	t.mu.Lock()
	defer t.mu.Unlock()

	now := t.now()
	removed := 0
	for id, session := range t.sessions {
		if expiredAt(session, now) {
			delete(t.sessions, id)
			removed++
		}
	}
	return removed
}

func (t *MemoryResumableTracker) recordRangeLocked(session ResumableUploadSession, r ResumableChunkRange) (ResumableUploadSession, error) {
	if err := validateRange(r, session.Size); err != nil {
		return ResumableUploadSession{}, err
	}
	for _, existing := range session.Ranges {
		if rangesOverlap(existing, r) {
			return ResumableUploadSession{}, ErrResumableRangeOverlap
		}
	}

	session.Ranges = append(session.Ranges, r)
	sortRanges(session.Ranges)
	session.UpdatedAt = t.now()
	t.sessions[session.ID] = session
	return cloneSession(session), nil
}

func (t *MemoryResumableTracker) mutableSessionLocked(id string) (ResumableUploadSession, error) {
	session, err := t.sessionLocked(id)
	if err != nil {
		return ResumableUploadSession{}, err
	}
	if session.State != ResumableUploadOpen {
		return ResumableUploadSession{}, ErrResumableSessionClosed
	}
	return session, nil
}

func (t *MemoryResumableTracker) sessionLocked(id string) (ResumableUploadSession, error) {
	session, ok := t.sessions[id]
	if !ok {
		return ResumableUploadSession{}, ErrResumableSessionNotFound
	}
	if expiredAt(session, t.now()) {
		delete(t.sessions, id)
		return ResumableUploadSession{}, ErrResumableSessionExpired
	}
	return session, nil
}

func (t *MemoryResumableTracker) ensureSessionsLocked() {
	if t.sessions == nil {
		t.sessions = make(map[string]ResumableUploadSession)
	}
}

func (t *MemoryResumableTracker) nextIDLocked() string {
	for {
		t.next++
		id := fmt.Sprintf("resumable-%d", t.next)
		if _, ok := t.sessions[id]; !ok {
			return id
		}
	}
}

func (t *MemoryResumableTracker) now() time.Time {
	if t.Clock != nil {
		return t.Clock()
	}
	return time.Now()
}

func validateRange(r ResumableChunkRange, size int64) error {
	if r.Start < 0 || r.End <= r.Start {
		return ErrResumableRangeInvalid
	}
	if r.End > size {
		return ErrResumableRangeInvalid
	}
	return nil
}

func rangesOverlap(left, right ResumableChunkRange) bool {
	return left.Start < right.End && right.Start < left.End
}

func expiredAt(session ResumableUploadSession, now time.Time) bool {
	return !session.ExpiresAt.IsZero() && !now.Before(session.ExpiresAt)
}

func cloneSession(session ResumableUploadSession) ResumableUploadSession {
	session.Ranges = cloneRanges(session.Ranges)
	return session
}

func cloneRanges(ranges []ResumableChunkRange) []ResumableChunkRange {
	if len(ranges) == 0 {
		return nil
	}
	clone := make([]ResumableChunkRange, len(ranges))
	copy(clone, ranges)
	return clone
}

func sortRanges(ranges []ResumableChunkRange) {
	sort.Slice(ranges, func(i, j int) bool {
		if ranges[i].Start == ranges[j].Start {
			return ranges[i].End < ranges[j].End
		}
		return ranges[i].Start < ranges[j].Start
	})
}

func contiguousEnd(ranges []ResumableChunkRange) int64 {
	next := int64(0)
	for _, r := range ranges {
		if r.Start > next {
			return next
		}
		if r.End > next {
			next = r.End
		}
	}
	return next
}

var (
	// ErrResumableSessionExists is returned when Create receives an
	// ID already tracked by this process.
	ErrResumableSessionExists = errors.New("lazuli/storage: resumable_session_exists")

	// ErrResumableSessionNotFound is returned for an unknown session ID.
	ErrResumableSessionNotFound = errors.New("lazuli/storage: resumable_session_not_found")

	// ErrResumableSessionExpired is returned when a session's TTL has elapsed.
	ErrResumableSessionExpired = errors.New("lazuli/storage: resumable_session_expired")

	// ErrResumableSessionClosed is returned when mutating a completed
	// or aborted session.
	ErrResumableSessionClosed = errors.New("lazuli/storage: resumable_session_closed")

	// ErrResumableSessionInvalid is returned when create options have
	// invalid sizes or TTLs.
	ErrResumableSessionInvalid = errors.New("lazuli/storage: resumable_session_invalid")

	// ErrResumableRangeInvalid is returned for negative, empty, or
	// out-of-bounds chunk ranges.
	ErrResumableRangeInvalid = errors.New("lazuli/storage: resumable_range_invalid")

	// ErrResumableRangeOverlap is returned when a recorded range
	// intersects bytes already present in the session.
	ErrResumableRangeOverlap = errors.New("lazuli/storage: resumable_range_overlap")

	// ErrResumableRangeGap is returned when an append or completion
	// observes missing bytes before the requested range or final size.
	ErrResumableRangeGap = errors.New("lazuli/storage: resumable_range_gap")
)
