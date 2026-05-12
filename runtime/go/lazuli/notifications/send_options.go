package notifications

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"
)

// SendOptions configures optional dispatch hooks for SendWithOptions.
type SendOptions struct {
	// ThrottleStore gates each channel dispatch when the contract
	// declares a throttle block. Nil disables runtime throttling.
	ThrottleStore ThrottleStore
	// IdempotencyStore claims the contract idempotency key before any
	// channel dispatch. Nil disables runtime dedupe.
	IdempotencyStore IdempotencyStore
	// RetrySleep waits between retry attempts. Nil uses a real timer;
	// tests can inject a no-op sleeper.
	RetrySleep func(context.Context, time.Duration) error
}

// IdempotencyStore atomically claims notification idempotency keys.
type IdempotencyStore interface {
	// Claim returns true when key was not seen before and has now been
	// recorded. It returns false when the key is already claimed.
	Claim(ctx context.Context, key string) (bool, error)
}

// MemoryIdempotencyStore is the in-process reference idempotency store.
type MemoryIdempotencyStore struct {
	mu   sync.Mutex
	keys map[string]struct{}
}

// NewMemoryIdempotencyStore returns an empty in-process idempotency store.
func NewMemoryIdempotencyStore() *MemoryIdempotencyStore {
	return &MemoryIdempotencyStore{keys: make(map[string]struct{})}
}

