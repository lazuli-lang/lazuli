package search

import (
	"context"
	"errors"
	"fmt"
)

var (
	errReindexerReaderRequired     = errors.New("lazuli/search: reindexer source reader is required")
	errReindexerSinkRequired       = errors.New("lazuli/search: reindexer document sink is required")
	errReindexerCheckpointRequired = errors.New("lazuli/search: reindexer checkpoint store is required")
	errReindexerCheckpointMismatch = errors.New("lazuli/search: reindexer checkpoint id mismatch")
)

// IndexDocument is one backend-neutral document emitted by a SourceReader.
type IndexDocument struct {
	ID       string
	Source   string
	Tenant   string
	Contents []byte
}

// SourceReader reads source rows for one planned reindex window.
type SourceReader interface {
	ReadIndexDocuments(ctx context.Context, shard IndexShardPlan, window IndexBatchWindow) ([]IndexDocument, error)
}

// DocumentSink writes normalized documents for one planned reindex window.
type DocumentSink interface {
	WriteIndexDocuments(ctx context.Context, shard IndexShardPlan, window IndexBatchWindow, documents []IndexDocument) (uint64, error)
}

// CheckpointStore persists per-shard reindex progress.
type CheckpointStore interface {
	LoadIndexCheckpoint(ctx context.Context, id string) (IndexCheckpoint, error)
	SaveIndexCheckpoint(ctx context.Context, checkpoint IndexCheckpoint) error
}

// IndexCheckpoint records the next source offset to read for one shard plan.
type IndexCheckpoint struct {
	ID          string
	Index       string
	Resource    string
	Tenant      string
	Mode        RebuildMode
	Shard       string
	WindowIndex int
	Offset      uint64
	Documents   uint64
}

// Reindexer executes IndexPlan windows against caller-provided adapters.
type Reindexer struct {
	Reader      SourceReader
	Sink        DocumentSink
	Checkpoints CheckpointStore
}

// ReindexResult summarizes a completed or partially completed reindex run.
type ReindexResult struct {
	Index          string
	Mode           RebuildMode
	ShardCount     uint32
	ShardPlanCount int
	WindowCount    int
	WindowsRead    int
	WindowsSkipped int
	DocumentsRead  uint64
	DocumentsWrote uint64
	Windows        []ReindexWindowResult
}

// ReindexWindowResult summarizes one planned window.
type ReindexWindowResult struct {
	CheckpointID   string
	Index          string
	Resource       string
	Tenant         string
	Shard          string
	Window         IndexBatchWindow
	Skipped        bool
	DocumentsRead  uint64
	DocumentsWrote uint64
	Checkpoint     IndexCheckpoint
}

