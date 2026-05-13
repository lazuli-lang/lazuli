package auth

import (
	"encoding/base64"
	"errors"
	"testing"
)

func TestFlashQueueEncodeDecodeRoundTrip(t *testing.T) {
	t.Parallel()

	queue := FlashQueue{}
	if err := queue.Push(FlashLevelSuccess, " Saved successfully "); err != nil {
		t.Fatalf("Push(success) error = %v", err)
	}
	if err := queue.Add(FlashLevelInfo, "Check your email"); err != nil {
		t.Fatalf("Add(info) error = %v", err)
	}

	encoded, err := queue.Encode()
	if err != nil {
		t.Fatalf("Encode() error = %v", err)
	}
	if encoded == "" {
		t.Fatal("Encode() returned empty payload")
	}

	decoded, err := DecodeFlashMessages(encoded)
	if err != nil {
		t.Fatalf("DecodeFlashMessages() error = %v", err)
	}
	if len(decoded) != 2 {
		t.Fatalf("decoded len = %d, want 2", len(decoded))
	}
	if decoded[0].Level != FlashLevelSuccess || decoded[0].Message != "Saved successfully" {
		t.Fatalf("decoded[0] = %#v", decoded[0])
	}
	if decoded[1].Level != FlashLevelInfo || decoded[1].Message != "Check your email" {
		t.Fatalf("decoded[1] = %#v", decoded[1])
	}
}

func TestFlashQueueConsumeClearsMessages(t *testing.T) {
	t.Parallel()

	queue, err := NewFlashQueue(
		FlashMessage{Level: FlashLevelWarning, Message: "Confirm your new email"},
		FlashMessage{Level: FlashLevelError, Message: "Card declined"},
	)
	if err != nil {
		t.Fatalf("NewFlashQueue() error = %v", err)
	}

	peeked := queue.Peek()
	peeked[0].Message = "mutated"
	if queue[0].Message != "Confirm your new email" {
		t.Fatalf("Peek() exposed internal storage: %#v", queue[0])
	}

	consumed := queue.Consume()
	if len(consumed) != 2 {
		t.Fatalf("Consume() len = %d, want 2", len(consumed))
	}
	if len(queue) != 0 {
		t.Fatalf("queue len after Consume() = %d, want 0", len(queue))
	}

	encoded, err := consumed.Encode()
	if err != nil {
		t.Fatalf("Encode(consumed) error = %v", err)
	}
	read, next, err := ConsumeFlashMessages(encoded)
	if err != nil {
		t.Fatalf("ConsumeFlashMessages() error = %v", err)
	}
	if len(read) != 2 {
		t.Fatalf("ConsumeFlashMessages() len = %d, want 2", len(read))
	}
	if next != "" {
		t.Fatalf("next storage value = %q, want empty", next)
	}
}

func TestFlashMessagesEmptyStorage(t *testing.T) {
	t.Parallel()

	encoded, err := EncodeFlashMessages(nil)
	if err != nil {
		t.Fatalf("EncodeFlashMessages(nil) error = %v", err)
	}
	if encoded != "" {
		t.Fatalf("EncodeFlashMessages(nil) = %q, want empty", encoded)
	}

	decoded, err := DecodeFlashMessages("")
	if err != nil {
		t.Fatalf("DecodeFlashMessages(empty) error = %v", err)
	}
	if len(decoded) != 0 {
		t.Fatalf("DecodeFlashMessages(empty) len = %d, want 0", len(decoded))
	}
}

func TestFlashValidationRejectsInvalidMessages(t *testing.T) {
	t.Parallel()

	for _, tt := range []struct {
		name     string
		messages []FlashMessage
	}{
		{
			name:     "unknown level",
			messages: []FlashMessage{{Level: FlashLevel("debug"), Message: "Details"}},
		},
		{
			name:     "empty message",
			messages: []FlashMessage{{Level: FlashLevelInfo, Message: " \t "}},
		},
	} {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := ValidateFlashMessages(tt.messages, 0); !errors.Is(err, ErrFlashInvalid) {
				t.Fatalf("ValidateFlashMessages() error = %v, want ErrFlashInvalid", err)
			}
		})
	}
}

func TestFlashStorageSizeValidation(t *testing.T) {
	t.Parallel()

	messages := []FlashMessage{{Level: FlashLevelInfo, Message: "This message is too large for the tiny test limit"}}

	if _, err := EncodeFlashMessagesWithLimit(messages, 24); !errors.Is(err, ErrFlashSizeExceeded) {
		t.Fatalf("EncodeFlashMessagesWithLimit() error = %v, want ErrFlashSizeExceeded", err)
	}

	encoded, err := EncodeFlashMessages(messages)
	if err != nil {
		t.Fatalf("EncodeFlashMessages() setup error = %v", err)
	}
	if _, err := DecodeFlashMessagesWithLimit(encoded, len(encoded)-1); !errors.Is(err, ErrFlashSizeExceeded) {
		t.Fatalf("DecodeFlashMessagesWithLimit() error = %v, want ErrFlashSizeExceeded", err)
	}
}

func TestDecodeFlashMessagesRejectsMalformedPayloads(t *testing.T) {
	t.Parallel()

	for _, tt := range []struct {
		name    string
		encoded string
	}{
		{
			name:    "invalid base64",
			encoded: "not a flash payload",
		},
		{
			name:    "invalid json",
			encoded: base64.RawURLEncoding.EncodeToString([]byte(`{"level":"info","message":"not an array"}`)),
		},
		{
			name:    "unknown field",
			encoded: base64.RawURLEncoding.EncodeToString([]byte(`[{"level":"info","message":"Saved","extra":true}]`)),
		},
		{
			name:    "trailing json",
			encoded: base64.RawURLEncoding.EncodeToString([]byte(`[{"level":"info","message":"Saved"}] []`)),
		},
	} {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if _, err := DecodeFlashMessages(tt.encoded); !errors.Is(err, ErrFlashInvalid) {
				t.Fatalf("DecodeFlashMessages() error = %v, want ErrFlashInvalid", err)
			}
		})
	}
}