// Claim implements IdempotencyStore.
func (m *MemoryIdempotencyStore) Claim(ctx context.Context, key string) (bool, error) {
	select {
	case <-ctx.Done():
		return false, ctx.Err()
	default:
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	if _, exists := m.keys[key]; exists {
		return false, nil
	}
	m.keys[key] = struct{}{}
	return true, nil
}

// ErrNotificationRegistryNil is returned when SendWithOptions receives no registry.
var ErrNotificationRegistryNil = errors.New("notifications: registry is nil")

// ErrNotificationNoChannels is returned when a contract declares no channels.
var ErrNotificationNoChannels = errors.New("notifications: no channels declared")

// ErrNotificationRecipientUnresolved is returned when the recipient path is missing or empty.
var ErrNotificationRecipientUnresolved = errors.New("notifications: recipient unresolved")

// ErrNotificationIdempotencyUnresolved is returned when the idempotency path is missing or empty.
var ErrNotificationIdempotencyUnresolved = errors.New("notifications: idempotency key unresolved")

// SendWithOptions dispatches a notification contract across its declared channels.
//
// Recipient, tenant_from, and idempotency paths are resolved against payload
// using dot notation. A leading "payload." segment is accepted for parity with
// lowered Lazuli contracts.
func SendWithOptions(
	ctx context.Context,
	registry *Registry,
	contract NotificationContract,
	payload map[string]any,
	opts SendOptions,
) error {
	if registry == nil {
		return ErrNotificationRegistryNil
	}
	if len(contract.Channels) == 0 {
		return ErrNotificationNoChannels
	}

	dispatchers := make([]ChannelDispatcher, len(contract.Channels))
	var preflightErrs []error
	for i, channel := range contract.Channels {
		disp := registry.dispatchers[channel]
		if disp == nil {
			preflightErrs = append(preflightErrs, fmt.Errorf(
				"%w: %s channel %q",
				ErrNotificationChannelUnsupported,
				notificationName(contract),
				channel,
			))
			continue
		}
		dispatchers[i] = disp
	}
	if err := errors.Join(preflightErrs...); err != nil {
		return err
	}

	recipient, err := resolveRequiredPathString(payload, contract.Recipient, ErrNotificationRecipientUnresolved, contract)
	if err != nil {
		return err
	}

	tenant := ""
	if contract.TenantFrom != nil && contract.TenantFrom.Path != "" {
		tenant, err = resolveRequiredPathString(payload, contract.TenantFrom.Path, ErrNotificationTenantUnresolved, contract)
		if err != nil {
			return err
		}
	}

	idempotencyKey := ""
	if contract.Idempotency != nil && contract.Idempotency.Path != "" {
		idempotencyKey, err = resolveRequiredPathString(payload, contract.Idempotency.Path, ErrNotificationIdempotencyUnresolved, contract)
		if err != nil {
			return err
		}
		if opts.IdempotencyStore != nil {
			claimed, err := opts.IdempotencyStore.Claim(ctx, scopedIdempotencyKey(contract, tenant, idempotencyKey))
			if err != nil {
				return fmt.Errorf("notifications: claim idempotency key for %s: %w", notificationName(contract), err)
			}
			if !claimed {
				return nil
			}
		}
	}

	var dispatchErrs []error
	for i, channel := range contract.Channels {
		if err := ctx.Err(); err != nil {
			return err
		}

		env := Envelope{
			ID:           idempotencyKey,
			Tenant:       tenant,
			Channel:      channel,
			Recipient:    recipient,
			Payload:      payload,
			TemplateData: payload,
		}

		if contract.Throttle != nil && opts.ThrottleStore != nil {
			allowed, _, err := opts.ThrottleStore.Allow(ctx, throttleKey(contract, env), *contract.Throttle)
			if err != nil {
				if errors.Is(err, ErrThrottleExceeded) {
					continue
				}
				dispatchErrs = append(dispatchErrs, fmt.Errorf(
					"notifications: throttle check failed for %s channel %q: %w",
					notificationName(contract),
					channel,
					err,
				))
				continue
			}
			if !allowed {
				continue
			}
		}

		if err := dispatchWithRetry(ctx, dispatchers[i], contract, env, opts.RetrySleep); err != nil {
			dispatchErrs = append(dispatchErrs, err)
		}
	}

	return errors.Join(dispatchErrs...)
}

func dispatchWithRetry(
	ctx context.Context,
	disp ChannelDispatcher,
	contract NotificationContract,
	env Envelope,
	sleep func(context.Context, time.Duration) error,
) error {
	if sleep == nil {
		sleep = sleepWithContext
	}

	attempts := uint64(1)
	if contract.Retry != nil {
		attempts = uint64(contract.Retry.Count) + 1
	}

	var lastErr error
	for attempt := uint64(0); attempt < attempts; attempt++ {
		if err := ctx.Err(); err != nil {
			return err
		}
		if attempt > 0 && contract.Retry != nil {
			delay := notificationNextDelay(*contract.Retry, uint32(attempt))
			if err := sleep(ctx, delay); err != nil {
				return err
			}
		}

		if err := disp.Dispatch(ctx, env); err != nil {
			lastErr = err
			continue
		}
		return nil
	}

	if lastErr == nil {
		return nil
	}
	return fmt.Errorf(
		"%w: %s channel %q: %w",
		ErrNotificationDeliveryFailed,
		notificationName(contract),
		env.Channel,
		lastErr,
	)
}

func sleepWithContext(ctx context.Context, delay time.Duration) error {
	if delay <= 0 {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
			return nil
		}
	}

	timer := time.NewTimer(delay)
	defer timer.Stop()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func notificationNextDelay(policy RetryPolicy, attempt uint32) time.Duration {
	if attempt == 0 {
		return 0
	}
	switch policy.Backoff {
	case "exponential":
		delay := notificationRetryBaseDelay
		for i := uint32(1); i < attempt; i++ {
			if delay >= notificationRetryMaxDelay/2 {
				return notificationRetryMaxDelay
			}
			delay *= 2
			if delay > notificationRetryMaxDelay {
				return notificationRetryMaxDelay
			}
		}
		return delay
	case "fixed":
		return notificationRetryBaseDelay
	default:
		return notificationRetryBaseDelay
	}
}

func resolveRequiredPathString(
	payload map[string]any,
	path string,
	sentinel error,
	contract NotificationContract,
) (string, error) {
	value, ok := resolvePayloadPath(payload, path)
	if !ok {
		return "", unresolvedPathError(sentinel, contract, path)
	}
	resolved, ok := stringifyPathValue(value)
	if !ok || resolved == "" {
		return "", unresolvedPathError(sentinel, contract, path)
	}
	return resolved, nil
}

func resolvePayloadPath(payload map[string]any, path string) (any, bool) {
	path = strings.TrimSpace(path)
	if path == "" {
		return nil, false
	}

	parts := strings.Split(path, ".")
	if len(parts) > 0 && parts[0] == "payload" {
		parts = parts[1:]
	}
	if len(parts) == 0 {
		return nil, false
	}

	var current any = payload
	for _, part := range parts {
		if part == "" {
			return nil, false
		}
		switch node := current.(type) {
		case map[string]any:
			next, ok := node[part]
			if !ok {
				return nil, false
			}
			current = next
		case map[string]string:
			next, ok := node[part]
			if !ok {
				return nil, false
			}
			current = next
		default:
			return nil, false
		}
	}

	return current, true
}

func stringifyPathValue(value any) (string, bool) {
	switch v := value.(type) {
	case string:
		return v, true
	case []byte:
		return string(v), true
	case fmt.Stringer:
		return v.String(), true
	case bool:
		return strconv.FormatBool(v), true
	case int:
		return strconv.FormatInt(int64(v), 10), true
	case int8:
		return strconv.FormatInt(int64(v), 10), true
	case int16:
		return strconv.FormatInt(int64(v), 10), true
	case int32:
		return strconv.FormatInt(int64(v), 10), true
	case int64:
		return strconv.FormatInt(v, 10), true
	case uint:
		return strconv.FormatUint(uint64(v), 10), true
	case uint8:
		return strconv.FormatUint(uint64(v), 10), true
	case uint16:
		return strconv.FormatUint(uint64(v), 10), true
	case uint32:
		return strconv.FormatUint(uint64(v), 10), true
	case uint64:
		return strconv.FormatUint(v, 10), true
	case float32:
		return strconv.FormatFloat(float64(v), 'f', -1, 32), true
	case float64:
		return strconv.FormatFloat(v, 'f', -1, 64), true
	default:
		return "", false
	}
}

func unresolvedPathError(sentinel error, contract NotificationContract, path string) error {
	return fmt.Errorf("%w: %s path %q", sentinel, notificationName(contract), path)
}

func throttleKey(contract NotificationContract, env Envelope) ThrottleKey {
	key := ThrottleKey{Notification: notificationName(contract)}
	if contract.Throttle != nil {
		if contract.Throttle.PerRecipient {
			key.Recipient = env.Recipient
		}
		if contract.Throttle.PerChannel {
			key.Channel = env.Channel
		}
	}
	return key
}

func scopedIdempotencyKey(contract NotificationContract, tenant, idempotencyKey string) string {
	name := notificationName(contract)
	if tenant == "" {
		return name + ":" + idempotencyKey
	}
	return name + ":" + tenant + ":" + idempotencyKey
}

func notificationName(contract NotificationContract) string {
	switch {
	case contract.Feature != "" && contract.Name != "":
		return contract.Feature + "." + contract.Name
	case contract.Feature != "":
		return contract.Feature
	case contract.Name != "":
		return contract.Name
	default:
		return "<unnamed>"
	}
}

const (
	notificationRetryBaseDelay = 5 * time.Second
	notificationRetryMaxDelay  = 5 * time.Minute
)
