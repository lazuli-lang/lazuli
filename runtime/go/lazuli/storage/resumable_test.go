package storage_test

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/storage"
)

func TestMemoryResumableTrackerAppendAndComplete(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	tracker := storage.NewMemoryResumableTracker(time.Hour)
	tracker.Clock = func() time.Time { return now }

	session, err := tracker.Create(storage.ResumableCreateOptions{
		ID:   "upload-1",
		Key:  storage.Key("imports/people.csv"),
		Size: 10,
		Metadata: storage.Metadata{
			Filename:    "people.csv",
			ContentType: "text/csv",
		},
	})
	if err != nil {
		t.Fatalf("Create failed: %v", err)
	}
	if session.ID != "upload-1" {
		t.Fatalf("ID = %q, want upload-1", session.ID)
	}
	if session.State != storage.ResumableUploadOpen {
		t.Fatalf("State = %v, want open", session.State)
	}
	if session.Metadata.Size != 10 {
		t.Fatalf("Metadata.Size = %d, want 10", session.Metadata.Size)
	}
	if session.ExpiresAt != now.Add(time.Hour) {
		t.Fatalf("ExpiresAt = %v, want %v", session.ExpiresAt, now.Add(time.Hour))
	}

	session, err = tracker.AppendRange("upload-1", storage.ResumableChunkRange{Start: 0, End: 4})
	if err != nil {
		t.Fatalf("AppendRange first chunk failed: %v", err)
	}
	if got := session.UploadedBytes(); got != 4 {
		t.Fatalf("UploadedBytes = %d, want 4", got)
	}

	session, err = tracker.AppendRange("upload-1", storage.ResumableChunkRange{Start: 4, End: 10})
	if err != nil {
		t.Fatalf("AppendRange second chunk failed: %v", err)
	}
	if got := session.UploadedBytes(); got != 10 {
		t.Fatalf("UploadedBytes = %d, want 10", got)
	}

	now = now.Add(30 * time.Second)
	completed, err := tracker.Complete("upload-1")
	if err != nil {
		t.Fatalf("Complete failed: %v", err)
	}
	if completed.State != storage.ResumableUploadCompleted {
		t.Fatalf("State = %v, want completed", completed.State)
	}
	if completed.CompletedAt != now {
		t.Fatalf("CompletedAt = %v, want %v", completed.CompletedAt, now)
	}

	_, err = tracker.AppendRange("upload-1", storage.ResumableChunkRange{Start: 10, End: 11})
	if !errors.Is(err, storage.ErrResumableSessionClosed) {
		t.Fatalf("expected ErrResumableSessionClosed, got %v", err)
	}
}

func TestMemoryResumableTrackerRecordsOutOfOrderRanges(t *testing.T) {
	t.Parallel()

	tracker := storage.NewMemoryResumableTracker(0)
	_, err := tracker.Create(storage.ResumableCreateOptions{
		ID:   "upload-2",
		Size: 10,
	})
	if err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	session, err := tracker.RecordRange("upload-2", storage.ResumableChunkRange{Start: 5, End: 10})
	if err != nil {
		t.Fatalf("RecordRange tail failed: %v", err)
	}
	if got := session.UploadedBytes(); got != 0 {
		t.Fatalf("UploadedBytes = %d, want 0 while first chunk is missing", got)
	}
	assertRanges(t, session.MissingRanges(), []storage.ResumableChunkRange{{Start: 0, End: 5}})

	_, err = tracker.RecordRange("upload-2", storage.ResumableChunkRange{Start: 4, End: 6})
	if !errors.Is(err, storage.ErrResumableRangeOverlap) {
		t.Fatalf("expected ErrResumableRangeOverlap, got %v", err)
	}

	_, err = tracker.Complete("upload-2")
	if !errors.Is(err, storage.ErrResumableRangeGap) {
		t.Fatalf("expected ErrResumableRangeGap, got %v", err)
	}

	session, err = tracker.RecordRange("upload-2", storage.ResumableChunkRange{Start: 0, End: 5})
	if err != nil {
		t.Fatalf("RecordRange head failed: %v", err)
	}
	if got := session.UploadedBytes(); got != 10 {
		t.Fatalf("UploadedBytes = %d, want 10", got)
	}
	assertRanges(t, session.MissingRanges(), nil)

	completed, err := tracker.Complete("upload-2")
	if err != nil {
		t.Fatalf("Complete failed after filling gap: %v", err)
	}
	if completed.State != storage.ResumableUploadCompleted {
		t.Fatalf("State = %v, want completed", completed.State)
	}
}

func TestMemoryResumableTrackerAppendDetectsGapsAndInvalidRanges(t *testing.T) {
	t.Parallel()

	tracker := storage.NewMemoryResumableTracker(0)
	_, err := tracker.Create(storage.ResumableCreateOptions{
		ID:   "upload-3",
		Size: 8,
	})
	if err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	_, err = tracker.AppendRange("upload-3", storage.ResumableChunkRange{Start: 2, End: 4})
	if !errors.Is(err, storage.ErrResumableRangeGap) {
		t.Fatalf("expected ErrResumableRangeGap, got %v", err)
	}

	_, err = tracker.AppendRange("upload-3", storage.ResumableChunkRange{Start: 0, End: 4})
	if err != nil {
		t.Fatalf("AppendRange first chunk failed: %v", err)
	}

	_, err = tracker.AppendRange("upload-3", storage.ResumableChunkRange{Start: 5, End: 8})
	if !errors.Is(err, storage.ErrResumableRangeGap) {
		t.Fatalf("expected ErrResumableRangeGap for skipped byte, got %v", err)
	}

	_, err = tracker.RecordRange("upload-3", storage.ResumableChunkRange{Start: 4, End: 4})
	if !errors.Is(err, storage.ErrResumableRangeInvalid) {
		t.Fatalf("expected ErrResumableRangeInvalid for empty range, got %v", err)
	}

	_, err = tracker.RecordRange("upload-3", storage.ResumableChunkRange{Start: 4, End: 9})
	if !errors.Is(err, storage.ErrResumableRangeInvalid) {
		t.Fatalf("expected ErrResumableRangeInvalid for range past size, got %v", err)
	}
}

