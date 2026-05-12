package cache

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"
)

var (
	// ErrNilWarmBackend reports that cache warming was called without a backend.
	ErrNilWarmBackend = errors.New("lazuli/cache: warm backend is nil")
	// ErrWarmTaskKeyRequired reports that a warmup task has no cache key.
	ErrWarmTaskKeyRequired = errors.New("lazuli/cache: warm task Key is required")
	// ErrWarmTaskLoadRequired reports that a warmup task has no load function.
	ErrWarmTaskLoadRequired = errors.New("lazuli/cache: warm task Load is required")
)

// WarmTask describes a single cache entry to load and store.
type WarmTask struct {
	// Name identifies the task in WarmResult. It is for reporting only.
	Name string
	// Key is the cache key passed to Backend.Put.
	Key string
	// TTL is the cache entry lifetime passed to Backend.Put.
	TTL time.Duration
	// Tags are invalidation labels passed to Backend.Put.
	Tags []string
	// Load fetches or computes the bytes to place in the cache.
	Load func(context.Context) ([]byte, error)
}

// WarmOptions configures cache warming execution.
type WarmOptions struct {
	// Concurrency caps the number of tasks run at once. Values <= 0 run one
	// task at a time.
	Concurrency int
}

// WarmTaskResult records the outcome for one WarmTask.
type WarmTaskResult struct {
	// Name is copied from the task for reporting.
	Name string
	// Key is the cache key the task attempted to warm.
	Key string
	// Err is nil when the task loaded and stored successfully.
	Err error

	// Skipped is true when the task was not started because the context was
	// already canceled or canceled while waiting for the concurrency limit.
	Skipped bool
}

// WarmResult summarizes a cache warming run.
type WarmResult struct {
	// Total is the number of tasks passed to Warm.
	Total int
	// Warmed is the number of tasks stored successfully.
	Warmed int
	// Failed is the number of tasks with an error, including skipped tasks.
	Failed int
	// Skipped is the number of tasks not started because the context was done.
	Skipped int

	// Tasks preserves the input order and carries per-task errors.
	Tasks []WarmTaskResult
}

// Warmer warms cache entries against a configured backend.
type Warmer struct {
	// Backend stores loaded task bytes.
	Backend Backend
	// Options configures concurrency for this warmer.
	Options WarmOptions
}

// Warm runs tasks with the Warmer's backend and options.
func (w Warmer) Warm(ctx context.Context, tasks []WarmTask) WarmResult {
	return Warm(ctx, w.Backend, tasks, w.Options)
}

// Warm runs cache warmup tasks with a concurrency limit and returns a summary.
//
// Each task calls Load, then stores the returned bytes with Backend.Put using
// the task's Key, TTL, and Tags. Task failures are isolated: one task error does
// not stop already scheduled work, but context cancellation prevents new tasks
// from starting.
func Warm(ctx context.Context, backend Backend, tasks []WarmTask, opts WarmOptions) WarmResult {
	if ctx == nil {
		ctx = context.Background()
	}

	result := newWarmResult(tasks)
	if len(tasks) == 0 {
		return result
	}
	if backend == nil {
		for i := range result.Tasks {
			result.Tasks[i].Err = ErrNilWarmBackend
		}
		result.summarize()
		return result
	}

	concurrency := opts.Concurrency
	if concurrency <= 0 {
		concurrency = 1
	}

	sem := make(chan struct{}, concurrency)
	var wg sync.WaitGroup

schedule:
	for i, task := range tasks {
		if err := ctx.Err(); err != nil {
			result.skipFrom(i, err)
			break
		}

		select {
		case sem <- struct{}{}:
			wg.Add(1)
			go func(i int, task WarmTask) {
				defer wg.Done()
				defer func() { <-sem }()

				result.Tasks[i].Err = warmOne(ctx, backend, task)
			}(i, task)
		case <-ctx.Done():
			result.skipFrom(i, ctx.Err())
			break schedule
		}
	}

	wg.Wait()
	result.summarize()
	return result
}

func newWarmResult(tasks []WarmTask) WarmResult {
	result := WarmResult{
		Total: len(tasks),
		Tasks: make([]WarmTaskResult, len(tasks)),
	}
	for i, task := range tasks {
		result.Tasks[i] = WarmTaskResult{Name: task.Name, Key: task.Key}
	}
	return result
}

func (r *WarmResult) skipFrom(start int, err error) {
	if err == nil {
		err = context.Canceled
	}
	for i := start; i < len(r.Tasks); i++ {
		r.Tasks[i].Err = err
		r.Tasks[i].Skipped = true
	}
}

func (r *WarmResult) summarize() {
	r.Warmed = 0
	r.Failed = 0
	r.Skipped = 0
	for _, task := range r.Tasks {
		if task.Err == nil {
			r.Warmed++
			continue
		}
		r.Failed++
		if task.Skipped {
			r.Skipped++
		}
	}
}

func warmOne(ctx context.Context, backend Backend, task WarmTask) error {
	if task.Key == "" {
		return ErrWarmTaskKeyRequired
	}
	if task.Load == nil {
		return ErrWarmTaskLoadRequired
	}

	if err := ctx.Err(); err != nil {
		return err
	}
	value, err := task.Load(ctx)
	if err != nil {
		return fmt.Errorf("load: %w", err)
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := backend.Put(ctx, task.Key, value, task.TTL, task.Tags); err != nil {
		return fmt.Errorf("put: %w", err)
	}
	return nil
}
