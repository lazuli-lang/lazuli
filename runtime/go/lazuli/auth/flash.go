package auth

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
)

// DefaultFlashMaxBytes is the default maximum encoded flash payload size.
// It is intentionally cookie-sized so callers can store the returned string
// in either a session row or a short-lived cookie.
const DefaultFlashMaxBytes = 4096

// FlashLevel is the presentation level attached to a flash message.
type FlashLevel string

const (
	FlashLevelInfo    FlashLevel = "info"
	FlashLevelSuccess FlashLevel = "success"
	FlashLevelWarning FlashLevel = "warning"
	FlashLevelError   FlashLevel = "error"

	FlashInfo    = FlashLevelInfo
	FlashSuccess = FlashLevelSuccess
	FlashWarning = FlashLevelWarning
	FlashError   = FlashLevelError
)

var (
	ErrFlashInvalid      = errors.New("auth: flash invalid")
	ErrFlashSizeExceeded = errors.New("auth: flash size exceeded")
)

// FlashMessage is a single flash notification.
type FlashMessage struct {
	Level   FlashLevel `json:"level"`
	Message string     `json:"message"`
}

// Validate reports whether the message can be stored and rendered.
func (m FlashMessage) Validate() error {
	if err := m.Level.Validate(); err != nil {
		return err
	}
	if strings.TrimSpace(m.Message) == "" {
		return fmt.Errorf("%w: message is required", ErrFlashInvalid)
	}
	return nil
}

// NewFlashMessage returns a normalized flash message.
func NewFlashMessage(level FlashLevel, message string) (FlashMessage, error) {
	flash := FlashMessage{
		Level:   level,
		Message: strings.TrimSpace(message),
	}
	if err := flash.Validate(); err != nil {
		return FlashMessage{}, err
	}
	return flash, nil
}

// Valid reports whether the level is one of the supported flash levels.
func (l FlashLevel) Valid() bool {
	switch l {
	case FlashLevelInfo, FlashLevelSuccess, FlashLevelWarning, FlashLevelError:
		return true
	default:
		return false
	}
}

// Validate reports whether the level is one of the supported flash levels.
func (l FlashLevel) Validate() error {
	if !l.Valid() {
		return fmt.Errorf("%w: unknown level %q", ErrFlashInvalid, l)
	}
	return nil
}

// String returns the JSON/storage representation of the level.
func (l FlashLevel) String() string {
	return string(l)
}

// FlashQueue is an ordered queue of messages. Consume returns the queued
// messages and clears the queue so flash messages are shown once.
type FlashQueue []FlashMessage

// NewFlashQueue returns a validated queue containing messages.
func NewFlashQueue(messages ...FlashMessage) (FlashQueue, error) {
	queue := FlashQueue(messages).Clone()
	if err := queue.Validate(0); err != nil {
		return nil, err
	}
	return queue, nil
}

// Push appends a normalized message to the queue.
func (q *FlashQueue) Push(level FlashLevel, message string) error {
	flash, err := NewFlashMessage(level, message)
	if err != nil {
		return err
	}
	*q = append(*q, flash)
	return nil
}

// Add appends a normalized message to the queue.
func (q *FlashQueue) Add(level FlashLevel, message string) error {
	return q.Push(level, message)
}

// Clone returns a copy of the queue.
func (q FlashQueue) Clone() FlashQueue {
	if len(q) == 0 {
		return FlashQueue{}
	}
	return append(FlashQueue(nil), q...)
}

// Peek returns the queued messages without clearing them.
func (q FlashQueue) Peek() FlashQueue {
	return q.Clone()
}

// Consume returns the queued messages and clears the queue.
func (q *FlashQueue) Consume() FlashQueue {
	if q == nil {
		return FlashQueue{}
	}
	messages := q.Clone()
	*q = nil
	return messages
}

// Validate reports whether the queue can be encoded within maxBytes. A
// non-positive maxBytes uses DefaultFlashMaxBytes.
func (q FlashQueue) Validate(maxBytes int) error {
	return ValidateFlashMessages(q, maxBytes)
}

// Encode serializes the queue for cookie or session storage.
func (q FlashQueue) Encode() (string, error) {
	return EncodeFlashMessages(q)
}

