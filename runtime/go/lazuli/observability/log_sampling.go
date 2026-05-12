package observability

import (
	"context"
	"log/slog"
	"math"
	"sync"
	"time"
)

// LogSampler decides whether a slog record should be emitted.
//
// EXPERIMENTAL: subject to change before 1.0.
type LogSampler interface {
	// Sample returns true when record should be passed to the next handler.
	Sample(ctx context.Context, record slog.Record) bool
}

// LogSamplerFunc adapts a function into a LogSampler.
type LogSamplerFunc func(context.Context, slog.Record) bool

// Sample returns f(ctx, record). A nil function keeps the record.
func (f LogSamplerFunc) Sample(ctx context.Context, record slog.Record) bool {
	if f == nil {
		return true
	}
	return f(ctx, record)
}

// DeterministicRateSampler keeps records at a fixed rate without randomness.
//
// The sampler accumulates rate budget on each call and emits when the budget
// reaches one, producing a stable, evenly spaced sequence for a given call
// order. Rates outside [0, 1] are clamped; NaN is treated as zero.
//
// EXPERIMENTAL: subject to change before 1.0.
type DeterministicRateSampler struct {
	mu     sync.Mutex
	rate   float64
	budget float64
}

// NewDeterministicRateSampler returns a deterministic sampler for rate in
// [0.0, 1.0].
func NewDeterministicRateSampler(rate float64) *DeterministicRateSampler {
	switch {
	case math.IsNaN(rate) || rate <= 0:
		rate = 0
	case rate >= 1:
		rate = 1
	}
	return &DeterministicRateSampler{rate: rate}
}

// Sample implements LogSampler.
func (s *DeterministicRateSampler) Sample(ctx context.Context, record slog.Record) bool {
	_ = ctx
	_ = record
	if s == nil {
		return true
	}
	if s.rate <= 0 {
		return false
	}
	if s.rate >= 1 {
		return true
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.budget += s.rate
	if s.budget < 1 {
		return false
	}
	s.budget -= 1
	return true
}

// BurstSampler keeps at most limit records in each fixed time window.
//
// A non-positive limit drops all records. A non-positive window never refills
// after the initial burst.
//
// EXPERIMENTAL: subject to change before 1.0.
type BurstSampler struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu          sync.Mutex
	limit       int
	window      time.Duration
	windowStart time.Time
	used        int
}

// NewBurstSampler returns a sampler that permits limit records per window.
func NewBurstSampler(limit int, window time.Duration) *BurstSampler {
	return &BurstSampler{limit: limit, window: window}
}

// Sample implements LogSampler.
func (s *BurstSampler) Sample(ctx context.Context, record slog.Record) bool {
	_ = ctx
	_ = record
	if s == nil {
		return true
	}
	if s.limit <= 0 {
		return false
	}

	now := s.now()
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.windowStart.IsZero() || (s.window > 0 && !now.Before(s.windowStart.Add(s.window))) {
		s.windowStart = now
		s.used = 0
	}
	if s.used >= s.limit {
		return false
	}
	s.used++
	return true
}

func (s *BurstSampler) now() time.Time {
	if s.Clock != nil {
		return s.Clock()
	}
	return time.Now()
}

// LogSamplerKeyFunc extracts a sampling key from a record.
type LogSamplerKeyFunc func(context.Context, slog.Record) string

// LogRecordAttrKey returns a key function that reads the first matching slog
// attribute, including attributes inside groups.
func LogRecordAttrKey(name string) LogSamplerKeyFunc {
	return func(ctx context.Context, record slog.Record) string {
		_ = ctx
		if name == "" {
			return ""
		}

		var key string
		record.Attrs(func(attr slog.Attr) bool {
			value, ok := logRecordAttrValue(attr, name)
			if !ok {
				return true
			}
			key = value
			return false
		})
		return key
	}
}

