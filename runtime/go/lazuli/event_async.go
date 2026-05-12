package lazuli

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"sync"
)

const (
	defaultAsyncEventBusWorkers   = 1
	defaultAsyncEventBusQueueSize = 64
)

var (
	// ErrAsyncEventBusClosed is returned when publishing to a bus after
	// Shutdown has started.
	ErrAsyncEventBusClosed = errors.New("lazuli async event bus is shut down")

	// ErrAsyncEventBusQueueFull is returned when the bounded queue cannot
	// accept another event without blocking.
	ErrAsyncEventBusQueueFull = errors.New("lazuli async event bus queue is full")
)

// AsyncEventErrorHandler receives subscriber failures from an AsyncEventBus.
// The bus calls it from a worker goroutine, so handlers should return quickly.
type AsyncEventErrorHandler func(ctx context.Context, e Event, err error)

// AsyncEventBusOptions configures a new AsyncEventBus.
type AsyncEventBusOptions struct {
	// Workers is the number of goroutines that deliver queued events. Values
	// less than one use the default.
	Workers int

	// QueueSize is the maximum number of events waiting for a worker. Values
	// less than one use the default.
	QueueSize int

	// ErrorHandler is called for subscriber errors and recovered subscriber
	// panics. When nil, failures are logged with slog.
	ErrorHandler AsyncEventErrorHandler
}

// AsyncEventBus is an opt-in in-process event bus that delivers events from a
// bounded queue. It is independent from the package-level Subscribe and
// Publish functions.
type AsyncEventBus struct {
	workers   int
	queueSize int
	queue     chan asyncEventDelivery

	errorHandler AsyncEventErrorHandler

	subscribersMu sync.RWMutex
	subscribers   map[string][]Subscriber

	publishMu    sync.RWMutex
	closed       bool
	shutdownOnce sync.Once

	drainMu sync.Mutex
	pending int
	drainCh chan struct{}

	workerWG    sync.WaitGroup
	workersDone chan struct{}
}

type asyncEventDelivery struct {
	ctx         context.Context
	event       Event
	subscribers []Subscriber
}

// NewAsyncEventBus starts a new opt-in asynchronous event bus.
func NewAsyncEventBus(opts AsyncEventBusOptions) *AsyncEventBus {
	workers := opts.Workers
	if workers < 1 {
		workers = defaultAsyncEventBusWorkers
	}
	queueSize := opts.QueueSize
	if queueSize < 1 {
		queueSize = defaultAsyncEventBusQueueSize
	}

	drainCh := make(chan struct{})
	close(drainCh)

	bus := &AsyncEventBus{
		workers:      workers,
		queueSize:    queueSize,
		queue:        make(chan asyncEventDelivery, queueSize),
		errorHandler: opts.ErrorHandler,
		subscribers:  make(map[string][]Subscriber),
		drainCh:      drainCh,
		workersDone:  make(chan struct{}),
	}

	bus.workerWG.Add(workers)
	for i := 0; i < workers; i++ {
		go bus.worker()
	}
	go func() {
		bus.workerWG.Wait()
		close(bus.workersDone)
	}()

	return bus
}

// WorkerCount returns the number of delivery workers started for this bus.
func (b *AsyncEventBus) WorkerCount() int {
	return b.workers
}

// QueueSize returns the bounded queue capacity for this bus.
func (b *AsyncEventBus) QueueSize() int {
	return b.queueSize
}

// Subscribe registers fn to receive every Event with the given name.
func (b *AsyncEventBus) Subscribe(name string, fn Subscriber) {
	b.subscribersMu.Lock()
	defer b.subscribersMu.Unlock()
	b.subscribers[name] = append(b.subscribers[name], fn)
}