// EncodeWithLimit serializes the queue for storage and enforces maxBytes.
func (q FlashQueue) EncodeWithLimit(maxBytes int) (string, error) {
	return EncodeFlashMessagesWithLimit(q, maxBytes)
}

// EncodeFlashMessages serializes messages as URL-safe base64 JSON.
func EncodeFlashMessages(messages []FlashMessage) (string, error) {
	return EncodeFlashMessagesWithLimit(messages, 0)
}

// EncodeFlashMessagesWithLimit serializes messages as URL-safe base64 JSON
// and rejects encoded payloads larger than maxBytes. A non-positive maxBytes
// uses DefaultFlashMaxBytes.
func EncodeFlashMessagesWithLimit(messages []FlashMessage, maxBytes int) (string, error) {
	if len(messages) == 0 {
		return "", nil
	}

	for i, message := range messages {
		if err := message.Validate(); err != nil {
			return "", fmt.Errorf("%w: message %d: %v", ErrFlashInvalid, i, err)
		}
	}

	raw, err := json.Marshal(messages)
	if err != nil {
		return "", fmt.Errorf("%w: encode json: %v", ErrFlashInvalid, err)
	}
	encoded := base64.RawURLEncoding.EncodeToString(raw)
	if err := validateFlashStorageSize(encoded, maxBytes); err != nil {
		return "", err
	}
	return encoded, nil
}

// DecodeFlashMessages deserializes a URL-safe base64 JSON flash payload.
func DecodeFlashMessages(encoded string) (FlashQueue, error) {
	return DecodeFlashMessagesWithLimit(encoded, 0)
}

// DecodeFlashMessagesWithLimit deserializes a URL-safe base64 JSON flash
// payload and rejects encoded payloads larger than maxBytes. A non-positive
// maxBytes uses DefaultFlashMaxBytes.
func DecodeFlashMessagesWithLimit(encoded string, maxBytes int) (FlashQueue, error) {
	if encoded == "" {
		return FlashQueue{}, nil
	}
	if err := validateFlashStorageSize(encoded, maxBytes); err != nil {
		return nil, err
	}

	raw, err := decodeFlashPayload(encoded)
	if err != nil {
		return nil, err
	}

	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()

	var messages []FlashMessage
	if err := decoder.Decode(&messages); err != nil {
		return nil, fmt.Errorf("%w: decode json: %v", ErrFlashInvalid, err)
	}
	if err := decoder.Decode(&struct{}{}); !errors.Is(err, io.EOF) {
		return nil, fmt.Errorf("%w: trailing json", ErrFlashInvalid)
	}

	queue := FlashQueue(messages)
	if err := queue.Validate(maxBytes); err != nil {
		return nil, err
	}
	return queue.Clone(), nil
}

// ConsumeFlashMessages decodes messages and returns an empty storage value for
// callers to persist after reading.
func ConsumeFlashMessages(encoded string) (FlashQueue, string, error) {
	return ConsumeFlashMessagesWithLimit(encoded, 0)
}

// ConsumeFlashMessagesWithLimit decodes messages and returns an empty storage
// value for callers to persist after reading.
func ConsumeFlashMessagesWithLimit(encoded string, maxBytes int) (FlashQueue, string, error) {
	queue, err := DecodeFlashMessagesWithLimit(encoded, maxBytes)
	if err != nil {
		return nil, "", err
	}
	return queue, "", nil
}

// ValidateFlashMessages checks message shape and encoded storage size.
func ValidateFlashMessages(messages []FlashMessage, maxBytes int) error {
	_, err := EncodeFlashMessagesWithLimit(messages, maxBytes)
	return err
}

func decodeFlashPayload(encoded string) ([]byte, error) {
	raw, err := base64.RawURLEncoding.DecodeString(encoded)
	if err == nil {
		return raw, nil
	}
	if raw, paddedErr := base64.URLEncoding.DecodeString(encoded); paddedErr == nil {
		return raw, nil
	}
	return nil, fmt.Errorf("%w: decode base64: %v", ErrFlashInvalid, err)
}

func validateFlashStorageSize(encoded string, maxBytes int) error {
	limit := maxBytes
	if limit <= 0 {
		limit = DefaultFlashMaxBytes
	}
	if len(encoded) > limit {
		return fmt.Errorf("%w: encoded bytes %d exceeds limit %d", ErrFlashSizeExceeded, len(encoded), limit)
	}
	return nil
}
