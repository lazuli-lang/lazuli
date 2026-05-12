package testkit

import (
	"context"
	"sort"
	"sync"
	"time"
)

// Clock is a manually advanced clock for tests that accept a time source.
//
// Pass Clock.Now anywhere the runtime expects a func() time.Time, Clock.After
// anywhere it expects a func(time.Duration) <-chan time.Time, and Clock.Sleep
// anywhere it expects a func(context.Context, time.Duration) error.
type Clock struct {
	mu  sync.Mutex
	now time.Time

	nextTimerSeq uint64
	timers       map[*clockTimer]struct{}
}

type clockTimer struct {
	at  time.Time
	seq uint64
	ch  chan time.Time
}

// NewClock returns a clock pinned to now.
func NewClock(now time.Time) *Clock {
	return &Clock{now: now}
}

// Now returns the clock's current time.
func (c *Clock) Now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.now
}

// Set pins the clock to now and fires timers due at or before that time.
func (c *Clock) Set(now time.Time) {
	c.mu.Lock()
	c.now = now
	due := c.dueTimersLocked()
	c.mu.Unlock()

	fireClockTimers(due)
}

// Advance moves the clock by d, fires timers due at or before the new time, and
// returns the new current time.
func (c *Clock) Advance(d time.Duration) time.Time {
	c.mu.Lock()
	c.now = c.now.Add(d)
	now := c.now
	due := c.dueTimersLocked()
	c.mu.Unlock()

	fireClockTimers(due)
	return now
}

// After waits for the clock to advance by d and then delivers the scheduled
// virtual time. Non-positive durations are ready immediately.
func (c *Clock) After(d time.Duration) <-chan time.Time {
	return c.afterTimer(d).ch
}

// Sleep blocks until the clock advances by d or ctx is canceled.
func (c *Clock) Sleep(ctx context.Context, d time.Duration) error {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if d <= 0 {
		return nil
	}

	timer := c.afterTimer(d)
	select {
	case <-timer.ch:
		return nil
	case <-ctx.Done():
		if !c.stopTimer(timer) {
			return nil
		}
		return ctx.Err()
	}
}

func (c *Clock) afterTimer(d time.Duration) *clockTimer {
	c.mu.Lock()
	defer c.mu.Unlock()

	ch := make(chan time.Time, 1)
	if d <= 0 {
		ch <- c.now
		return &clockTimer{at: c.now, ch: ch}
	}

	timer := &clockTimer{
		at:  c.now.Add(d),
		seq: c.nextTimerSeq,
		ch:  ch,
	}
	c.nextTimerSeq++
	if c.timers == nil {
		c.timers = make(map[*clockTimer]struct{})
	}
	c.timers[timer] = struct{}{}
	return timer
}

func (c *Clock) stopTimer(timer *clockTimer) bool {
	c.mu.Lock()
	defer c.mu.Unlock()

	if _, ok := c.timers[timer]; !ok {
		return false
	}
	delete(c.timers, timer)
	return true
}

func (c *Clock) dueTimersLocked() []*clockTimer {
	if len(c.timers) == 0 {
		return nil
	}

	due := make([]*clockTimer, 0, len(c.timers))
	for timer := range c.timers {
		if timer.at.After(c.now) {
			continue
		}
		due = append(due, timer)
		delete(c.timers, timer)
	}
	sort.Slice(due, func(i, j int) bool {
		if due[i].at.Equal(due[j].at) {
			return due[i].seq < due[j].seq
		}
		return due[i].at.Before(due[j].at)
	})
	return due
}

func fireClockTimers(timers []*clockTimer) {
	for _, timer := range timers {
		timer.ch <- timer.at
	}
}