// PublishAsync queues e for asynchronous delivery to the subscribers that were
// registered at publish time. It returns ErrAsyncEventBusQueueFull when the
// bounded queue is full.
func (b *AsyncEventBus) PublishAsync(ctx context.Context, e Event) error {
	if ctx == nil {
		ctx = context.Background()
	}

	b.publishMu.RLock()
	if b.closed {
		b.publishMu.RUnlock()
		return ErrAsyncEventBusClosed
	}

	subs := b.subscribersSnapshot(e.Name)
	if len(subs) == 0 {
		b.publishMu.RUnlock()
		slog.Debug("lazuli async event published with no subscribers", "name", e.Name)
		return nil
	}

	b.addPending()
	delivery := asyncEventDelivery{
		ctx:         ctx,
		event:       cloneAsyncEvent(e),
		subscribers: subs,
	}

	select {
	case b.queue <- delivery:
		b.publishMu.RUnlock()
		return nil
	default:
		b.donePending()
		b.publishMu.RUnlock()
		return ErrAsyncEventBusQueueFull
	}
}

// Drain waits until all events accepted by PublishAsync have been delivered.
// It does not stop the bus or prevent later publishes.
func (b *AsyncEventBus) Drain(ctx context.Context) error {
	if ctx == nil {
		ctx = context.Background()
	}

	for {
		b.drainMu.Lock()
		if b.pending == 0 {
			b.drainMu.Unlock()
			return nil
		}
		drainCh := b.drainCh
		b.drainMu.Unlock()

		select {
		case <-drainCh:
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}

// Shutdown stops accepting new events, drains queued events, and waits for
// worker goroutines to exit. The context limits how long Shutdown waits; it
// does not cancel subscriber contexts for events already accepted.
func (b *AsyncEventBus) Shutdown(ctx context.Context) error {
	if ctx == nil {
		ctx = context.Background()
	}

	b.shutdownOnce.Do(func() {
		b.publishMu.Lock()
		b.closed = true
		close(b.queue)
		b.publishMu.Unlock()
	})

	select {
	case <-b.workersDone:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (b *AsyncEventBus) subscribersSnapshot(name string) []Subscriber {
	b.subscribersMu.RLock()
	defer b.subscribersMu.RUnlock()

	subs := b.subscribers[name]
	dup := make([]Subscriber, len(subs))
	copy(dup, subs)
	return dup
}

func (b *AsyncEventBus) addPending() {
	b.drainMu.Lock()
	defer b.drainMu.Unlock()

	if b.pending == 0 {
		b.drainCh = make(chan struct{})
	}
	b.pending++
}

func (b *AsyncEventBus) donePending() {
	b.drainMu.Lock()
	defer b.drainMu.Unlock()

	b.pending--
	if b.pending == 0 {
		close(b.drainCh)
	}
}

func (b *AsyncEventBus) worker() {
	defer b.workerWG.Done()

	for delivery := range b.queue {
		b.deliver(delivery)
		b.donePending()
	}
}

func (b *AsyncEventBus) deliver(delivery asyncEventDelivery) {
	for _, fn := range delivery.subscribers {
		if err := callAsyncSubscriber(delivery.ctx, delivery.event, fn); err != nil {
			b.handleError(delivery.ctx, delivery.event, err)
		}
	}
}

func callAsyncSubscriber(ctx context.Context, e Event, fn Subscriber) (err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			err = fmt.Errorf("lazuli async event subscriber panicked: %v", recovered)
		}
	}()
	return fn(ctx, e)
}

func (b *AsyncEventBus) handleError(ctx context.Context, e Event, err error) {
	if b.errorHandler == nil {
		slog.Error("lazuli async event subscriber failed",
			"event", e.Name, "error", err)
		return
	}

	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("lazuli async event error handler panicked",
				"event", e.Name, "panic", recovered)
		}
	}()
	b.errorHandler(ctx, e, err)
}

func cloneAsyncEvent(e Event) Event {
	if e.Payload == nil {
		return e
	}

	payload := make(map[string]any, len(e.Payload))
	for key, value := range e.Payload {
		payload[key] = value
	}
	e.Payload = payload
	return e
}
