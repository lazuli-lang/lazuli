// Package notifications — dispatcher surface. The generated code
// calls `notifications.Send(ctx, contract, payload)` to fire a
// notification across every channel the contract declares.
//
// Phase L Tier 3 / row 33 stubs.
package notifications

import (
	"context"
	"errors"
)

// Registry binds channel dispatchers by Channel for the running app.
// Codegen builds the registry from the registry / app bindings at
// boot.
type Registry struct {
	dispatchers map[Channel]ChannelDispatcher
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

// Send dispatches one notification across every channel its contract
// declares. Honors the retry policy on per-channel delivery failure.
//
// Stub: full implementation (template rendering, idempotency dedupe,
// tenant resolution, fan-out to channels, retry budget tracking)
// lands with the runtime team.
func Send(
	ctx context.Context,
	registry *Registry,
	contract NotificationContract,
	payload map[string]any,
) error {
	if contract.WithSource != nil {
		ctx = contract.WithSource(ctx)
	}
	_ = ctx
	_ = registry
	_ = contract
	_ = payload
	// TODO(runtime): resolve recipient via contract.Recipient path,
	// dedupe via contract.Idempotency.Path, dispatch to each channel
	// via registry.dispatchers, apply RetryPolicy on per-channel
	// delivery error, emit declared events on success, propagate
	// audit + observability span.
	return errors.New("notifications: Send not yet implemented")
}
