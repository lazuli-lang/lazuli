package jobs

import (
	"context"
	"errors"
	"sync"
	"time"
)

var (
	// ErrJobShutdown is returned by Begin after shutdown has started and is
	// used as the cancellation cause for active work during shutdown.
	ErrJobShutdown = errors.New("jobs: shutting down")
	// ErrJobShutdownTimeout is returned when active work does not finish within
	// the configured shutdown timeout.
	ErrJobShutdownTimeout = errors.New("jobs: shutdown timeout")
)

// ShutdownHook is a provider-neutral lifecycle hook invoked by Shutdown.
type ShutdownHook func(context.Context) error

// ShutdownHooks groups optional lifecycle hooks for a graceful worker shutdown.
//
// OnStopAccepting runs after the coordinator rejects new work. Queue adapters
// can use this hook to stop polling or accepting jobs. OnDrained runs after all
// active work has finished. OnTimeout runs when active work did not finish
// before the timeout.
type ShutdownHooks struct {
	OnStopAccepting []ShutdownHook
	OnDrained       []ShutdownHook
	OnTimeout       []ShutdownHook
}

// ShutdownOptions configures a single Shutdown call.
type ShutdownOptions struct {
	// Timeout bounds how long Shutdown waits for active work after cancellation.
	// A non-positive timeout waits until active work finishes or ctx is canceled.
	Timeout time.Duration
	Hooks   ShutdownHooks
}

// ShutdownCoordinator tracks active job work and coordinates graceful shutdown.
//
// The zero value is ready to use. Call Begin when a worker starts handling a
// job, pass the returned context to the handler, and call the returned finish
// function when the handler exits. Shutdown stops new Begin calls, cancels
// active job contexts, and waits for active work to finish.
type ShutdownCoordinator struct {
	mu       sync.Mutex
	stopping bool
	active   int
	done     chan struct{}
	nextID   uint64
	cancels  map[uint64]context.CancelCauseFunc
}

// Begin registers active work and returns a context canceled by Shutdown.
//
// The returned finish function is safe to call more than once. A nil context is
// treated as context.Background().
func (c *ShutdownCoordinator) Begin(ctx context.Context) (context.Context, func(), error) {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return nil, nil, err
	}

	jobCtx, cancel := context.WithCancelCause(ctx)

	c.mu.Lock()
	if c.stopping {
		c.mu.Unlock()
		cancel(ErrJobShutdown)
		return nil, nil, ErrJobShutdown
	}
	if c.active == 0 {
		c.done = make(chan struct{})
	}
	c.active++
	c.nextID++
	id := c.nextID
	if c.cancels == nil {
		c.cancels = make(map[uint64]context.CancelCauseFunc)
	}
	c.cancels[id] = cancel
	c.mu.Unlock()

	var once sync.Once
	finish := func() {
		once.Do(func() {
			c.finish(id, cancel)
		})
	}
	return jobCtx, finish, nil
}

// Shutdown stops accepting new work, cancels active work, and waits for drain.
//
// Hook errors are joined into the returned error. Shutdown is safe to call more
// than once, but the coordinator remains stopped after the first call.
func (c *ShutdownCoordinator) Shutdown(ctx context.Context, opts ShutdownOptions) error {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	cancels, done := c.stopAccepting()
	hookErr := runShutdownHooks(ctx, opts.Hooks.OnStopAccepting)
	for _, cancel := range cancels {
		cancel(ErrJobShutdown)
	}

	waitCtx := ctx
	var cancelWait context.CancelFunc
	if opts.Timeout > 0 {
		waitCtx, cancelWait = context.WithTimeout(ctx, opts.Timeout)
		defer cancelWait()
	}

	select {
	case <-done:
		hookErr = errors.Join(hookErr, runShutdownHooks(ctx, opts.Hooks.OnDrained))
		return hookErr
	case <-waitCtx.Done():
		if errors.Is(waitCtx.Err(), context.DeadlineExceeded) {
			hookErr = errors.Join(hookErr, runShutdownHooks(ctx, opts.Hooks.OnTimeout))
			return errors.Join(ErrJobShutdownTimeout, hookErr)
		}
		return errors.Join(waitCtx.Err(), hookErr)
	}
}

// Active returns the number of registered active jobs.
func (c *ShutdownCoordinator) Active() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.active
}

// Stopping reports whether Shutdown has started.
func (c *ShutdownCoordinator) Stopping() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.stopping
}

func (c *ShutdownCoordinator) finish(id uint64, cancel context.CancelCauseFunc) {
	c.mu.Lock()
	if _, ok := c.cancels[id]; !ok {
		c.mu.Unlock()
		cancel(context.Canceled)
		return
	}
	delete(c.cancels, id)
	c.active--
	if c.active == 0 && c.done != nil {
		close(c.done)
		c.done = nil
	}
	c.mu.Unlock()

	cancel(context.Canceled)
}

func (c *ShutdownCoordinator) stopAccepting() ([]context.CancelCauseFunc, <-chan struct{}) {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.stopping = true
	var done <-chan struct{} = c.done
	if c.active == 0 {
		done = closedShutdownChannel()
	}

	cancels := make([]context.CancelCauseFunc, 0, len(c.cancels))
	for _, cancel := range c.cancels {
		cancels = append(cancels, cancel)
	}
	return cancels, done
}

func runShutdownHooks(ctx context.Context, hooks []ShutdownHook) error {
	var hookErr error
	for _, hook := range hooks {
		if hook == nil {
			continue
		}
		hookErr = errors.Join(hookErr, hook(ctx))
	}
	return hookErr
}

func closedShutdownChannel() <-chan struct{} {
	ch := make(chan struct{})
	close(ch)
	return ch
}
