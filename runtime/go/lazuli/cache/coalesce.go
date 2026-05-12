package cache

import (
	"context"
	"sync"
)

// Coalescer suppresses duplicate concurrent cache producers by key.
//
// The zero value is ready to use. Coalescer does not cache completed
// results; it only shares work that is already in flight.
type Coalescer struct {
	mu       sync.Mutex
	inflight map[string]*coalesceCall
}

type coalesceCall struct {
	done       chan struct{}
	value      []byte
	err        error
	panicValue any
	panicked   bool
}

// Do runs fn for key when no producer is already in flight.
//
// Concurrent callers for the same key wait for the in-flight result
// instead of running fn again. shared reports whether this call joined
// an existing in-flight producer. A waiting caller's context
// cancellation returns ctx.Err without cancelling the producer. Returned
// byte slices are defensive copies and may be mutated by the caller.
func (c *Coalescer) Do(ctx context.Context, key string, fn func(context.Context) ([]byte, error)) ([]byte, bool, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	call, shared := c.callFor(key)
	if shared {
		return call.wait(ctx)
	}

	defer func() {
		if value := recover(); value != nil {
			c.finish(key, call, nil, nil, value, true)
			panic(value)
		}
	}()

	value, err := fn(ctx)
	stored := cloneBytes(value)
	c.finish(key, call, stored, err, nil, false)

	return cloneBytes(stored), false, err
}

func (c *Coalescer) callFor(key string) (*coalesceCall, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if c.inflight == nil {
		c.inflight = make(map[string]*coalesceCall)
	}
	if call := c.inflight[key]; call != nil {
		return call, true
	}

	call := &coalesceCall{done: make(chan struct{})}
	c.inflight[key] = call
	return call, false
}

func (c *Coalescer) finish(key string, call *coalesceCall, value []byte, err error, panicValue any, panicked bool) {
	c.mu.Lock()
	defer c.mu.Unlock()

	call.value = value
	call.err = err
	call.panicValue = panicValue
	call.panicked = panicked
	delete(c.inflight, key)
	close(call.done)
}

func (call *coalesceCall) wait(ctx context.Context) ([]byte, bool, error) {
	select {
	case <-call.done:
		if call.panicked {
			panic(call.panicValue)
		}
		return cloneBytes(call.value), true, call.err
	case <-ctx.Done():
		return nil, true, ctx.Err()
	}
}

func cloneBytes(value []byte) []byte {
	if value == nil {
		return nil
	}
	clone := make([]byte, len(value))
	copy(clone, value)
	return clone
}
