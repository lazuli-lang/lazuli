// Package perf contains small performance helpers for generated Lazuli
// runtimes.
package perf

import (
	"bytes"
	"reflect"
	"sync"
	"sync/atomic"
)

const (
	// DefaultByteBufferMaxRetainedSize is the largest bytes.Buffer capacity
	// retained by a zero-value ByteBufferPool.
	DefaultByteBufferMaxRetainedSize = 64 * 1024
)

// PoolStats is a point-in-time snapshot of Pool activity.
type PoolStats struct {
	// Gets is the cumulative number of Get calls.
	Gets uint64 `json:"gets"`
	// Hits is the cumulative number of Get calls served by a retained value.
	Hits uint64 `json:"hits"`
	// Misses is the cumulative number of Get calls that had to create or return
	// a zero value.
	Misses uint64 `json:"misses"`
	// Puts is the cumulative number of values retained by Put.
	Puts uint64 `json:"puts"`
	// Drops is the cumulative number of values rejected by Put.
	Drops uint64 `json:"drops"`
}

// Pool is a typed wrapper around sync.Pool with lightweight counters.
//
// The zero value is ready to use and returns the zero value of T on misses.
// Like sync.Pool, Pool must not be copied after first use.
type Pool[T any] struct {
	pool     sync.Pool
	newValue func() T

	gets   atomic.Uint64
	hits   atomic.Uint64
	misses atomic.Uint64
	puts   atomic.Uint64
	drops  atomic.Uint64
}

// NewPool returns a Pool that calls newValue when Get finds no retained value.
func NewPool[T any](newValue func() T) *Pool[T] {
	return &Pool[T]{
		newValue: newValue,
	}
}

// Get returns a retained value or a new value when the pool is empty.
func (p *Pool[T]) Get() T {
	var zero T
	if p == nil {
		return zero
	}

	p.gets.Add(1)
	value := p.pool.Get()
	if value == nil {
		p.misses.Add(1)
		if p.newValue != nil {
			return p.newValue()
		}
		return zero
	}

	p.hits.Add(1)
	return value.(T)
}

// Put returns value to the pool. Nil values are dropped so later Get calls do
// not receive typed nils.
func (p *Pool[T]) Put(value T) {
	if p == nil {
		return
	}
	if isNilValue(value) {
		p.drop()
		return
	}

	p.pool.Put(value)
	p.puts.Add(1)
}

// Stats returns a point-in-time snapshot of pool counters.
func (p *Pool[T]) Stats() PoolStats {
	if p == nil {
		return PoolStats{}
	}
	return PoolStats{
		Gets:   p.gets.Load(),
		Hits:   p.hits.Load(),
		Misses: p.misses.Load(),
		Puts:   p.puts.Load(),
		Drops:  p.drops.Load(),
	}
}

func (p *Pool[T]) drop() {
	p.drops.Add(1)
}

// ByteBufferPool pools bytes.Buffer values and drops buffers whose capacity is
// larger than the configured maximum retained size.
//
// The zero value is ready to use with DefaultByteBufferMaxRetainedSize.
type ByteBufferPool struct {
	once sync.Once
	pool Pool[*bytes.Buffer]

	maxRetainedSize int
}

// NewByteBufferPool returns a buffer pool. Non-positive maxRetainedSize uses
// DefaultByteBufferMaxRetainedSize.
func NewByteBufferPool(maxRetainedSize int) *ByteBufferPool {
	p := &ByteBufferPool{
		maxRetainedSize: maxRetainedSize,
	}
	p.init()
	return p
}

// Get returns an empty buffer from the pool.
func (p *ByteBufferPool) Get() *bytes.Buffer {
	if p == nil {
		return new(bytes.Buffer)
	}
	p.init()

	buf := p.pool.Get()
	if buf == nil {
		return new(bytes.Buffer)
	}
	buf.Reset()
	return buf
}

// Put resets buf and returns it to the pool when its capacity is within the
// pool's retention limit.
func (p *ByteBufferPool) Put(buf *bytes.Buffer) {
	if p == nil {
		return
	}
	p.init()

	if buf == nil {
		p.pool.Put(buf)
		return
	}
	if buf.Cap() > p.maxRetainedSize {
		buf.Reset()
		p.pool.drop()
		return
	}

	buf.Reset()
	p.pool.Put(buf)
}

// Stats returns a point-in-time snapshot of buffer pool counters.
func (p *ByteBufferPool) Stats() PoolStats {
	if p == nil {
		return PoolStats{}
	}
	p.init()
	return p.pool.Stats()
}

func (p *ByteBufferPool) init() {
	p.once.Do(func() {
		if p.maxRetainedSize <= 0 {
			p.maxRetainedSize = DefaultByteBufferMaxRetainedSize
		}
		p.pool.newValue = func() *bytes.Buffer {
			return new(bytes.Buffer)
		}
	})
}

func isNilValue[T any](value T) bool {
	v := reflect.ValueOf(value)
	if !v.IsValid() {
		return true
	}

	switch v.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Pointer, reflect.Slice:
		return v.IsNil()
	default:
		return false
	}
}
