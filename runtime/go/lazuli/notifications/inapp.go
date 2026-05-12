package notifications

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"time"
)

// ErrInAppMessageNotFound is returned when an in-app inbox helper
// cannot find the requested message for the recipient.
var ErrInAppMessageNotFound = errors.New("notifications: in-app message not found")

// InAppMessage is the stored representation of a ChannelInApp
// notification in an InAppDispatcher.
type InAppMessage struct {
	ID           string
	Tenant       string
	Recipient    string
	Payload      map[string]any
	TemplateData map[string]any
	CreatedAt    time.Time
	Acknowledged bool
	AckedAt      time.Time
}

// InAppDispatcher is an in-memory ChannelDispatcher for ChannelInApp.
// It is intended for tests and development deployments that need a
// process-local inbox. The zero value is usable.
type InAppDispatcher struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu       sync.Mutex
	nextID   uint64
	messages map[string][]InAppMessage
}

var _ ChannelDispatcher = (*InAppDispatcher)(nil)

// NewInAppDispatcher returns an empty in-memory in-app dispatcher.
func NewInAppDispatcher() *InAppDispatcher {
	return &InAppDispatcher{
		messages: make(map[string][]InAppMessage),
	}
}

// Channel implements ChannelDispatcher.
func (d *InAppDispatcher) Channel() Channel {
	return ChannelInApp
}

// Dispatch implements ChannelDispatcher by storing the envelope in the
// recipient's in-memory inbox.
func (d *InAppDispatcher) Dispatch(ctx context.Context, env Envelope) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	d.mu.Lock()
	defer d.mu.Unlock()
	if d.messages == nil {
		d.messages = make(map[string][]InAppMessage)
	}

	id := env.ID
	if id == "" {
		d.nextID++
		id = "in_app:" + strconv.FormatUint(d.nextID, 10)
	}
	msg := InAppMessage{
		ID:           id,
		Tenant:       env.Tenant,
		Recipient:    env.Recipient,
		Payload:      cloneNotificationPayload(env.Payload),
		TemplateData: cloneNotificationPayload(env.TemplateData),
		CreatedAt:    d.now(),
	}
	d.messages[env.Recipient] = append(d.messages[env.Recipient], msg)
	return nil
}

// List returns a snapshot of the recipient's in-app messages. The
// returned messages do not share payload maps with the dispatcher.
func (d *InAppDispatcher) List(ctx context.Context, recipient string) ([]InAppMessage, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	stored := d.messages[recipient]
	out := make([]InAppMessage, len(stored))
	for i := range stored {
		out[i] = cloneInAppMessage(stored[i])
	}
	return out, nil
}

// Ack marks a stored in-app message as acknowledged. This helper is
// process-local and does not model a cross-channel read receipt.
func (d *InAppDispatcher) Ack(ctx context.Context, recipient, id string) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	messages := d.messages[recipient]
	for i := range messages {
		if messages[i].ID == id {
			messages[i].Acknowledged = true
			messages[i].AckedAt = d.now()
			d.messages[recipient] = messages
			return nil
		}
	}
	return ErrInAppMessageNotFound
}

// Delete removes a stored in-app message from the recipient's inbox.
func (d *InAppDispatcher) Delete(ctx context.Context, recipient, id string) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	d.mu.Lock()
	defer d.mu.Unlock()

	messages := d.messages[recipient]
	for i := range messages {
		if messages[i].ID == id {
			var zero InAppMessage
			last := len(messages) - 1
			copy(messages[i:], messages[i+1:])
			messages[last] = zero
			messages = messages[:last]
			if len(messages) == 0 {
				delete(d.messages, recipient)
			} else {
				d.messages[recipient] = messages
			}
			return nil
		}
	}
	return ErrInAppMessageNotFound
}

func (d *InAppDispatcher) now() time.Time {
	if d.Clock != nil {
		return d.Clock()
	}
	return time.Now()
}

func cloneInAppMessage(msg InAppMessage) InAppMessage {
	msg.Payload = cloneNotificationPayload(msg.Payload)
	msg.TemplateData = cloneNotificationPayload(msg.TemplateData)
	return msg
}

func cloneNotificationPayload(payload map[string]any) map[string]any {
	if payload == nil {
		return nil
	}
	out := make(map[string]any, len(payload))
	for k, v := range payload {
		out[k] = cloneNotificationValue(v)
	}
	return out
}

func cloneNotificationValue(value any) any {
	switch v := value.(type) {
	case map[string]any:
		return cloneNotificationPayload(v)
	case []any:
		out := make([]any, len(v))
		for i := range v {
			out[i] = cloneNotificationValue(v[i])
		}
		return out
	case []byte:
		out := make([]byte, len(v))
		copy(out, v)
		return out
	default:
		return v
	}
}
