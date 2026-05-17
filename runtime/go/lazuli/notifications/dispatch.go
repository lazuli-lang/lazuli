// Package notifications — dispatcher surface. The generated code
// calls `notifications.Send(ctx, contract, payload)` to fire a
// notification across every channel the contract declares.
//
// Phase L Tier 3 / row 33 stubs.
package notifications

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"time"

	"github.com/tidwall/gjson"

	lazuli "lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/jobs"
)

// Registry binds channel dispatchers by Channel for the running app.
// Codegen builds the registry from the registry / app bindings at
// boot.
type Registry struct {
	dispatchers map[Channel]ChannelDispatcher
	throttle    ThrottleStore
	digest      DigestStore
}

// NewRegistry returns an empty registry. Adapter bindings register
// themselves via `Register`.
func NewRegistry() *Registry {
	return &Registry{dispatchers: make(map[Channel]ChannelDispatcher)}
}

// Register adds a per-channel dispatcher. Duplicate channels return
// an error; the language enforces at most one adapter per channel via
// registry contracts.
func (r *Registry) Register(disp ChannelDispatcher) error {
	if _, exists := r.dispatchers[disp.Channel()]; exists {
		return errors.New("notifications: channel already registered: " + string(disp.Channel()))
	}
	r.dispatchers[disp.Channel()] = disp
	return nil
}

// Lookup returns the registered dispatcher for `ch`, or (nil, false)
// when no adapter is bound. Lets `@fn` handlers reuse the same
// transport the codegen `notification.Send` path uses, without
// constructing a parallel dispatcher.
func (r *Registry) Lookup(ch Channel) (ChannelDispatcher, bool) {
	d, ok := r.dispatchers[ch]
	return d, ok
}

// RegisterThrottleStore wires the store consulted before each
// dispatch. The default registry has no throttle store — contracts
// declaring `throttle` then fall through to direct dispatch.
func (r *Registry) RegisterThrottleStore(store ThrottleStore) {
	r.throttle = store
}

// RegisterDigestStore wires the store consulted for `digest`
// contracts. nil disables batching (synchronous dispatch).
func (r *Registry) RegisterDigestStore(store DigestStore) {
	r.digest = store
}

// Send dispatches one notification across every channel its contract
// declares. Honors the throttle bucket + retry policy + emits events
// on success. Idempotency is delegated to channel adapters via the
// stable `Envelope.ID` (content-addressed); cross-provider dedupe is
// a future cell when a pilot needs it.
func Send(
	ctx context.Context,
	registry *Registry,
	contract NotificationContract,
	payload map[string]any,
) error {
	if contract.WithSource != nil {
		ctx = contract.WithSource(ctx)
	}

	payloadBytes, err := json.Marshal(payload)
	if err != nil {
		return fmt.Errorf("notifications: marshal payload: %w", err)
	}

	recipient := resolvePath(payloadBytes, contract.Recipient)
	if recipient == "" {
		slog.Info("notifications: recipient unresolved, skipping",
			"notification", contract.Name, "feature", contract.Feature)
		return nil
	}

	tenant := ""
	if contract.TenantFrom != nil {
		tenant = resolvePath(payloadBytes, contract.TenantFrom.Path)
		if tenant == "" {
			return ErrNotificationTenantUnresolved
		}
	}

	envelopeID := resolveIdempotencyKey(payloadBytes, contract)

	// Digest path: the synchronous dispatcher does not flush windows
	// itself — that's an external flusher cell. When `Digest` is set
	// and a store is wired, we Add and return; the flusher emits on
	// window close. When no store is wired the contract falls through
	// to direct dispatch (matches `bucket-notifications-expanded` v0).
	if contract.Digest != nil && registry.digest != nil {
		groupBy := resolvePath(payloadBytes, contract.Digest.GroupBy)
		_, _, derr := registry.digest.Add(ctx, contract.Name, groupBy, payload)
		if derr != nil && !errors.Is(derr, ErrDigestFull) {
			return fmt.Errorf("notifications: digest add: %w", derr)
		}
		return nil
	}

	deliveredAny := false
	throttledAny := false
	var lastErr error
	for _, ch := range contract.Channels {
		disp, ok := registry.dispatchers[ch]
		if !ok {
			lastErr = ErrNotificationChannelUnsupported
			continue
		}
		if registry.throttle != nil && contract.Throttle != nil {
			allowed, _, terr := registry.throttle.Allow(ctx, ThrottleKey{
				Notification: contract.Name,
				Recipient:    recipient,
				Channel:      ch,
			}, *contract.Throttle)
			if terr != nil && !errors.Is(terr, ErrThrottleExceeded) {
				lastErr = terr
				continue
			}
			if !allowed {
				slog.Info("notifications: throttle exceeded, skipping",
					"notification", contract.Name, "channel", ch,
					"recipient", recipient)
				throttledAny = true
				continue
			}
		}
		env := Envelope{
			ID:           envelopeID,
			Tenant:       tenant,
			Channel:      ch,
			Recipient:    recipient,
			Payload:      payload,
			TemplateData: payload,
		}
		if err := dispatchWithRetry(ctx, disp, env, contract.Retry); err != nil {
			lastErr = err
			continue
		}
		deliveredAny = true
	}

	if !deliveredAny {
		if throttledAny && lastErr == nil {
			// Every eligible channel was throttle-skipped — intentional,
			// not a delivery failure. Return nil per contract.go:142.
			return nil
		}
		if lastErr == nil {
			return ErrNotificationDeliveryFailed
		}
		return lastErr
	}

	for _, name := range contract.Emits {
		lazuli.Publish(ctx, lazuli.Event{
			Name:       name,
			Payload:    payload,
			OccurredAt: time.Now(),
		})
	}
	return nil
}

