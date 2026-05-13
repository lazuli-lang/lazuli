package search

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"testing"
)

func TestReindexerRunExecutesWindowsAndUpdatesCheckpoints(t *testing.T) {
	plan := testReindexPlan(t)
	reader := &fakeSourceReader{}
	sink := &fakeDocumentSink{}
	checkpoints := newFakeCheckpointStore()

	result, err := (Reindexer{
		Reader:      reader,
		Sink:        sink,
		Checkpoints: checkpoints,
	}).Run(context.Background(), plan)
	if err != nil {
		t.Fatalf("Run() error = %v", err)
	}

	if result.Index != "customer.search" ||
		result.Mode != RebuildModeFull ||
		result.WindowCount != 3 ||
		result.WindowsRead != 3 ||
		result.WindowsSkipped != 0 ||
		result.DocumentsRead != 5 ||
		result.DocumentsWrote != 5 {
		t.Fatalf("result = %#v, want full three-window summary", result)
	}
	if len(result.Windows) != 3 {
		t.Fatalf("len(result.Windows) = %d, want 3", len(result.Windows))
	}

	wantWindows := []IndexBatchWindow{
		{Index: 1, Count: 3, Start: 0, End: 2, Offset: 0, Limit: 2},
		{Index: 2, Count: 3, Start: 2, End: 4, Offset: 2, Limit: 2},
		{Index: 3, Count: 3, Start: 4, End: 5, Offset: 4, Limit: 1},
	}
	if !reflect.DeepEqual(reader.windows, wantWindows) {
		t.Fatalf("reader windows = %#v, want %#v", reader.windows, wantWindows)
	}
	if !reflect.DeepEqual(sink.windows, wantWindows) {
		t.Fatalf("sink windows = %#v, want %#v", sink.windows, wantWindows)
	}

	checkpoint := checkpoints.saved[len(checkpoints.saved)-1]
	if checkpoint.ID != plan.Shards[0].CheckpointID ||
		checkpoint.Index != "customer.search" ||
		checkpoint.Resource != "customers" ||
		checkpoint.Mode != RebuildModeFull ||
		checkpoint.Shard != "shard-00-of-01" ||
		checkpoint.WindowIndex != 3 ||
		checkpoint.Offset != 5 ||
		checkpoint.Documents != 5 {
		t.Fatalf("last checkpoint = %#v, want completed source shard", checkpoint)
	}

	lastWindow := result.Windows[2]
	if lastWindow.Skipped ||
		lastWindow.DocumentsRead != 1 ||
		lastWindow.DocumentsWrote != 1 ||
		lastWindow.Checkpoint.Offset != 5 {
		t.Fatalf("last window result = %#v, want one written document and offset 5", lastWindow)
	}
}

func TestReindexerRunSkipsCheckpointedWindows(t *testing.T) {
	plan := testReindexPlan(t)
	checkpoints := newFakeCheckpointStore()
	checkpoints.checkpoints[plan.Shards[0].CheckpointID] = IndexCheckpoint{
		ID:          plan.Shards[0].CheckpointID,
		Index:       plan.Name,
		Resource:    "customers",
		Mode:        RebuildModeFull,
		Shard:       "shard-00-of-01",
		WindowIndex: 1,
		Offset:      2,
		Documents:   2,
	}
	reader := &fakeSourceReader{}
	sink := &fakeDocumentSink{}

	result, err := (Reindexer{
		Reader:      reader,
		Sink:        sink,
		Checkpoints: checkpoints,
	}).Run(context.Background(), plan)
	if err != nil {
		t.Fatalf("Run() error = %v", err)
	}

	if result.WindowsRead != 2 || result.WindowsSkipped != 1 || result.DocumentsRead != 3 || result.DocumentsWrote != 3 {
		t.Fatalf("result = %#v, want one skipped and two executed windows", result)
	}
	if len(result.Windows) != 3 || !result.Windows[0].Skipped || result.Windows[1].Skipped || result.Windows[2].Skipped {
		t.Fatalf("window skipped flags = %#v, want only first skipped", result.Windows)
	}

	wantRead := []IndexBatchWindow{
		{Index: 2, Count: 3, Start: 2, End: 4, Offset: 2, Limit: 2},
		{Index: 3, Count: 3, Start: 4, End: 5, Offset: 4, Limit: 1},
	}
	if !reflect.DeepEqual(reader.windows, wantRead) {
		t.Fatalf("reader windows = %#v, want %#v", reader.windows, wantRead)
	}

	checkpoint := checkpoints.saved[len(checkpoints.saved)-1]
	if checkpoint.Offset != 5 || checkpoint.Documents != 5 {
		t.Fatalf("last checkpoint = %#v, want resumed cumulative checkpoint", checkpoint)
	}
}