func logRecordAttrValue(attr slog.Attr, name string) (string, bool) {
	attr.Value = attr.Value.Resolve()
	if attr.Key == name {
		return attr.Value.String(), true
	}
	if attr.Value.Kind() != slog.KindGroup {
		return "", false
	}
	for _, child := range attr.Value.Group() {
		if value, ok := logRecordAttrValue(child, name); ok {
			return value, true
		}
	}
	return "", false
}

// PerKeySampler keeps independent sampler state for each extracted key.
//
// EXPERIMENTAL: subject to change before 1.0.
type PerKeySampler struct {
	key        LogSamplerKeyFunc
	newSampler func(string) LogSampler

	mu       sync.Mutex
	samplers map[string]LogSampler
}

// NewPerKeySampler returns a sampler that delegates records with the same key
// to the same child sampler.
func NewPerKeySampler(key LogSamplerKeyFunc, newSampler func(string) LogSampler) *PerKeySampler {
	return &PerKeySampler{key: key, newSampler: newSampler}
}

// Sample implements LogSampler.
func (s *PerKeySampler) Sample(ctx context.Context, record slog.Record) bool {
	if s == nil {
		return true
	}
	if s.newSampler == nil {
		return true
	}

	key := ""
	if s.key != nil {
		key = s.key(ctx, record)
	}
	sampler := s.samplerFor(key)
	if sampler == nil {
		return true
	}
	return sampler.Sample(ctx, record)
}

func (s *PerKeySampler) samplerFor(key string) LogSampler {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.samplers == nil {
		s.samplers = make(map[string]LogSampler)
	}
	sampler, ok := s.samplers[key]
	if ok {
		return sampler
	}
	sampler = s.newSampler(key)
	s.samplers[key] = sampler
	return sampler
}

// SamplingHandler wraps a slog.Handler and drops records rejected by sampler.
//
// Enabled delegates to the wrapped handler because slog only supplies the
// complete record to Handle.
//
// EXPERIMENTAL: subject to change before 1.0.
type SamplingHandler struct {
	next    slog.Handler
	sampler LogSampler
	attrs   []slog.Attr
}

// NewSamplingHandler returns a slog handler that drops sampled-out records.
// A nil sampler leaves next unchanged.
func NewSamplingHandler(next slog.Handler, sampler LogSampler) slog.Handler {
	if sampler == nil {
		return next
	}
	return &SamplingHandler{next: next, sampler: sampler}
}

// Enabled implements slog.Handler.
func (h *SamplingHandler) Enabled(ctx context.Context, level slog.Level) bool {
	if h == nil || h.next == nil {
		return false
	}
	return h.next.Enabled(ctx, level)
}

// Handle implements slog.Handler.
func (h *SamplingHandler) Handle(ctx context.Context, record slog.Record) error {
	if h == nil || h.next == nil {
		return nil
	}
	if h.sampler != nil && !h.sampler.Sample(ctx, h.recordForSampling(record)) {
		return nil
	}
	return h.next.Handle(ctx, record)
}

// WithAttrs implements slog.Handler.
func (h *SamplingHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	if h == nil {
		return h
	}

	next := h.next
	if next != nil {
		next = next.WithAttrs(attrs)
	}
	combined := make([]slog.Attr, 0, len(h.attrs)+len(attrs))
	combined = append(combined, h.attrs...)
	combined = append(combined, attrs...)
	return &SamplingHandler{
		next:    next,
		sampler: h.sampler,
		attrs:   combined,
	}
}

// WithGroup implements slog.Handler.
func (h *SamplingHandler) WithGroup(name string) slog.Handler {
	if h == nil {
		return h
	}

	next := h.next
	if next != nil {
		next = next.WithGroup(name)
	}
	return &SamplingHandler{
		next:    next,
		sampler: h.sampler,
		attrs:   append([]slog.Attr(nil), h.attrs...),
	}
}

func (h *SamplingHandler) recordForSampling(record slog.Record) slog.Record {
	if len(h.attrs) == 0 {
		return record
	}
	cloned := record.Clone()
	cloned.AddAttrs(h.attrs...)
	return cloned
}