// dispatchWithRetry invokes the adapter's Dispatch, retrying per the
// contract policy with `jobs.NextDelay`-driven backoff. Returns nil
// on first success.
func dispatchWithRetry(
	ctx context.Context,
	disp ChannelDispatcher,
	env Envelope,
	retry *RetryPolicy,
) error {
	maxAttempts := uint32(1)
	if retry != nil && retry.Count > 0 {
		maxAttempts = retry.Count + 1
	}
	var err error
	for attempt := uint32(0); attempt < maxAttempts; attempt++ {
		if attempt > 0 && retry != nil {
			delay := jobs.NextDelay(jobs.RetryPolicy{
				Count:   retry.Count,
				Backoff: jobs.BackoffStrategy(retry.Backoff),
			}, attempt)
			if delay > 0 {
				timer := time.NewTimer(delay)
				select {
				case <-ctx.Done():
					timer.Stop()
					return ctx.Err()
				case <-timer.C:
				}
			}
		}
		err = disp.Dispatch(ctx, env)
		if err == nil {
			return nil
		}
	}
	return err
}

// resolvePath returns the string value at `path` inside the marshaled
// payload. Unresolved dotted paths return ""; non-dotted literals
// (e.g. a fixed email address or a slug) pass through unchanged so
// pilots can author `recipient "ops@example.com"` directly.
func resolvePath(payload []byte, path string) string {
	if path == "" {
		return ""
	}
	res := gjson.GetBytes(payload, path)
	if res.Exists() {
		return res.String()
	}
	// Dotted paths that don't resolve are unresolved (skip); literal
	// strings without a dot are treated as authored constants.
	for i := 0; i < len(path); i++ {
		if path[i] == '.' {
			return ""
		}
	}
	return path
}

// resolveIdempotencyKey returns the contract-declared key resolved
// against the payload, falling back to a content-addressed SHA-256
// of the payload + notification name.
func resolveIdempotencyKey(payload []byte, contract NotificationContract) string {
	if contract.Idempotency != nil {
		key := resolvePath(payload, contract.Idempotency.Path)
		if key != "" {
			return key
		}
	}
	h := sha256.New()
	h.Write([]byte(contract.Feature))
	h.Write([]byte{':'})
	h.Write([]byte(contract.Name))
	h.Write([]byte{':'})
	h.Write(payload)
	return hex.EncodeToString(h.Sum(nil))
}