func TestReindexerRunStopsOnCancellationBeforeWritingCheckpoint(t *testing.T) {
	plan := testReindexPlan(t)
	ctx, cancel := context.WithCancel(context.Background())
	reader := &fakeSourceReader{cancel: cancel}
	sink := &fakeDocumentSink{}
	checkpoints := newFakeCheckpointStore()

	result, err := (Reindexer{
		Reader:      reader,
		Sink:        sink,
		Checkpoints: checkpoints,
	}).Run(ctx, plan)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Run() error = %v, want %v", err, context.Canceled)
	}
	if result.WindowsRead != 0 || result.DocumentsRead != 0 || len(result.Windows) != 0 {
		t.Fatalf("partial result = %#v, want no completed windows", result)
	}
	if len(reader.windows) != 1 {
		t.Fatalf("reader calls = %d, want cancellation after first read", len(reader.windows))
	}
	if len(sink.windows) != 0 {
		t.Fatalf("sink calls = %d, want none after cancellation", len(sink.windows))
	}
	if len(checkpoints.saved) != 0 {
		t.Fatalf("saved checkpoints = %#v, want none after cancellation", checkpoints.saved)
	}
}

func TestReindexerRunRejectsMissingAdapters(t *testing.T) {
	plan := testReindexPlan(t)
	tests := []struct {
		name      string
		reindexer Reindexer
		wantErr   error
	}{
		{
			name:      "reader",
			reindexer: Reindexer{Sink: &fakeDocumentSink{}, Checkpoints: newFakeCheckpointStore()},
			wantErr:   errReindexerReaderRequired,
		},
		{
			name:      "sink",
			reindexer: Reindexer{Reader: &fakeSourceReader{}, Checkpoints: newFakeCheckpointStore()},
			wantErr:   errReindexerSinkRequired,
		},
		{
			name:      "checkpoint",
			reindexer: Reindexer{Reader: &fakeSourceReader{}, Sink: &fakeDocumentSink{}},
			wantErr:   errReindexerCheckpointRequired,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := tt.reindexer.Run(context.Background(), plan)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("Run() error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}

func testReindexPlan(t *testing.T) IndexPlan {
	t.Helper()
	plan, err := PlanIndex(IndexPlanOptions{
		Name:         "customer.search",
		Mode:         RebuildModeFull,
		MaxBatchSize: 2,
		Sources: []IndexSourceResource{
			{Name: "customers", Count: 5},
		},
	})
	if err != nil {
		t.Fatalf("PlanIndex() error = %v", err)
	}
	return plan
}

type fakeSourceReader struct {
	windows []IndexBatchWindow
	cancel  context.CancelFunc
}

func (r *fakeSourceReader) ReadIndexDocuments(ctx context.Context, shard IndexShardPlan, window IndexBatchWindow) ([]IndexDocument, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	r.windows = append(r.windows, window)
	documents := make([]IndexDocument, 0, window.Limit)
	for offset := window.Start; offset < window.End; offset++ {
		documents = append(documents, IndexDocument{
			ID:       fmt.Sprintf("%s-%d", shard.Source.Name, offset),
			Source:   shard.Source.Name,
			Tenant:   shard.Source.Tenant,
			Contents: []byte(fmt.Sprintf("document %d", offset)),
		})
	}
	if r.cancel != nil {
		r.cancel()
	}
	return documents, nil
}

type fakeDocumentSink struct {
	windows []IndexBatchWindow
}

func (s *fakeDocumentSink) WriteIndexDocuments(ctx context.Context, shard IndexShardPlan, window IndexBatchWindow, documents []IndexDocument) (uint64, error) {
	if err := ctx.Err(); err != nil {
		return 0, err
	}
	s.windows = append(s.windows, window)
	return uint64(len(documents)), nil
}

type fakeCheckpointStore struct {
	checkpoints map[string]IndexCheckpoint
	saved       []IndexCheckpoint
}

func newFakeCheckpointStore() *fakeCheckpointStore {
	return &fakeCheckpointStore{checkpoints: make(map[string]IndexCheckpoint)}
}

func (s *fakeCheckpointStore) LoadIndexCheckpoint(ctx context.Context, id string) (IndexCheckpoint, error) {
	if err := ctx.Err(); err != nil {
		return IndexCheckpoint{}, err
	}
	return s.checkpoints[id], nil
}

func (s *fakeCheckpointStore) SaveIndexCheckpoint(ctx context.Context, checkpoint IndexCheckpoint) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	s.checkpoints[checkpoint.ID] = checkpoint
	s.saved = append(s.saved, checkpoint)
	return nil
}
