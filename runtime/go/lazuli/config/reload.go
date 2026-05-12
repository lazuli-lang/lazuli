// Package config provides helpers for loading and reloading Lazuli runtime
// configuration.
package config

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"sync"
	"time"
)

// Snapshot is one loaded configuration value and the identity used to detect
// changes.
//
// Set Version when the source already has a stable revision identifier. When
// Version is empty, Poller hashes Content and suppresses duplicate callbacks
// for identical bytes.
type Snapshot[T any] struct {
	// Value is the parsed configuration passed to Poller.OnChange.
	Value T
	// Content is the raw configuration content used for hashing when Version
	// is empty.
	Content []byte
	// Version is an optional stable revision identifier for sources that do
	// not want to compare raw content.
	Version string
}

// Loader loads and parses the current configuration snapshot.
type Loader[T any] func(ctx context.Context) (Snapshot[T], error)

// ParseFunc parses configuration bytes into a typed runtime value.
type ParseFunc[T any] func(ctx context.Context, content []byte) (T, error)

// ChangeFunc receives a parsed configuration value when Poller observes a new
// snapshot. Returning an error leaves the current snapshot uncommitted so the
// same change can be retried.
type ChangeFunc[T any] func(ctx context.Context, value T) error

// ErrorFunc receives load, parse, or change errors encountered by Run. When it
// is nil, Run stops on the first non-cancellation error.
type ErrorFunc func(ctx context.Context, err error)

// Ticker is the small subset of time.Ticker used by Poller. Tests can inject a
// manual ticker through Poller.TickerFactory.
type Ticker interface {
	C() <-chan time.Time
	Stop()
}

// TickerFactory creates a ticker for Run.
type TickerFactory func(interval time.Duration) Ticker

// Poller periodically loads configuration and invokes OnChange only after a
// successfully parsed, newly observed snapshot. The first successful check is
// treated as a change from the empty state.
type Poller[T any] struct {
	// Loader returns the current parsed configuration snapshot.
	Loader Loader[T]
	// Interval is the delay between checks used by Run.
	Interval time.Duration
	// OnChange is called with a parsed value after a new snapshot is observed.
	OnChange ChangeFunc[T]
	// OnError is called by Run for non-cancellation errors. When nil, Run
	// returns the error and stops polling.
	OnError ErrorFunc
	// TickerFactory creates the ticker used by Run. It defaults to time.NewTicker.
	TickerFactory TickerFactory

	mu       sync.Mutex
	seen     bool
	identity string
}

// NewPoller returns a Poller using loader as its source.
func NewPoller[T any](interval time.Duration, loader Loader[T], onChange ChangeFunc[T]) *Poller[T] {
	return &Poller[T]{
		Loader:   loader,
		Interval: interval,
		OnChange: onChange,
	}
}

// NewFilePoller returns a Poller that reads path and parses its contents.
func NewFilePoller[T any](path string, interval time.Duration, parse ParseFunc[T], onChange ChangeFunc[T]) *Poller[T] {
	return NewPoller(interval, FileLoader(path, parse), onChange)
}

// FileLoader returns a Loader that reads path on each call, parses the file,
// and uses the file bytes as the content identity.
func FileLoader[T any](path string, parse ParseFunc[T]) Loader[T] {
	return func(ctx context.Context) (Snapshot[T], error) {
		if err := ctx.Err(); err != nil {
			return Snapshot[T]{}, err
		}
		if parse == nil {
			return Snapshot[T]{}, errors.New("config: missing parse function")
		}

		content, err := os.ReadFile(path)
		if err != nil {
			return Snapshot[T]{}, err
		}
		value, err := parse(ctx, content)
		if err != nil {
			return Snapshot[T]{}, err
		}
		if err := ctx.Err(); err != nil {
			return Snapshot[T]{}, err
		}

		return Snapshot[T]{
			Value:   value,
			Content: content,
		}, nil
	}
}

// CheckOnce loads the current snapshot and invokes OnChange when it differs
// from the last successfully applied snapshot. The returned boolean reports
// whether OnChange was called and succeeded.
func (p *Poller[T]) CheckOnce(ctx context.Context) (bool, error) {
	if err := p.validate(); err != nil {
		return false, err
	}
	if err := ctx.Err(); err != nil {
		return false, err
	}

	p.mu.Lock()
	defer p.mu.Unlock()

	snapshot, err := p.Loader(ctx)
	if err != nil {
		return false, err
	}
	if err := ctx.Err(); err != nil {
		return false, err
	}

	identity := snapshotIdentity(snapshot)
	if p.seen && identity == p.identity {
		return false, nil
	}

	if err := p.OnChange(ctx, snapshot.Value); err != nil {
		return false, err
	}
	p.identity = identity
	p.seen = true
	return true, nil
}

// Run checks immediately, then checks again every Interval until ctx is
// cancelled or an unrecoverable error occurs.
func (p *Poller[T]) Run(ctx context.Context) error {
	if err := p.validate(); err != nil {
		return err
	}
	if p.Interval <= 0 {
		return fmt.Errorf("config: interval must be positive: %s", p.Interval)
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	if err := p.checkAndHandle(ctx); err != nil {
		return err
	}

	ticker := p.newTicker(p.Interval)
	if ticker == nil {
		return errors.New("config: ticker factory returned nil")
	}
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C():
			if err := p.checkAndHandle(ctx); err != nil {
				return err
			}
		}
	}
}

func (p *Poller[T]) validate() error {
	if p == nil {
		return errors.New("config: nil poller")
	}
	if p.Loader == nil {
		return errors.New("config: missing loader")
	}
	if p.OnChange == nil {
		return errors.New("config: missing OnChange")
	}
	return nil
}

func (p *Poller[T]) checkAndHandle(ctx context.Context) error {
	_, err := p.CheckOnce(ctx)
	if err == nil {
		return nil
	}
	if ctxErr := ctx.Err(); ctxErr != nil {
		return ctxErr
	}
	if p.OnError != nil {
		p.OnError(ctx, err)
		return nil
	}
	return err
}

func (p *Poller[T]) newTicker(interval time.Duration) Ticker {
	if p.TickerFactory != nil {
		return p.TickerFactory(interval)
	}
	return newRealTicker(interval)
}

func snapshotIdentity[T any](snapshot Snapshot[T]) string {
	if snapshot.Version != "" {
		return "version:" + snapshot.Version
	}
	sum := sha256.Sum256(snapshot.Content)
	return "sha256:" + hex.EncodeToString(sum[:])
}

type realTicker struct {
	ticker *time.Ticker
}

func newRealTicker(interval time.Duration) Ticker {
	return realTicker{ticker: time.NewTicker(interval)}
}

func (t realTicker) C() <-chan time.Time {
	return t.ticker.C
}

func (t realTicker) Stop() {
	t.ticker.Stop()
}