func TestMemoryResumableTrackerAbortClosesSession(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	tracker := storage.NewMemoryResumableTracker(0)
	tracker.Clock = func() time.Time { return now }

	_, err := tracker.Create(storage.ResumableCreateOptions{
		ID:   "upload-4",
		Size: 5,
	})
	if err != nil {
		t.Fatalf("Create failed: %v", err)
	}
	_, err = tracker.AppendRange("upload-4", storage.ResumableChunkRange{Start: 0, End: 2})
	if err != nil {
		t.Fatalf("AppendRange failed: %v", err)
	}

	now = now.Add(time.Second)
	aborted, err := tracker.Abort("upload-4")
	if err != nil {
		t.Fatalf("Abort failed: %v", err)
	}
	if aborted.State != storage.ResumableUploadAborted {
		t.Fatalf("State = %v, want aborted", aborted.State)
	}
	if aborted.AbortedAt != now {
		t.Fatalf("AbortedAt = %v, want %v", aborted.AbortedAt, now)
	}

	_, err = tracker.RecordRange("upload-4", storage.ResumableChunkRange{Start: 2, End: 5})
	if !errors.Is(err, storage.ErrResumableSessionClosed) {
		t.Fatalf("expected ErrResumableSessionClosed from RecordRange, got %v", err)
	}
	_, err = tracker.Complete("upload-4")
	if !errors.Is(err, storage.ErrResumableSessionClosed) {
		t.Fatalf("expected ErrResumableSessionClosed from Complete, got %v", err)
	}
}

func TestMemoryResumableTrackerExpiryAndCleanup(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	tracker := storage.NewMemoryResumableTracker(time.Minute)
	tracker.Clock = func() time.Time { return now }

	_, err := tracker.Create(storage.ResumableCreateOptions{
		ID:   "cleanup",
		Size: 1,
	})
	if err != nil {
		t.Fatalf("Create cleanup session failed: %v", err)
	}
	now = now.Add(2 * time.Minute)
	if removed := tracker.CleanupExpired(); removed != 1 {
		t.Fatalf("CleanupExpired removed %d sessions, want 1", removed)
	}
	_, err = tracker.Get("cleanup")
	if !errors.Is(err, storage.ErrResumableSessionNotFound) {
		t.Fatalf("expected ErrResumableSessionNotFound after cleanup, got %v", err)
	}

	now = time.Date(2026, 5, 12, 13, 0, 0, 0, time.UTC)
	_, err = tracker.Create(storage.ResumableCreateOptions{
		ID:   "on-access",
		Size: 1,
	})
	if err != nil {
		t.Fatalf("Create on-access session failed: %v", err)
	}
	now = now.Add(2 * time.Minute)
	_, err = tracker.AppendRange("on-access", storage.ResumableChunkRange{Start: 0, End: 1})
	if !errors.Is(err, storage.ErrResumableSessionExpired) {
		t.Fatalf("expected ErrResumableSessionExpired, got %v", err)
	}
	_, err = tracker.Get("on-access")
	if !errors.Is(err, storage.ErrResumableSessionNotFound) {
		t.Fatalf("expected ErrResumableSessionNotFound after expired access, got %v", err)
	}
}

func TestMemoryResumableTrackerRejectsDuplicateSessionID(t *testing.T) {
	t.Parallel()

	tracker := storage.NewMemoryResumableTracker(0)
	_, err := tracker.Create(storage.ResumableCreateOptions{ID: "duplicate", Size: 1})
	if err != nil {
		t.Fatalf("Create failed: %v", err)
	}
	_, err = tracker.Create(storage.ResumableCreateOptions{ID: "duplicate", Size: 1})
	if !errors.Is(err, storage.ErrResumableSessionExists) {
		t.Fatalf("expected ErrResumableSessionExists, got %v", err)
	}
}

func TestMemoryResumableTrackerRejectsInvalidSessionOptions(t *testing.T) {
	t.Parallel()

	tracker := storage.NewMemoryResumableTracker(0)
	_, err := tracker.Create(storage.ResumableCreateOptions{
		ID:   "invalid",
		Size: -1,
	})
	if !errors.Is(err, storage.ErrResumableSessionInvalid) {
		t.Fatalf("expected ErrResumableSessionInvalid, got %v", err)
	}

	tracker = storage.NewMemoryResumableTracker(-time.Second)
	_, err = tracker.Create(storage.ResumableCreateOptions{
		ID:   "invalid-default-ttl",
		Size: 1,
	})
	if !errors.Is(err, storage.ErrResumableSessionInvalid) {
		t.Fatalf("expected ErrResumableSessionInvalid for negative default TTL, got %v", err)
	}
}

func assertRanges(t *testing.T, got, want []storage.ResumableChunkRange) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("range count = %d, want %d: got %#v", len(got), len(want), got)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Fatalf("range[%d] = %#v, want %#v", i, got[i], want[i])
		}
	}
}