// Run executes each uncheckpointed IndexPlan window in deterministic plan
// order. A checkpoint is saved only after the sink accepts the window's
// documents.
func (r Reindexer) Run(ctx context.Context, plan IndexPlan) (ReindexResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if r.Reader == nil {
		return ReindexResult{}, errReindexerReaderRequired
	}
	if r.Sink == nil {
		return ReindexResult{}, errReindexerSinkRequired
	}
	if r.Checkpoints == nil {
		return ReindexResult{}, errReindexerCheckpointRequired
	}

	result := ReindexResult{
		Index:          plan.Name,
		Mode:           plan.Mode,
		ShardCount:     plan.ShardCount,
		ShardPlanCount: plan.ShardPlanCount,
		WindowCount:    plan.BatchCount,
		Windows:        make([]ReindexWindowResult, 0, plan.BatchCount),
	}

	for _, shard := range plan.Shards {
		checkpoint, err := r.Checkpoints.LoadIndexCheckpoint(ctx, shard.CheckpointID)
		if err != nil {
			return result, fmt.Errorf("lazuli/search: load reindex checkpoint %q: %w", shard.CheckpointID, err)
		}
		checkpoint, err = normalizeIndexCheckpoint(plan.Mode, shard, checkpoint)
		if err != nil {
			return result, err
		}

		for _, window := range shard.Windows {
			if err := ctx.Err(); err != nil {
				return result, err
			}

			if window.End <= checkpoint.Offset {
				result.WindowsSkipped++
				result.Windows = append(result.Windows, reindexWindowResult(shard, window, true, 0, 0, checkpoint))
				continue
			}

			documents, err := r.Reader.ReadIndexDocuments(ctx, shard, window)
			if err != nil {
				return result, fmt.Errorf("lazuli/search: read reindex window %q %s #%d: %w",
					shard.CheckpointID,
					shard.Shard,
					window.Index,
					err,
				)
			}
			if err := ctx.Err(); err != nil {
				return result, err
			}

			wrote, err := r.Sink.WriteIndexDocuments(ctx, shard, window, documents)
			if err != nil {
				return result, fmt.Errorf("lazuli/search: write reindex window %q %s #%d: %w",
					shard.CheckpointID,
					shard.Shard,
					window.Index,
					err,
				)
			}
			if err := ctx.Err(); err != nil {
				return result, err
			}

			checkpoint = advanceIndexCheckpoint(checkpoint, plan.Mode, shard, window, wrote)
			if err := r.Checkpoints.SaveIndexCheckpoint(ctx, checkpoint); err != nil {
				return result, fmt.Errorf("lazuli/search: save reindex checkpoint %q: %w", shard.CheckpointID, err)
			}

			read := uint64(len(documents))
			result.WindowsRead++
			result.DocumentsRead += read
			result.DocumentsWrote += wrote
			result.Windows = append(result.Windows, reindexWindowResult(shard, window, false, read, wrote, checkpoint))
		}
	}

	return result, nil
}

func normalizeIndexCheckpoint(mode RebuildMode, shard IndexShardPlan, checkpoint IndexCheckpoint) (IndexCheckpoint, error) {
	if checkpoint.ID == "" {
		return IndexCheckpoint{
			ID:       shard.CheckpointID,
			Index:    shard.Index,
			Resource: shard.Source.Name,
			Tenant:   shard.Source.Tenant,
			Mode:     mode,
			Shard:    shard.Shard,
		}, nil
	}
	if checkpoint.ID != shard.CheckpointID {
		return IndexCheckpoint{}, fmt.Errorf("%w: got %q want %q", errReindexerCheckpointMismatch, checkpoint.ID, shard.CheckpointID)
	}
	if checkpoint.Index == "" {
		checkpoint.Index = shard.Index
	}
	if checkpoint.Resource == "" {
		checkpoint.Resource = shard.Source.Name
	}
	if checkpoint.Tenant == "" {
		checkpoint.Tenant = shard.Source.Tenant
	}
	if checkpoint.Mode == "" {
		checkpoint.Mode = mode
	}
	if checkpoint.Shard == "" {
		checkpoint.Shard = shard.Shard
	}
	return checkpoint, nil
}

func advanceIndexCheckpoint(checkpoint IndexCheckpoint, mode RebuildMode, shard IndexShardPlan, window IndexBatchWindow, wrote uint64) IndexCheckpoint {
	checkpoint.ID = shard.CheckpointID
	checkpoint.Index = shard.Index
	checkpoint.Resource = shard.Source.Name
	checkpoint.Tenant = shard.Source.Tenant
	checkpoint.Mode = mode
	checkpoint.Shard = shard.Shard
	checkpoint.WindowIndex = window.Index
	checkpoint.Offset = window.End
	checkpoint.Documents += wrote
	return checkpoint
}

func reindexWindowResult(shard IndexShardPlan, window IndexBatchWindow, skipped bool, read, wrote uint64, checkpoint IndexCheckpoint) ReindexWindowResult {
	return ReindexWindowResult{
		CheckpointID:   shard.CheckpointID,
		Index:          shard.Index,
		Resource:       shard.Source.Name,
		Tenant:         shard.Source.Tenant,
		Shard:          shard.Shard,
		Window:         window,
		Skipped:        skipped,
		DocumentsRead:  read,
		DocumentsWrote: wrote,
		Checkpoint:     checkpoint,
	}
}
